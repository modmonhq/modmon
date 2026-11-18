use super::*;

#[derive(Debug, Clone, Subcommand)]
pub enum EnvCommand {
    /// Show effective values, provenance, and secret status.
    Status {
        /// Do not contact secret providers.
        #[arg(long)]
        no_check_secrets: bool,
    },

    /// Explain every use of one global key or local variable.
    Explain {
        #[arg(value_name = "KEY")]
        key: String,
        /// Do not contact secret providers.
        #[arg(long)]
        no_check_secrets: bool,
    },

    /// Set a workspace-local non-secret override. On a remote environment
    /// the value is staged on every host for the next deployment.
    Set {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(value_name = "VALUE")]
        value: Option<String>,
        /// Read the value from stdin instead of an argument.
        #[arg(long, conflicts_with = "value")]
        stdin: bool,
        /// Fail if the existing override has another revision.
        #[arg(long)]
        expected_revision: Option<i64>,
    },

    /// Remove a workspace-local override. On a remote environment the
    /// removal is staged for the next deployment.
    Unset {
        #[arg(value_name = "KEY")]
        key: String,
    },

    /// Drop a variable change staged on a remote environment's hosts
    /// before it is deployed.
    Discard {
        #[arg(value_name = "KEY")]
        key: String,
    },

    /// Get or change the project-wide default environment.
    Default {
        #[command(subcommand)]
        command: DefaultEnvironmentCommand,
    },

    /// Resolve and compile every service environment in memory.
    Check,

    /// Explicitly write a module's conventional .env file.
    Materialize {
        #[arg(value_name = "MODULE")]
        module: String,
        /// Limit materialization to one service in the module.
        #[arg(long, value_name = "SERVICE")]
        service: Option<String>,
        /// Overwrite an existing .env file.
        #[arg(long)]
        force: bool,
    },

    /// Compare a module's .env with the effective compiled environment.
    Diff {
        #[arg(value_name = "MODULE")]
        module: String,
        /// Limit the diff to one service in the module.
        #[arg(long, value_name = "SERVICE")]
        service: Option<String>,
    },

    /// Explain source and delivery without revealing secret values.
    Review,

    /// Show missing bindings and the resolved delivery mode.
    Setup,

    /// Show or persist environment delivery mode.
    Delivery {
        #[command(subcommand)]
        command: EnvDeliveryCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum DefaultEnvironmentCommand {
    /// Print the configured project default environment.
    Show,
    /// Persist a new project default environment.
    Set {
        #[arg(value_name = "ENVIRONMENT")]
        environment: String,
    },
    /// Remove the project default environment.
    Unset,
}

#[derive(Debug, Clone, Subcommand)]
pub enum EnvDeliveryCommand {
    /// Show the resolved delivery mode and provenance.
    Show,
    /// Persist an explicit delivery mode for one environment.
    Set {
        #[arg(value_name = "ENVIRONMENT")]
        environment: String,
        #[arg(value_enum, value_name = "MODE")]
        mode: EnvironmentDeliveryModeArg,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum EnvironmentDeliveryModeArg {
    Direct,
    EnvFile,
}

impl From<EnvironmentDeliveryModeArg>
    for crate::domain::environment_delivery::EnvironmentDeliveryMode
{
    fn from(value: EnvironmentDeliveryModeArg) -> Self {
        match value {
            EnvironmentDeliveryModeArg::Direct => Self::Direct,
            EnvironmentDeliveryModeArg::EnvFile => Self::EnvFile,
        }
    }
}

pub(super) async fn run(
    command: EnvCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    if matches!(
        &command,
        EnvCommand::Set { .. } | EnvCommand::Unset { .. } | EnvCommand::Discard { .. }
    ) && let Some(remote) = super::values::remote(
        &invocation.start,
        invocation.selection.environment.clone(),
        invocation.runtime_kind,
        invocation.home(),
    )? {
        return run_remote_variable(command, &remote, invocation.format).await;
    }
    run_environment_command(
        command,
        &invocation.start,
        invocation.selection.environment.clone(),
        invocation.home(),
        invocation.format,
        Arc::clone(&invocation.clock),
    )
    .await
}

async fn run_remote_variable(
    command: EnvCommand,
    remote: &crate::app::RemoteEnvironment,
    format: OutputFormat,
) -> Result<u8, MmError> {
    use super::values::RemoteValueCommand as C;
    let command = match command {
        EnvCommand::Set {
            key,
            value,
            stdin,
            expected_revision,
        } => {
            if expected_revision.is_some() {
                return Err(MmError::Environment {
                    message: "--expected-revision applies to this machine's overrides; a remote environment's hosts take every change in full".to_owned(),
                });
            }
            C::Set(key, Zeroizing::new(read_public_value(value, stdin)?))
        }
        EnvCommand::Unset { key } => C::Unset(key),
        EnvCommand::Discard { key } => C::Discard(key),
        _ => {
            return Err(MmError::State {
                message: "this env command does not run on remote hosts".to_owned(),
            });
        }
    };
    super::values::run_remote_values(
        remote,
        crate::domain::staged::ValueKind::Variable,
        command,
        format,
    )
    .await
}

async fn run_environment_command(
    command: EnvCommand,
    start: &std::path::Path,
    environment: Option<String>,
    explicit_home: Option<&std::path::Path>,
    format: OutputFormat,
    clock: Arc<dyn Clock>,
) -> Result<u8, MmError> {
    match command {
        EnvCommand::Default { command } => run_default(command, start, format),
        EnvCommand::Delivery { command } => {
            run_delivery(command, start, environment, explicit_home, format, clock)
        }
        command => {
            let services = composition::environment_services(start, explicit_home, clock)?;
            let selection = WorkspaceSelection::new(start, environment);
            run_environment_operation(command, &services.manager, &selection, format).await
        }
    }
}

fn run_default(
    command: DefaultEnvironmentCommand,
    start: &std::path::Path,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let documents = composition::project_documents();
    let selected = match command {
        DefaultEnvironmentCommand::Show => documents.load_from(start)?.config.default_environment,
        DefaultEnvironmentCommand::Set { environment } => {
            documents
                .set_default_environment(start, Some(&environment))?
                .config
                .default_environment
        }
        DefaultEnvironmentCommand::Unset => {
            documents
                .set_default_environment(start, None)?
                .config
                .default_environment
        }
    };
    output::write_default_environment(selected.as_deref(), format)?;
    Ok(0)
}

fn run_delivery(
    command: EnvDeliveryCommand,
    start: &std::path::Path,
    environment: Option<String>,
    explicit_home: Option<&std::path::Path>,
    format: OutputFormat,
    clock: Arc<dyn Clock>,
) -> Result<u8, MmError> {
    let services = composition::environment_services(start, explicit_home, clock)?;
    let (name, delivery) = match command {
        EnvDeliveryCommand::Show => {
            let selection = WorkspaceSelection::new(start, environment.clone());
            let document = composition::project_documents().load_from(start)?;
            let name =
                crate::domain::config::resolve_environment(&document, environment.as_deref())?;
            let delivery = services.manager.delivery(&selection)?;
            (name, delivery)
        }
        EnvDeliveryCommand::Set {
            environment: name,
            mode,
        } => {
            let delivery = services.manager.set_delivery(start, &name, mode.into())?;
            (name, delivery)
        }
    };
    output::write_environment_delivery(&name, &delivery, format)?;
    Ok(0)
}

async fn run_environment_operation(
    command: EnvCommand,
    manager: &crate::app::EnvironmentManager,
    selection: &WorkspaceSelection,
    format: OutputFormat,
) -> Result<u8, MmError> {
    match command {
        EnvCommand::Status { no_check_secrets } => {
            let snapshot = manager.status(selection, !no_check_secrets).await?;
            output::write_environment_status(&snapshot, format)?;
            Ok(environment_exit_code(&snapshot))
        }
        EnvCommand::Explain {
            key,
            no_check_secrets,
        } => {
            let snapshot = manager.status(selection, !no_check_secrets).await?;
            let snapshot = filter_environment_snapshot(snapshot, &key)?;
            output::write_environment_status(&snapshot, format)?;
            Ok(environment_exit_code(&snapshot))
        }
        EnvCommand::Set {
            key,
            value,
            stdin,
            expected_revision,
        } => {
            let value = read_public_value(value, stdin)?;
            let updated = manager.set_override(selection, &key, &value, expected_revision)?;
            output::write_environment_override(&updated, None, format)?;
            Ok(0)
        }
        EnvCommand::Unset { key } => {
            let removed = manager.unset_override(selection, &key)?;
            output::write_removed(&key, removed, format)?;
            Ok(0)
        }
        EnvCommand::Check | EnvCommand::Materialize { .. } | EnvCommand::Diff { .. } => {
            run_environment_artifact(command, manager, selection, format).await
        }
        EnvCommand::Review => {
            let review = manager.review(selection).await?;
            output::write_environment_review(&review, format)?;
            Ok(0)
        }
        EnvCommand::Setup => {
            let snapshot = manager.status(selection, true).await?;
            output::write_environment_status(&snapshot, format)?;
            Ok(environment_exit_code(&snapshot))
        }
        EnvCommand::Default { .. } | EnvCommand::Delivery { .. } => Ok(0),
        EnvCommand::Discard { key } => Err(super::values::nothing_staged_here(&key)),
    }
}

async fn run_environment_artifact(
    command: EnvCommand,
    manager: &crate::app::EnvironmentManager,
    selection: &WorkspaceSelection,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let status = manager.status(selection, true).await?;
    let exit_code = environment_exit_code(&status);
    if exit_code != 0 {
        output::write_environment_status(&status, format)?;
        return Ok(exit_code);
    }
    match command {
        EnvCommand::Check => {
            let bundle = manager.compile(selection).await?;
            output::write_compiled_manifest(&bundle.manifest, format)?;
            Ok(0)
        }
        EnvCommand::Materialize {
            module,
            service,
            force,
        } => {
            let materialized = manager
                .materialize(selection, &module, service.as_deref(), force)
                .await?;
            output::write_materialized(&materialized, format)?;
            Ok(0)
        }
        EnvCommand::Diff { module, service } => {
            let difference = manager.diff(selection, &module, service.as_deref()).await?;
            let changed = difference.changed;
            output::write_environment_diff(&difference, format)?;
            Ok(u8::from(changed))
        }
        _ => Err(MmError::State {
            message: "environment command was routed to the wrong capability".to_owned(),
        }),
    }
}

pub(super) fn filter_environment_snapshot(
    mut snapshot: EnvironmentStatusSnapshot,
    key: &str,
) -> Result<EnvironmentStatusSnapshot, MmError> {
    let mut found = false;
    for module in &mut snapshot.modules {
        for service in &mut module.services {
            service.variables.retain(|variable| {
                let matches = variable.name == key || variable.global_key.as_deref() == Some(key);
                found |= matches;
                matches
            });
        }
        module
            .services
            .retain(|service| !service.variables.is_empty());
    }
    snapshot
        .modules
        .retain(|module| !module.services.is_empty());
    if !found {
        return Err(MmError::Environment {
            message: format!("no environment variable or global key matches '{key}'"),
        });
    }
    Ok(snapshot)
}

pub(super) fn environment_exit_code(snapshot: &EnvironmentStatusSnapshot) -> u8 {
    diagnostics_exit_code(&snapshot.diagnostics)
}
