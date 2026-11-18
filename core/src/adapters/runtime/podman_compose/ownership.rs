#[derive(Debug, Clone, Copy)]
enum PodmanResourceKind {
    Network,
    Volume,
}

impl PodmanResourceKind {
    const fn command_name(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Volume => "volume",
        }
    }

    const fn ownership_kind(self) -> ResourceKind {
        match self {
            Self::Network => ResourceKind::Network,
            Self::Volume => ResourceKind::Volume,
        }
    }
}

impl PodmanComposeRuntime {
    async fn local_command(
        &self,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<CommandOutput, MmError> {
        self.require_local_engine_environment()?;
        self.process
            .run(self.command_spec(args).timeout(timeout))
            .await
    }

    async fn preflight_project(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        let ownership = ResourceOwnership::for_runtime_target(
            target,
            ResourceKind::Stack,
            target.project_name.clone(),
            target.stack_name.clone(),
        )
        .ok_or_else(|| MmError::Security {
            message: "runtime project has no stable target identity".to_owned(),
        })?;
        let output = self
            .local_command(
                vec![
                    "ps".to_owned(),
                    "--all".to_owned(),
                    "--filter".to_owned(),
                    format!("label=com.docker.compose.project={}", target.project_name),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
                Duration::from_secs(30),
            )
            .await?;
        let output = self.ensure_success(output)?;
        for id in parse_resource_ids(&output, "Podman", "container")? {
            let inspected = self
                .local_command(
                    vec![
                        "container".to_owned(),
                        "inspect".to_owned(),
                        "--format".to_owned(),
                        "json".to_owned(),
                        id,
                    ],
                    Duration::from_secs(30),
                )
                .await?;
            require_inspected_project_ownership(
                self.ensure_success(inspected)?,
                &ownership,
                "Podman",
            )?;
        }
        Ok(())
    }

    async fn ensure_resource(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
        kind: PodmanResourceKind,
        options: &[&str],
    ) -> Result<ResourceEnsureResult, MmError> {
        let target_id = require_local_named_target(target)?;
        self.require_local_engine_environment()?;
        validate_resource_name(name)?;
        validate_resource_label(resource)?;
        let ownership = ResourceOwnership {
            project_id: None,
            environment: None,
            target_id,
            kind: kind.ownership_kind(),
            logical_name: name.to_owned(),
            resource: resource.to_owned(),
            generation: crate::domain::ownership::OWNERSHIP_GENERATION,
        };

        let exists = self
            .local_command(
                vec![
                    kind.command_name().to_owned(),
                    "exists".to_owned(),
                    name.to_owned(),
                ],
                Duration::from_secs(30),
            )
            .await?;
        match exists.code {
            Some(0) => {
                self.inspect_owned_resource(kind, name, target_id, resource)
                    .await?;
                Ok(ResourceEnsureResult::Unchanged)
            }
            Some(1) => {
                let mut create = vec![kind.command_name().to_owned(), "create".to_owned()];
                create.extend(options.iter().map(|option| (*option).to_owned()));
                create.extend(ownership_label_args(&ownership));
                create.push(name.to_owned());
                let output = self.local_command(create, Duration::from_secs(30)).await?;
                self.ensure_success(output)?;
                Ok(ResourceEnsureResult::Created)
            }
            _ => {
                self.ensure_success(exists)?;
                Err(MmError::RuntimeResponse {
                    message: format!(
                        "Podman returned an invalid result while checking {} '{name}'",
                        kind.command_name()
                    ),
                })
            }
        }
    }

    async fn inspect_owned_resource(
        &self,
        kind: PodmanResourceKind,
        name: &str,
        target_id: TargetId,
        resource: &str,
    ) -> Result<Value, MmError> {
        let output = self
            .local_command(
                vec![
                    kind.command_name().to_owned(),
                    "inspect".to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                    name.to_owned(),
                ],
                Duration::from_secs(30),
            )
            .await?;
        let output = self.ensure_success(output)?;
        parse_owned_resource(output, kind, name, target_id, resource)
    }

    async fn volume_mountpoint(
        &self,
        target_id: TargetId,
        volume_name: &str,
        volume_resource: &str,
    ) -> Result<PathBuf, MmError> {
        let inspected = self
            .inspect_owned_resource(
                PodmanResourceKind::Volume,
                volume_name,
                target_id,
                volume_resource,
            )
            .await?;
        let mountpoint =
            string_for_keys(&inspected, &["Mountpoint", "mountpoint"]).ok_or_else(|| {
                MmError::RuntimeResponse {
                    message: format!(
                        "Podman volume inspect omitted the mountpoint for '{volume_name}'"
                    ),
                }
            })?;
        validate_mountpoint(&mountpoint, volume_name)
    }

    /// Runs a file command on a volume as the engine's owner: inside the
    /// user's namespace for rootless Podman (`podman unshare`, which root
    /// may not use), as root itself for rootful Podman.
    async fn unshare(&self, args: Vec<String>) -> Result<(), MmError> {
        let output = if (self.root)() {
            self.require_local_engine_environment()?;
            let Some((program, arguments)) = args.split_first() else {
                return Ok(());
            };
            self.process
                .run(
                    CommandSpec::new(program.as_str())
                        .args(arguments.iter().cloned())
                        .timeout(Duration::from_secs(30)),
                )
                .await?
        } else {
            self.local_command(
                std::iter::once("unshare".to_owned()).chain(args).collect(),
                Duration::from_secs(30),
            )
            .await?
        };
        self.ensure_success(output)?;
        Ok(())
    }

    async fn stage_volume_file(
        &self,
        parent: &Path,
        source: &Path,
        mode: &str,
    ) -> Result<PathBuf, MmError> {
        let parent = path_argument(parent, "volume directory")?;
        let source = path_argument(source, "volume-file source")?;
        let temporary = Path::new(&parent).join(format!(".mm-{}.tmp", uuid::Uuid::new_v4()));
        let temporary_argument = path_argument(&temporary, "temporary volume file")?;

        self.unshare(vec![
            "mkdir".to_owned(),
            "--parents".to_owned(),
            "--".to_owned(),
            parent,
        ])
        .await?;
        self.unshare(vec![
            "install".to_owned(),
            "--mode".to_owned(),
            mode.to_owned(),
            "--no-target-directory".to_owned(),
            "--".to_owned(),
            source,
            temporary_argument.clone(),
        ])
        .await?;
        self.unshare(vec![
            "sync".to_owned(),
            "--file-system".to_owned(),
            "--".to_owned(),
            temporary_argument,
        ])
        .await?;
        Ok(temporary)
    }

    async fn cleanup_volume_file(&self, temporary: &Path) -> Result<(), MmError> {
        self.unshare(vec![
            "rm".to_owned(),
            "--force".to_owned(),
            "--".to_owned(),
            path_argument(temporary, "temporary volume file")?,
        ])
        .await
    }

    async fn sync_volume_directory(&self, parent: &Path) -> Result<(), MmError> {
        self.unshare(vec![
            "sync".to_owned(),
            "--file-system".to_owned(),
            "--".to_owned(),
            path_argument(parent, "volume directory")?,
        ])
        .await
    }

    fn compose_args(&self, target: &RuntimeTarget) -> Result<Vec<String>, MmError> {
        if target.compose_files.is_empty() {
            return Err(MmError::Security {
                message: "refusing to mutate a Compose project without explicit Compose files"
                    .to_owned(),
            });
        }
        let mut args = vec!["compose".to_owned()];
        for compose_file in &target.compose_files {
            args.push("--file".to_owned());
            args.push(path_argument(compose_file, "Compose file")?);
        }
        args.extend([
            "--project-directory".to_owned(),
            path_argument(&target.project_directory, "Compose project directory")?,
            "--project-name".to_owned(),
            target.project_name.clone(),
        ]);
        Ok(args)
    }
}
