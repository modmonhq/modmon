use super::*;

#[derive(Debug, Clone, Subcommand)]
pub enum GateCommand {
    /// Review or create a password-cap policy using the managed local Cap service.
    Add(GateAddArgs),
    /// Review or remove an unattached policy and its owned secret material.
    Remove(GateRemoveArgs),
    /// Show redacted policy metadata.
    Show {
        /// Optional policy name; omit to show every policy in the selected scope.
        #[arg(value_name = "POLICY")]
        policy: Option<String>,
    },
    /// Review or attach one policy middleware to an owned router.
    Attach(GateRouteArgs),
    /// Review or detach one policy middleware from an owned router.
    Detach(GateRouteArgs),
    /// Manage application-access principals (separate from deployment RBAC users).
    User {
        #[command(subcommand)]
        command: GateUserCommand,
    },
    /// List, revoke, or rotate opaque sessions.
    Session {
        #[command(subcommand)]
        command: GateSessionCommand,
    },
}

pub type AccessCommand = GateCommand;

#[derive(Debug, Clone, Args)]
pub struct GateAddArgs {
    /// Unique policy name in the selected project and environment.
    #[arg(value_name = "POLICY")]
    name: String,
    /// Driver evaluation mode for this policy.
    #[arg(long, value_enum, default_value_t = GateModeArg::Any)]
    mode: GateModeArg,
    /// Session lifetime in days (default seven, maximum thirty).
    #[arg(long, default_value_t = 7, value_parser = clap::value_parser!(u64).range(1..=30))]
    session_days: u64,
    /// Secret storage provider for generated gate keys and pepper material.
    #[arg(long, default_value = "age")]
    secret_provider: String,
    /// Exact hash printed by the read-only policy review.
    #[arg(long, value_name = "HASH", requires = "yes")]
    plan: Option<String>,
    /// Apply the exact reviewed policy plan.
    #[arg(long, requires = "plan")]
    yes: bool,
}

#[derive(Debug, Clone, Args)]
pub struct GateRemoveArgs {
    /// Policy to remove.
    #[arg(value_name = "POLICY")]
    name: String,
    /// Exact hash printed by the read-only removal review.
    #[arg(long, value_name = "HASH", requires = "yes")]
    plan: Option<String>,
    /// Apply the exact reviewed removal plan.
    #[arg(long, requires = "plan")]
    yes: bool,
}

#[derive(Debug, Clone, Args)]
pub struct GateRouteArgs {
    /// Policy to attach or detach.
    #[arg(value_name = "POLICY")]
    policy: String,
    /// Owned ingress router to update.
    #[arg(value_name = "ROUTER")]
    router: String,
    /// Target that owns the ingress router.
    #[arg(long, value_name = "UUID")]
    target_id: uuid::Uuid,
    /// Exact hash printed by the read-only route review.
    #[arg(long, value_name = "HASH", requires = "yes")]
    plan: Option<String>,
    /// Apply the exact reviewed route plan.
    #[arg(long, requires = "plan")]
    yes: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum GateUserCommand {
    /// Review or create an application-access user with a hidden password.
    Add(GateUserPasswordArgs),
    /// Review or disable an application-access user and revoke active sessions.
    Disable(GateUserMutationArgs),
    /// Review or re-enable an application-access user.
    Enable(GateUserMutationArgs),
    /// Review or replace an application-access user's hidden password.
    #[command(name = "password-reset")]
    PasswordReset(GateUserPasswordArgs),
    /// Review or remove an application-access user and revoke active sessions.
    Remove(GateUserMutationArgs),
    /// Show redacted application-user metadata for one policy.
    List {
        /// Policy whose application users will be listed.
        #[arg(value_name = "POLICY")]
        policy: String,
    },
}

#[derive(Debug, Clone, Args)]
pub struct GateUserPasswordArgs {
    /// Policy that owns the application user.
    #[arg(value_name = "POLICY")]
    policy: String,
    /// Application-access username.
    #[arg(value_name = "USERNAME")]
    username: String,
    /// Read the password from standard input instead of prompting.
    #[arg(long)]
    stdin: bool,
    /// Exact hash printed by the read-only user review.
    #[arg(long, value_name = "HASH", requires = "yes")]
    plan: Option<String>,
    /// Apply the exact reviewed user plan.
    #[arg(long, requires = "plan")]
    yes: bool,
}
#[derive(Debug, Clone, Args)]
pub struct GateUserMutationArgs {
    /// Policy that owns the application user.
    #[arg(value_name = "POLICY")]
    policy: String,
    /// Application-access username.
    #[arg(value_name = "USERNAME")]
    username: String,
    /// Exact hash printed by the read-only user review.
    #[arg(long, value_name = "HASH", requires = "yes")]
    plan: Option<String>,
    /// Apply the exact reviewed user plan.
    #[arg(long, requires = "plan")]
    yes: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum GateSessionCommand {
    /// Show redacted active-session metadata for one policy.
    List {
        /// Policy whose active sessions will be listed.
        #[arg(value_name = "POLICY")]
        policy: String,
    },
    /// Review or revoke one active session.
    Revoke {
        /// Policy that owns the session.
        #[arg(value_name = "POLICY")]
        policy: String,
        /// Opaque session identifier to revoke.
        #[arg(value_name = "SESSION_ID")]
        session_id: uuid::Uuid,
        /// Exact hash printed by the read-only revocation review.
        #[arg(long, value_name = "HASH", requires = "yes")]
        plan: Option<String>,
        /// Apply the exact reviewed revocation plan.
        #[arg(long, requires = "plan")]
        yes: bool,
    },
    /// Review or rotate a policy's session encryption key.
    #[command(name = "rotate-key")]
    RotateKey {
        /// Policy whose session encryption key will be rotated.
        #[arg(value_name = "POLICY")]
        policy: String,
        /// Exact hash printed by the read-only key-rotation review.
        #[arg(long, value_name = "HASH", requires = "yes")]
        plan: Option<String>,
        /// Apply the exact reviewed key-rotation plan.
        #[arg(long, requires = "plan")]
        yes: bool,
    },
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
#[value(rename_all = "lower")]
pub enum GateModeArg {
    #[default]
    Any,
    All,
}
impl From<GateModeArg> for crate::domain::gate::GatePolicyMode {
    fn from(value: GateModeArg) -> Self {
        match value {
            GateModeArg::Any => Self::Any,
            GateModeArg::All => Self::All,
        }
    }
}

pub(super) async fn run(
    command: AccessCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    run_gate_command(
        command,
        &invocation.start,
        invocation.selection.environment.clone(),
        invocation.home(),
        invocation.format,
        Arc::clone(&invocation.clock),
    )
    .await
}

async fn run_gate_command(
    command: GateCommand,
    start: &std::path::Path,
    requested_environment: Option<String>,
    explicit_home: Option<&std::path::Path>,
    format: OutputFormat,
    clock: Arc<dyn Clock>,
) -> Result<u8, MmError> {
    let document = composition::project_documents().load_from(start)?;
    let environment =
        crate::domain::config::resolve_environment(&document, requested_environment.as_deref())?;
    let services = composition::gate_services(start, explicit_home, clock)?;
    dispatch_gate(
        command,
        &GateContext {
            services: &services,
            project_id: document.config.project.id,
            environment: &environment,
            format,
        },
    )
    .await
}

struct GateContext<'a> {
    services: &'a composition::GateServices,
    project_id: crate::domain::workspace::ProjectId,
    environment: &'a str,
    format: OutputFormat,
}

async fn dispatch_gate(command: GateCommand, context: &GateContext<'_>) -> Result<u8, MmError> {
    match command {
        GateCommand::Add(args) => add_policy(args, context).await?,
        GateCommand::Remove(args) => remove_policy(args, context).await?,
        GateCommand::Show { policy } => output::write_gate_policies(
            &context.services.manager.show(
                context.project_id,
                context.environment,
                policy.as_deref(),
            )?,
            context.format,
        )?,
        GateCommand::Attach(args) => mutate_route(args, true, context).await?,
        GateCommand::Detach(args) => mutate_route(args, false, context).await?,
        GateCommand::User { command } => run_user(command, context).await?,
        GateCommand::Session { command } => run_session(command, context).await?,
    }
    Ok(0)
}

async fn add_policy(args: GateAddArgs, context: &GateContext<'_>) -> Result<(), MmError> {
    let request = crate::app::AddGatePolicyRequest {
        project_id: context.project_id,
        environment: context.environment.to_owned(),
        name: crate::domain::gate::GatePolicyName::new(args.name)
            .map_err(|message| MmError::Security { message })?,
        mode: args.mode.into(),
        session_ttl_seconds: args.session_days.saturating_mul(24 * 60 * 60),
        secret_provider: args.secret_provider,
    };
    let preview = context.services.manager.plan_add_policy(&request)?;
    let Some(reviewed) = args.plan else {
        return output::write_gate_plan(&preview, context.format);
    };
    if !args.yes {
        return Err(MmError::Terminal {
            message: "gate creation requires --yes with the reviewed plan hash".to_owned(),
        });
    }
    context
        .services
        .initialize_selected_backend(&request.secret_provider)?;
    let policy = context
        .services
        .manager
        .add_policy_reviewed(request, &reviewed)
        .await?;
    output::write_gate_policies(&[policy], context.format)
}

async fn remove_policy(args: GateRemoveArgs, context: &GateContext<'_>) -> Result<(), MmError> {
    let preview = context.services.manager.plan_remove_policy(
        context.project_id,
        context.environment,
        &args.name,
    )?;
    let Some(reviewed) = args.plan else {
        return output::write_gate_plan(&preview, context.format);
    };
    if !args.yes {
        return Err(MmError::Terminal {
            message: "gate removal requires --yes with the reviewed plan hash".to_owned(),
        });
    }
    context
        .services
        .manager
        .remove_policy_reviewed(
            context.project_id,
            context.environment,
            &args.name,
            &reviewed,
        )
        .await?;
    output::write_gate_applied(&preview, context.format)
}

async fn mutate_route(
    args: GateRouteArgs,
    attach: bool,
    context: &GateContext<'_>,
) -> Result<(), MmError> {
    let target_id = crate::domain::target::TargetId(args.target_id);
    let preview = context.services.manager.plan_attach(
        context.project_id,
        context.environment,
        &args.policy,
        target_id,
        &args.router,
        attach,
    )?;
    let Some(reviewed) = args.plan else {
        return output::write_gate_plan(&preview, context.format);
    };
    if !args.yes {
        return Err(MmError::Terminal {
            message: "gate route mutation requires --yes with the reviewed plan hash".to_owned(),
        });
    }
    let attachment = context
        .services
        .manager
        .apply_attach_reviewed(crate::app::ApplyGateRouteRequest {
            project_id: context.project_id,
            environment: context.environment,
            policy_name: &args.policy,
            target_id,
            router_name: &args.router,
            attach,
            reviewed_plan_hash: &reviewed,
        })
        .await?;
    output::write_gate_attachments(&[attachment], context.format)
}

async fn run_user(command: GateUserCommand, context: &GateContext<'_>) -> Result<(), MmError> {
    use crate::domain::gate::GateMutationAction;
    match command {
        GateUserCommand::List { policy } => output::write_gate_users(
            &context.services.manager.list_users(
                context.project_id,
                context.environment,
                &policy,
            )?,
            context.format,
        ),
        GateUserCommand::Add(args) => {
            apply_gate_user_password(GateMutationAction::AddUser, args, context).await
        }
        GateUserCommand::PasswordReset(args) => {
            apply_gate_user_password(GateMutationAction::ResetPassword, args, context).await
        }
        GateUserCommand::Disable(args) => {
            apply_gate_user_metadata(
                &context.services.manager,
                GateMutationAction::DisableUser,
                context.project_id,
                context.environment,
                args,
                context.format,
            )
            .await
        }
        GateUserCommand::Enable(args) => {
            apply_gate_user_metadata(
                &context.services.manager,
                GateMutationAction::EnableUser,
                context.project_id,
                context.environment,
                args,
                context.format,
            )
            .await
        }
        GateUserCommand::Remove(args) => {
            apply_gate_user_metadata(
                &context.services.manager,
                GateMutationAction::RemoveUser,
                context.project_id,
                context.environment,
                args,
                context.format,
            )
            .await
        }
    }
}

async fn apply_gate_user_password(
    action: crate::domain::gate::GateMutationAction,
    args: GateUserPasswordArgs,
    context: &GateContext<'_>,
) -> Result<(), MmError> {
    let preview = context.services.manager.plan_user(
        action,
        context.project_id,
        context.environment,
        &args.policy,
        &args.username,
    )?;
    let Some(reviewed) = args.plan else {
        return output::write_gate_plan(&preview, context.format);
    };
    if !args.yes {
        return Err(MmError::Terminal {
            message: "gate user password mutation requires --yes with the reviewed plan hash"
                .to_owned(),
        });
    }
    let password = read_secret_material(args.stdin, "Gate user password")?;
    let mutation = match action {
        crate::domain::gate::GateMutationAction::AddUser => {
            crate::app::GateUserMutation::Add(password)
        }
        crate::domain::gate::GateMutationAction::ResetPassword => {
            crate::app::GateUserMutation::ResetPassword(password)
        }
        _ => {
            return Err(MmError::Security {
                message: "invalid gate password mutation".to_owned(),
            });
        }
    };
    let user = context
        .services
        .manager
        .apply_user_reviewed(crate::app::ApplyGateUserRequest {
            project_id: context.project_id,
            environment: context.environment,
            policy_name: &args.policy,
            username: &args.username,
            reviewed_plan_hash: &reviewed,
            mutation,
        })
        .await?;
    output::write_gate_users(&[user], context.format)
}

async fn run_session(
    command: GateSessionCommand,
    context: &GateContext<'_>,
) -> Result<(), MmError> {
    match command {
        GateSessionCommand::List { policy } => output::write_gate_sessions(
            &context.services.manager.list_sessions(
                context.project_id,
                context.environment,
                &policy,
            )?,
            context.format,
        ),
        GateSessionCommand::Revoke {
            policy,
            session_id,
            plan,
            yes,
        } => revoke_session(&policy, session_id, plan, yes, context),
        GateSessionCommand::RotateKey { policy, plan, yes } => {
            rotate_session_key(&policy, plan, yes, context).await
        }
    }
}

fn revoke_session(
    policy: &str,
    session_id: uuid::Uuid,
    plan: Option<String>,
    yes: bool,
    context: &GateContext<'_>,
) -> Result<(), MmError> {
    let preview = context.services.manager.plan_revoke_session(
        context.project_id,
        context.environment,
        policy,
        session_id,
    )?;
    let Some(reviewed) = plan else {
        return output::write_gate_plan(&preview, context.format);
    };
    if !yes {
        return Err(MmError::Terminal {
            message: "session revocation requires --yes with the reviewed plan hash".to_owned(),
        });
    }
    context.services.manager.revoke_session_reviewed(
        context.project_id,
        context.environment,
        policy,
        session_id,
        &reviewed,
    )?;
    output::write_gate_applied(&preview, context.format)
}

async fn rotate_session_key(
    policy: &str,
    plan: Option<String>,
    yes: bool,
    context: &GateContext<'_>,
) -> Result<(), MmError> {
    let preview = context.services.manager.plan_rotate_session_key(
        context.project_id,
        context.environment,
        policy,
    )?;
    let Some(reviewed) = plan else {
        return output::write_gate_plan(&preview, context.format);
    };
    if !yes {
        return Err(MmError::Terminal {
            message: "session key rotation requires --yes with the reviewed plan hash".to_owned(),
        });
    }
    let policy = context
        .services
        .manager
        .rotate_session_key(context.project_id, context.environment, policy, &reviewed)
        .await?;
    output::write_gate_policies(&[policy], context.format)
}

pub(super) async fn apply_gate_user_metadata(
    manager: &crate::app::GateManager,
    action: crate::domain::gate::GateMutationAction,
    project_id: crate::domain::workspace::ProjectId,
    environment: &str,
    args: GateUserMutationArgs,
    format: OutputFormat,
) -> Result<(), MmError> {
    let preview = manager.plan_user(
        action,
        project_id,
        environment,
        &args.policy,
        &args.username,
    )?;
    let Some(reviewed) = args.plan else {
        return output::write_gate_plan(&preview, format);
    };
    if !args.yes {
        return Err(MmError::Terminal {
            message: "gate user mutation requires --yes with the reviewed plan hash".to_owned(),
        });
    }
    let mutation = match action {
        crate::domain::gate::GateMutationAction::DisableUser => {
            crate::app::GateUserMutation::Disable
        }
        crate::domain::gate::GateMutationAction::EnableUser => crate::app::GateUserMutation::Enable,
        crate::domain::gate::GateMutationAction::RemoveUser => crate::app::GateUserMutation::Remove,
        _ => {
            return Err(MmError::Security {
                message: "invalid gate metadata mutation".to_owned(),
            });
        }
    };
    let user = manager
        .apply_user_reviewed(crate::app::ApplyGateUserRequest {
            project_id,
            environment,
            policy_name: &args.policy,
            username: &args.username,
            reviewed_plan_hash: &reviewed,
            mutation,
        })
        .await?;
    output::write_gate_users(&[user], format)
}
