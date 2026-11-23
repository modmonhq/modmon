#[async_trait]
impl RuntimeDriver for DockerComposeRuntime {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::DockerCompose
    }

    async fn probe(&self) -> RuntimeProbe {
        // The three questions do not depend on each other: ask them at once.
        let (version, info, compose) = tokio::join!(
            self.command(vec!["--version".to_owned()]),
            self.command(vec![
                "info".to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
            ]),
            self.command(vec!["compose".to_owned(), "version".to_owned()]),
        );
        let version = match version {
            Ok(output) if output.success() => output,
            Ok(output) => {
                return RuntimeProbe::unavailable(
                    self.kind(),
                    Diagnostic::error(
                        "MM_DOCKER_UNAVAILABLE",
                        format!(
                            "Docker returned exit code {:?}: {}",
                            output.code,
                            crate::domain::error::printed(output.stderr.trim())
                        ),
                    ),
                );
            }
            Err(error) => {
                return RuntimeProbe::unavailable(
                    self.kind(),
                    probe_failure(&error, "MM_DOCKER_UNAVAILABLE")
                        .with_help("install Docker and ensure it is available on PATH"),
                );
            }
        };
        let mut diagnostics = Vec::new();
        let info = match info {
            Ok(output) if output.success() => Some(output),
            Ok(output) => {
                diagnostics.push(Diagnostic::error(
                    "MM_DOCKER_INFO_FAILED",
                    format!("Docker info failed with exit code {:?}", output.code),
                ));
                None
            }
            Err(error) => {
                diagnostics.push(probe_failure(&error, "MM_DOCKER_INFO_FAILED"));
                None
            }
        };
        let compose = match compose {
            Ok(output) if output.success() => Some(output),
            Ok(output) => {
                diagnostics.push(Diagnostic::error(
                    "MM_DOCKER_COMPOSE_UNAVAILABLE",
                    format!("Docker Compose failed with exit code {:?}", output.code),
                ));
                None
            }
            Err(error) => {
                diagnostics.push(probe_failure(&error, "MM_DOCKER_COMPOSE_UNAVAILABLE"));
                None
            }
        };
        let available = info.is_some() && compose.is_some();
        let rootless = info
            .as_ref()
            .and_then(|output| docker_rootless(output, &mut diagnostics));
        diagnostics.extend(info.as_ref().and_then(docker_userns_remap));
        RuntimeProbe {
            kind: self.kind(),
            available,
            engine_version: version
                .stdout
                .lines()
                .next()
                .map(|line| line.trim().to_owned()),
            compose_provider: compose.map(|output| crate::domain::runtime::ComposeProvider {
                name: "docker-compose".to_owned(),
                version: output
                    .stdout
                    .lines()
                    .next()
                    .map(|line| line.trim().to_owned()),
                executable: None,
            }),
            rootless,
            socket_path: None,
            capabilities: if available {
                runtime_descriptor(self.kind()).advertised_capabilities()
            } else {
                Vec::new()
            },
            diagnostics,
        }
    }

    async fn status_probe(&self) -> RuntimeProbe {
        let Some(engine) = &self.engine else {
            return self.probe().await;
        };
        // A status reads containers and nothing else: whether the engine
        // answers is all it needs, and one request on its socket says so.
        match crate::adapters::runtime::engine_containers::engine_version(engine).await {
            Ok(version) => RuntimeProbe {
                kind: self.kind(),
                available: true,
                engine_version: Some(version),
                compose_provider: None,
                rootless: None,
                socket_path: Some(engine.socket().to_path_buf()),
                capabilities: runtime_descriptor(self.kind()).advertised_capabilities(),
                diagnostics: Vec::new(),
            },
            Err(error) => RuntimeProbe::unavailable(
                self.kind(),
                Diagnostic::error("MM_DOCKER_UNAVAILABLE", error.to_string())
                    .with_help("check that the Docker engine is running: `systemctl status docker`"),
            ),
        }
    }

    async fn observe_unrendered(
        &self,
        target: &RuntimeTarget,
        expected_services: &[String],
    ) -> Option<Result<crate::domain::runtime::ObservedProject, MmError>> {
        let engine = self.engine.as_ref()?;
        Some(
            crate::adapters::runtime::engine_containers::project_containers(
                engine,
                &target.project_name,
                None,
            )
            .await
            .map(|containers| {
                let containers = containers
                    .into_iter()
                    .map(|container| container.observation)
                    .collect::<Vec<_>>();
                crate::domain::runtime::ObservedProject {
                    stack: target.stack_name.clone(),
                    services: normalize_observation(target, expected_services, &containers),
                    containers,
                }
            }),
        )
    }

    async fn render(&self, target: &RuntimeTarget) -> Result<RenderedProject, MmError> {
        require_local_target(target)?;
        let mut args = self.compose_args(target)?;
        args.push("config".to_owned());
        let output = self
            .process
            .run(
                self.command_spec(args)
                    .interpolation_environment(&target.interpolation_environment)
                    .current_dir(&target.root)
                    .timeout(Duration::from_secs(45))
                    // Parsed here and never shown; redaction would corrupt it.
                    .internal_stdout(),
            )
            .await?;
        let output = self.ensure_success(output)?;
        if output.truncated {
            return Err(MmError::RuntimeResponse {
                message: "Docker Compose config output was truncated".to_owned(),
            });
        }
        let service_names = parse_compose_service_names(&output.stdout)?;
        let semantics = normalized_rendered_semantics(&target.project_name, &output.stdout)?;
        Ok(RenderedProject {
            stack: target.stack_name.clone(),
            project_name: target.project_name.clone(),
            service_names,
            services: semantics.services,
            resources: semantics.resources,
            config_hash: semantics.config_hash,
        })
    }

    async fn observe(
        &self,
        target: &RuntimeTarget,
    ) -> Result<crate::domain::runtime::ObservedProject, MmError> {
        Ok(self.render_and_observe(target).await?.1)
    }

    async fn render_and_observe(
        &self,
        target: &RuntimeTarget,
    ) -> Result<(RenderedProject, crate::domain::runtime::ObservedProject), MmError> {
        require_local_target(target)?;
        // The containers are listed while the project renders: the names
        // the rendering gives are only needed to put the two together.
        let (rendered, containers) = tokio::join!(self.render(target), self.containers(target));
        let rendered = rendered?;
        let containers = containers?;
        let observed = crate::domain::runtime::ObservedProject {
            stack: target.stack_name.clone(),
            services: normalize_observation(target, &rendered.service_names, &containers),
            containers,
        };
        Ok((rendered, observed))
    }

    async fn wait_for_convergence(
        &self,
        target: &RuntimeTarget,
        policy: ConvergencePolicy,
        cancellation: ConvergenceCancellation,
    ) -> Result<ConvergenceOutcome, MmError> {
        crate::adapters::runtime::convergence::wait_for_project(
            self,
            target,
            policy,
            cancellation,
            None,
        )
        .await
    }

    async fn wait_for_services(
        &self,
        target: &RuntimeTarget,
        services: &[String],
        policy: ConvergencePolicy,
        cancellation: ConvergenceCancellation,
    ) -> Result<ConvergenceOutcome, MmError> {
        crate::adapters::runtime::convergence::wait_for_project(
            self,
            target,
            policy,
            cancellation,
            Some(services),
        )
        .await
    }

    async fn container_log_tail(
        &self,
        container: &str,
        lines: usize,
    ) -> Result<Vec<String>, MmError> {
        crate::adapters::runtime::project_services::log_tail(self, container, lines).await
    }

    async fn observe_cluster_network(
        &self,
        name: &str,
        wanted: Option<&crate::domain::cluster_network::LocalNetwork>,
    ) -> Result<Option<crate::domain::cluster_network::ObservedNetwork>, MmError> {
        self.observed_cluster_network(name, wanted).await
    }

    async fn observe_cluster_networks(
        &self,
        wanted: &[crate::domain::cluster_network::LocalNetwork],
    ) -> Result<Vec<Option<crate::domain::cluster_network::ObservedNetwork>>, MmError> {
        self.observed_cluster_networks(wanted).await
    }

    async fn project_services(
        &self,
        project: &str,
    ) -> Result<std::collections::BTreeSet<String>, MmError> {
        crate::adapters::runtime::project_services::project_services(self, project).await
    }

    async fn network_containers(
        &self,
        networks: &[String],
    ) -> Result<Vec<crate::domain::discovery::ObservedContainer>, MmError> {
        crate::adapters::runtime::endpoints::network_containers(
            self,
            self.engine.as_ref(),
            networks,
        )
        .await
    }

    async fn watch_containers(&self, changed: &(dyn Fn() + Send + Sync)) -> Result<(), MmError> {
        crate::adapters::runtime::endpoints::watch_changes(
            "docker",
            crate::domain::runtime::DOCKER_INHERITED_ENVIRONMENT,
            changed,
        )
        .await
    }

    async fn waiting_containers(
        &self,
        label: &str,
    ) -> Result<Vec<crate::domain::shared_storage::WaitingContainer>, MmError> {
        crate::adapters::runtime::waiting::waiting_containers(self, self.engine.as_ref(), label)
            .await
    }

    async fn start_container(&self, id: &str) -> Result<(), MmError> {
        crate::adapters::runtime::waiting::start_container(self, id).await
    }

    async fn container_usage(
        &self,
        containers: &[String],
    ) -> Result<Vec<crate::domain::metrics::ContainerSample>, MmError> {
        crate::adapters::runtime::usage::container_usage(self, containers).await
    }

    async fn check_oci_runtime(&self, name: &str) -> Result<(), String> {
        crate::adapters::runtime::oci_runtime::check_oci_runtime(self, name).await
    }
}

#[async_trait]
impl crate::adapters::runtime::project_services::ContainerEngine for DockerComposeRuntime {
    fn engine_name(&self) -> &'static str {
        "Docker"
    }

    async fn engine(&self, args: Vec<String>, timeout: Duration) -> Result<CommandOutput, MmError> {
        let output = self
            .process
            .run(self.command_spec(args).timeout(timeout))
            .await?;
        self.ensure_success(output)
    }
}

/// A probe's engine command that failed: a timeout keeps its own code, so
/// it is reported as one, not as a missing capability.
fn probe_failure(error: &MmError, otherwise: &'static str) -> Diagnostic {
    Diagnostic::error(
        crate::domain::runtime::probe_failure_code(error, otherwise),
        error.to_string(),
    )
}

fn docker_rootless(output: &CommandOutput, diagnostics: &mut Vec<Diagnostic>) -> Option<bool> {
    match serde_json::from_str::<serde_json::Value>(&output.stdout) {
        Ok(value) => value
            .get("SecurityOptions")
            .and_then(serde_json::Value::as_array)
            .map(|options| {
                options.iter().any(|option| {
                    option
                        .as_str()
                        .is_some_and(|option| option.contains("rootless"))
                })
            }),
        Err(error) => {
            diagnostics.push(Diagnostic::error(
                "MM_DOCKER_INFO_INVALID",
                format!("Docker info returned invalid JSON: {error}"),
            ));
            None
        }
    }
}

fn docker_userns_remap(output: &CommandOutput) -> Option<Diagnostic> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&output.stdout) else {
        return None;
    };
    let remapped = value
        .get("SecurityOptions")?
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .any(|option| option.split(',').any(|part| part == "name=userns"));
    remapped.then(|| {
        Diagnostic::info(
            crate::domain::runtime::DOCKER_USERNS_REMAP,
            "Docker runs with userns-remap: containers do not have the host's user ids, so a secret file can only be given to root (uid 0, gid 0) with a mode every user can read",
        )
    })
}
