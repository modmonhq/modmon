use super::*;

pub(super) fn write_family_help(name: &str) -> Result<(), MmError> {
    let mut command = Cli::command();
    let family = command
        .find_subcommand_mut(name)
        .ok_or_else(|| MmError::State {
            message: format!("CLI family '{name}' is missing from the canonical command tree"),
        })?;
    family.print_long_help().map_err(|source| MmError::Io {
        context: format!("writing help for mm {name}"),
        source,
    })?;
    writeln!(crate::presentation::record::stdout()).map_err(|source| MmError::Io {
        context: format!("finishing help for mm {name}"),
        source,
    })
}

pub(super) async fn run_boundary_command(
    cli: &Cli,
    cancellation: crate::domain::observability::OperationCancellation,
) -> Result<Option<u8>, MmError> {
    let Some(command) = cli.command.as_ref() else {
        return Ok(None);
    };
    match command {
        Command::Relay {
            command:
                RelayCommand::Serve {
                    stdio,
                    socket,
                    daemon_uid,
                    idle_timeout_seconds,
                },
        } => {
            if !stdio || *idle_timeout_seconds == 0 || *idle_timeout_seconds > 24 * 60 * 60 {
                return Err(MmError::Security {
                    message: "relay requires --stdio and a bounded positive idle timeout"
                        .to_owned(),
                });
            }
            composition::serve_stdio_relay(
                socket,
                *daemon_uid,
                std::time::Duration::from_secs(*idle_timeout_seconds),
            )
            .await?;
            Ok(Some(0))
        }
        Command::Mcp {
            command: McpCommand::Serve { config },
        } => {
            let server = composition::mcp::server(
                config.as_deref(),
                cli.mm_home.as_deref(),
                cli.root.as_deref(),
            )?;
            crate::presentation::mcp::serve_stdio(server, cancellation).await?;
            Ok(Some(0))
        }
        Command::Generate { command } => {
            run_generate(command)?;
            Ok(Some(0))
        }
        Command::InstallIngress {
            artifact_root,
            target_id,
            artifact_hash,
        } => {
            let target_id = uuid::Uuid::parse_str(target_id).map_err(|_| MmError::Security {
                message: "invalid target identity for privileged ingress installation".to_owned(),
            })?;
            composition::install_ingress(
                artifact_root,
                crate::domain::target::TargetId(target_id),
                artifact_hash,
            )?;
            Ok(Some(0))
        }
        Command::UninstallIngress {
            target_id,
            artifact_hash,
        } => {
            let target_id = uuid::Uuid::parse_str(target_id).map_err(|_| MmError::Security {
                message: "invalid target identity for privileged ingress removal".to_owned(),
            })?;
            composition::uninstall_ingress(
                crate::domain::target::TargetId(target_id),
                artifact_hash,
            )?;
            Ok(Some(0))
        }
        Command::Config {
            command: ConfigCommand::Schema { key: None },
        } => {
            output::write_json(&composition::schema())?;
            Ok(Some(0))
        }
        Command::Config {
            command: ConfigCommand::Schema { key: Some(key) },
        } => {
            let schema = composition::key_schema(key).ok_or_else(|| MmError::Environment {
                message: format!("'{key}' is not a top-level configuration key"),
            })?;
            output::write_json(&schema)?;
            Ok(Some(0))
        }
        Command::DashboardSchema => {
            output::write_json(&crate::domain::dashboard::dashboard_contract_schema())?;
            Ok(Some(0))
        }
        _ => Ok(None),
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum McpCommand {
    /// Serve MCP on standard input/output for the project in the current
    /// directory (or --root). No network listener is opened.
    Serve {
        /// Private limits for this device. Defaults to $MM_HOME/mcp.json,
        /// which is optional.
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum RelayCommand {
    /// Bridge SSH standard I/O to the authenticated daemon Unix socket.
    Serve {
        /// Require framed transport on standard input and output.
        #[arg(long)]
        stdio: bool,
        #[arg(long, default_value = "/run/mm/mm.sock")]
        socket: PathBuf,
        #[arg(long, default_value_t = 0)]
        daemon_uid: u32,
        #[arg(long, default_value_t = 30)]
        idle_timeout_seconds: u64,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum GenerateCommand {
    /// Print a shell completion script to stdout.
    Completions {
        /// Shell whose completion syntax should be generated.
        #[arg(value_enum)]
        shell: CompletionShell,
    },
    /// Print an mdoc-compatible manual page to stdout.
    Manpage,
    /// Print Markdown reference for every visible command and option.
    Reference,
    /// Write all checked-in reference, completion, and manpage artifacts.
    Artifacts {
        /// Repository root receiving docs/, site/, contrib/, and core/ui/ generated files.
        #[arg(long, default_value = ".", value_name = "PATH")]
        directory: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Elvish,
}

pub(super) fn run_generate(command: &GenerateCommand) -> Result<(), MmError> {
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    match command {
        GenerateCommand::Completions { shell } => {
            let generator = match shell {
                CompletionShell::Bash => clap_complete::Shell::Bash,
                CompletionShell::Zsh => clap_complete::Shell::Zsh,
                CompletionShell::Fish => clap_complete::Shell::Fish,
                CompletionShell::PowerShell => clap_complete::Shell::PowerShell,
                CompletionShell::Elvish => clap_complete::Shell::Elvish,
            };
            writer
                .write_all(&render_completion(generator))
                .map_err(|source| MmError::Io {
                    context: "writing generated completion".to_owned(),
                    source,
                })
        }
        GenerateCommand::Manpage => {
            writer
                .write_all(render_manpage().as_bytes())
                .map_err(|source| MmError::Io {
                    context: "writing generated manpage".to_owned(),
                    source,
                })
        }
        GenerateCommand::Reference => {
            writer
                .write_all(render_reference().as_bytes())
                .map_err(|source| MmError::Io {
                    context: "writing generated command reference".to_owned(),
                    source,
                })
        }
        GenerateCommand::Artifacts { directory } => write_generated_artifacts(directory),
    }
}

pub(super) fn write_generated_artifacts(root: &std::path::Path) -> Result<(), MmError> {
    let reference = root.join("docs/reference/cli.md");
    let manpage = root.join("contrib/man/mm.1");
    let completions = root.join("contrib/completions");
    let schemas = root.join("site/public/schema");
    for directory in [
        reference.parent(),
        manpage.parent(),
        Some(completions.as_path()),
        Some(schemas.as_path()),
    ]
    .into_iter()
    .flatten()
    {
        std::fs::create_dir_all(directory).map_err(|source| MmError::Io {
            context: format!(
                "creating generated artifact directory {}",
                directory.display()
            ),
            source,
        })?;
    }
    std::fs::write(&reference, render_reference()).map_err(|source| MmError::Io {
        context: format!("writing generated reference {}", reference.display()),
        source,
    })?;
    std::fs::write(&manpage, render_manpage()).map_err(|source| MmError::Io {
        context: format!("writing generated manpage {}", manpage.display()),
        source,
    })?;
    for (shell, name) in [
        (clap_complete::Shell::Bash, "mm.bash"),
        (clap_complete::Shell::Zsh, "_mm"),
        (clap_complete::Shell::Fish, "mm.fish"),
        (clap_complete::Shell::PowerShell, "_mm.ps1"),
        (clap_complete::Shell::Elvish, "mm.elvish"),
    ] {
        let bytes = render_completion(shell);
        let path = completions.join(name);
        std::fs::write(&path, bytes).map_err(|source| MmError::Io {
            context: format!("writing generated completion {}", path.display()),
            source,
        })?;
    }
    write_json_artifact(&schemas.join("mm.schema.json"), &composition::schema())?;
    write_key_schemas(&schemas.join("mm"))?;
    write_json_artifact(
        &root.join("core/ui/src/dashboard.schema.json"),
        &crate::domain::dashboard::dashboard_contract_schema(),
    )?;
    write_json_artifact(
        &schemas.join("dashboard.schema.json"),
        &crate::domain::dashboard::dashboard_contract_schema(),
    )?;
    write_json_artifact(
        &schemas.join("mcp-project.schema.json"),
        &composition::schema(),
    )?;
    write_json_artifact(
        &schemas.join("mcp-tools.schema.json"),
        &artifact_with_comment(
            crate::presentation::mcp::generated_tool_schema(),
            "Generated from the MCP tool router; run ./scripts/generate-artifacts.sh. Do not edit.",
        ),
    )?;
    write_json_artifact(
        &schemas.join("mcp-resources.schema.json"),
        &artifact_with_comment(
            crate::presentation::mcp::generated_resource_schema(),
            "Generated from the MCP resource handler; run ./scripts/generate-artifacts.sh. Do not edit.",
        ),
    )?;
    write_json_artifact(
        &schemas.join("mcp-config.schema.json"),
        &schema_with_comment::<crate::domain::mcp::McpServerConfig>(
            "Generated from McpServerConfig; run ./scripts/generate-artifacts.sh. Do not edit.",
        ),
    )?;
    write_json_artifact(
        &schemas.join("provider-descriptors.json"),
        &serde_json::json!({
            "generated": {
                "source": "OpenBao and Vault provider descriptors",
                "command": "./scripts/generate-artifacts.sh",
                "notice": "Do not edit."
            },
            "drivers": composition::provider_driver_descriptors()?,
        }),
    )?;
    write_json_artifact(
        &schemas.join("capabilities.json"),
        &generated_capabilities()?,
    )?;
    Ok(())
}

/// One schema per top-level key, for split files under `.config/mm/`.
fn write_key_schemas(directory: &std::path::Path) -> Result<(), MmError> {
    std::fs::create_dir_all(directory).map_err(|source| MmError::Io {
        context: format!(
            "creating generated schema directory {}",
            directory.display()
        ),
        source,
    })?;
    for key in crate::domain::config::SPLITTABLE_CONFIG_KEYS {
        let schema = composition::key_schema(key).ok_or_else(|| MmError::State {
            message: format!("no schema for top-level configuration key '{key}'"),
        })?;
        write_json_artifact(&directory.join(format!("{key}.schema.json")), &schema)?;
    }
    Ok(())
}

fn write_json_artifact(
    path: &std::path::Path,
    value: &impl serde::Serialize,
) -> Result<(), MmError> {
    let mut rendered = serde_json::to_string_pretty(value).map_err(|error| MmError::State {
        message: format!("serializing generated artifact {}: {error}", path.display()),
    })?;
    rendered.push('\n');
    std::fs::write(path, rendered).map_err(|source| MmError::Io {
        context: format!("writing generated artifact {}", path.display()),
        source,
    })
}

fn schema_with_comment<T: schemars::JsonSchema>(comment: &str) -> schemars::Schema {
    let mut schema = schemars::schema_for!(T);
    schema.insert(
        "$comment".to_owned(),
        serde_json::Value::String(comment.to_owned()),
    );
    schema
}

fn artifact_with_comment(mut value: serde_json::Value, comment: &str) -> serde_json::Value {
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "$comment".to_owned(),
            serde_json::Value::String(comment.to_owned()),
        );
    }
    value
}

fn generated_capabilities() -> Result<serde_json::Value, MmError> {
    use crate::domain::runtime::{RuntimeKind, runtime_descriptor};

    let runtimes = [RuntimeKind::PodmanCompose, RuntimeKind::DockerCompose]
        .into_iter()
        .map(|kind| {
            let descriptor = runtime_descriptor(kind);
            serde_json::json!({
                "runtime": kind,
                "capabilities": descriptor.capabilities.iter().map(|evidence| serde_json::json!({
                    "name": evidence.capability.as_str(),
                    "fixture": evidence.fixture,
                    "live_job": evidence.live_job,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "generated": {
            "source": "runtime, secret delivery, storage, and provider descriptors",
            "command": "./scripts/generate-artifacts.sh",
            "notice": "Do not edit."
        },
        "runtimes": runtimes,
        "secrets": composition::secret_capabilities()?,
        "providers": composition::provider_driver_descriptors()?,
    }))
}

fn completion_with_header(shell: clap_complete::Shell, bytes: Vec<u8>) -> Vec<u8> {
    const HEADER: &str =
        "Generated from the canonical Clap tree by ./scripts/generate-artifacts.sh. Do not edit.";
    let source = String::from_utf8_lossy(&bytes);
    if shell == clap_complete::Shell::Zsh
        && let Some((first, rest)) = source.split_once('\n')
    {
        return format!("{first}\n# {HEADER}\n{rest}").into_bytes();
    }
    format!("# {HEADER}\n{source}").into_bytes()
}

fn render_completion(shell: clap_complete::Shell) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut command = public_command();
    clap_complete::generate(shell, &mut command, "mm", &mut bytes);
    completion_with_header(shell, bytes)
}

pub(super) fn render_manpage() -> String {
    let command = public_command();
    let commands = command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set())
        .map(|subcommand| {
            format!(
                ".TP\n\\fB{}\\fR\n{}\n",
                subcommand.get_name(),
                subcommand
                    .get_about()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            )
        })
        .collect::<String>();
    format!(
        ".\\\" Generated from the canonical Clap tree by ./scripts/generate-artifacts.sh. Do not edit.\n.TH MM 1 \"2026-08-24\" \"mm {}\" \"User Commands\"\n.SH NAME\nmm \\- reviewed monorepo development and deployment\n.SH SYNOPSIS\n.B mm\n[GLOBAL OPTIONS] [COMMAND]\n.SH DESCRIPTION\nPlans are deterministic and secret-safe. Mutations require review hashes; configuration and runtime changes reject stale input.\n.SH COMMANDS\n{}.SH EXIT STATUS\n0 success; 1 local I/O, state, or terminal failure; 2 invalid request or configuration; 3 security, runtime, or provider failure.\n.SH SECURITY\nSecret plaintext must be submitted only through hidden input or stdin on dedicated secret commands. Remote bootstrap and trust changes disclose privilege escalation before apply.\n",
        env!("CARGO_PKG_VERSION"),
        commands
    )
}

pub(super) fn render_reference() -> String {
    let mut output = String::from(
        "---\ntitle: CLI command reference\ndescription: Generated from the mm Clap command tree.\n---\n\n<!-- Generated from the canonical Clap tree by ./scripts/generate-artifacts.sh. Do not edit. -->\n\n",
    );
    render_reference_command(&mut output, &[], &public_command());
    output.truncate(output.trim_end_matches('\n').len());
    output.push('\n');
    output
}

pub(super) fn public_command() -> clap::Command {
    let command = Cli::command();
    clap::Command::new("mm")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Plan and apply reviewed monorepo builds and deployments.")
        .long_about("mm compiles one engine-neutral project contract into reviewed plans, then applies them through a selected runtime. Selection is resolved once from CLI flags, private preferences, environment config, project defaults, then target defaults. Missing or contradictory selection is an error. Plans, logs, and JSON never contain secret values.\n\n`--help` never probes a container engine or opens project state.")
        .after_help("Examples:\n  mm init\n  mm dev setup\n  mm config validate\n  mm service status\n  mm deploy plan\n\nSelection: --runtime and --target override private preferences and project defaults. Use `mm target preference show` to inspect provenance.")
        .propagate_version(true)
        .args(command.get_arguments().cloned())
        .groups(command.get_groups().cloned())
        .subcommands(
            command
                .get_subcommands()
                .filter(|subcommand| !subcommand.is_hide_set())
                .cloned(),
        )
}

pub(super) fn render_reference_command(
    output: &mut String,
    parents: &[String],
    command: &clap::Command,
) {
    if command.is_hide_set() {
        return;
    }
    let mut path = parents.to_vec();
    path.push(command.get_name().to_owned());
    let level = path.len().min(6);
    output.push_str(&"#".repeat(level));
    output.push(' ');
    output.push('`');
    output.push_str(&path.join(" "));
    output.push_str("`\n\n```text\n");
    let mut rendered = command.clone();
    let help = rendered.render_long_help().to_string();
    for line in help.lines() {
        output.push_str(line.trim_end());
        output.push('\n');
    }
    output.push_str("```\n\n");
    for subcommand in command.get_subcommands() {
        render_reference_command(output, &path, subcommand);
    }
}
