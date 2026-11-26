// What a `stop-first` update on a cluster node replaces: the running
// containers whose Compose configuration differs from the target's files.
// They leave first (`domain::leaving`) when another node serves their
// service. On a node Podman's Compose provider is Docker Compose, which
// hashes each service's configuration and labels its containers with it,
// as on Docker.
//
// A `start-first` update starts the new container next to the old one with
// `up --scale N+1 --no-recreate`, as on Docker; the old one is removed once
// the new one is healthy and known to every node. On a remote host both
// have the service's fixed user ids, so the new one keeps the old one's
// volumes (measured on Podman 5.4 with Docker Compose 2.26).

impl PodmanComposeRuntime {
    /// Compose's hash of each service's configuration in the target's
    /// files.
    async fn config_hashes(
        &self,
        target: &RuntimeTarget,
    ) -> Result<std::collections::BTreeMap<String, String>, MmError> {
        let mut args = self.compose_args(target)?;
        args.extend(["config".to_owned(), "--hash".to_owned(), "*".to_owned()]);
        let output = self
            .process
            .run(
                self.command_spec(args)
                    .interpolation_environment(&target.interpolation_environment)
                    .current_dir(&target.root)
                    .timeout(Duration::from_secs(45))
                    .internal_stdout(),
            )
            .await?;
        let output = self.ensure_success(output)?;
        Ok(output
            .stdout
            .lines()
            .filter_map(|line| line.trim().split_once(' '))
            .map(|(service, hash)| (service.to_owned(), hash.trim().to_owned()))
            .collect())
    }

    /// Has Compose count `count` containers of `service`: it starts the
    /// ones it misses and touches none it has. The autoscaler's step: one
    /// more container beside the others, so it needs no `start-first`.
    async fn start_more(
        &self,
        target: &RuntimeTarget,
        service: &str,
        count: u32,
    ) -> Result<(), MmError> {
        require_local_target(target)?;
        self.require_local_engine_environment()?;
        self.preflight_project(target).await?;
        crate::adapters::runtime::podman_apparmor::require_shared_pid_confined(target)?;
        let auth = crate::adapters::runtime::registry_auth::RegistryAuth::prepare(
            &target.registry_credentials,
        )?;
        let mut args = self.compose_args(target)?;
        args.extend([
            "up".to_owned(),
            "-d".to_owned(),
            "--no-deps".to_owned(),
            "--no-recreate".to_owned(),
            "--no-build".to_owned(),
            "--scale".to_owned(),
            format!("{service}={count}"),
            service.to_owned(),
        ]);
        let output = self
            .process
            .run(
                auth.podman(self.command_spec(args))
                    .interpolation_environment(&target.interpolation_environment)
                    .current_dir(&target.root)
                    .timeout(Duration::from_secs(300)),
            )
            .await?;
        self.ensure_started(output, target, &[service.to_owned()])
    }

    async fn outdated(
        &self,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        require_local_target(target)?;
        if services.is_empty() {
            return Ok(Vec::new());
        }
        self.require_local_engine_environment()?;
        self.preflight_project(target).await?;
        let hashes = self.config_hashes(target).await?;
        crate::adapters::runtime::project_services::outdated_containers(
            self,
            &target.project_name,
            services,
            &hashes,
        )
        .await
    }

    async fn start_beside_old(
        &self,
        target: &RuntimeTarget,
        services: &[String],
        leaving: &[String],
    ) -> Result<Vec<String>, MmError> {
        use crate::domain::rollout::{Beside, beside};

        require_local_target(target)?;
        self.require_local_engine_environment()?;
        self.preflight_project(target).await?;
        crate::adapters::runtime::podman_apparmor::require_shared_pid_confined(target)?;
        let hashes = self.config_hashes(target).await?;
        let containers = crate::adapters::runtime::project_services::stack_containers(
            self,
            &target.project_name,
            leaving,
        )
        .await?;
        let mut started = Vec::new();
        for service in services {
            let Some(wanted) = hashes.get(service) else {
                continue;
            };
            let own = (containers.iter())
                .filter(|container| &container.service == service)
                .cloned()
                .collect::<Vec<_>>();
            let scale = match beside(wanted, &own) {
                Beside::Nothing => continue,
                Beside::Started => {
                    started.push(service.clone());
                    continue;
                }
                Beside::Start { scale } => scale,
            };
            let auth = crate::adapters::runtime::registry_auth::RegistryAuth::prepare(
                &target.registry_credentials,
            )?;
            let mut args = self.compose_args(target)?;
            args.extend([
                "up".to_owned(),
                "-d".to_owned(),
                "--no-deps".to_owned(),
                "--no-recreate".to_owned(),
                "--no-build".to_owned(),
                "--scale".to_owned(),
                format!("{service}={scale}"),
                service.clone(),
            ]);
            let output = self
                .process
                .run(
                    auth.podman(self.command_spec(args))
                        .interpolation_environment(&target.interpolation_environment)
                        .current_dir(&target.root)
                        .timeout(Duration::from_secs(300)),
                )
                .await?;
            self.ensure_started(output, target, std::slice::from_ref(service))?;
            started.push(service.clone());
        }
        Ok(started)
    }

    async fn replaced_by_newer(
        &self,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        require_local_target(target)?;
        self.require_local_engine_environment()?;
        let mut replaced = Vec::new();
        for service in services {
            let containers = crate::adapters::runtime::project_services::service_containers(
                self,
                &target.project_name,
                service,
            )
            .await?;
            replaced.extend(containers.into_iter().skip(1).map(|container| container.id));
        }
        Ok(replaced)
    }
}
