use super::*;
use crate::domain::secret::SecretGenerationSelection;

#[derive(Debug, Clone, Subcommand)]
pub enum SecretCommand {
    /// List available provider capabilities.
    Providers,

    /// Initialize a secret provider for this project.
    Init {
        #[arg(value_name = "PROVIDER")]
        provider: SecretProvider,
    },

    /// Show secret set/unset/default state without revealing values. On a remote
    /// environment: what each host holds and has staged.
    Status {
        #[arg(value_name = "KEY")]
        key: Option<String>,
        /// Read this machine's store, the one `mm target setup` pushes, even
        /// for a remote environment.
        #[arg(long)]
        local: bool,
    },

    /// Set a configured secret using a hidden prompt or stdin. On a remote
    /// environment the value is staged on every host for the next
    /// deployment.
    Set {
        #[arg(value_name = "KEY")]
        key: String,
        /// Read exactly one value from stdin without prompting.
        #[arg(long)]
        stdin: bool,
        /// Write this machine's store, the one `mm target setup` pushes,
        /// even for a remote environment.
        #[arg(long)]
        local: bool,
    },

    /// Remove a configured secret. On a remote environment the removal is
    /// staged for the next deployment.
    Unset {
        #[arg(value_name = "KEY")]
        key: String,
        /// Skip the destructive confirmation prompt.
        #[arg(long)]
        yes: bool,
        /// Remove it from this machine's store, even for a remote
        /// environment.
        #[arg(long)]
        local: bool,
    },

    /// Drop a change staged on a remote environment's hosts before it is
    /// deployed.
    Discard {
        #[arg(value_name = "KEY")]
        key: String,
    },

    /// Create configured generated secrets without overwriting existing values.
    ///
    /// With --missing or --all, a secret whose provider has no connection
    /// from this machine is listed as skipped with the reason, and the
    /// others are still created.
    #[command(group(
        clap::ArgGroup::new("selection")
            .required(true)
            .args(["key", "missing", "all"])
    ))]
    Generate {
        /// Logical secret name to generate.
        key: Option<String>,
        /// Generate the secrets the selected environment uses: those its
        /// bindings name, its services mount, and its registries, TLS,
        /// tunnel and deploy key read.
        #[arg(long)]
        missing: bool,
        /// Generate every configured generated secret, whichever
        /// environment uses it.
        #[arg(long)]
        all: bool,
    },

    /// Explicitly replace one secret using its configured source.
    Rotate {
        #[arg(value_name = "KEY")]
        key: String,
        /// Read a prompted replacement from stdin.
        #[arg(long)]
        stdin: bool,
        /// Apply the exact displayed rotation plan without an interactive confirmation.
        #[arg(long)]
        yes: bool,
    },

    /// Move a cluster environment's secrets from the age store into the
    /// store mm runs on its nodes. Each becomes a version there with the
    /// revision it has now, so pointing the secrets at the store afterwards
    /// replaces no container. Safe to run again.
    Migrate {
        /// The store the values come from.
        #[arg(long, value_enum)]
        from: MigrateSource,
        /// The managed store's name under `providers:`.
        #[arg(long, value_name = "NAME")]
        to: String,
    },

    /// Plan or apply scheduled rotations.
    Rotation {
        #[command(subcommand)]
        command: SecretRotationCommand,
    },
    /// Manage OpenBao and Vault provider instances.
    Provider {
        #[command(subcommand)]
        command: SecretProviderCommand,
    },
}

/// Where `mm secret migrate` takes the values from.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MigrateSource {
    Age,
}

#[derive(Debug, Clone, Subcommand)]
pub enum SecretRotationCommand {
    /// Inspect metadata and produce an approval hash without changing secrets.
    Plan,
    /// Apply the exact reviewed plan using revision-guarded writes.
    Apply {
        /// Hash printed by `mm secret rotation plan`.
        #[arg(long, value_name = "HASH")]
        plan: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}

pub(super) async fn run(
    command: SecretCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    if matches!(
        &command,
        SecretCommand::Status { local: false, .. }
            | SecretCommand::Set { local: false, .. }
            | SecretCommand::Unset { local: false, .. }
            | SecretCommand::Discard { .. }
    ) && let Some(remote) = super::values::remote(
        &invocation.start,
        invocation.selection.environment.clone(),
        invocation.runtime_kind,
        invocation.home(),
    )? {
        return run_remote_secret(command, &remote, invocation.format).await;
    }
    if let SecretCommand::Discard { key } = &command {
        return Err(super::values::nothing_staged_here(key));
    }
    if let SecretCommand::Migrate { to, .. } = &command {
        return super::values::run_migrate(to, invocation).await;
    }
    if let Some(code) = super::values::generate_in_store(&command, invocation).await? {
        return Ok(code);
    }
    run_secret_command(
        command,
        &invocation.start,
        invocation.selection.environment.clone(),
        invocation.runtime_kind,
        invocation.home(),
        invocation.format,
        Arc::clone(&invocation.clock),
    )
    .await
}

async fn run_secret_command(
    command: SecretCommand,
    start: &std::path::Path,
    environment: Option<String>,
    runtime: crate::domain::runtime::RuntimeKind,
    explicit_home: Option<&std::path::Path>,
    format: OutputFormat,
    clock: Arc<dyn Clock>,
) -> Result<u8, MmError> {
    if matches!(&command, SecretCommand::Providers) {
        output::write_secret_providers(&composition::secret_capabilities()?, format)?;
        return Ok(0);
    }
    let services = composition::environment_services(start, explicit_home, Arc::clone(&clock))?;
    let mut selection = WorkspaceSelection::new(start, environment);
    selection.runtime = Some(runtime);
    let command = match command {
        SecretCommand::Rotate { key, stdin, yes } => {
            return run_direct_rotation(&services, start, &selection, &key, stdin, yes, format)
                .await;
        }
        SecretCommand::Rotation { command } => {
            return run_scheduled_rotation(&services, start, &selection, command, format).await;
        }
        command => command,
    };
    match command {
        SecretCommand::Providers => {
            output::write_secret_providers(&services.secrets.capabilities(), format)?;
            Ok(0)
        }
        SecretCommand::Init {
            provider: SecretProvider::Age,
        } => {
            let initialized = services.initialize_age()?;
            output::write_age_initialized(&initialized, format)?;
            Ok(0)
        }
        SecretCommand::Status { key, .. } => {
            let snapshot = services
                .manager
                .secret_status(&selection, key.as_deref())
                .await?;
            output::write_secret_status(&snapshot, format)?;
            Ok(diagnostics_exit_code(&snapshot.diagnostics))
        }
        SecretCommand::Set { key, stdin, .. } => {
            let prompt = services.secrets.input_prompt(start, &key)?;
            let material = read_secret_material(stdin, &prompt)?;
            let result = services
                .secrets
                .set(start, selection.environment.as_deref(), &key, material)
                .await?;
            output::write_secret_mutation(
                &result.name,
                &result.provider,
                Some(&result.revision),
                None,
                format,
            )?;
            Ok(0)
        }
        SecretCommand::Unset { key, yes, .. } => {
            if format == OutputFormat::Json && !yes {
                return Err(MmError::Terminal {
                    message: "JSON secret removal requires --yes".to_owned(),
                });
            }
            if !yes && !confirm(&format!("Remove secret '{}'?", output::terminal_safe(&key)))? {
                return Ok(0);
            }
            let (provider, removed) = services
                .secrets
                .unset(start, selection.environment.as_deref(), &key)
                .await?;
            output::write_secret_mutation(&key, &provider, None, Some(removed), format)?;
            Ok(0)
        }
        SecretCommand::Generate { key, all, .. } => {
            let report = services
                .secrets
                .generate(
                    start,
                    selection.environment.as_deref(),
                    &generation_selection(key, all),
                )
                .await?;
            output::write_secret_generation(&report, format)?;
            Ok(0)
        }
        SecretCommand::Rotate { .. }
        | SecretCommand::Rotation { .. }
        | SecretCommand::Migrate { .. }
        | SecretCommand::Discard { .. } => Ok(0),
        SecretCommand::Provider { .. } => Err(MmError::State {
            message: "secret provider command reached the value-only dispatcher".to_owned(),
        }),
    }
}

async fn run_remote_secret(
    command: SecretCommand,
    remote: &crate::app::RemoteEnvironment,
    format: OutputFormat,
) -> Result<u8, MmError> {
    use super::values::RemoteValueCommand as C;
    let command = match command {
        SecretCommand::Status { key, .. } => C::Status(key),
        SecretCommand::Set { key, stdin, .. } => {
            let material = read_secret_material(stdin, &format!("Value for {key}"))?;
            C::Set(key, material.into_zeroizing())
        }
        SecretCommand::Unset { key, yes, .. } => {
            if format == OutputFormat::Json && !yes {
                return Err(MmError::Terminal {
                    message: "JSON secret removal requires --yes".to_owned(),
                });
            }
            if !yes
                && !confirm(&format!(
                    "Stage the removal of secret '{}' on every host?",
                    output::terminal_safe(&key)
                ))?
            {
                return Ok(0);
            }
            C::Unset(key)
        }
        SecretCommand::Discard { key } => C::Discard(key),
        _ => {
            return Err(MmError::State {
                message: "this secret command does not run on remote hosts".to_owned(),
            });
        }
    };
    super::values::run_remote_values(
        remote,
        crate::domain::staged::ValueKind::Secret,
        command,
        format,
    )
    .await
}

async fn run_direct_rotation(
    services: &composition::EnvironmentServices,
    start: &std::path::Path,
    selection: &WorkspaceSelection,
    key: &str,
    stdin: bool,
    yes: bool,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let plan = services
        .secrets
        .plan_direct_rotation(start, selection.environment.as_deref(), key)
        .await?;
    if format == OutputFormat::Json && !yes {
        return Err(MmError::Terminal {
            message: "JSON direct secret rotation requires --yes".to_owned(),
        });
    }
    if format == OutputFormat::Human {
        output::write_direct_secret_rotation_plan(&plan, format)?;
    }
    if !yes
        && !confirm(&format!(
            "Apply secret rotation plan {}?",
            output::terminal_safe(&plan.plan_hash)
        ))?
    {
        return Ok(0);
    }
    let material = match services.secrets.rotation_prompt(start, key)? {
        Some(prompt) => Some(read_secret_material(stdin, &prompt)?),
        None if stdin => {
            return Err(MmError::Secret {
                message: format!(
                    "secret '{key}' uses its configured generator and does not accept --stdin"
                ),
            });
        }
        None => None,
    };
    let result = services
        .secrets
        .apply_direct_rotation(
            start,
            selection.environment.as_deref(),
            key,
            &plan.plan_hash,
            material,
        )
        .await?;
    output::write_secret_rotated(&result, format)?;
    Ok(0)
}

async fn run_scheduled_rotation(
    services: &composition::EnvironmentServices,
    start: &std::path::Path,
    selection: &WorkspaceSelection,
    command: SecretRotationCommand,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let SecretRotationCommand::Apply { plan, yes } = command else {
        let plan = services
            .secrets
            .rotation_plan(start, selection.environment.as_deref())
            .await?;
        output::write_secret_rotation_plan(&plan, format)?;
        return Ok(0);
    };
    if format == OutputFormat::Json && !yes {
        return Err(MmError::Terminal {
            message: "JSON rotation apply requires --yes".to_owned(),
        });
    }
    if !yes
        && !confirm(&format!(
            "Apply reviewed rotation plan {}?",
            output::terminal_safe(&plan)
        ))?
    {
        return Ok(0);
    }
    let report = services
        .secrets
        .apply_rotation(start, selection.environment.as_deref(), &plan)
        .await?;
    output::write_secret_rotation_apply(&report, format)?;
    Ok(0)
}

pub(super) fn generation_selection(key: Option<String>, all: bool) -> SecretGenerationSelection {
    match key {
        Some(key) => SecretGenerationSelection::Named(key),
        None if all => SecretGenerationSelection::All,
        None => SecretGenerationSelection::Missing,
    }
}
