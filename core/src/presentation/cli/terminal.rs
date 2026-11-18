use super::*;

pub(super) fn read_public_value(value: Option<String>, stdin: bool) -> Result<String, MmError> {
    if stdin {
        return read_stdin_value().map(|value| value.to_string());
    }
    value.ok_or_else(|| MmError::Environment {
        message: "provide VALUE or use --stdin".to_owned(),
    })
}

pub(super) fn read_secret_material(stdin: bool, prompt: &str) -> Result<SecretMaterial, MmError> {
    if stdin {
        return SecretMaterial::from_zeroizing(read_stdin_value()?)
            .map_err(|message| MmError::Secret { message });
    }
    if !std::io::stdin().is_terminal() {
        return Err(MmError::Secret {
            message: "non-interactive secret input requires --stdin".to_owned(),
        });
    }
    let prompt = prompt.trim();
    let first_prompt = format!("{prompt}: ");
    let confirmation_prompt = format!("Confirm {prompt}: ");
    let first =
        Zeroizing::new(
            rpassword::prompt_password(first_prompt).map_err(|source| MmError::Io {
                context: "reading secret value".to_owned(),
                source,
            })?,
        );
    let second = Zeroizing::new(rpassword::prompt_password(confirmation_prompt).map_err(
        |source| MmError::Io {
            context: "confirming secret value".to_owned(),
            source,
        },
    )?);
    if first.as_str() != second.as_str() {
        return Err(MmError::Secret {
            message: "secret values did not match".to_owned(),
        });
    }
    if first.is_empty() {
        return Err(MmError::Secret {
            message: "secret value may not be empty".to_owned(),
        });
    }
    SecretMaterial::from_zeroizing(first).map_err(|message| MmError::Secret { message })
}

fn read_stdin_value() -> Result<Zeroizing<String>, MmError> {
    let mut value = Zeroizing::new(String::new());
    std::io::stdin()
        .read_to_string(&mut value)
        .map_err(|source| MmError::Io {
            context: "reading value from stdin".to_owned(),
            source,
        })?;
    if value.ends_with("\r\n") {
        let new_length = value.len() - 2;
        value.truncate(new_length);
    } else if value.ends_with('\n') {
        value.pop();
    }
    Ok(value)
}

pub(super) fn confirm(prompt: &str) -> Result<bool, MmError> {
    if !std::io::stdin().is_terminal() {
        return Err(MmError::Terminal {
            message: "confirmation requires a terminal or --yes".to_owned(),
        });
    }
    let mut stderr = std::io::stderr().lock();
    write!(stderr, "{prompt} [y/N] ").map_err(cli_output_error)?;
    stderr.flush().map_err(cli_output_error)?;
    let mut response = String::new();
    std::io::stdin()
        .read_line(&mut response)
        .map_err(|source| MmError::Io {
            context: "reading confirmation".to_owned(),
            source,
        })?;
    Ok(matches!(
        response.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub(super) enum SetupDecision {
    Cancelled,
    Initialize { name: Option<String> },
}

pub(super) fn prompt_interactive_setup(root: &std::path::Path) -> Result<SetupDecision, MmError> {
    let mut stdout = crate::presentation::record::stdout().lock();
    writeln!(
        stdout,
        "No mm project was found at {}.",
        output::terminal_safe(&root.display().to_string())
    )
    .map_err(cli_output_error)?;
    write!(stdout, "Initialize this directory? [Y/n] ").map_err(cli_output_error)?;
    stdout.flush().map_err(cli_output_error)?;
    let mut response = String::new();
    let read = std::io::stdin()
        .read_line(&mut response)
        .map_err(|source| MmError::Io {
            context: "reading setup confirmation".to_owned(),
            source,
        })?;
    if read == 0 {
        writeln!(stdout, "Setup cancelled.").map_err(cli_output_error)?;
        return Ok(SetupDecision::Cancelled);
    }
    if !matches!(
        response.trim().to_ascii_lowercase().as_str(),
        "" | "y" | "yes"
    ) {
        writeln!(stdout, "Setup cancelled.").map_err(cli_output_error)?;
        return Ok(SetupDecision::Cancelled);
    }
    write!(stdout, "Project name [directory name]: ").map_err(cli_output_error)?;
    stdout.flush().map_err(cli_output_error)?;
    let mut name = String::new();
    std::io::stdin()
        .read_line(&mut name)
        .map_err(|source| MmError::Io {
            context: "reading project name".to_owned(),
            source,
        })?;
    let name = (!name.trim().is_empty()).then(|| name.trim().to_owned());
    Ok(SetupDecision::Initialize { name })
}

fn cli_output_error(source: std::io::Error) -> MmError {
    MmError::Io {
        context: "writing command output".to_owned(),
        source,
    }
}

pub(super) fn snapshot_exit_code(snapshot: &crate::domain::workspace::WorkspaceSnapshot) -> u8 {
    if !snapshot.validation.is_valid() {
        2
    } else if !snapshot.runtime.available
        || snapshot
            .runtime
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        3
    } else {
        0
    }
}

fn command_allows_host_only_runtime(command: &Option<Command>) -> bool {
    matches!(
        command,
        None | Some(
            Command::Doctor { .. }
                | Command::Init { .. }
                | Command::DashboardSchema
                | Command::Dev { .. }
                | Command::Relay { .. }
                | Command::Secret { .. }
                | Command::Access { .. }
                | Command::Env { .. }
                | Command::Build { .. }
                | Command::Config { .. }
                | Command::Generate { .. }
                | Command::Target { .. }
                | Command::Key { .. }
                | Command::Team { .. }
                | Command::Forward(_)
                | Command::InstallIngress { .. }
                | Command::UninstallIngress { .. }
        )
    )
}

fn command_requires_resolved_selection(command: &Option<Command>) -> bool {
    matches!(
        command,
        Some(
            Command::Service { .. }
                | Command::Job { .. }
                | Command::Deploy { .. }
                | Command::Config {
                    command: ConfigCommand::Stack {
                        command: ConfigStackCommand::Compile
                    } | ConfigCommand::Ingress {
                        command: ConfigIngressCommand::List | ConfigIngressCommand::Explain { .. }
                    }
                }
                | Command::Dev {
                    command: Some(
                        DevCommand::Up { .. }
                            | DevCommand::Down { .. }
                            | DevCommand::Status
                            | DevCommand::Mounts { .. }
                            | DevCommand::Open { .. }
                            | DevCommand::Services { .. }
                            | DevCommand::Setup {
                                install_services: true,
                                ..
                            }
                    )
                }
        )
    )
}

pub(super) fn resolve_cli_runtime(
    start: &std::path::Path,
    explicit_home: Option<&std::path::Path>,
    cli: &Cli,
) -> Result<crate::domain::runtime::RuntimeKind, MmError> {
    let allow_missing_project = command_allows_host_only_runtime(&cli.command);
    let require_resolved_selection =
        !allow_missing_project || command_requires_resolved_selection(&cli.command);
    composition::resolve_runtime(
        start,
        explicit_home,
        cli.runtime.map(Into::into),
        cli.target.as_deref(),
        cli.environment.as_deref(),
        allow_missing_project,
        require_resolved_selection,
    )
}

pub(super) fn current_directory() -> Result<PathBuf, MmError> {
    std::env::current_dir().map_err(|source| MmError::Io {
        context: "resolving the current directory".to_owned(),
        source,
    })
}
