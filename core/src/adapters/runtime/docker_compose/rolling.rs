// Replacing a service's container without a gap: the new container starts
// next to the old one, and the old one is removed once the new one is
// healthy and known to the other nodes.

impl DockerComposeRuntime {
    /// Compose's hash of each service's configuration in the target's
    /// files; it creates a new container when a container's hash differs.
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

    async fn start_beside_old(
        &self,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        use crate::adapters::runtime::project_services::service_containers;

        require_local_target(target)?;
        self.preflight_project(target).await?;
        let hashes = self.config_hashes(target).await?;
        let mut started = Vec::new();
        for service in services {
            let Some(wanted) = hashes.get(service) else {
                continue;
            };
            let containers = service_containers(self, &target.project_name, service).await?;
            let running = containers.iter().filter(|container| container.running).count();
            let current = |container: &crate::adapters::runtime::project_services::ServiceContainer| {
                container.config_hash.as_ref() == Some(wanted)
            };
            // Nothing runs, or what runs is what the files say: the
            // ordinary apply has nothing to replace.
            if running == 0 || containers.iter().all(current) {
                continue;
            }
            // A start that was interrupted already left the new container.
            if containers.iter().any(current) {
                started.push(service.clone());
                continue;
            }
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
                format!("{service}={}", containers.len() + 1),
                service.clone(),
            ]);
            let output = self
                .process
                .run(
                    auth.docker(self.command_spec(args))
                        .interpolation_environment(&target.interpolation_environment)
                        .current_dir(&target.root)
                        .timeout(Duration::from_secs(300)),
                )
                .await?;
            self.ensure_success(output)?;
            started.push(service.clone());
        }
        Ok(started)
    }

    /// Has Compose count `count` containers of `service`: it starts the
    /// ones it misses and touches none it has.
    async fn start_more(
        &self,
        target: &RuntimeTarget,
        service: &str,
        count: u32,
    ) -> Result<(), MmError> {
        require_local_target(target)?;
        self.preflight_project(target).await?;
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
                auth.docker(self.command_spec(args))
                    .interpolation_environment(&target.interpolation_environment)
                    .current_dir(&target.root)
                    .timeout(Duration::from_secs(300)),
            )
            .await?;
        self.ensure_success(output)?;
        Ok(())
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

    async fn replaced_by_newer(
        &self,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        require_local_target(target)?;
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
