use super::*;

#[derive(Debug, Clone, Subcommand)]
pub enum ServiceCommand {
    /// Show desired and observed state, optionally for one service.
    Status {
        #[arg(value_name = "SERVICE")]
        service: Option<String>,
    },
    /// Read a service's log, or every service's, from all its containers
    /// in the order the lines were written.
    Logs(LogsArgs),
    /// Execute an exact argv vector; no shell is implied.
    Exec(ExecArgs),
    /// Open an explicitly approved high-risk shell.
    Shell(ShellArgs),
    /// Restart one service through runtime policy.
    Restart {
        #[arg(value_name = "SERVICE")]
        service: String,
    },
}

pub(super) async fn run(
    command: ServiceCommand,
    invocation: &super::dispatch::Invocation,
    cancellation: crate::domain::observability::OperationCancellation,
) -> Result<u8, MmError> {
    match command {
        ServiceCommand::Status { service } => {
            run_service_status(
                &invocation.selection,
                invocation.home(),
                invocation.runtime_kind,
                invocation.format,
                &invocation.services,
                service.as_deref(),
            )
            .await
        }
        ServiceCommand::Logs(arguments) => {
            run_logs(
                arguments,
                &invocation.selection,
                invocation.home(),
                invocation.runtime_kind,
                invocation.format,
                &invocation.services,
                cancellation,
            )
            .await
        }
        ServiceCommand::Exec(arguments) => {
            let request = exec_request(arguments, false)?;
            run_exec(
                request,
                &invocation.selection,
                invocation.home(),
                invocation.runtime_kind,
                invocation.format,
                &invocation.services,
                cancellation,
            )
            .await
        }
        ServiceCommand::Shell(arguments) => {
            let request = shell_request(arguments)?;
            run_exec(
                request,
                &invocation.selection,
                invocation.home(),
                invocation.runtime_kind,
                invocation.format,
                &invocation.services,
                cancellation,
            )
            .await
        }
        ServiceCommand::Restart { service } => {
            run_service_restart(
                &service,
                &invocation.selection,
                invocation.home(),
                invocation.runtime_kind,
                invocation.format,
                &invocation.services,
            )
            .await
        }
    }
}

fn shell_request(
    arguments: ShellArgs,
) -> Result<crate::domain::observability::ExecRequest, MmError> {
    let approved = arguments.approve_high_risk;
    exec_request(
        ExecArgs {
            service: arguments.service,
            stack: arguments.stack,
            container: arguments.container,
            user: arguments.user,
            workdir: arguments.workdir,
            environment_references: Vec::new(),
            tty: true,
            stdin: true,
            timeout: 30,
            argv: vec!["/bin/sh".to_owned()],
        },
        true,
    )
    .map(|mut request| {
        request.approval = approval("cli", approved);
        request
    })
}

#[derive(Debug, Clone, Args)]
pub struct LogsArgs {
    /// Service whose containers are read. Leave it out to read every
    /// service of the environment.
    #[arg(value_name = "SERVICE")]
    service: Option<String>,
    /// Restrict the service to one configured stack.
    #[arg(long, value_name = "STACK", requires = "service")]
    stack: Option<String>,
    /// On a cluster target, read only this node. Repeat it for more.
    #[arg(long = "node", value_name = "NODE")]
    nodes: Vec<String>,
    /// Continue streaming new records until interrupted.
    #[arg(long)]
    follow: bool,
    /// Number of recent records requested before following.
    #[arg(long, default_value_t = crate::domain::observability::DEFAULT_LOG_TAIL, value_name = "COUNT")]
    tail: u32,
    /// Include records at or after this Unix millisecond timestamp.
    #[arg(long, value_name = "UNIX_MS")]
    since: Option<i64>,
    /// Include records before this Unix millisecond timestamp.
    #[arg(long, value_name = "UNIX_MS")]
    until: Option<i64>,
    /// Show when the engine recorded each line.
    #[arg(long)]
    timestamps: bool,
    /// Include only standard output records.
    #[arg(long, conflicts_with = "stderr")]
    stdout: bool,
    /// Include only standard error records.
    #[arg(long, conflicts_with = "stdout")]
    stderr: bool,
    /// Maximum bytes retained from any one log line.
    #[arg(long, default_value_t = crate::domain::observability::DEFAULT_MAX_LOG_LINE_BYTES)]
    max_line_bytes: usize,
    /// Maximum aggregate log bytes accepted per second.
    #[arg(long, default_value_t = 1024 * 1024)]
    max_bytes_per_second: u64,
    /// Maximum aggregate log records accepted per second.
    #[arg(long, default_value_t = 10_000)]
    max_lines_per_second: u32,
    /// Disable redaction. Requires --approve-high-risk.
    #[arg(long, requires = "approve_high_risk")]
    raw: bool,
    /// Confirm that raw application logs can disclose arbitrary secret values.
    #[arg(long)]
    approve_high_risk: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ExecArgs {
    /// Service containing the command target.
    #[arg(value_name = "SERVICE")]
    service: String,
    /// Restrict the service to one configured stack.
    #[arg(long, value_name = "STACK")]
    stack: Option<String>,
    /// Select one exact container when the service has multiple replicas.
    #[arg(long, value_name = "CONTAINER")]
    container: Option<String>,
    /// User or numeric identity used inside the container.
    #[arg(long, value_name = "USER")]
    user: Option<String>,
    /// Working directory used inside the container.
    #[arg(long, value_name = "PATH")]
    workdir: Option<PathBuf>,
    /// Set NAME from a secret reference (NAME=provider://location); plaintext is rejected.
    #[arg(id = "exec_environment", long = "env", value_name = "NAME=REFERENCE")]
    environment_references: Vec<String>,
    /// Allocate an interactive terminal; output capture is disabled.
    #[arg(long)]
    tty: bool,
    /// Forward standard input to the remote command.
    #[arg(long)]
    stdin: bool,
    /// Maximum command runtime before cancellation.
    #[arg(long, default_value_t = crate::domain::observability::DEFAULT_EXEC_TIMEOUT_SECONDS, value_name = "SECONDS")]
    timeout: u64,
    /// Exact executable and argument vector; no shell is implied.
    #[arg(last = true, required = true, value_name = "COMMAND")]
    argv: Vec<String>,
}

#[derive(Debug, Clone, Args)]
pub struct ShellArgs {
    /// Service in which to start the interactive shell.
    #[arg(value_name = "SERVICE")]
    pub(super) service: String,
    /// Restrict the service to one configured stack.
    #[arg(long, value_name = "STACK")]
    pub(super) stack: Option<String>,
    /// Select one exact container when the service has multiple replicas.
    #[arg(long, value_name = "CONTAINER")]
    pub(super) container: Option<String>,
    /// User or numeric identity used inside the container.
    #[arg(long, value_name = "USER")]
    pub(super) user: Option<String>,
    /// Working directory used inside the container.
    #[arg(long, value_name = "PATH")]
    pub(super) workdir: Option<PathBuf>,
    /// Confirm the shell's high-risk, authorization-gated behavior.
    #[arg(long, required = true)]
    pub(super) approve_high_risk: bool,
}

pub(super) fn approval(
    source: &str,
    approved_high_risk: bool,
) -> crate::domain::observability::ApprovalContext {
    crate::domain::observability::ApprovalContext {
        principal: std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "local-operator".to_owned()),
        source: source.to_owned(),
        approved_high_risk,
    }
}

pub(super) fn exec_request(
    arguments: ExecArgs,
    shell_wrapper: bool,
) -> Result<crate::domain::observability::ExecRequest, MmError> {
    let environment = arguments
        .environment_references
        .into_iter()
        .map(|value| {
            let (name, reference) = value.split_once('=').ok_or_else(|| MmError::Environment {
                message: "exec --env must be NAME=provider://location".to_owned(),
            })?;
            Ok(crate::domain::observability::ExecEnvironmentReference {
                name: name.to_owned(),
                reference: reference
                    .parse()
                    .map_err(|message| MmError::Environment { message })?,
            })
        })
        .collect::<Result<Vec<_>, MmError>>()?;
    Ok(crate::domain::observability::ExecRequest {
        user: arguments.user,
        workdir: arguments.workdir,
        environment,
        tty: if arguments.tty {
            crate::domain::observability::ExecTtyMode::Enabled
        } else {
            crate::domain::observability::ExecTtyMode::Disabled
        },
        stdin: if arguments.stdin || arguments.tty {
            crate::domain::observability::ExecStdinMode::Inherit
        } else {
            crate::domain::observability::ExecStdinMode::Closed
        },
        timeout: std::time::Duration::from_secs(arguments.timeout),
        shell_wrapper,
        ..crate::domain::observability::ExecRequest::new(
            crate::domain::observability::ServiceSelector {
                stack: arguments.stack,
                module: None,
                service: arguments.service,
                container: arguments.container,
            },
            arguments.argv,
            approval("cli", false),
        )
    })
}

pub(super) fn filter_service_snapshot(
    mut snapshot: crate::domain::workspace::WorkspaceSnapshot,
    service: Option<&str>,
) -> Result<crate::domain::workspace::WorkspaceSnapshot, MmError> {
    let Some(service) = service else {
        return Ok(snapshot);
    };
    let mut found = false;
    for stack in &mut snapshot.stacks {
        stack.services.retain(|candidate| {
            let matches = candidate.name == service;
            found |= matches;
            matches
        });
    }
    snapshot.stacks.retain(|stack| !stack.services.is_empty());
    if !found {
        return Err(MmError::Environment {
            message: format!("service '{service}' is not configured"),
        });
    }
    snapshot.names = snapshot.names.map(|names| names.only(service));
    Ok(snapshot)
}

pub(super) async fn run_service_status(
    selection: &WorkspaceSelection,
    explicit_home: Option<&std::path::Path>,
    runtime: crate::domain::runtime::RuntimeKind,
    format: OutputFormat,
    services: &RuntimeComposition,
    service: Option<&str>,
) -> Result<u8, MmError> {
    let gateway = composition::service_gateway(selection, explicit_home, runtime, services)?;
    let progress = crate::presentation::inline::Progress::new(format);
    let snapshot = crate::presentation::inline::step(
        &progress,
        ("status", "Reading the services"),
        gateway.status(&approval("cli", false)),
        |_| Some("Read the services".to_owned()),
    )
    .await?;
    progress.finish();
    let snapshot = filter_service_snapshot(snapshot, service)?;
    output::write_status(&snapshot, format)?;
    Ok(snapshot_exit_code(&snapshot))
}

pub(super) async fn run_service_restart(
    service: &str,
    selection: &WorkspaceSelection,
    explicit_home: Option<&std::path::Path>,
    runtime: crate::domain::runtime::RuntimeKind,
    format: OutputFormat,
    services: &RuntimeComposition,
) -> Result<u8, MmError> {
    let gateway = composition::service_gateway(selection, explicit_home, runtime, services)?;
    let progress = crate::presentation::inline::Progress::new(format);
    let outcome = crate::presentation::inline::step(
        &progress,
        ("restart", &format!("Restarting {service}")),
        gateway.restart(crate::domain::observability::RestartRequest {
            selector: crate::domain::observability::ServiceSelector {
                stack: None,
                module: None,
                service: service.to_owned(),
                container: None,
            },
            approval: approval("cli", false),
        }),
        |_| Some(format!("Restarted {service}")),
    )
    .await?;
    progress.finish();
    if format == OutputFormat::Json {
        output::write_json(&outcome)?;
    } else {
        let summary = output::restart_summary(&outcome);
        writeln!(
            crate::presentation::record::stdout().lock(),
            "{}",
            output::terminal_safe(&summary)
        )
        .map_err(|source| MmError::Io {
            context: "writing restart result".to_owned(),
            source,
        })?;
    }
    Ok(0)
}

pub(super) async fn run_logs(
    arguments: LogsArgs,
    selection: &WorkspaceSelection,
    explicit_home: Option<&std::path::Path>,
    runtime: crate::domain::runtime::RuntimeKind,
    format: OutputFormat,
    services: &RuntimeComposition,
    cancellation: crate::domain::observability::OperationCancellation,
) -> Result<u8, MmError> {
    let approval = approval("cli", arguments.approve_high_risk);
    let request = crate::domain::observability::LogRequest {
        nodes: arguments.nodes,
        tail: arguments.tail,
        since_unix_ms: arguments.since,
        until_unix_ms: arguments.until,
        follow: arguments.follow,
        timestamps: arguments.timestamps,
        output: if arguments.stdout {
            crate::domain::observability::LogOutputPolicy::Stdout
        } else if arguments.stderr {
            crate::domain::observability::LogOutputPolicy::Stderr
        } else {
            crate::domain::observability::LogOutputPolicy::Both
        },
        mode: if arguments.raw {
            crate::domain::observability::LogMode::Raw
        } else {
            crate::domain::observability::LogMode::Redacted
        },
        limits: crate::domain::observability::LogLimits {
            maximum_line_bytes: arguments.max_line_bytes,
            // Fewer lines are held at once when each may be long, so
            // the largest line the host accepts can be asked for.
            buffered_records: (crate::domain::observability::MAX_LOG_BUFFER_BYTES
                / arguments.max_line_bytes.max(1))
            .clamp(1, crate::domain::observability::DEFAULT_LOG_BUFFER_RECORDS),
            maximum_bytes_per_second: Some(arguments.max_bytes_per_second),
            maximum_lines_per_second: Some(arguments.max_lines_per_second),
        },
        ..match arguments.service {
            Some(service) => crate::domain::observability::LogRequest::new(
                crate::domain::observability::ServiceSelector {
                    stack: arguments.stack,
                    module: None,
                    service,
                    container: None,
                },
                approval,
            ),
            None => crate::domain::observability::LogRequest::every_service(approval),
        }
    };
    let gateway = composition::service_gateway(selection, explicit_home, runtime, services)?;
    let mut lines = output::LogLines::new(arguments.timestamps)
        .with_nodes(gateway.node_names().iter().map(String::as_str));
    let mut stream = gateway.logs(request, cancellation).await?;
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    while let Some(record) = stream.recv().await {
        let record = record?;
        if format == OutputFormat::Json {
            serde_json::to_writer(&mut writer, &record).map_err(|error| MmError::Io {
                context: "serializing log record".to_owned(),
                source: std::io::Error::other(error),
            })?;
            writeln!(writer).map_err(|source| MmError::Io {
                context: "writing log record".to_owned(),
                source,
            })?;
        } else {
            writeln!(writer, "{}", lines.line(&record)).map_err(|source| MmError::Io {
                context: "writing log record".to_owned(),
                source,
            })?;
        }
        writer.flush().map_err(|source| MmError::Io {
            context: "flushing log output".to_owned(),
            source,
        })?;
    }
    Ok(0)
}

pub(super) async fn run_exec(
    request: crate::domain::observability::ExecRequest,
    selection: &WorkspaceSelection,
    explicit_home: Option<&std::path::Path>,
    runtime: crate::domain::runtime::RuntimeKind,
    format: OutputFormat,
    services: &RuntimeComposition,
    cancellation: crate::domain::observability::OperationCancellation,
) -> Result<u8, MmError> {
    let mut stream = composition::service_gateway(selection, explicit_home, runtime, services)?
        .exec(request, cancellation)
        .await?;
    let mut remote_exit = None;
    while let Some(event) = stream.recv().await {
        let event = event?;
        if format == OutputFormat::Json {
            let stdout = crate::presentation::record::stdout();
            let mut writer = stdout.lock();
            serde_json::to_writer(&mut writer, &event).map_err(|error| MmError::Io {
                context: "serializing exec event".to_owned(),
                source: std::io::Error::other(error),
            })?;
            writeln!(writer).map_err(|source| MmError::Io {
                context: "writing exec event".to_owned(),
                source,
            })?;
        } else if let crate::domain::observability::ExecEvent::Output { source, value, .. } = &event
        {
            match source {
                crate::domain::observability::ExecOutputSource::Stdout => {
                    crate::presentation::record::stdout()
                        .write_all(value.as_bytes())
                        .and_then(|()| crate::presentation::record::stdout().flush())
                        .map_err(|source| MmError::Io {
                            context: "writing exec stdout".to_owned(),
                            source,
                        })?;
                }
                crate::domain::observability::ExecOutputSource::Stderr => {
                    std::io::stderr()
                        .write_all(value.as_bytes())
                        .and_then(|()| std::io::stderr().flush())
                        .map_err(|source| MmError::Io {
                            context: "writing exec stderr".to_owned(),
                            source,
                        })?;
                }
            }
        }
        if let crate::domain::observability::ExecEvent::Finished(outcome) = event {
            remote_exit = Some(outcome.exit_code);
        }
    }
    let code = remote_exit.ok_or_else(|| MmError::RuntimeResponse {
        message: "exec stream ended without a remote command result".to_owned(),
    })?;
    u8::try_from(code).map_err(|_| MmError::RuntimeResponse {
        message: format!("remote command returned non-portable exit code {code}"),
    })
}

pub(super) fn diagnostics_exit_code(diagnostics: &[crate::domain::diagnostic::Diagnostic]) -> u8 {
    if diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && matches!(
                diagnostic.code.as_str(),
                "MM_SECRET_PROVIDER_UNAVAILABLE" | "MM_SECRET_ACCESS_DENIED" | "MM_SECRET_INVALID"
            )
    }) {
        3
    } else if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        2
    } else {
        0
    }
}
