use super::*;

#[derive(Clone)]
pub(super) struct Invocation {
    pub(super) start: PathBuf,
    pub(super) home: Option<PathBuf>,
    pub(super) format: OutputFormat,
    pub(super) runtime_kind: crate::domain::runtime::RuntimeKind,
    pub(super) runtime: Arc<dyn crate::app::ports::RuntimeDriver>,
    pub(super) clock: Arc<dyn Clock>,
    pub(super) services: RuntimeComposition,
    pub(super) selection: WorkspaceSelection,
    pub(super) recovery_context: output::RecoveryContext,
}

impl Invocation {
    fn prepare(cli: &Cli) -> Result<Self, MmError> {
        let start = cli.root.clone().map_or_else(current_directory, Ok)?;
        let services = RuntimeComposition::new();
        let runtime_kind = if matches!(
            cli.command,
            None | Some(
                Command::Doctor { .. }
                    | Command::Target { .. }
                    | Command::Ingress { .. }
                    | Command::Key { .. }
                    | Command::Agent { .. }
                    | Command::Workspace { .. }
            )
        ) {
            cli.runtime
                .map(Into::into)
                .unwrap_or(crate::domain::runtime::RuntimeKind::PodmanCompose)
        } else {
            resolve_cli_runtime(&start, cli.mm_home.as_deref(), cli)?
        };
        let runtime = services.driver(runtime_kind);
        let clock = services.clock();
        let mut selection = WorkspaceSelection::new(&start, cli.environment.clone());
        selection.runtime = cli.runtime.map(Into::into);
        selection.target = cli.target.clone();
        Ok(Self {
            start,
            home: cli.mm_home.clone(),
            format: cli.output,
            runtime_kind,
            runtime,
            clock,
            services,
            selection,
            recovery_context: cli.recovery_context(),
        })
    }

    pub(super) fn home(&self) -> Option<&std::path::Path> {
        self.home.as_deref()
    }
}

pub async fn run(
    mut cli: Cli,
    cancellation: crate::domain::observability::OperationCancellation,
) -> Result<u8, MmError> {
    super::agent::refuse_direct_agent_use(&cli)?;
    if normalize_interactive_command(&mut cli)? {
        return Ok(0);
    }
    if let Some(code) = internal::run_boundary_command(&cli, cancellation.clone()).await? {
        return Ok(code);
    }
    if let Some(Command::History(args)) = &cli.command {
        return super::history::run(&cli, args);
    }
    if let Some(Command::Update(args)) = &cli.command {
        return super::update::run(args, cli.output).await;
    }
    let invocation = Invocation::prepare(&cli)?;
    let command = cli.command.take();
    route(command, &invocation, cancellation).await
}

fn normalize_interactive_command(cli: &mut Cli) -> Result<bool, MmError> {
    if matches!(cli.command, Some(Command::Dev { command: None })) {
        if cli.output != OutputFormat::Human {
            return Err(MmError::Terminal {
                message: "structured output requires an explicit `mm dev` subcommand".to_owned(),
            });
        }
        if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
            write_family_help("dev")?;
            return Ok(true);
        }
        cli.command = None;
    }
    if matches!(
        cli.command,
        Some(Command::Init {
            name: None,
            plan: None,
            yes: false
        })
    ) && cli.output == OutputFormat::Human
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
    {
        cli.command = None;
    }
    Ok(false)
}

async fn route(
    command: Option<Command>,
    invocation: &Invocation,
    cancellation: crate::domain::observability::OperationCancellation,
) -> Result<u8, MmError> {
    match command {
        Some(Command::Ingress { command }) => super::ingress::run(command, invocation).await,
        Some(Command::Init { name, plan, yes }) => {
            super::init::run(name, plan, yes, invocation).await
        }
        Some(Command::Config { command }) => config::run(command, invocation),
        Some(Command::Doctor {
            command,
            check_updates,
        }) => match command {
            Some(command) => super::state::run(command, invocation),
            None => super::doctor::run(check_updates, invocation).await,
        },
        Some(Command::Service { command }) => {
            service::run(command, invocation, cancellation.clone()).await
        }
        Some(Command::Job { command }) => job::run(command, invocation, cancellation.clone()).await,
        Some(Command::Env { command }) => env::run(command, invocation).await,
        Some(Command::Secret {
            command: SecretCommand::Provider { command },
        }) => provider::run(command, invocation).await,
        Some(Command::Secret { command }) => secret::run(command, invocation).await,
        Some(Command::Access { command }) => access::run(command, invocation).await,
        Some(Command::Build {
            publish,
            all,
            yes,
            command,
        }) => {
            build::run(
                command,
                crate::app::BuildRequest::new(publish, all),
                yes,
                invocation,
            )
            .await
        }
        Some(Command::Deploy {
            yes,
            nodes,
            release,
            release_inputs,
            command,
        }) => {
            let release = crate::domain::release_facts::ReleaseRequest::from_arguments(
                release,
                &release_inputs,
            )
            .map_err(|message| MmError::ReleaseInput { message })?;
            deploy::run(command, yes, (nodes, Some(release)), invocation).await
        }
        Some(Command::Target { command }) => target::run(command, invocation).await,
        Some(Command::Key { command }) => super::key::run(command, invocation),
        Some(Command::Agent { command }) => super::agent::run(command, invocation),
        Some(Command::Workspace { command }) => super::workspace::run(command, invocation),
        Some(Command::Forward(args)) => super::forward::run(args, invocation).await,
        Some(Command::Team { command }) => super::team::run_team(command, invocation).await,
        Some(Command::Dev { command }) => dev::run(command, invocation).await,
        Some(
            Command::Generate { .. }
            | Command::Mcp { .. }
            | Command::History(_)
            | Command::Update(_)
            | Command::DashboardSchema
            | Command::Relay { .. }
            | Command::InstallIngress { .. }
            | Command::UninstallIngress { .. },
        ) => Ok(0),
        None => super::app::run(invocation).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_ingress_does_not_require_project_or_state() -> Result<(), Box<dyn std::error::Error>>
    {
        use clap::Parser;
        let directory = crate::test_support::tempdir()?;
        let blocked_home = directory.path().join("not-a-directory");
        std::fs::write(&blocked_home, "blocked")?;
        for action in ["start", "stop"] {
            let cli = Cli::try_parse_from([
                "mm",
                "--root",
                directory.path().to_str().ok_or("root")?,
                "--mm-home",
                blocked_home.to_str().ok_or("home")?,
                "ingress",
                action,
            ])?;
            Invocation::prepare(&cli)?;
        }
        Ok(())
    }
}
