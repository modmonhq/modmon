#[async_trait]
impl LocalDeploymentRuntime for PodmanComposeRuntime {
    async fn preflight(
        &self,
        target: &NamedTarget,
        resources: &[RuntimeResource],
        projects: &[RuntimeTarget],
    ) -> Result<(), MmError> {
        for resource in resources {
            match resource.kind {
                ResourceKind::Network => {
                    self.inspect_network(target, &resource.name, &resource.resource)
                        .await?;
                }
                ResourceKind::Volume => {
                    let target_id = require_local_named_target(target)?;
                    self.require_local_engine_environment()?;
                    validate_resource_name(&resource.name)?;
                    validate_resource_label(&resource.resource)?;
                    let exists = self
                        .local_command(
                            vec![
                                "volume".to_owned(),
                                "exists".to_owned(),
                                resource.name.clone(),
                            ],
                            Duration::from_secs(30),
                        )
                        .await?;
                    match exists.code {
                        Some(0) => {
                            self.inspect_owned_resource(
                                PodmanResourceKind::Volume,
                                &resource.name,
                                target_id,
                                &resource.resource,
                            )
                            .await?;
                        }
                        Some(1) => {}
                        _ => {
                            self.ensure_success(exists)?;
                        }
                    }
                }
                _ => {
                    return Err(MmError::Security {
                        message:
                            "deployment preflight received an unsupported Podman resource kind"
                                .to_owned(),
                    });
                }
            }
        }
        for project in projects {
            self.preflight_project(project).await?;
        }
        Ok(())
    }
    async fn ensure_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        self.ensure_resource(target, name, resource, PodmanResourceKind::Network, &[])
            .await
    }

    async fn ensure_internal_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        // Takes effect when the network is created; an existing network
        // keeps the setting it was created with.
        self.ensure_resource(
            target,
            name,
            resource,
            PodmanResourceKind::Network,
            &["--internal"],
        )
        .await
    }

    async fn ensure_cluster_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
        network: &crate::domain::cluster_network::LocalNetwork,
    ) -> Result<ResourceEnsureResult, MmError> {
        self.ensured_podman_cluster_network(target, name, resource, network)
            .await
    }

    async fn inspect_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<bool, MmError> {
        let target_id = require_local_named_target(target)?;
        self.require_local_engine_environment()?;
        validate_resource_name(name)?;
        validate_resource_label(resource)?;
        let exists = self
            .local_command(
                vec!["network".to_owned(), "exists".to_owned(), name.to_owned()],
                Duration::from_secs(30),
            )
            .await?;
        match exists.code {
            Some(0) => {
                self.inspect_owned_resource(PodmanResourceKind::Network, name, target_id, resource)
                    .await?;
                Ok(true)
            }
            Some(1) => Ok(false),
            _ => {
                self.ensure_success(exists)?;
                Ok(false)
            }
        }
    }

    async fn remove_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<(), MmError> {
        if !self.inspect_network(target, name, resource).await? {
            return Ok(());
        }
        let output = self
            .local_command(
                vec!["network".to_owned(), "rm".to_owned(), name.to_owned()],
                Duration::from_secs(30),
            )
            .await?;
        self.ensure_success(output)?;
        Ok(())
    }

    async fn ensure_volume(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        self.ensure_resource(target, name, resource, PodmanResourceKind::Volume, &[])
            .await
    }

    async fn remove_volume(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<(), MmError> {
        let target_id = require_local_named_target(target)?;
        self.require_local_engine_environment()?;
        validate_resource_name(name)?;
        validate_resource_label(resource)?;
        let exists = self
            .local_command(
                vec!["volume".to_owned(), "exists".to_owned(), name.to_owned()],
                Duration::from_secs(30),
            )
            .await?;
        match exists.code {
            Some(1) => Ok(()),
            Some(0) => {
                self.inspect_owned_resource(PodmanResourceKind::Volume, name, target_id, resource)
                    .await?;
                let output = self
                    .local_command(
                        vec!["volume".to_owned(), "rm".to_owned(), name.to_owned()],
                        Duration::from_secs(30),
                    )
                    .await?;
                self.ensure_success(output)?;
                Ok(())
            }
            _ => {
                self.ensure_success(exists)?;
                Err(MmError::RuntimeResponse {
                    message: format!(
                        "Podman returned an invalid result while checking volume '{name}'"
                    ),
                })
            }
        }
    }

    async fn set_volume_owner(
        &self,
        target: &NamedTarget,
        volume_name: &str,
        volume_resource: &str,
        ownership: VolumeOwnership,
    ) -> Result<(), MmError> {
        let target_id = require_local_named_target(target)?;
        self.require_local_engine_environment()?;
        validate_resource_name(volume_name)?;
        validate_resource_label(volume_resource)?;
        let mountpoint = self
            .volume_mountpoint(target_id, volume_name, volume_resource)
            .await?;
        let path = path_argument(&mountpoint, "volume mountpoint")?;
        self.unshare(vec![
            "chown".to_owned(),
            format!("{}:{}", ownership.uid, ownership.gid),
            "--".to_owned(),
            path.clone(),
        ])
        .await?;
        self.unshare(vec![
            "chmod".to_owned(),
            unix_mode(ownership.mode)?,
            "--".to_owned(),
            path,
        ])
        .await
    }

    async fn create_volume_file_if_absent(
        &self,
        target: &NamedTarget,
        volume_name: &str,
        volume_resource: &str,
        file_name: &str,
        mode: u32,
    ) -> Result<(), MmError> {
        let target_id = require_local_named_target(target)?;
        self.require_local_engine_environment()?;
        validate_resource_name(volume_name)?;
        validate_resource_label(volume_resource)?;
        let relative_file = validate_volume_file_name(file_name)?;
        let mode = unix_mode(mode)?;
        let mountpoint = self
            .volume_mountpoint(target_id, volume_name, volume_resource)
            .await?;
        let destination = mountpoint.join(relative_file);
        let parent = destination
            .parent()
            .ok_or_else(|| MmError::RuntimeResponse {
                message: "Podman volume destination has no parent directory".to_owned(),
            })?;
        let temporary = self
            .stage_volume_file(parent, Path::new("/dev/null"), &mode)
            .await?;
        let move_result = self
            .unshare(vec![
                "mv".to_owned(),
                "--no-clobber".to_owned(),
                "--no-target-directory".to_owned(),
                "--".to_owned(),
                path_argument(&temporary, "temporary volume file")?,
                path_argument(&destination, "volume file")?,
            ])
            .await;
        let cleanup_result = self.cleanup_volume_file(&temporary).await;
        if let Err(error) = move_result {
            return Err(volume_cleanup_error(error, cleanup_result));
        }
        cleanup_result?;
        self.sync_volume_directory(parent).await
    }

    async fn replace_volume_file(
        &self,
        target: &NamedTarget,
        volume_name: &str,
        volume_resource: &str,
        file_name: &str,
        source: &Path,
        mode: u32,
    ) -> Result<(), MmError> {
        let target_id = require_local_named_target(target)?;
        self.require_local_engine_environment()?;
        validate_resource_name(volume_name)?;
        validate_resource_label(volume_resource)?;
        let relative_file = validate_volume_file_name(file_name)?;
        let mode = unix_mode(mode)?;
        let source = verified_local_source(source)?;
        let mountpoint = self
            .volume_mountpoint(target_id, volume_name, volume_resource)
            .await?;
        let destination = mountpoint.join(relative_file);
        let parent = destination
            .parent()
            .ok_or_else(|| MmError::RuntimeResponse {
                message: "Podman volume destination has no parent directory".to_owned(),
            })?;
        let temporary = self.stage_volume_file(parent, &source, &mode).await?;
        let move_result = self
            .unshare(vec![
                "mv".to_owned(),
                "--force".to_owned(),
                "--no-target-directory".to_owned(),
                "--".to_owned(),
                path_argument(&temporary, "temporary volume file")?,
                path_argument(&destination, "volume file")?,
            ])
            .await;
        if let Err(error) = move_result {
            return Err(volume_cleanup_error(
                error,
                self.cleanup_volume_file(&temporary).await,
            ));
        }
        self.sync_volume_directory(parent).await
    }

    async fn replace_volume_file_owned(
        &self,
        target: &NamedTarget,
        volume_name: &str,
        volume_resource: &str,
        file: OwnedVolumeFile<'_>,
        source: &Path,
    ) -> Result<(), MmError> {
        let target_id = require_local_named_target(target)?;
        self.require_local_engine_environment()?;
        validate_resource_name(volume_name)?;
        validate_resource_label(volume_resource)?;
        let relative_file = validate_volume_file_name(file.name)?;
        let mode = unix_mode(file.ownership.mode)?;
        let source = verified_local_source(source)?;
        let mountpoint = self
            .volume_mountpoint(target_id, volume_name, volume_resource)
            .await?;
        let destination = mountpoint.join(relative_file);
        let parent = destination
            .parent()
            .ok_or_else(|| MmError::RuntimeResponse {
                message: "Podman volume destination has no parent directory".to_owned(),
            })?;
        let temporary = self.stage_volume_file(parent, &source, &mode).await?;
        let chown_result = self
            .unshare(vec![
                "chown".to_owned(),
                format!("{}:{}", file.ownership.uid, file.ownership.gid),
                "--".to_owned(),
                path_argument(&temporary, "temporary volume file")?,
            ])
            .await;
        if let Err(error) = chown_result {
            return Err(volume_cleanup_error(
                error,
                self.cleanup_volume_file(&temporary).await,
            ));
        }
        let move_result = self
            .unshare(vec![
                "mv".to_owned(),
                "--force".to_owned(),
                "--no-target-directory".to_owned(),
                "--".to_owned(),
                path_argument(&temporary, "temporary volume file")?,
                path_argument(&destination, "volume file")?,
            ])
            .await;
        if let Err(error) = move_result {
            return Err(volume_cleanup_error(
                error,
                self.cleanup_volume_file(&temporary).await,
            ));
        }
        self.sync_volume_directory(parent).await
    }

    async fn remove_volume_file(
        &self,
        target: &NamedTarget,
        volume_name: &str,
        volume_resource: &str,
        file_name: &str,
    ) -> Result<(), MmError> {
        let target_id = require_local_named_target(target)?;
        self.require_local_engine_environment()?;
        validate_resource_name(volume_name)?;
        validate_resource_label(volume_resource)?;
        let relative_file = validate_volume_file_name(file_name)?;
        let mountpoint = self
            .volume_mountpoint(target_id, volume_name, volume_resource)
            .await?;
        let destination = mountpoint.join(relative_file);
        self.unshare(vec![
            "rm".to_owned(),
            "--force".to_owned(),
            "--".to_owned(),
            path_argument(&destination, "volume file")?,
        ])
        .await
    }

    async fn apply(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        self.up(target, true, &[], &BTreeMap::new()).await
    }

    async fn apply_keeping_other_services(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        self.up(target, false, &[], &BTreeMap::new()).await
    }

    async fn apply_services(
        &self,
        target: &RuntimeTarget,
        services: &[String],
        counts: &BTreeMap<String, u32>,
    ) -> Result<(), MmError> {
        self.up(target, false, services, counts).await
    }

    async fn apply_scaled(
        &self,
        target: &RuntimeTarget,
        counts: &BTreeMap<String, u32>,
        keep_others: bool,
    ) -> Result<(), MmError> {
        self.up(target, !keep_others, &[], counts).await
    }

    async fn scale_service(
        &self,
        target: &RuntimeTarget,
        service: &str,
        count: u32,
    ) -> Result<(), MmError> {
        self.start_more(target, service, count).await
    }

    async fn pull_images(
        &self,
        target: &RuntimeTarget,
        services: Option<&[String]>,
        limit: Duration,
    ) -> Result<(), MmError> {
        require_local_target(target)?;
        self.require_local_engine_environment()?;
        let base = |args: Vec<String>| self.command_spec(args);
        crate::adapters::runtime::image_pull::ImagePuller {
            process: self.process.as_ref(),
            kind: crate::domain::runtime::RuntimeKind::PodmanCompose,
            program: &self.executable,
            base: &base,
        }
        .pull(target, self.compose_args(target)?, services, limit)
        .await
    }

    async fn service_container_ids(
        &self,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        require_local_target(target)?;
        self.require_local_engine_environment()?;
        crate::adapters::runtime::project_services::service_container_ids(
            self,
            &target.project_name,
            services,
        )
        .await
    }

    async fn remove_services(
        &self,
        target: &RuntimeTarget,
        services: &[String],
        whole_project: bool,
    ) -> Result<(), MmError> {
        require_local_target(target)?;
        self.require_local_engine_environment()?;
        self.preflight_project(target).await?;
        crate::adapters::runtime::project_services::remove_services(
            self,
            &target.project_name,
            services,
            whole_project,
            project_limit(target, STOP_FLOOR),
        )
        .await
    }

    async fn start_beside(
        &self,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        self.start_beside_old(target, services).await
    }

    async fn replaced_containers(
        &self,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        self.replaced_by_newer(target, services).await
    }

    async fn outdated_containers(
        &self,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        self.outdated(target, services).await
    }

    async fn remove_containers(
        &self,
        target: &RuntimeTarget,
        containers: &[String],
    ) -> Result<(), MmError> {
        require_local_target(target)?;
        self.require_local_engine_environment()?;
        crate::adapters::runtime::project_services::remove_containers(
            self,
            containers,
            project_limit(target, STOP_FLOOR),
        )
        .await
    }

    async fn remove(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        require_local_target(target)?;
        self.require_local_engine_environment()?;
        self.preflight_project(target).await?;
        let mut args = self.compose_args(target)?;
        args.extend(["down".to_owned(), "--remove-orphans".to_owned()]);
        let output = self
            .process
            .run(
                self.command_spec(args)
                    .current_dir(&target.root)
                    .timeout(project_limit(target, COMPOSE_FLOOR)),
            )
            .await?;
        self.ensure_success(output)?;
        Ok(())
    }
}

impl PodmanComposeRuntime {
    /// `compose up` for the target's project (only `only` and none of what
    /// they depend on, when it names any), with `counts[service]`
    /// containers of each service `counts` names. Without
    /// `remove_orphans`, containers of services the files no longer list
    /// keep running. A count is Compose's `--scale`, which the Docker
    /// Compose provider of a cluster node (talking to Podman's socket)
    /// honours.
    async fn up(
        &self,
        target: &RuntimeTarget,
        remove_orphans: bool,
        only: &[String],
        counts: &BTreeMap<String, u32>,
    ) -> Result<(), MmError> {
        require_local_target(target)?;
        self.require_local_engine_environment()?;
        self.preflight_project(target).await?;
        let auth = crate::adapters::runtime::registry_auth::RegistryAuth::prepare(
            &target.registry_credentials,
        )?;
        let mut args = self.compose_args(target)?;
        args.extend(["up".to_owned(), "-d".to_owned()]);
        if remove_orphans {
            args.push("--remove-orphans".to_owned());
        }
        // Compose refuses a count for a service it was not asked to start.
        for (service, count) in counts
            .iter()
            .filter(|(service, _)| only.is_empty() || only.contains(service))
        {
            args.extend(["--scale".to_owned(), format!("{service}={count}")]);
        }
        args.push("--no-build".to_owned());
        if !only.is_empty() {
            args.push("--no-deps".to_owned());
            args.extend(only.iter().cloned());
        }
        let output = self
            .process
            .run(
                auth.podman(self.command_spec(args))
                    .interpolation_environment(&target.interpolation_environment)
                    .current_dir(&target.root)
                    .timeout(project_limit(target, COMPOSE_FLOOR)),
            )
            .await?;
        self.ensure_started(output, target, only)
    }
}

#[async_trait]
impl crate::adapters::runtime::project_services::ContainerEngine for PodmanComposeRuntime {
    fn engine_name(&self) -> &'static str {
        "Podman"
    }

    async fn engine(&self, args: Vec<String>, timeout: Duration) -> Result<CommandOutput, MmError> {
        let output = self.local_command(args, timeout).await?;
        self.ensure_success(output)
    }
}

fn volume_cleanup_error(primary: MmError, cleanup: Result<(), MmError>) -> MmError {
    match cleanup {
        Ok(()) => primary,
        Err(cleanup) => MmError::RuntimeResponse {
            message: format!(
                "{primary}; temporary Podman volume file cleanup also failed: {cleanup}"
            ),
        },
    }
}
