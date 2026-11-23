use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::adapters::private_fs::ensure_no_symlink_components;
use crate::adapters::runtime::compose_limit::project_limit;
use crate::adapters::runtime::inspect::{
    ownership_label_args, parse_resource_ids, require_inspected_project_ownership,
};
use crate::adapters::runtime::launch_failure::{Launched, launch_error};
use crate::adapters::runtime::observe::{
    normalize_observation, normalized_rendered_semantics,
    parse_compose_observation, parse_compose_service_names,
};
use crate::app::ports::{
    CommandOutput, CommandSpec, LocalDeploymentRuntime, OwnedVolumeFile, ProcessRunner,
    RuntimeDriver, VolumeOwnership,
};
use crate::domain::compose_timing::COMPOSE_FLOOR;
use crate::domain::diagnostic::Diagnostic;
use crate::domain::error::MmError;
use crate::domain::ownership::{ResourceKind, ResourceOwnership};
use crate::domain::runtime::{
    ComposeProvider, ContainerObservation, ConvergenceCancellation, ConvergenceOutcome,
    ConvergencePolicy, ObservedProject, PODMAN_INHERITED_ENVIRONMENT,
    RenderedProject, ResourceEnsureResult, RuntimeCapability, RuntimeKind, RuntimeProbe,
    RuntimeResource, RuntimeTarget, runtime_descriptor,
};
use crate::domain::target::{NamedTarget, TargetId, TargetTransport};

const STOP_FLOOR: Duration = Duration::from_secs(300);

const ENGINE_REDIRECT_ENVIRONMENT: &[&str] = &[
    "CONTAINER_HOST",
    "CONTAINER_CONNECTION",
    "DOCKER_HOST",
    "DOCKER_CONTEXT",
];

fn ambient_environment(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

fn process_is_root() -> bool {
    rustix::process::geteuid().is_root()
}

#[derive(Clone)]
pub struct PodmanComposeRuntime {
    process: Arc<dyn ProcessRunner>,
    executable: String,
    environment: fn(&str) -> Option<OsString>,
    /// Whether this process runs as root, and so drives rootful Podman.
    root: fn() -> bool,
    /// The engine's Docker-compatible API socket, when reads go there
    /// instead of to the CLI.
    engine: Option<crate::adapters::engine_api::EngineApi>,
}

impl std::fmt::Debug for PodmanComposeRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PodmanComposeRuntime")
            .field("executable", &self.executable)
            .finish_non_exhaustive()
    }
}

impl PodmanComposeRuntime {
    #[must_use]
    pub fn new(process: Arc<dyn ProcessRunner>) -> Self {
        Self {
            process,
            executable: "podman".to_owned(),
            environment: ambient_environment,
            root: process_is_root,
            engine: None,
        }
    }

    /// Reads what the engine's Docker-compatible API answers from its
    /// socket at `engine` instead of starting the CLI: a status's
    /// containers, the engine's version, logs. `None` keeps the CLI.
    #[must_use]
    pub fn reading_engine_api(
        mut self,
        engine: Option<crate::adapters::engine_api::EngineApi>,
    ) -> Self {
        self.engine = engine;
        self
    }

    #[must_use]
    pub fn with_executable(process: Arc<dyn ProcessRunner>, executable: impl Into<String>) -> Self {
        Self {
            process,
            executable: executable.into(),
            environment: ambient_environment,
            root: process_is_root,
            engine: None,
        }
    }

    #[cfg(test)]
    fn with_environment(
        process: Arc<dyn ProcessRunner>,
        executable: impl Into<String>,
        environment: fn(&str) -> Option<OsString>,
    ) -> Self {
        Self {
            process,
            executable: executable.into(),
            environment,
            root: || false,
            engine: None,
        }
    }

    async fn command(&self, args: Vec<String>) -> Result<CommandOutput, MmError> {
        self.process
            .run(self.command_spec(args).timeout(Duration::from_secs(30)))
            .await
    }

    fn command_spec(&self, args: impl IntoIterator<Item = impl Into<String>>) -> CommandSpec {
        CommandSpec::new(&self.executable)
            .args(args)
            .inherit_environment(PODMAN_INHERITED_ENVIRONMENT)
    }

    fn ensure_success(&self, output: CommandOutput) -> Result<CommandOutput, MmError> {
        if output.success() {
            return Ok(output);
        }

        Err(MmError::ProcessFailed {
            program: self.executable.clone(),
            code: output.code,
            stderr: output.stderr.trim().to_owned(),
        })
    }

    /// Nothing when `compose up` of `services` of the target's stack
    /// started them; otherwise why it could not.
    fn ensure_started(
        &self,
        output: CommandOutput,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<(), MmError> {
        if output.success() {
            return Ok(());
        }
        Err(launch_error(
            RuntimeKind::PodmanCompose,
            self.executable.clone(),
            &Launched::Services {
                stack: &target.stack_name,
                project: &target.project_name,
                services,
            },
            &output,
        ))
    }

    fn require_local_engine_environment(&self) -> Result<(), MmError> {
        if let Some(variable) = ENGINE_REDIRECT_ENVIRONMENT
            .iter()
            .find(|variable| (self.environment)(variable).is_some_and(|value| !value.is_empty()))
        {
            return Err(MmError::Security {
                message: format!(
                    "refusing local runtime mutation while {variable} redirects the container engine"
                ),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl RuntimeDriver for PodmanComposeRuntime {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::PodmanCompose
    }

    async fn check_oci_runtime(&self, name: &str) -> Result<(), String> {
        crate::adapters::runtime::oci_runtime::check_oci_runtime(self, name).await
    }

    async fn probe(&self) -> RuntimeProbe {
        // The three questions do not depend on each other: ask them at once.
        let (version_output, info, compose_output) = tokio::join!(
            self.command(vec!["--version".to_owned()]),
            self.command(vec![
                "info".to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
            ]),
            self.command(vec!["compose".to_owned(), "version".to_owned()]),
        );
        let version_output = match version_output {
            Ok(output) if output.success() => output,
            Ok(output) => {
                return RuntimeProbe::unavailable(
                    self.kind(),
                    Diagnostic::error(
                        "MM_PODMAN_UNAVAILABLE",
                        format!(
                            "Podman returned exit code {:?}: {}",
                            output.code,
                            crate::domain::error::printed(output.stderr.trim())
                        ),
                    ),
                );
            }
            Err(error) => {
                return RuntimeProbe::unavailable(
                    self.kind(),
                    Diagnostic::error(
                        crate::domain::runtime::probe_failure_code(&error, "MM_PODMAN_UNAVAILABLE"),
                        error.to_string(),
                    )
                    .with_help("install Podman and ensure it is available on PATH"),
                );
            }
        };

        let mut diagnostics = Vec::new();
        let info = match info {
            Ok(output) if output.success() && output.truncated => {
                diagnostics.push(Diagnostic::error(
                    "MM_PODMAN_INFO_TRUNCATED",
                    "Podman info exceeded the configured output limit",
                ));
                None
            }
            Ok(output) if output.success() => match serde_json::from_str::<Value>(&output.stdout) {
                Ok(info) => Some(info),
                Err(error) => {
                    diagnostics.push(Diagnostic::error(
                        "MM_PODMAN_INFO_INVALID",
                        format!("Podman returned invalid info JSON: {error}"),
                    ));
                    None
                }
            },
            Ok(output) => {
                diagnostics.push(Diagnostic::error(
                    "MM_PODMAN_INFO_FAILED",
                    format!(
                        "Podman is installed but unavailable: {}",
                        crate::domain::error::printed(output.stderr.trim())
                    ),
                ));
                None
            }
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    "MM_PODMAN_INFO_FAILED",
                    error.to_string(),
                ));
                None
            }
        };

        let compose_provider = match compose_output {
            Ok(output) if output.success() => {
                let executable = parse_provider_executable(&output.stderr);
                let name = provider_name(&output.stdout);
                if let Some(path) = &executable {
                    diagnostics.push(Diagnostic::info(
                        "MM_COMPOSE_EXTERNAL_PROVIDER",
                        format!(
                            "podman compose delegates to external provider {}",
                            path.display()
                        ),
                    ));
                }
                Some(ComposeProvider {
                    name,
                    version: first_nonempty_line(&output.stdout),
                    executable,
                })
            }
            Ok(output) => {
                diagnostics.push(
                    Diagnostic::error(
                        "MM_COMPOSE_PROVIDER_UNAVAILABLE",
                        crate::domain::error::printed(output.stderr.trim()),
                    )
                    .with_help("install a Compose provider supported by podman compose"),
                );
                None
            }
            Err(error) => {
                diagnostics.push(
                    Diagnostic::error("MM_COMPOSE_PROVIDER_UNAVAILABLE", error.to_string())
                        .with_help("install a Compose provider supported by podman compose"),
                );
                None
            }
        };

        let rootless = info
            .as_ref()
            .and_then(|value| value.pointer("/host/security/rootless"))
            .and_then(Value::as_bool);
        let socket_path = info
            .as_ref()
            .and_then(|value| value.pointer("/host/remoteSocket/path"))
            .and_then(Value::as_str)
            .map(PathBuf::from);
        let info_version = info
            .as_ref()
            .and_then(|value| value.pointer("/version/Version"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let engine_version = info_version.or_else(|| first_nonempty_line(&version_output.stdout));
        let rootful_problem = self.rootful_engine_problem(rootless);
        let available = info.is_some() && compose_provider.is_some() && rootful_problem.is_none();
        diagnostics.extend(rootful_problem);
        let capabilities = if available {
            let mut capabilities = runtime_descriptor(self.kind()).advertised_capabilities();
            if self.require_local_engine_environment().is_ok() {
            } else {
                capabilities.retain(|capability| {
                    !matches!(
                        capability,
                        RuntimeCapability::ApplyCompose
                            | RuntimeCapability::ManageNetworks
                            | RuntimeCapability::ManageVolumes
                            | RuntimeCapability::MutateVolumeFiles
                    )
                });
                diagnostics.push(
                    Diagnostic::warning(
                        "MM_LOCAL_RUNTIME_REDIRECTED",
                        "local runtime mutation is disabled by a container-engine redirect environment variable",
                    )
                    .with_help(
                        "unset CONTAINER_HOST, CONTAINER_CONNECTION, DOCKER_HOST, and DOCKER_CONTEXT",
                    ),
                );
            }
            capabilities
        } else {
            Vec::new()
        };

        RuntimeProbe {
            kind: self.kind(),
            available,
            engine_version,
            compose_provider,
            rootless,
            socket_path,
            capabilities,
            diagnostics,
        }
    }

    async fn status_probe(&self) -> RuntimeProbe {
        self.status_through_api().await
    }

    async fn observe_unrendered(
        &self,
        target: &RuntimeTarget,
        expected_services: &[String],
    ) -> Option<Result<ObservedProject, MmError>> {
        self.observe_through_api(target, expected_services).await
    }

    async fn render(&self, target: &RuntimeTarget) -> Result<RenderedProject, MmError> {
        require_local_target(target)?;
        let mut args = vec!["compose".to_owned()];
        for compose_file in &target.compose_files {
            args.push("--file".to_owned());
            args.push(compose_file.to_string_lossy().into_owned());
        }
        args.extend([
            "--project-directory".to_owned(),
            target.project_directory.to_string_lossy().into_owned(),
            "--project-name".to_owned(),
            target.project_name.clone(),
            "config".to_owned(),
            "--no-interpolate".to_owned(),
        ]);

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
        if output.truncated {
            return Err(MmError::RuntimeResponse {
                message: "rendered Compose configuration exceeded the output limit".to_owned(),
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

    async fn observe(&self, target: &RuntimeTarget) -> Result<ObservedProject, MmError> {
        Ok(self.render_and_observe(target).await?.1)
    }

    async fn render_and_observe(
        &self,
        target: &RuntimeTarget,
    ) -> Result<(RenderedProject, ObservedProject), MmError> {
        require_local_target(target)?;
        // The containers are listed while the project renders: the names
        // the rendering gives are only needed to put the two together.
        let (rendered, containers) = tokio::join!(self.render(target), self.containers(target));
        let rendered = rendered?;
        let containers = containers?;
        let observed = ObservedProject {
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

    async fn project_services(
        &self,
        project: &str,
    ) -> Result<std::collections::BTreeSet<String>, MmError> {
        crate::adapters::runtime::project_services::project_services(self, project).await
    }

    async fn observe_cluster_network(
        &self,
        name: &str,
        wanted: Option<&crate::domain::cluster_network::LocalNetwork>,
    ) -> Result<Option<crate::domain::cluster_network::ObservedNetwork>, MmError> {
        self.observed_podman_network(name, wanted).await
    }

    async fn observe_cluster_networks(
        &self,
        wanted: &[crate::domain::cluster_network::LocalNetwork],
    ) -> Result<Vec<Option<crate::domain::cluster_network::ObservedNetwork>>, MmError> {
        self.observed_podman_networks(wanted).await
    }

    async fn network_containers(
        &self,
        networks: &[String],
    ) -> Result<Vec<crate::domain::discovery::ObservedContainer>, MmError> {
        // Through Podman's Docker-compatible API where `mmd` reads it, as
        // Docker's: no process per look.
        crate::adapters::runtime::endpoints::network_containers(
            self,
            self.engine.as_ref(),
            networks,
        )
        .await
    }

    async fn watch_containers(&self, changed: &(dyn Fn() + Send + Sync)) -> Result<(), MmError> {
        crate::adapters::runtime::endpoints::watch_changes(
            &self.executable,
            PODMAN_INHERITED_ENVIRONMENT,
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
}

impl PodmanComposeRuntime {
    /// Every container of the project, running or not, with its health.
    async fn containers(
        &self,
        target: &RuntimeTarget,
    ) -> Result<Vec<crate::domain::runtime::ContainerObservation>, MmError> {
        let args = vec![
            "ps".to_owned(),
            "--all".to_owned(),
            "--filter".to_owned(),
            format!("label=com.docker.compose.project={}", target.project_name),
            "--format".to_owned(),
            "json".to_owned(),
        ];
        let output = self
            .process
            .run(
                self.command_spec(args)
                    .interpolation_environment(&target.interpolation_environment)
                    .current_dir(&target.root)
                    .timeout(Duration::from_secs(30))
                    .internal_stdout(),
            )
            .await?;
        let output = self.ensure_success(output)?;
        if output.truncated {
            return Err(MmError::RuntimeResponse {
                message: "Podman container list exceeded the output limit".to_owned(),
            });
        }
        let mut containers = parse_compose_observation(&output, "Podman")?;
        self.populate_health(&target.root, &mut containers).await?;
        containers.sort_by(|left, right| {
            left.service
                .cmp(&right.service)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(containers)
    }
}
