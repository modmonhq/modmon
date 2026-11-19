use super::*;

#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ConfigResourceArg {
    Module,
    Service,
    Stack,
    Build,
    Environment,
    Binding,
    Secret,
    Ingress,
    LiveMount,
    Job,
    Schedule,
    Gate,
    User,
}

impl From<ConfigResourceArg> for crate::domain::onboarding::ConfigResourceKind {
    fn from(value: ConfigResourceArg) -> Self {
        use crate::domain::onboarding::ConfigResourceKind;
        match value {
            ConfigResourceArg::Module => ConfigResourceKind::Module,
            ConfigResourceArg::Service => ConfigResourceKind::Service,
            ConfigResourceArg::Stack => ConfigResourceKind::Stack,
            ConfigResourceArg::Build => ConfigResourceKind::Build,
            ConfigResourceArg::Environment => ConfigResourceKind::Environment,
            ConfigResourceArg::Binding => ConfigResourceKind::Binding,
            ConfigResourceArg::Secret => ConfigResourceKind::Secret,
            ConfigResourceArg::Ingress => ConfigResourceKind::Ingress,
            ConfigResourceArg::LiveMount => ConfigResourceKind::LiveMount,
            ConfigResourceArg::Job => ConfigResourceKind::Job,
            ConfigResourceArg::Schedule => ConfigResourceKind::Schedule,
            ConfigResourceArg::Gate => ConfigResourceKind::Gate,
            ConfigResourceArg::User => ConfigResourceKind::User,
        }
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum ResourceCommand {
    /// Plan adding a new named resource; apply only with --plan and --yes.
    Add(ResourceDefinitionArgs),
    /// Plan replacing one named resource definition; apply only with --plan and --yes.
    Edit(ResourceDefinitionArgs),
    /// Plan removing one named resource; apply only with --plan and --yes.
    Remove(ResourceRemoveArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommand {
    /// Validate .config/mm.yaml, its split files, and local overrides, without probing a runtime.
    Validate {
        /// Also refuse Compose features a remote host blocks, in the services the environment runs; without --environment, in every environment whose target is not `local`.
        #[arg(long)]
        for_remote: bool,
        /// With --for-remote: check against this copy of a host's host.yaml, its `allow` list and, without --environment, the environments it runs.
        #[arg(long, value_name = "FILE", requires = "for_remote")]
        host_policy: Option<PathBuf>,
    },
    /// Review typed module declarations.
    Module {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Review typed service declarations.
    Service {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Review stack declarations or compile generated artifacts.
    Stack {
        #[command(subcommand)]
        command: ConfigStackCommand,
    },
    /// Review typed image-build declarations.
    Build {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Review typed environment declarations.
    Environment {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Review typed environment bindings.
    Binding {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Review secret declarations and references, never material.
    Secret {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Review ingress declarations or inspect compiled routes.
    Ingress {
        #[command(subcommand)]
        command: ConfigIngressCommand,
    },
    /// Review typed development mount declarations.
    Mount {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Review typed finite-job declarations.
    Job {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Review typed schedule declarations.
    Schedule {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Review typed application-access declarations.
    Access {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Review who may reach remote environments (the users: key).
    User {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Print the generated JSON Schema for .config/mm.yaml, or for one split file's value.
    Schema {
        /// Top-level key whose split-file schema to print, for .config/mm/KEY.yaml.
        #[arg(value_name = "KEY", value_parser = splittable_key_parser())]
        key: Option<String>,
    },
    /// Plan moving a top-level key into its own .config/mm/KEY.yaml; apply only with --plan and --yes.
    Split(LayoutArgs),
    /// Plan moving a split key back into .config/mm.yaml; apply only with --plan and --yes.
    Join(LayoutArgs),
    /// Show which file declares each top-level key.
    Sources,
}

#[derive(Debug, Clone, Args)]
pub struct LayoutArgs {
    /// Top-level configuration key, such as stacks or environments.
    #[arg(value_name = "KEY", value_parser = splittable_key_parser())]
    key: String,
    /// Exact hash printed by an earlier review of the same move.
    #[arg(long, value_name = "HASH", requires = "yes")]
    plan: Option<String>,
    /// Apply the exact reviewed plan; never bypasses stale-file checks.
    #[arg(long, requires = "plan")]
    yes: bool,
}

fn splittable_key_parser() -> clap::builder::PossibleValuesParser {
    clap::builder::PossibleValuesParser::new(
        crate::domain::config::SPLITTABLE_CONFIG_KEYS
            .iter()
            .copied(),
    )
}

pub(super) fn run(
    command: ConfigCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    match command {
        ConfigCommand::Validate {
            for_remote,
            host_policy,
        } => run_validate(invocation, for_remote, host_policy.as_deref()),
        ConfigCommand::Schema { key: None } => {
            output::write_json(&composition::schema())?;
            Ok(0)
        }
        ConfigCommand::Schema { key: Some(key) } => {
            let schema = composition::key_schema(&key).ok_or_else(|| MmError::Environment {
                message: format!("'{key}' is not a top-level configuration key"),
            })?;
            output::write_json(&schema)?;
            Ok(0)
        }
        ConfigCommand::Split(arguments) => run_layout_command(
            crate::domain::onboarding::ConfigLayoutAction::Split,
            arguments,
            &invocation.start,
            invocation.format,
        ),
        ConfigCommand::Join(arguments) => run_layout_command(
            crate::domain::onboarding::ConfigLayoutAction::Join,
            arguments,
            &invocation.start,
            invocation.format,
        ),
        ConfigCommand::Sources => {
            let document = composition::project_documents().load_from(&invocation.start)?;
            output::write_config_sources(&document, invocation.format)?;
            Ok(0)
        }
        ConfigCommand::Stack {
            command: ConfigStackCommand::Compile,
        } => run_stack_compile(&invocation.selection, invocation.home(), invocation.format),
        ConfigCommand::Ingress {
            command: ConfigIngressCommand::List,
        } => run_ingress_inspection(
            &invocation.selection,
            invocation.home(),
            invocation.format,
            None,
        ),
        ConfigCommand::Ingress {
            command: ConfigIngressCommand::Explain { service },
        } => run_ingress_inspection(
            &invocation.selection,
            invocation.home(),
            invocation.format,
            Some(&service),
        ),
        command => {
            let (kind, resource) = resource_command(command)?;
            run_resource_command(kind, resource, &invocation.start, invocation.format)
        }
    }
}

fn run_validate(
    invocation: &super::dispatch::Invocation,
    for_remote: bool,
    host_policy: Option<&std::path::Path>,
) -> Result<u8, MmError> {
    let mut validation = crate::app::Application::read_only(
        composition::project_documents(),
        composition::environment_sources(),
        Arc::clone(&invocation.runtime),
        Arc::clone(&invocation.clock),
    )
    .validate(&invocation.selection)?;
    if for_remote {
        let document = composition::project_documents().load_from(&invocation.start)?;
        let policy = host_policy
            .map(|path| composition::host_policy_file(path).map(|policy| (path, policy)))
            .transpose()?;
        for diagnostic in crate::app::remote_guardrail_diagnostics(
            &document,
            invocation.selection.environment.as_deref(),
            policy.as_ref().map(|(path, policy)| (*path, policy)),
        )? {
            validation.validation.push(diagnostic);
        }
    }
    output::write_validation(&validation, invocation.format)?;
    Ok(if validation.validation.is_valid() {
        0
    } else {
        2
    })
}

fn resource_command(
    command: ConfigCommand,
) -> Result<
    (
        crate::domain::onboarding::ConfigResourceKind,
        ResourceCommand,
    ),
    MmError,
> {
    use crate::domain::onboarding::ConfigResourceKind as Kind;
    let pair = match command {
        ConfigCommand::Module { command } => (Kind::Module, command),
        ConfigCommand::Service { command } => (Kind::Service, command),
        ConfigCommand::Build { command } => (Kind::Build, command),
        ConfigCommand::Environment { command } => (Kind::Environment, command),
        ConfigCommand::Binding { command } => (Kind::Binding, command),
        ConfigCommand::Secret { command } => (Kind::Secret, command),
        ConfigCommand::Mount { command } => (Kind::LiveMount, command),
        ConfigCommand::Job { command } => (Kind::Job, command),
        ConfigCommand::Schedule { command } => (Kind::Schedule, command),
        ConfigCommand::Access { command } => (Kind::Gate, command),
        ConfigCommand::User { command } => (Kind::User, command),
        ConfigCommand::Stack { command } => (Kind::Stack, stack_resource(command)?),
        ConfigCommand::Ingress { command } => (Kind::Ingress, ingress_resource(command)?),
        ConfigCommand::Validate { .. }
        | ConfigCommand::Schema { .. }
        | ConfigCommand::Split(_)
        | ConfigCommand::Join(_)
        | ConfigCommand::Sources => {
            return Err(MmError::State {
                message: "configuration command was routed to the wrong capability".to_owned(),
            });
        }
    };
    Ok(pair)
}

fn stack_resource(command: ConfigStackCommand) -> Result<ResourceCommand, MmError> {
    match command {
        ConfigStackCommand::Add(args) => Ok(ResourceCommand::Add(args)),
        ConfigStackCommand::Edit(args) => Ok(ResourceCommand::Edit(args)),
        ConfigStackCommand::Remove(args) => Ok(ResourceCommand::Remove(args)),
        ConfigStackCommand::Compile => Err(MmError::State {
            message: "stack compilation was routed as a mutation".to_owned(),
        }),
    }
}

fn ingress_resource(command: ConfigIngressCommand) -> Result<ResourceCommand, MmError> {
    match command {
        ConfigIngressCommand::Add(args) => Ok(ResourceCommand::Add(args)),
        ConfigIngressCommand::Edit(args) => Ok(ResourceCommand::Edit(args)),
        ConfigIngressCommand::Remove(args) => Ok(ResourceCommand::Remove(args)),
        ConfigIngressCommand::List | ConfigIngressCommand::Explain { .. } => Err(MmError::State {
            message: "ingress inspection was routed as a mutation".to_owned(),
        }),
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigStackCommand {
    /// Plan adding a stack declaration.
    Add(ResourceDefinitionArgs),
    /// Plan replacing a stack declaration.
    Edit(ResourceDefinitionArgs),
    /// Plan removing a stack declaration.
    Remove(ResourceRemoveArgs),
    /// Publish deterministic Compose manifests and generated overlays.
    Compile,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigIngressCommand {
    /// Plan adding an ingress declaration.
    Add(ResourceDefinitionArgs),
    /// Plan replacing an ingress declaration.
    Edit(ResourceDefinitionArgs),
    /// Plan removing an ingress declaration.
    Remove(ResourceRemoveArgs),
    /// List every service URL or the reason it was not routed.
    List,
    /// Explain ingress selection for one service name.
    Explain {
        #[arg(value_name = "SERVICE")]
        service: String,
    },
}

#[derive(Debug, Clone, Args)]
pub struct ResourceDefinitionArgs {
    /// Portable resource name.
    #[arg(value_name = "NAME")]
    name: String,
    /// Owning module, stack, or environment when the resource is nested.
    #[arg(long, value_name = "NAME")]
    parent: Option<String>,
    /// Public JSON or YAML definition. Secret plaintext is rejected.
    #[arg(long, value_name = "DOCUMENT")]
    definition: String,
    /// Exact hash printed by an earlier review with identical arguments.
    #[arg(long, value_name = "HASH", requires = "yes")]
    plan: Option<String>,
    /// Apply the exact reviewed plan; never bypasses stale-file checks.
    #[arg(long, requires = "plan")]
    yes: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ResourceRemoveArgs {
    /// Portable resource name.
    #[arg(value_name = "NAME")]
    name: String,
    /// Owning module, stack, or environment when the resource is nested.
    #[arg(long, value_name = "NAME")]
    parent: Option<String>,
    /// Exact hash printed by an earlier removal review.
    #[arg(long, value_name = "HASH", requires = "yes")]
    plan: Option<String>,
    /// Apply the exact reviewed removal; never bypasses stale-file checks.
    #[arg(long, requires = "plan")]
    yes: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum NodeCommand {
    /// Plan or apply bootstrap for the resolved target.
    Setup {
        /// Exact hash printed by the node setup review.
        #[arg(long, value_name = "HASH", requires = "yes")]
        plan: Option<String>,
        /// Apply the reviewed Ansible plan, including disclosed remote mutation.
        #[arg(long, requires = "plan")]
        yes: bool,
    },
}

pub(super) fn run_resource_command(
    resource: crate::domain::onboarding::ConfigResourceKind,
    command: ResourceCommand,
    start: &std::path::Path,
    format: OutputFormat,
) -> Result<u8, MmError> {
    use crate::domain::onboarding::{ConfigMutationAction, ConfigMutationRequest};

    let (request, reviewed_plan, yes) = match command {
        ResourceCommand::Add(arguments) => (
            ConfigMutationRequest {
                action: ConfigMutationAction::Add,
                resource,
                name: arguments.name,
                parent: arguments.parent,
                definition: Some(parse_public_definition(&arguments.definition)?),
            },
            arguments.plan,
            arguments.yes,
        ),
        ResourceCommand::Edit(arguments) => (
            ConfigMutationRequest {
                action: ConfigMutationAction::Edit,
                resource,
                name: arguments.name,
                parent: arguments.parent,
                definition: Some(parse_public_definition(&arguments.definition)?),
            },
            arguments.plan,
            arguments.yes,
        ),
        ResourceCommand::Remove(arguments) => (
            ConfigMutationRequest {
                action: ConfigMutationAction::Remove,
                resource,
                name: arguments.name,
                parent: arguments.parent,
                definition: None,
            },
            arguments.plan,
            arguments.yes,
        ),
    };
    let service = composition::onboarding_service();
    let plan = service.plan_mutation(start, request.clone())?;
    let Some(reviewed_plan) = reviewed_plan else {
        output::write_config_mutation_plan(&plan, format)?;
        return Ok(0);
    };
    if !yes {
        return Err(MmError::Terminal {
            message: "configuration mutation requires --yes with the reviewed --plan hash"
                .to_owned(),
        });
    }
    if plan.plan_hash != reviewed_plan {
        return Err(MmError::State {
            message: format!(
                "configuration plan changed (expected {reviewed_plan}, current {}); review the new plan",
                plan.plan_hash
            ),
        });
    }
    if format == OutputFormat::Human {
        output::write_config_mutation_plan(&plan, format)?;
    }
    let result = service.apply_mutation(start, request, &reviewed_plan)?;
    output::write_config_mutation_result(&result, format)?;
    Ok(0)
}

fn run_layout_command(
    action: crate::domain::onboarding::ConfigLayoutAction,
    arguments: LayoutArgs,
    start: &std::path::Path,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let request = crate::domain::onboarding::ConfigLayoutRequest {
        action,
        key: arguments.key,
    };
    let service = composition::onboarding_service();
    let plan = service.plan_layout(start, request.clone())?;
    let Some(reviewed_plan) = arguments.plan else {
        output::write_config_layout_plan(&plan, format)?;
        return Ok(0);
    };
    if !arguments.yes {
        return Err(MmError::Terminal {
            message: "configuration layout change requires --yes with the reviewed --plan hash"
                .to_owned(),
        });
    }
    if plan.plan_hash != reviewed_plan {
        return Err(MmError::State {
            message: format!(
                "configuration plan changed (expected {reviewed_plan}, current {}); review the new plan",
                plan.plan_hash
            ),
        });
    }
    let result = service.apply_layout(start, request, &reviewed_plan)?;
    output::write_config_layout_result(&result, format)?;
    Ok(0)
}

pub(super) fn parse_public_definition(source: &str) -> Result<serde_json::Value, MmError> {
    serde_yaml_ng::from_str(source).map_err(|error| MmError::Environment {
        message: format!("definition is not valid JSON or YAML: {error}"),
    })
}

pub(super) fn run_stack_compile(
    selection: &WorkspaceSelection,
    explicit_home: Option<&std::path::Path>,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let report = composition::stack_manager(explicit_home)?.compile(selection)?;
    output::write_stack_compilation(&report, format)?;
    Ok(0)
}

pub(super) fn run_ingress_inspection(
    selection: &WorkspaceSelection,
    explicit_home: Option<&std::path::Path>,
    format: OutputFormat,
    service: Option<&str>,
) -> Result<u8, MmError> {
    let report = composition::stack_manager(explicit_home)?.compile(selection)?;
    let statuses = match service {
        None => report.ingress,
        Some(service) => {
            let selected = report
                .ingress
                .into_iter()
                .filter(|status| status.service == service)
                .collect::<Vec<_>>();
            if selected.is_empty() {
                return Err(MmError::Environment {
                    message: format!("service '{service}' is not configured"),
                });
            }
            selected
        }
    };
    output::write_ingress_statuses(&statuses, format)?;
    Ok(0)
}
