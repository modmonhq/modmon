use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use zeroize::{Zeroize, Zeroizing};

use crate::adapters::runtime::launch_failure::{Launched, launch_error};
use crate::app::ports::{
    CommandOutput, CommandSpec, ExecStream, LogStream, ProcessOutputSource, ProcessRunner,
    ProcessStreamEvent, RuntimeJobDriver, RuntimeOperationDriver, StreamCommandSpec,
    StreamingProcessRunner,
};
use crate::domain::error::MmError;
use crate::domain::observability::{
    ExecEvent, ExecOutcome, ExecOutputSource, ExecTtyMode, LogOutputPolicy, LogRecord, LogRequest,
    LogSource, OperationCancellation, ResolvedExecRequest, RestartOutcome, ServiceSelector,
};
use crate::domain::runtime::{
    DOCKER_INHERITED_ENVIRONMENT, PODMAN_INHERITED_ENVIRONMENT, RuntimeKind, RuntimeTarget,
    ServiceIdentity,
};
use crate::domain::scheduler::{JobLaunch, RuntimeJobState};

const LOG_RECONNECT_ATTEMPTS: u32 = 3;
/// Long enough for every replica to stop within the engine's grace period.
const RESTART_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone)]
pub struct RuntimeOperationsAdapter {
    kind: RuntimeKind,
    process: Arc<dyn ProcessRunner>,
    streaming: Arc<dyn StreamingProcessRunner>,
    /// The engine's API socket, when reads go there instead of to the CLI.
    engine: Option<crate::adapters::engine_api::EngineApi>,
}

impl RuntimeOperationsAdapter {
    #[must_use]
    pub fn new(
        kind: RuntimeKind,
        process: Arc<dyn ProcessRunner>,
        streaming: Arc<dyn StreamingProcessRunner>,
    ) -> Self {
        Self {
            kind,
            process,
            streaming,
            engine: None,
        }
    }

    /// Reads logs from the engine's API on `engine` instead of starting
    /// the engine's CLI. `None` keeps the CLI.
    #[must_use]
    pub fn reading_engine_api(
        mut self,
        engine: Option<crate::adapters::engine_api::EngineApi>,
    ) -> Self {
        self.engine = engine;
        self
    }

    fn program(&self) -> &'static str {
        match self.kind {
            RuntimeKind::PodmanCompose => "podman",
            RuntimeKind::DockerCompose => "docker",
        }
    }

    fn inherited_environment(&self) -> &'static [&'static str] {
        match self.kind {
            RuntimeKind::PodmanCompose => PODMAN_INHERITED_ENVIRONMENT,
            RuntimeKind::DockerCompose => DOCKER_INHERITED_ENVIRONMENT,
        }
    }

    fn base_spec(&self, args: Vec<String>) -> CommandSpec {
        CommandSpec::new(self.program())
            .args(args)
            .inherit_environment(self.inherited_environment())
    }

    async fn command(&self, args: Vec<String>) -> Result<CommandOutput, MmError> {
        self.process
            .run(self.base_spec(args).timeout(Duration::from_secs(30)))
            .await
    }

    async fn select_container(
        &self,
        target: &RuntimeTarget,
        requested: Option<&str>,
        service: &str,
    ) -> Result<String, MmError> {
        let output = self
            .command(vec![
                "ps".to_owned(),
                "--filter".to_owned(),
                format!("label=com.docker.compose.project={}", target.project_name),
                "--filter".to_owned(),
                format!("label=com.docker.compose.service={service}"),
                "--filter".to_owned(),
                "status=running".to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
            ])
            .await?;
        let candidates = successful_json_ids(output, self.program(), &["ID", "Id", "id"])?;
        let mut candidates = candidates
            .into_iter()
            .filter(|candidate| valid_container_id(candidate))
            .collect::<Vec<_>>();
        candidates.sort();
        candidates.dedup();
        if let Some(requested) = requested {
            let selected = candidates
                .into_iter()
                .filter(|candidate| same_container(candidate, requested))
                .collect::<Vec<_>>();
            return match selected.as_slice() {
                [container] => Ok(container.clone()),
                [] => Err(MmError::RuntimeResponse {
                    message: "the requested running container is not owned by the selected service"
                        .to_owned(),
                }),
                _ => Err(MmError::RuntimeResponse {
                    message: "the requested container prefix matches multiple running containers"
                        .to_owned(),
                }),
            };
        }
        match candidates.as_slice() {
            [container] => Ok(container.clone()),
            [] => Err(MmError::RuntimeResponse {
                message: format!("service '{service}' has no running container on this node"),
            }),
            _ => Err(MmError::RuntimeResponse {
                message: format!(
                    "service '{service}' has multiple running containers; select one explicitly"
                ),
            }),
        }
    }

    /// Every container of a Compose service, running or not, so a crashed
    /// service can be restarted, and whether it runs. A job's one-off
    /// container carries its service's labels and is none of them.
    async fn compose_service_containers(
        &self,
        target: &RuntimeTarget,
        service: &str,
    ) -> Result<Vec<(String, bool)>, MmError> {
        let output = self
            .command(vec![
                "ps".to_owned(),
                "--all".to_owned(),
                "--filter".to_owned(),
                format!("label=com.docker.compose.project={}", target.project_name),
                "--filter".to_owned(),
                format!("label=com.docker.compose.service={service}"),
                "--filter".to_owned(),
                "label=com.docker.compose.oneoff=False".to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
            ])
            .await?;
        let mut containers = successful_json_rows(output, self.program())?
            .iter()
            .filter_map(|row| {
                let id = ["ID", "Id", "id"]
                    .iter()
                    .find_map(|key| row.get(*key).and_then(Value::as_str))?;
                let running = row
                    .get("State")
                    .and_then(Value::as_str)
                    .is_some_and(|state| state.eq_ignore_ascii_case("running"));
                valid_container_id(id).then(|| (id.to_owned(), running))
            })
            .collect::<Vec<_>>();
        containers.sort();
        containers.dedup_by(|left, right| left.0 == right.0);
        Ok(containers)
    }

    /// The containers a restart of `selector` restarts, and whether each
    /// runs: the service's, or the one it names.
    async fn selected_containers(
        &self,
        target: &RuntimeTarget,
        selector: &ServiceSelector,
    ) -> Result<Vec<(String, bool)>, MmError> {
        let mut containers = self
            .compose_service_containers(target, &selector.service)
            .await?;
        if let Some(requested) = &selector.container {
            containers.retain(|(candidate, _)| {
                same_container(candidate, requested)
            });
            if containers.len() > 1 {
                return Err(MmError::RuntimeResponse {
                    message: "the requested container prefix matches multiple containers"
                        .to_owned(),
                });
            }
            if containers.is_empty() {
                return Err(MmError::RuntimeResponse {
                    message: "the requested container is not owned by the selected service"
                        .to_owned(),
                });
            }
        }
        Ok(containers)
    }

    fn log_args(
        &self,
        target: &RuntimeTarget,
        request: &LogRequest,
        selected_container: Option<&str>,
    ) -> Result<Vec<String>, MmError> {
        if let Some(container) = selected_container {
            let mut args = vec!["logs".to_owned()];
            append_log_options(&mut args, request);
            args.push(container.to_owned());
            return Ok(args);
        }
        if target.compose_files.is_empty() {
            return Err(MmError::Security {
                message: "refusing logs without explicit Compose files".to_owned(),
            });
        }
        let mut args = vec!["compose".to_owned()];
        for file in &target.compose_files {
            args.extend(["--file".to_owned(), file.to_string_lossy().into_owned()]);
        }
        args.extend([
            "--project-directory".to_owned(),
            target.project_directory.to_string_lossy().into_owned(),
            "--project-name".to_owned(),
            target.project_name.clone(),
            "logs".to_owned(),
            "--no-color".to_owned(),
        ]);
        if self.kind == RuntimeKind::DockerCompose {
            args.push("--no-log-prefix".to_owned());
        }
        append_log_options(&mut args, request);
        args.extend(request.service().map(str::to_owned));
        Ok(args)
    }

    /// Where a container's lines come from: Docker's API, or Podman's
    /// `logs` command, whose API stamps lines to the second only.
    fn log_lines(&self) -> crate::adapters::runtime::engine_logs::LineSource {
        match self.kind {
            RuntimeKind::DockerCompose => crate::adapters::runtime::engine_logs::LineSource::Api,
            RuntimeKind::PodmanCompose => {
                crate::adapters::runtime::engine_logs::LineSource::Command {
                    streaming: Arc::clone(&self.streaming),
                    program: self.program(),
                    inherited: self.inherited_environment(),
                }
            }
        }
    }
}

#[async_trait]
impl RuntimeOperationDriver for RuntimeOperationsAdapter {
    async fn restart(
        &self,
        target: &RuntimeTarget,
        selector: &ServiceSelector,
    ) -> Result<RestartOutcome, MmError> {
        let containers = self
            .selected_containers(target, selector)
            .await?
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        if containers.is_empty() {
            return Err(MmError::RuntimeResponse {
                message: format!(
                    "service '{}' has no containers to restart; start the stack first",
                    selector.service
                ),
            });
        }
        let mut args = vec!["restart".to_owned()];
        args.extend(containers.iter().cloned());
        let output = self
            .process
            .run(self.base_spec(args).timeout(RESTART_TIMEOUT))
            .await?;
        require_success(&output, self.program(), "restart service containers")?;
        Ok(RestartOutcome {
            runtime: self.kind,
            identity: identity(target, selector),
            containers,
            rolling: false,
            nodes: Vec::new(),
        })
    }

    async fn restartable(
        &self,
        target: &RuntimeTarget,
        selector: &ServiceSelector,
    ) -> Result<Vec<(String, bool)>, MmError> {
        self.selected_containers(target, selector).await
    }

    async fn container_address(
        &self,
        target: &RuntimeTarget,
        service: &str,
    ) -> Result<std::net::IpAddr, MmError> {
        let container = self.select_container(target, None, service).await?;
        let output = self
            .command(vec![
                "inspect".to_owned(),
                "--format".to_owned(),
                "{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}".to_owned(),
                container,
            ])
            .await?;
        require_success(&output, self.program(), "inspect the service container's address")?;
        for address in output.stdout.split_whitespace() {
            if let Ok(address) = address.parse::<std::net::IpAddr>() {
                return Ok(address);
            }
        }
        Err(MmError::RuntimeResponse {
            message: format!(
                "service '{service}' has no container address the daemon can reach; rootless runtimes do not support forwarding yet"
            ),
        })
    }

    fn reads_whole_projects(&self) -> bool {
        self.engine.is_some()
    }

    async fn logs(
        &self,
        target: &RuntimeTarget,
        request: LogRequest,
        mut redactions: Vec<Zeroizing<String>>,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        redactions.retain(|value| !value.is_empty());
        redactions.sort_by_key(|value| std::cmp::Reverse(value.len()));
        redactions.dedup();
        if let Some(engine) = &self.engine {
            return crate::adapters::runtime::engine_logs::read(
                crate::adapters::runtime::engine_logs::EngineLogRead {
                    api: engine.clone(),
                    lines: self.log_lines(),
                    project: target.project_name.clone(),
                    identity: ServiceIdentity {
                        project_id: target.project_id,
                        environment: target.environment.clone(),
                        stack: target.stack_name.clone(),
                        module: request
                            .selector
                            .as_ref()
                            .and_then(|selector| selector.module.clone()),
                        service: String::new(),
                    },
                    request,
                    redactions,
                    cancellation,
                },
            )
            .await;
        }
        // The engine's CLI reads one service at a time.
        let Some(selector) = request.selector.clone() else {
            return Err(MmError::State {
                message: "this runtime reads the log of one service at a time".to_owned(),
            });
        };
        let selected_container = if selector.container.is_some() {
            Some(
                self.select_container(target, selector.container.as_deref(), &selector.service)
                    .await?,
            )
        } else {
            None
        };
        let args = self.log_args(target, &request, selected_container.as_deref())?;
        let identity = identity(target, &selector);
        let streaming = Arc::clone(&self.streaming);
        let program = self.program().to_owned();
        let inherited = self.inherited_environment();
        let root = target.root.clone();
        let event_limit = request.limits.buffered_records;
        let (sender, receiver) = tokio::sync::mpsc::channel(event_limit);
        let task_cancellation = cancellation.clone();
        tokio::spawn(async move {
            let result = forward_logs(ForwardLogsContext {
                streaming,
                program,
                inherited,
                root,
                args,
                identity,
                request,
                redactions,
                cancellation: task_cancellation.clone(),
                sender: sender.clone(),
            })
            .await;
            if let Err(error) = result {
                if sender.send(Err(error)).await.is_err() {
                    tracing::debug!("log receiver closed before failure diagnostics");
                }
                task_cancellation.cancel();
            }
        });
        Ok(LogStream::new(receiver, cancellation))
    }

    async fn exec(
        &self,
        target: &RuntimeTarget,
        request: ResolvedExecRequest,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        if request.request.tty == ExecTtyMode::Enabled && !request.environment.is_empty() {
            return Err(MmError::Security {
                message: "TTY exec with resolved secret environment is not supported because output cannot be redacted"
                    .to_owned(),
            });
        }
        let container = self
            .select_container(
                target,
                request.request.selector.container.as_deref(),
                &request.request.selector.service,
            )
            .await?;
        let suppress_output = !request.environment.is_empty();
        let mut args = vec!["exec".to_owned()];
        if request.request.stdin == crate::domain::observability::ExecStdinMode::Inherit {
            args.push("--interactive".to_owned());
        }
        if request.request.tty == ExecTtyMode::Enabled {
            args.push("--tty".to_owned());
        }
        if let Some(user) = &request.request.user {
            args.extend(["--user".to_owned(), user.clone()]);
        }
        if let Some(workdir) = &request.request.workdir {
            args.extend([
                "--workdir".to_owned(),
                workdir.to_string_lossy().into_owned(),
            ]);
        }
        for name in request.environment.keys() {
            args.extend(["--env".to_owned(), name.to_owned()]);
        }
        args.push(container.clone());
        args.extend(request.request.argv.clone());
        let spec = StreamCommandSpec::new(self.program())
            .args(args)
            .inherit_environment(self.inherited_environment())
            .sensitive_environment(request.environment)
            .current_dir(&target.root)
            .stdin(request.request.stdin)
            .tty(request.request.tty)
            .timeout(request.request.timeout)
            .buffered_events(128);
        let process = self.streaming.spawn(spec, cancellation.clone()).await?;
        let identity = identity(target, &request.request.selector);
        let kind = self.kind;
        let started_at_unix_ms = unix_now_ms();
        let (sender, receiver) = tokio::sync::mpsc::channel(128);
        let task_cancellation = cancellation.clone();
        tokio::spawn(async move {
            if sender
                .send(Ok(ExecEvent::Selected {
                    runtime: kind,
                    identity: identity.clone(),
                    container_id: container.clone(),
                }))
                .await
                .is_err()
            {
                task_cancellation.cancel();
                return;
            }
            let result = forward_exec(
                process,
                &sender,
                kind,
                identity,
                container,
                started_at_unix_ms,
                suppress_output,
            )
            .await;
            if let Err(error) = result
                && sender.send(Err(error)).await.is_err()
            {
                tracing::debug!("exec receiver closed before failure diagnostics");
            }
            task_cancellation.cancel();
        });
        Ok(ExecStream::new(receiver, cancellation))
    }
}

#[async_trait]
impl RuntimeJobDriver for RuntimeOperationsAdapter {
    async fn pull_image(
        &self,
        target: &RuntimeTarget,
        service: &str,
        limit: Duration,
    ) -> Result<(), MmError> {
        let base = |args: Vec<String>| self.base_spec(args);
        crate::adapters::runtime::image_pull::ImagePuller {
            process: self.process.as_ref(),
            kind: self.kind,
            program: self.program(),
            base: &base,
        }
        .pull(
            target,
            self.compose_prefix(target)?,
            Some(&[service.to_owned()]),
            limit,
        )
        .await
    }

    async fn launch(&self, launch: JobLaunch) -> Result<String, MmError> {
        if let Some(existing) = self
            .owned_job_container(&launch.target, &launch.runtime_name)
            .await?
        {
            return Ok(existing.id);
        }
        if launch.target.compose_files.is_empty() {
            return Err(MmError::Security {
                message: "refusing a one-shot without explicit reviewed Compose files".to_owned(),
            });
        }
        let mut args = self.compose_prefix(&launch.target)?;
        args.extend([
            "run".to_owned(),
            "--detach".to_owned(),
            "--no-deps".to_owned(),
            "--name".to_owned(),
            launch.runtime_name.clone(),
        ]);
        for name in launch.environment.keys() {
            args.extend(["--env".to_owned(), name.to_owned()]);
        }
        if let Some(user) = &launch.user {
            args.extend(["--user".to_owned(), user.clone()]);
        }
        args.push(launch.service.clone());
        args.extend(launch.argv);
        let mut environment = launch.target.interpolation_environment.clone();
        for (name, value) in launch.environment.values() {
            environment.insert(name.to_owned(), value.to_owned());
        }
        let auth = crate::adapters::runtime::registry_auth::RegistryAuth::prepare(
            &launch.target.registry_credentials,
        )?;
        let output = self
            .process
            .run(
                auth.for_runtime(self.kind, self.base_spec(args))
                    .interpolation_environment(&environment)
                    .current_dir(&launch.target.root)
                    .timeout(Duration::from_secs(120))
                    .with_output_limit(64 * 1024),
            )
            .await?;
        drop(auth);
        if !output.success() {
            return Err(launch_error(
                self.kind,
                format!("{} (launch one-shot job)", self.program()),
                &Launched::Job {
                    job: &launch.job,
                    service: &launch.service,
                },
                &output,
            ));
        }
        let candidate = output.stdout.trim();
        let container = if valid_container_id(candidate) {
            candidate.to_owned()
        } else {
            self.owned_job_container(&launch.target, &launch.runtime_name)
                .await?
                .map(|container| container.id)
                .ok_or_else(|| MmError::RuntimeResponse {
                    message: "runtime launched a one-shot but returned no owned container identity"
                        .to_owned(),
                })?
        };
        Ok(container)
    }

    async fn inspect(
        &self,
        target: &RuntimeTarget,
        runtime_name: &str,
        runtime_id: Option<&str>,
        output_limit_bytes: usize,
        redactions: Vec<Zeroizing<String>>,
    ) -> Result<RuntimeJobState, MmError> {
        let Some(container) = self.owned_job_container(target, runtime_name).await? else {
            return Ok(RuntimeJobState::Missing);
        };
        if runtime_id
            .is_some_and(|expected| expected != container.id && !container.id.starts_with(expected))
        {
            return Err(MmError::Security {
                message: "one-shot runtime identity changed ownership".to_owned(),
            });
        }
        if container.running {
            return Ok(RuntimeJobState::Running);
        }
        let mut spec = self
            .base_spec(vec!["logs".to_owned(), container.id])
            .timeout(Duration::from_secs(30))
            .with_output_limit(output_limit_bytes);
        for redaction in &redactions {
            spec = spec.redact_value(redaction.to_string());
        }
        let output = self.process.run(spec).await?;
        require_success(&output, self.program(), "read one-shot output")?;
        let (value, truncated) = bounded_output(output, output_limit_bytes);
        let value = crate::domain::observability::redact_output(&value, &redactions);
        Ok(RuntimeJobState::Exited {
            exit_code: container.exit_code,
            output: value,
            truncated,
        })
    }

    async fn cancel(
        &self,
        target: &RuntimeTarget,
        runtime_name: &str,
        runtime_id: Option<&str>,
    ) -> Result<(), MmError> {
        let Some(container) = self.owned_job_container(target, runtime_name).await? else {
            return Ok(());
        };
        verify_runtime_id(runtime_id, &container.id)?;
        if container.running {
            let output = self
                .command(vec![
                    "stop".to_owned(),
                    "--time".to_owned(),
                    "10".to_owned(),
                    container.id,
                ])
                .await?;
            require_success(&output, self.program(), "cancel one-shot job")?;
        }
        Ok(())
    }

    async fn job_containers(&self, target: &RuntimeTarget) -> Result<Vec<String>, MmError> {
        let prefix = crate::domain::scheduler::runtime_job_name(target.project_id, uuid::Uuid::nil());
        let prefix = prefix.trim_end_matches('0');
        let output = self
            .command(vec![
                "ps".to_owned(),
                "--all".to_owned(),
                "--filter".to_owned(),
                format!("label=com.docker.compose.project={}", target.project_name),
                "--filter".to_owned(),
                format!("name={prefix}"),
                "--format".to_owned(),
                "{{.Names}}".to_owned(),
            ])
            .await?;
        require_success(&output, self.program(), "list job containers")?;
        Ok(output
            .stdout
            .lines()
            .map(str::trim)
            .filter(|name| name.starts_with(prefix) && valid_job_name(name))
            .map(str::to_owned)
            .collect())
    }

    async fn cleanup(
        &self,
        target: &RuntimeTarget,
        runtime_name: &str,
        runtime_id: Option<&str>,
    ) -> Result<(), MmError> {
        let Some(container) = self.owned_job_container(target, runtime_name).await? else {
            return Ok(());
        };
        verify_runtime_id(runtime_id, &container.id)?;
        let output = self
            .command(vec!["rm".to_owned(), "--force".to_owned(), container.id])
            .await?;
        require_success(&output, self.program(), "clean one-shot artifact")
    }
}

/// Whether `candidate`, an id the engine listed, is the container
/// `requested` names. Compose lists the short id (12 characters); a status
/// read, and so the terminal UI, carries the full one; a person may type a
/// prefix. Either is a prefix of the other.
fn same_container(candidate: &str, requested: &str) -> bool {
    !requested.is_empty() && (candidate.starts_with(requested) || requested.starts_with(candidate))
}
