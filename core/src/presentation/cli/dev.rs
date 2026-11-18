use super::*;

mod command;
mod mounts;
mod units;
pub(super) use command::run;
pub use mounts::{DevMountCommand, DevMountOptions};

#[derive(Debug, Clone, Subcommand)]
pub enum SetupCommand {
    /// Inspect and deploy an existing clone using its configured development environment.
    #[command(
        after_help = "Examples:\n  mm dev setup project\n  mm dev setup project --plan WORKFLOW_HASH --deploy-plan DEPLOY_HASH --yes\n\nThe first command is read-only. Apply requires both reviewed hashes. Secret plaintext is entered separately with `mm secret set`; it is never displayed. Exit codes: 0 success; 1 stale/local state; 2 missing values/configuration; 3 runtime/security failure."
    )]
    Project {
        /// Hash printed by the read-only guided workflow review.
        #[arg(long, value_name = "HASH")]
        plan: Option<String>,
        /// Hash printed by `mm deploy plan` for the same environment and target.
        #[arg(long, value_name = "HASH", requires = "plan")]
        deploy_plan: Option<String>,
        /// Apply reviewed local dependencies and deployment without prompting.
        #[arg(long, requires_all = ["plan", "deploy_plan"])]
        yes: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum GuidedJourneyArg {
    GlobalSetup,
    NewProject,
    ExistingProject,
    NodeSetup,
    ProjectEditing,
}

impl From<GuidedJourneyArg> for crate::domain::onboarding::GuidedJourney {
    fn from(value: GuidedJourneyArg) -> Self {
        match value {
            GuidedJourneyArg::GlobalSetup => Self::GlobalSetup,
            GuidedJourneyArg::NewProject => Self::NewProject,
            GuidedJourneyArg::ExistingProject => Self::ExistingProject,
            GuidedJourneyArg::NodeSetup => Self::NodeSetup,
            GuidedJourneyArg::ProjectEditing => Self::ProjectEditing,
        }
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum DevCommand {
    /// Set up local trust, ingress, Mailpit, Cap, dashboard, and project routes.
    Setup {
        #[command(subcommand)]
        command: Option<SetupCommand>,
        /// Generate certificates without trusting the root.
        #[arg(long)]
        no_trust: bool,
        /// Use the privileged system trust store on macOS or Windows.
        #[arg(long)]
        system: bool,
        /// Review and install the shared local service stack.
        #[arg(long = "services")]
        install_services: bool,
        /// Publish SMTP on this explicit loopback port.
        #[arg(long, value_name = "PORT", requires = "install_services")]
        smtp_port: Option<u16>,
        /// Apply the displayed service plan without prompting.
        #[arg(long, requires = "install_services")]
        yes: bool,
    },
    /// Review or apply the local development deployment.
    Up {
        /// Turn an optional module, service or job on for this run only;
        /// repeat for more.
        #[arg(long = "with", value_name = "UNIT")]
        with: Vec<String>,
        /// Hash printed by the deployment review.
        #[arg(long, value_name = "HASH", requires = "yes")]
        plan: Option<String>,
        /// Apply the exact reviewed plan without prompting.
        #[arg(long, requires = "plan")]
        yes: bool,
    },
    /// Turn optional modules, services or jobs on for this machine; `mm dev
    /// up` starts them.
    #[command(
        after_help = "A unit is a module, a service (or MODULE/SERVICE) or a job with `development: optional`. What it needs through Compose `depends_on` comes on with it. The choice is kept in the gitignored .config/mm.overrides.yaml."
    )]
    Enable {
        /// A module, service (or MODULE/SERVICE) or job with `development: optional`.
        #[arg(value_name = "UNIT", required = true)]
        units: Vec<String>,
    },
    /// Turn optional units off for this machine, stop them and remove their
    /// routes; their volumes stay.
    Disable {
        /// A unit `mm dev enable` turned on.
        #[arg(value_name = "UNIT", required = true)]
        units: Vec<String>,
        /// Apply the deployment that stops them without prompting.
        #[arg(long)]
        yes: bool,
    },
    /// Stop the project's containers; volumes, routes, and overrides are kept.
    Down {
        /// Confirm stopping every stack of the selected environment.
        #[arg(long)]
        yes: bool,
    },
    /// Show local project and runtime status.
    Status,
    /// Inspect and maintain local HTTPS certificates and trust.
    Tls {
        /// Use the privileged system trust store on macOS or Windows.
        #[arg(long, global = true)]
        system: bool,
        #[command(subcommand)]
        command: TlsCommand,
    },
    /// Manage the installation-wide local service stack.
    Services {
        #[command(subcommand)]
        command: ServicesCommand,
    },
    /// Serve the local developer dashboard.
    Dashboard {
        #[command(subcommand)]
        command: DashboardCommand,
    },
    /// Discover, review, and disable source-code mounts.
    Mounts {
        #[command(subcommand)]
        command: DevMountCommand,
    },
    /// Print a configured local service URL.
    Open {
        #[arg(value_name = "SERVICE")]
        service: Option<String>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum CaCommand {
    /// Show whether the local CA is installed and which hosts it has issued.
    Status,
    /// Generate the installation-wide local CA if it does not already exist.
    Install,
    /// Replace the CA key and reissue every previously issued host certificate.
    Rotate,
    /// Remove the local CA key material and issued certificates.
    Uninstall,
}

#[derive(Debug, Clone, Subcommand)]
pub enum TlsCommand {
    /// Query CA files, leaf renewal, and the actual platform trust store.
    Status,
    /// Restore trust, renew stale leaves, and finish an interrupted rotation.
    Repair,
    /// Rotate the root with staged trust overlap and reissued leaves.
    Rotate,
    /// Remove only exact mm-owned trust entries and local private material.
    Uninstall,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ServicesCommand {
    /// Publish a deterministic reviewed lifecycle plan.
    Plan {
        /// Lifecycle operation represented by the plan.
        #[arg(long, value_enum, default_value_t = ServicesOperationArg::Apply)]
        operation: ServicesOperationArg,
        /// Publish SMTP on this loopback port for host-native applications.
        #[arg(long, value_name = "PORT")]
        smtp_port: Option<u16>,
        /// Include persistent data deletion in a remove plan.
        #[arg(long)]
        purge: bool,
        /// Prepare conditional Cap and Valkey components before creating the first gate policy.
        #[arg(long)]
        gate: bool,
    },
    /// Apply an exact install/convergence plan hash.
    Apply {
        /// Hash printed by `mm dev services plan` for an apply operation.
        #[arg(long, value_name = "HASH")]
        plan: String,
        /// Publish SMTP on this loopback port; must match the reviewed plan.
        #[arg(long, value_name = "PORT")]
        smtp_port: Option<u16>,
        /// Include the reviewed first-policy Cap and Valkey preparation.
        #[arg(long)]
        gate: bool,
        /// Skip the service mutation confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Show component health, ownership, image, route, certificate, and drift status.
    Status,
    /// Apply an exact version upgrade plan hash.
    Upgrade {
        /// Hash printed by `mm dev services plan --operation upgrade`.
        #[arg(long, value_name = "HASH")]
        plan: String,
        /// Change the loopback SMTP port as part of the reviewed upgrade.
        #[arg(long, value_name = "PORT")]
        smtp_port: Option<u16>,
        /// Include the reviewed Cap and Valkey components.
        #[arg(long)]
        gate: bool,
        /// Skip the service mutation confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Recover an interrupted operation or reconcile drift from an exact plan.
    Repair {
        /// Hash printed by `mm dev services plan --operation repair`.
        #[arg(long, value_name = "HASH")]
        plan: String,
        /// Skip the service mutation confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Remove only mm-owned runtime resources using an exact reviewed plan.
    Remove {
        /// Hash printed by `mm dev services plan --operation remove`.
        #[arg(long, value_name = "HASH")]
        plan: String,
        /// Delete retained Mailpit and ingress volumes; must match the reviewed plan.
        #[arg(long)]
        purge: bool,
        /// Skip the service mutation confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum DashboardCommand {
    /// Run the ingress-authenticated dashboard backend.
    Serve {
        /// Control listener used by the managed ingress route.
        #[arg(long, default_value = "0.0.0.0:42831", value_name = "ADDRESS")]
        listen: std::net::SocketAddr,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ServicesOperationArg {
    Apply,
    Upgrade,
    Repair,
    Remove,
}

impl From<ServicesOperationArg> for crate::domain::service_stack::ServiceStackAction {
    fn from(value: ServicesOperationArg) -> Self {
        match value {
            ServicesOperationArg::Apply => Self::Apply,
            ServicesOperationArg::Upgrade => Self::Upgrade,
            ServicesOperationArg::Repair => Self::Repair,
            ServicesOperationArg::Remove => Self::Remove,
        }
    }
}

struct ProjectSetupRequest<'a> {
    invocation: &'a super::dispatch::Invocation,
    reviewed_workflow: Option<&'a str>,
    reviewed_deployment: Option<&'a str>,
    yes: bool,
}

async fn run_project_setup(request: ProjectSetupRequest<'_>) -> Result<u8, MmError> {
    let ProjectSetupRequest {
        invocation,
        reviewed_workflow,
        reviewed_deployment,
        yes,
    } = request;
    let start = &invocation.start;
    let selection = &invocation.selection;
    let explicit_home = invocation.home();
    let format = invocation.format;
    let request = crate::domain::onboarding::WorkflowRequest {
        journey: crate::domain::onboarding::GuidedJourney::ExistingProject,
        root: start.to_path_buf(),
        environment: selection.environment.clone(),
        target: selection.target.clone(),
        mutation: None,
    };
    let guidance = composition::onboarding_service().plan_workflow(request)?;
    let Some(reviewed_workflow) = reviewed_workflow else {
        output::write_workflow_review(&guidance, format)?;
        return Ok(0);
    };
    if guidance.plan_hash != reviewed_workflow {
        return Err(MmError::State {
            message: format!(
                "project setup plan changed (expected {reviewed_workflow}, current {}); review the new plan",
                guidance.plan_hash
            ),
        });
    }
    let environment =
        composition::environment_services(start, explicit_home, Arc::clone(&invocation.clock))?;
    let environment_status = environment.manager.status(selection, true).await?;
    if format == OutputFormat::Human {
        output::write_workflow_review(&guidance, format)?;
        output::write_environment_status(&environment_status, format)?;
    }
    let service_status = composition::service_stack_manager(
        explicit_home,
        invocation.runtime_kind,
        &invocation.services,
    )?
    .status()
    .await?;
    if !service_status.installed {
        return Err(MmError::State {
            message: "shared local services are not installed; review and apply `mm dev setup --services` first"
                .to_owned(),
        });
    }
    let deployment = invocation
        .services
        .deployment_service(selection, explicit_home)?;
    let plan = deployment.plan(selection).await?;
    let Some(reviewed_deployment) = reviewed_deployment else {
        if format == OutputFormat::Json {
            output::write_deployment_plan(&plan, format)?;
        } else {
            output::write_deployment_plan(&plan, format)?;
            output::write_dashboard_links(&plan.project_name)?;
        }
        return Ok(0);
    };
    if plan.plan_hash != reviewed_deployment {
        return Err(MmError::State {
            message: format!(
                "deployment plan changed (expected {reviewed_deployment}, current {}); review the new plan",
                plan.plan_hash
            ),
        });
    }
    if !yes {
        return Err(MmError::Terminal {
            message: "project setup apply requires --yes with both reviewed plan hashes".to_owned(),
        });
    }
    if format == OutputFormat::Human {
        output::write_deployment_plan(&plan, format)?;
    }
    let result = deployment
        .apply_reviewed(selection, reviewed_deployment)
        .await?;
    output::write_deployment_result(&result, format)?;
    if format == OutputFormat::Human {
        output::write_dashboard_links(&plan.project_name)?;
    }
    Ok(0)
}

pub(super) async fn run_services_command(
    command: ServicesCommand,
    manager: &crate::app::ServiceStackManager,
    format: OutputFormat,
) -> Result<u8, MmError> {
    use crate::domain::service_stack::ServiceStackAction;

    match command {
        ServicesCommand::Plan {
            operation,
            smtp_port,
            purge,
            gate,
        } => {
            let action: ServiceStackAction = operation.into();
            let plan = manager.plan(service_plan_request(action, smtp_port, purge, gate))?;
            output::write_service_stack_plan(&plan, format)?;
            Ok(0)
        }
        ServicesCommand::Status => {
            let status = manager.status().await?;
            let unhealthy = status
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error);
            output::write_service_stack_status(&status, format)?;
            Ok(if unhealthy { 3 } else { 0 })
        }
        ServicesCommand::Apply {
            plan,
            smtp_port,
            gate,
            yes,
        } => {
            execute_service_plan(
                manager,
                service_plan_request(ServiceStackAction::Apply, smtp_port, false, gate),
                &plan,
                yes,
                format,
            )
            .await
        }
        ServicesCommand::Upgrade {
            plan,
            smtp_port,
            gate,
            yes,
        } => {
            execute_service_plan(
                manager,
                service_plan_request(ServiceStackAction::Upgrade, smtp_port, false, gate),
                &plan,
                yes,
                format,
            )
            .await
        }
        ServicesCommand::Repair { plan, yes } => {
            execute_service_plan(
                manager,
                service_plan_request(ServiceStackAction::Repair, None, false, false),
                &plan,
                yes,
                format,
            )
            .await
        }
        ServicesCommand::Remove { plan, purge, yes } => {
            execute_service_plan(
                manager,
                service_plan_request(ServiceStackAction::Remove, None, purge, false),
                &plan,
                yes,
                format,
            )
            .await
        }
    }
}

fn service_plan_request(
    action: crate::domain::service_stack::ServiceStackAction,
    host_smtp_port: Option<u16>,
    purge_data: bool,
    include_gate: bool,
) -> crate::app::ServicePlanRequest {
    crate::app::ServicePlanRequest {
        action,
        host_smtp_port,
        purge_data,
        include_gate,
    }
}

pub(super) async fn execute_service_plan(
    manager: &crate::app::ServiceStackManager,
    request: crate::app::ServicePlanRequest,
    hash: &str,
    yes: bool,
    format: OutputFormat,
) -> Result<u8, MmError> {
    execute_service_plan_reporting(manager, request, hash, yes, format, true).await
}

async fn execute_service_plan_reporting(
    manager: &crate::app::ServiceStackManager,
    request: crate::app::ServicePlanRequest,
    hash: &str,
    yes: bool,
    format: OutputFormat,
    emit_result: bool,
) -> Result<u8, MmError> {
    let preview = manager.plan(request)?;
    if preview.plan_hash != hash {
        return Err(MmError::State {
            message: format!(
                "service stack plan changed (expected {hash}, current {}); review the new plan",
                preview.plan_hash
            ),
        });
    }
    if format == OutputFormat::Json && !yes {
        return Err(MmError::Terminal {
            message: "JSON service mutation requires --yes".to_owned(),
        });
    }
    if format == OutputFormat::Human {
        output::write_service_stack_plan(&preview, format)?;
    }
    if !yes
        && !confirm(&format!(
            "{} installation-wide service plan {}?",
            preview.action.as_str(),
            output::terminal_safe(hash)
        ))?
    {
        return Ok(0);
    }
    let result = manager.execute_reviewed(request, hash).await?;
    if emit_result {
        output::write_service_stack_result(&result, format)?;
    }
    Ok(0)
}

pub(super) async fn run_services_setup(
    manager: &crate::app::ServiceStackManager,
    smtp_port: Option<u16>,
    yes: bool,
    format: OutputFormat,
) -> Result<u8, MmError> {
    run_services_setup_reporting(manager, smtp_port, yes, format, true).await
}

pub(super) async fn run_services_setup_reporting(
    manager: &crate::app::ServiceStackManager,
    smtp_port: Option<u16>,
    yes: bool,
    format: OutputFormat,
    emit_results: bool,
) -> Result<u8, MmError> {
    use crate::domain::service_stack::ServiceStackAction;
    // At most one recovery followed by one convergence; both retain their own
    // exact review hash and confirmation. Setup never proposes data purging.
    for _ in 0..2 {
        let action = manager.setup_action()?;
        let request = service_plan_request(
            action,
            if action == ServiceStackAction::Apply {
                smtp_port
            } else {
                None
            },
            false,
            false,
        );
        let preview = manager.plan(request)?;
        let code = execute_service_plan_reporting(
            manager,
            request,
            &preview.plan_hash,
            yes,
            format,
            emit_results,
        )
        .await?;
        if code != 0 {
            return Ok(code);
        }
        let status = manager.status().await?;
        let healthy = status.installed
            && status.components.iter().all(|component| component.healthy)
            && !status
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error)
            && smtp_port.is_none_or(|port| status.host_smtp_port == Some(port));
        if healthy {
            return Ok(0);
        }
        if action == ServiceStackAction::Apply {
            return Ok(2);
        }
        if manager.setup_action()? == ServiceStackAction::Repair {
            return Ok(1);
        }
    }
    Err(MmError::State {
        message: "shared services changed during setup; review setup again".to_owned(),
    })
}

pub(super) async fn local_route_tls_diagnostics(
    start: &std::path::Path,
    environment: Option<&str>,
    ca: &crate::app::LocalCaManager,
    process: &dyn ProcessRunner,
) -> Result<Vec<crate::domain::diagnostic::Diagnostic>, MmError> {
    let document = match composition::project_documents().load_from(start) {
        Ok(document) => document,
        Err(MmError::ConfigNotFound { .. }) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let environment_name = crate::domain::config::resolve_environment(&document, environment)?;
    let hosts = document
        .config
        .environments
        .get(&environment_name)
        .into_iter()
        .flat_map(|environment| environment.domains.values())
        .filter_map(|host| match crate::domain::ca::normalize_hostname(host) {
            Ok(host) if crate::domain::ca::is_local_ca_host(&host) => Some(host),
            Ok(_) | Err(_) => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    if hosts.is_empty() {
        return Ok(Vec::new());
    }
    let status = ca.status().await?;
    let mut diagnostics = Vec::new();
    for host in hosts {
        if !status.issued_hosts.contains(&host) {
            diagnostics.push(
                crate::domain::diagnostic::Diagnostic::error(
                    "MM_TLS_ROUTE_NOT_USING_LEAF",
                    format!("local HTTPS route '{host}' has no referenced mm leaf certificate"),
                )
                .with_help("run `mm dev setup`, then apply the reviewed deployment"),
            );
            continue;
        }
        if !status.trust.as_ref().is_some_and(|trust| trust.installed) {
            continue;
        }
        let Some(curl) = platform_curl() else {
            diagnostics.push(
                crate::domain::diagnostic::Diagnostic::warning(
                    "MM_TLS_BROWSER_VERIFICATION_UNAVAILABLE",
                    "system TLS verification could not run because the allowlisted curl executable is absent",
                )
                .with_help("install the platform curl package or verify the route in a browser"),
            );
            continue;
        };
        let output = process
            .run(
                CommandSpec::new(curl)
                    .args([
                        "--fail",
                        "--silent",
                        "--show-error",
                        "--head",
                        "--max-time",
                        "3",
                        "--",
                    ])
                    .arg(format!("https://{host}/"))
                    .timeout(std::time::Duration::from_secs(5)),
            )
            .await?;
        if !output.success() {
            diagnostics.push(
                crate::domain::diagnostic::Diagnostic::error(
                    "MM_TLS_BROWSER_VERIFICATION_FAILED",
                    format!(
                        "the system TLS client could not verify and reach HTTPS route '{host}'"
                    ),
                )
                .with_help("confirm local DNS/ingress is running, then run `mm dev tls status`"),
            );
        }
    }
    Ok(diagnostics)
}

pub(super) fn platform_curl() -> Option<&'static str> {
    if cfg!(windows) {
        std::path::Path::new("C:\\Windows\\System32\\curl.exe")
            .is_file()
            .then_some("C:\\Windows\\System32\\curl.exe")
    } else {
        std::path::Path::new("/usr/bin/curl")
            .is_file()
            .then_some("/usr/bin/curl")
    }
}

pub(super) async fn run_dev_down(
    yes: bool,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    if !yes {
        return Err(MmError::Terminal {
            message: "local development shutdown requires explicit --yes".to_owned(),
        });
    }
    if composition::remote_transport_context(
        &invocation.selection,
        invocation.home(),
        invocation.runtime_kind,
    )?
    .is_some()
    {
        return Err(MmError::RuntimeResponse {
            message: "`mm dev down` stops local targets only; stop remote stacks on their host"
                .to_owned(),
        });
    }
    let stopped = invocation
        .services
        .deployment_service(&invocation.selection, invocation.home())?
        .stop(&invocation.selection)
        .await?;
    output::write_stopped_stacks(&stopped, invocation.format)?;
    Ok(0)
}
