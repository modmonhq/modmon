use super::*;
use crate::composition::{declared_provider_locations, managed_provider_request};

#[derive(Debug, Clone, Subcommand)]
pub enum SecretProviderCommand {
    /// List configured provider instances and registered drivers.
    List,
    /// Show one provider instance with its typed descriptor and capabilities.
    Show {
        /// Stable provider instance name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Configure a declared provider instance.
    Add {
        #[command(flatten)]
        provider: ProviderAddArgs,
    },
    /// Reconfigure a declared provider instance without changing its driver or mode.
    Edit {
        /// Stable provider instance name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Assert the existing driver; changing it requires replacement.
        #[arg(long)]
        driver: Option<String>,
        /// Assert the existing mode; changing it requires replacement.
        #[arg(long, value_enum)]
        mode: Option<ProviderModeArg>,
        /// Enable missing mounts for an external provider.
        #[arg(long)]
        bootstrap: bool,
        /// Read the bootstrap credential from stdin instead of a hidden prompt.
        #[arg(long)]
        stdin: bool,
        /// Apply after displaying the exact setup plan.
        #[arg(long)]
        yes: bool,
    },
    /// Test one external provider instance.
    Test {
        /// Stable external provider instance name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Show health and capabilities for one provider instance.
    Status {
        /// Stable provider instance name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Produce a deterministic provider lifecycle plan.
    Plan(ProviderPlanArgs),
    /// Apply an exact provider lifecycle plan.
    Apply {
        #[command(flatten)]
        request: ProviderPlanArgs,
        /// Hash printed by the identical provider lifecycle plan.
        #[arg(long, value_name = "HASH")]
        plan: String,
        /// Apply the exact reviewed plan without prompting.
        #[arg(long)]
        yes: bool,
    },
    /// Write an encrypted provider snapshot after exact review.
    Backup(ProviderDataArgs),
    /// Restore an encrypted provider snapshot after exact review. For the
    /// store mm runs on a cluster's nodes: put a snapshot from `snapshot`
    /// back on every node; without --plan it shows the plan.
    Restore(ProviderDataArgs),
    /// Take a snapshot of the store mm runs on a cluster environment's
    /// nodes: its data and the keys it opens with, sealed to you.
    Snapshot {
        /// The store's name under `providers:`.
        #[arg(value_name = "NAME")]
        name: String,
        /// Write it to this new file instead of standard output.
        #[arg(long, value_name = "FILE")]
        to: Option<PathBuf>,
    },
    /// The recovery copy of the store mm runs on a cluster environment's
    /// nodes.
    Recovery {
        #[command(subcommand)]
        command: super::store_recovery::RecoveryCommand,
    },
    /// Remove a provider through its exact reviewed lifecycle plan.
    Remove {
        /// Stable managed provider instance name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Hash printed by the corresponding remove plan.
        #[arg(long, value_name = "HASH")]
        plan: String,
        /// Apply the exact reviewed removal.
        #[arg(long)]
        yes: bool,
        /// Delete provider-owned persistent data.
        #[arg(long)]
        purge: bool,
    },
}

#[derive(Debug, Clone, Args)]
pub struct ProviderAddArgs {
    /// Stable provider instance name already declared in mm.yaml.
    #[arg(value_name = "NAME")]
    name: String,
    /// Registered provider driver ID.
    #[arg(long)]
    driver: String,
    /// Provider operating mode.
    #[arg(long, value_enum)]
    mode: ProviderModeArg,
    /// Provider ownership scope.
    #[arg(long, value_enum)]
    scope: Option<ProviderScopeArg>,
    /// Enable missing mounts for an external provider.
    #[arg(long)]
    bootstrap: bool,
    /// Read an external bootstrap credential from stdin instead of a hidden prompt.
    #[arg(long)]
    stdin: bool,
    /// Apply after displaying the exact setup or install plan.
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ProviderPlanArgs {
    /// Stable managed provider instance name.
    #[arg(value_name = "NAME")]
    name: String,
    /// Lifecycle operation represented by the plan.
    #[arg(long, value_enum)]
    operation: ProviderLifecycleOperationArg,
    /// Delete owned persistent data when removing the provider.
    #[arg(long)]
    purge: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ProviderDataArgs {
    /// Stable managed provider instance name.
    #[arg(value_name = "NAME")]
    name: String,
    /// Encrypted snapshot source or destination.
    #[arg(long, value_name = "PATH")]
    file: PathBuf,
    /// Hash for the exact current state and selected path.
    #[arg(long, value_name = "HASH")]
    plan: Option<String>,
    /// Apply the exact reviewed backup or restore.
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ProviderModeArg {
    Managed,
    External,
}
impl From<ProviderModeArg> for crate::domain::secret_backend::ProviderMode {
    fn from(value: ProviderModeArg) -> Self {
        match value {
            ProviderModeArg::Managed => Self::Managed,
            ProviderModeArg::External => Self::External,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ProviderScopeArg {
    Installation,
    Project,
}
impl From<ProviderScopeArg> for crate::domain::secret_backend::ProviderScope {
    fn from(value: ProviderScopeArg) -> Self {
        match value {
            ProviderScopeArg::Installation => Self::Installation,
            ProviderScopeArg::Project => Self::Project,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ProviderLifecycleOperationArg {
    Install,
    Restart,
    Upgrade,
    Seal,
    Unseal,
    Remove,
}
impl From<ProviderLifecycleOperationArg>
    for crate::domain::secret_backend::ManagedSecretBackendAction
{
    fn from(value: ProviderLifecycleOperationArg) -> Self {
        match value {
            ProviderLifecycleOperationArg::Install => Self::Install,
            ProviderLifecycleOperationArg::Restart => Self::Restart,
            ProviderLifecycleOperationArg::Upgrade => Self::Upgrade,
            ProviderLifecycleOperationArg::Seal => Self::Seal,
            ProviderLifecycleOperationArg::Unseal => Self::Unseal,
            ProviderLifecycleOperationArg::Remove => Self::Remove,
        }
    }
}

pub(super) async fn run(
    command: SecretProviderCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    run_provider_command(
        command,
        &invocation.start,
        invocation.selection.environment.clone(),
        invocation.home(),
        invocation.format,
        &invocation.services,
        invocation.runtime_kind,
    )
    .await
}

async fn run_provider_command(
    command: SecretProviderCommand,
    start: &std::path::Path,
    requested_environment: Option<String>,
    explicit_home: Option<&std::path::Path>,
    format: OutputFormat,
    runtime: &RuntimeComposition,
    runtime_kind: crate::domain::runtime::RuntimeKind,
) -> Result<u8, MmError> {
    let document = composition::project_documents().load_from(start)?;
    let environment =
        crate::domain::config::resolve_environment(&document, requested_environment.as_deref())?;
    let manager = composition::interactive_secret_backend_manager(
        explicit_home,
        runtime,
        runtime_kind,
        document.config.providers.clone(),
    )?;
    let context = ProviderContext {
        document: &document,
        environment: &environment,
        manager: &manager,
        format,
    };
    let selected = (start, &document, environment.as_str(), runtime_kind);
    match command {
        SecretProviderCommand::List => {
            output::write_provider_catalog(&manager.drivers(), &manager.instances()?, format)?;
            Ok(0)
        }
        SecretProviderCommand::Show { name } => {
            let name = provider_name(name)?;
            output::write_provider_instance(&manager.instance(&name)?, format)?;
            Ok(0)
        }
        SecretProviderCommand::Add { provider } => add_provider(provider, &context).await,
        SecretProviderCommand::Edit {
            name,
            driver,
            mode,
            bootstrap,
            stdin,
            yes,
        } => edit_provider(name, driver, mode, bootstrap, stdin, yes, &context).await,
        SecretProviderCommand::Test { name } => external_status(name, &context).await,
        SecretProviderCommand::Status { name }
            if super::store_recovery::is_cluster_store(&document, &environment, &name) =>
        {
            super::store_recovery::run_status(selected, explicit_home, format).await
        }
        SecretProviderCommand::Status { name } => provider_status(name, &context).await,
        SecretProviderCommand::Plan(arguments) => plan_provider(arguments, &context),
        SecretProviderCommand::Apply { request, plan, yes } => {
            apply_provider(request, plan, yes, &context).await
        }
        SecretProviderCommand::Remove {
            name,
            plan,
            yes,
            purge,
        } => remove_provider(name, plan, yes, purge, &context).await,
        SecretProviderCommand::Recovery { command } => {
            super::store_recovery::run_recovery(command, &document, &environment, explicit_home)
        }
        SecretProviderCommand::Backup(arguments) => {
            provider_data(
                arguments,
                crate::domain::secret_backend::ManagedSecretDataAction::Backup,
                &context,
            )
            .await
        }
        SecretProviderCommand::Snapshot { name, to } => {
            super::store_recovery::run_snapshot(selected, explicit_home, (name, to), format).await
        }
        SecretProviderCommand::Restore(arguments)
            if super::store_recovery::is_cluster_store(
                &document,
                &environment,
                &arguments.name,
            ) =>
        {
            let restore = (
                arguments.name,
                arguments.file,
                arguments.plan,
                arguments.yes,
            );
            super::store_recovery::run_restore(selected, explicit_home, restore, format).await
        }
        SecretProviderCommand::Restore(arguments) => {
            provider_data(
                arguments,
                crate::domain::secret_backend::ManagedSecretDataAction::Restore,
                &context,
            )
            .await
        }
    }
}

struct ProviderContext<'a> {
    document: &'a crate::domain::config::ConfigDocument,
    environment: &'a str,
    manager: &'a crate::app::SecretBackendManager,
    format: OutputFormat,
}

async fn add_provider(
    provider: ProviderAddArgs,
    context: &ProviderContext<'_>,
) -> Result<u8, MmError> {
    let name = provider_name(provider.name)?;
    let descriptor = context.manager.instance(&name)?;
    let driver = crate::domain::secret_backend::ProviderDriverId::new(provider.driver)
        .map_err(|message| MmError::Security { message })?;
    let requested_mode = provider.mode.into();
    if descriptor.driver != driver
        || descriptor.mode != requested_mode
        || provider
            .scope
            .is_some_and(|scope| descriptor.scope != scope.into())
    {
        return Err(MmError::Security {
            message: format!(
                "provider instance '{name}' is declared as driver '{}', mode {}, scope {:?}; changing driver or mode requires an explicit destructive replacement plan",
                descriptor.driver,
                descriptor.mode.as_str(),
                descriptor.scope
            ),
        });
    }
    if descriptor.mode == crate::domain::secret_backend::ProviderMode::External {
        return apply_external_setup(
            context.manager,
            context.document,
            context.environment,
            name,
            ProviderExecutionOptions {
                operation: crate::domain::secret_backend::ProviderOperation::Add,
                bootstrap: provider.bootstrap,
                stdin: provider.stdin,
                yes: provider.yes,
            },
            context.format,
        )
        .await;
    }
    context
        .manager
        .require_operation(&name, crate::domain::secret_backend::ProviderOperation::Add)?;
    let request = managed_provider_request(
        context.document,
        context.environment,
        name,
        ProviderLifecycleOperationArg::Install.into(),
        false,
    )?;
    apply_provider_lifecycle(context.manager, request, None, provider.yes, context.format).await
}

async fn edit_provider(
    name: String,
    driver: Option<String>,
    mode: Option<ProviderModeArg>,
    bootstrap: bool,
    stdin: bool,
    yes: bool,
    context: &ProviderContext<'_>,
) -> Result<u8, MmError> {
    let name = provider_name(name)?;
    let descriptor = context.manager.instance(&name)?;
    let requested_driver = driver
        .map(crate::domain::secret_backend::ProviderDriverId::new)
        .transpose()
        .map_err(|message| MmError::Security { message })?;
    if requested_driver
        .as_ref()
        .is_some_and(|driver| *driver != descriptor.driver)
        || mode.is_some_and(|mode| {
            crate::domain::secret_backend::ProviderMode::from(mode) != descriptor.mode
        })
    {
        return Err(MmError::Security { message: "changing a provider driver or mode requires an explicit destructive replacement plan".to_owned() });
    }
    context.manager.require_operation(
        &name,
        crate::domain::secret_backend::ProviderOperation::Edit,
    )?;
    if descriptor.mode != crate::domain::secret_backend::ProviderMode::External {
        return Err(MmError::Secret {
            message: "managed provider configuration changes require a destructive replacement plan; use plan/apply".to_owned(),
        });
    }
    apply_external_setup(
        context.manager,
        context.document,
        context.environment,
        name,
        ProviderExecutionOptions {
            operation: crate::domain::secret_backend::ProviderOperation::Edit,
            bootstrap,
            stdin,
            yes,
        },
        context.format,
    )
    .await
}

async fn external_status(name: String, context: &ProviderContext<'_>) -> Result<u8, MmError> {
    let name = provider_name(name)?;
    let status = context
        .manager
        .remote_status(
            &crate::domain::secret_backend::RemoteSecretBackendStatusRequest {
                instance: name,
                project_id: context.document.config.project.id,
                environment: context.environment.to_owned(),
            },
        )
        .await?;
    let healthy = status.health.healthy && status.authenticated;
    output::write_provider_status(&status, context.format)?;
    Ok(if healthy { 0 } else { 3 })
}

async fn provider_status(name: String, context: &ProviderContext<'_>) -> Result<u8, MmError> {
    let name = provider_name(name)?;
    let descriptor = context.manager.instance(&name)?;
    if descriptor.mode == crate::domain::secret_backend::ProviderMode::External {
        return external_status(name.to_string(), context).await;
    }
    let status = context.manager.managed_status(&name).await?;
    let healthy = status.installed
        && status.health.as_ref().is_some_and(|health| health.healthy)
        && status.recovery_available_off_instance
        && status.diagnostics.is_empty();
    output::write_managed_provider_status(&status, context.format)?;
    Ok(if healthy { 0 } else { 3 })
}

fn plan_provider(
    arguments: ProviderPlanArgs,
    context: &ProviderContext<'_>,
) -> Result<u8, MmError> {
    let request = managed_provider_request(
        context.document,
        context.environment,
        provider_name(arguments.name)?,
        arguments.operation.into(),
        arguments.purge,
    )?;
    let plan = context.manager.plan_managed(request)?;
    output::write_managed_provider_plan(&plan, context.format)?;
    Ok(0)
}

async fn apply_provider(
    request: ProviderPlanArgs,
    plan: String,
    yes: bool,
    context: &ProviderContext<'_>,
) -> Result<u8, MmError> {
    let request = managed_provider_request(
        context.document,
        context.environment,
        provider_name(request.name)?,
        request.operation.into(),
        request.purge,
    )?;
    apply_provider_lifecycle(context.manager, request, Some(plan), yes, context.format).await
}

async fn remove_provider(
    name: String,
    plan: String,
    yes: bool,
    purge: bool,
    context: &ProviderContext<'_>,
) -> Result<u8, MmError> {
    let request = managed_provider_request(
        context.document,
        context.environment,
        provider_name(name)?,
        ProviderLifecycleOperationArg::Remove.into(),
        purge,
    )?;
    apply_provider_lifecycle(context.manager, request, Some(plan), yes, context.format).await
}

async fn provider_data(
    arguments: ProviderDataArgs,
    action: crate::domain::secret_backend::ManagedSecretDataAction,
    context: &ProviderContext<'_>,
) -> Result<u8, MmError> {
    apply_provider_data(
        context.manager,
        context.document,
        arguments,
        action,
        context.format,
    )
    .await
}

pub(super) fn provider_name(
    value: String,
) -> Result<crate::domain::secret_backend::ProviderInstanceName, MmError> {
    crate::domain::secret_backend::ProviderInstanceName::new(value)
        .map_err(|message| MmError::Security { message })
}

async fn apply_external_setup(
    manager: &crate::app::SecretBackendManager,
    document: &crate::domain::config::ConfigDocument,
    environment: &str,
    name: crate::domain::secret_backend::ProviderInstanceName,
    options: ProviderExecutionOptions,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let instance = document
        .config
        .providers
        .get(&name)
        .ok_or_else(|| MmError::Secret {
            message: format!("provider instance '{name}' is not configured"),
        })?;
    let crate::domain::secret_backend::ProviderConfiguration::External {
        address,
        namespace,
        storage_mount,
        automation_auth_mount,
        trust_bundle,
        expected_trust_sha256,
    } = &instance.configuration
    else {
        return Err(MmError::Secret {
            message: format!("provider instance '{name}' is managed, not external"),
        });
    };
    let request = crate::domain::secret_backend::RemoteSecretBackendSetupRequest {
        instance: name.clone(),
        operation: options.operation,
        address: address.clone(),
        namespace: namespace.clone(),
        storage_mount: storage_mount.clone(),
        automation_auth_mount: automation_auth_mount.clone(),
        trust_bundle: if trust_bundle.is_absolute() {
            trust_bundle.clone()
        } else {
            document.root.join(trust_bundle)
        },
        expected_trust_sha256: expected_trust_sha256.clone(),
        project_id: document.config.project.id,
        environment: environment.to_owned(),
        locations: declared_provider_locations(&document.config, &name)?,
        allow_bootstrap: options.bootstrap,
        target: document.config.target.clone(),
        config_hash: document.source_hash.clone(),
    };
    let plan = manager.plan_remote_setup(request.clone())?;
    if format == OutputFormat::Json && !options.yes {
        return Err(MmError::Terminal {
            message: "JSON provider setup requires --yes".to_owned(),
        });
    }
    if format == OutputFormat::Human {
        output::write_remote_secret_backend_setup_plan(&plan, format)?;
    }
    if !options.yes
        && !confirm(&format!(
            "Apply provider '{}' setup plan {}?",
            name,
            output::terminal_safe(&plan.plan_hash)
        ))?
    {
        return Ok(0);
    }
    let credential =
        read_secret_material(options.stdin, &format!("{} bootstrap credential", name))?;
    let status = manager
        .apply_remote_setup(request, &plan.plan_hash, credential)
        .await?;
    output::write_provider_status(&status, format)?;
    Ok(if status.health.healthy && status.authenticated {
        0
    } else {
        3
    })
}

#[derive(Debug, Clone, Copy)]
struct ProviderExecutionOptions {
    operation: crate::domain::secret_backend::ProviderOperation,
    bootstrap: bool,
    stdin: bool,
    yes: bool,
}

pub(super) async fn apply_provider_lifecycle(
    manager: &crate::app::SecretBackendManager,
    request: crate::domain::secret_backend::ManagedSecretBackendPlanRequest,
    reviewed: Option<String>,
    yes: bool,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let preview = manager.plan_managed(request.clone())?;
    let Some(reviewed) = reviewed else {
        output::write_managed_provider_plan(&preview, format)?;
        if !yes
            && !confirm(&format!(
                "Apply provider '{}' {} plan {}?",
                preview.instance,
                preview.action.as_str(),
                preview.plan_hash
            ))?
        {
            return Ok(0);
        }
        let result = manager.apply_managed(request, &preview.plan_hash).await?;
        output::write_managed_provider_result(&result, format)?;
        return Ok(0);
    };
    if preview.plan_hash != reviewed {
        return Err(MmError::State {
            message: format!(
                "provider plan changed (expected {reviewed}, current {}); review the new plan",
                preview.plan_hash
            ),
        });
    }
    if !yes {
        return Err(MmError::Terminal {
            message: "provider apply requires --yes".to_owned(),
        });
    }
    let result = manager.apply_managed(request, &reviewed).await?;
    output::write_managed_provider_result(&result, format)?;
    Ok(0)
}

pub(super) async fn apply_provider_data(
    manager: &crate::app::SecretBackendManager,
    document: &crate::domain::config::ConfigDocument,
    arguments: ProviderDataArgs,
    action: crate::domain::secret_backend::ManagedSecretDataAction,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let name = provider_name(arguments.name)?;
    let plan = manager.plan_managed_data(
        &name,
        action,
        &arguments.file,
        &document.config.target,
        &document.source_hash,
    )?;
    let reviewed = arguments.plan.unwrap_or_default();
    if plan.plan_hash != reviewed {
        return Err(MmError::State {
            message: format!(
                "provider data plan changed (expected {reviewed}, current {}); review the new plan",
                plan.plan_hash
            ),
        });
    }
    if !arguments.yes {
        return Err(MmError::Terminal {
            message: format!("provider {} requires --yes", action.as_str()),
        });
    }
    let result = manager.apply_managed_data(&plan).await?;
    if format == OutputFormat::Json {
        output::write_json(&result)?;
    } else {
        output::write_managed_secret_data_result(&result, format)?;
    }
    Ok(0)
}
