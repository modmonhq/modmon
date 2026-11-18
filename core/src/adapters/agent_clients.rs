//! Connects coding tools to `mm mcp serve`: plans the files to write (or
//! the command to run) for each tool, for one project or for the user.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::domain::error::MmError;

/// A coding tool mm knows how to connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentClient {
    ClaudeCode,
    Opencode,
    Codex,
    Cursor,
}

impl AgentClient {
    pub const ALL: [Self; 4] = [Self::ClaudeCode, Self::Opencode, Self::Codex, Self::Cursor];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Opencode => "OpenCode",
            Self::Codex => "Codex",
            Self::Cursor => "Cursor",
        }
    }

    /// The program whose presence on `PATH` means the tool is installed.
    #[must_use]
    pub const fn program(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Opencode => "opencode",
            Self::Codex => "codex",
            Self::Cursor => "cursor-agent",
        }
    }
}

/// The tools found on `PATH`.
#[must_use]
pub fn installed_clients() -> Vec<AgentClient> {
    AgentClient::ALL
        .into_iter()
        .filter(|client| find_program(client.program()).is_some())
        .collect()
}

fn find_program(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileAction {
    Create,
    Update,
}

/// One file the setup writes, with its full new contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileChange {
    pub client: Option<AgentClient>,
    pub path: PathBuf,
    pub action: FileAction,
    pub summary: String,
    #[serde(skip)]
    pub contents: String,
}

/// One command the setup runs, for tools that own their configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandStep {
    pub client: AgentClient,
    pub program: String,
    pub arguments: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AgentSetupPlan {
    pub files: Vec<FileChange>,
    pub commands: Vec<CommandStep>,
    /// Things already in place, and things the user must do by hand.
    pub notes: Vec<String>,
}

impl AgentSetupPlan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.commands.is_empty()
    }
}

const SECTION_START: &str = "<!-- mm:agents:start -->";
const SECTION_END: &str = "<!-- mm:agents:end -->";

/// The instructions `mm agent setup` keeps in `AGENTS.md`.
#[must_use]
pub fn agent_instructions() -> String {
    format!(
        "{SECTION_START}\n## mm (deploys and environments)\n\n\
- Use the `mm` MCP tools for every mm operation. Never run `mm`, `mmd`, `docker`, `podman`, or `ssh` for this project yourself.\n\
- Call `mm_access` first. It lists the environments and what you may do in each.\n\
- When a tool returns `MM_AGENT_HANDOFF`, show the user its `user_commands`, ask them to run them in their own terminal, and wait. Do not work around it.\n\
- Do not edit the `agents:` block or the users file to widen your own access.\n\
{SECTION_END}\n"
    )
}

/// The files that connect `clients` to mm in the project at `root`. They
/// are meant to be committed, so the server command is plain `mm`.
pub fn plan_project_setup(root: &Path, clients: &[AgentClient]) -> Result<AgentSetupPlan, MmError> {
    let mut plan = AgentSetupPlan::default();
    let server = serde_json::json!({ "command": "mm", "args": ["mcp", "serve"] });
    for client in clients {
        match client {
            AgentClient::ClaudeCode => {
                merge_json(
                    &mut plan,
                    *client,
                    &root.join(".mcp.json"),
                    &["mcpServers", "mm"],
                    server.clone(),
                    "register the mm MCP server",
                )?;
                deny_direct_commands(&mut plan, &root.join(".claude").join("settings.json"))?;
                claude_memory(&mut plan, root)?;
            }
            AgentClient::Cursor => merge_json(
                &mut plan,
                *client,
                &root.join(".cursor").join("mcp.json"),
                &["mcpServers", "mm"],
                server.clone(),
                "register the mm MCP server",
            )?,
            AgentClient::Opencode => opencode(&mut plan, root, "mm")?,
            AgentClient::Codex => {
                codex(&mut plan, &root.join(".codex").join("config.toml"), "mm")?;
            }
        }
    }
    if !clients.is_empty() {
        managed_section(&mut plan, None, &root.join("AGENTS.md"))?;
    }
    Ok(plan)
}

/// What connects `clients` to mm for every project of this user. `mm` is
/// the absolute path of the binary, because a coding tool may not share the
/// shell's `PATH`.
pub fn plan_user_setup(
    home: &Path,
    clients: &[AgentClient],
    mm: &Path,
) -> Result<AgentSetupPlan, MmError> {
    let mut plan = AgentSetupPlan::default();
    let mm = mm.to_string_lossy().into_owned();
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    for client in clients {
        match client {
            AgentClient::ClaudeCode => claude_user(&mut plan, home, &mm),
            AgentClient::Cursor => merge_json(
                &mut plan,
                *client,
                &home.join(".cursor").join("mcp.json"),
                &["mcpServers", "mm"],
                serde_json::json!({ "command": mm, "args": ["mcp", "serve"] }),
                "register the mm MCP server",
            )?,
            AgentClient::Opencode => opencode(&mut plan, &config_home.join("opencode"), &mm)?,
            AgentClient::Codex => {
                let codex_home = std::env::var_os("CODEX_HOME")
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .unwrap_or_else(|| home.join(".codex"));
                codex(&mut plan, &codex_home.join("config.toml"), &mm)?;
            }
        }
    }
    Ok(plan)
}

/// Writes the plan's files and runs its commands. A command that fails is
/// reported and does not undo the files.
pub fn apply_setup(plan: &AgentSetupPlan) -> Result<Vec<String>, MmError> {
    for change in &plan.files {
        if let Some(parent) = change.path.parent() {
            fs::create_dir_all(parent).map_err(|source| MmError::Io {
                context: format!("creating {}", parent.display()),
                source,
            })?;
        }
        let temporary = change
            .path
            .with_extension(format!("mm-{}", uuid::Uuid::new_v4()));
        fs::write(&temporary, &change.contents)
            .and_then(|()| fs::rename(&temporary, &change.path))
            .map_err(|source| {
                if let Err(cleanup) = fs::remove_file(&temporary)
                    && cleanup.kind() != std::io::ErrorKind::NotFound
                {
                    tracing::warn!(%cleanup, "could not remove a temporary setup file");
                }
                MmError::Io {
                    context: format!("writing {}", change.path.display()),
                    source,
                }
            })?;
    }
    let mut failures = Vec::new();
    for step in &plan.commands {
        let output = std::process::Command::new(&step.program)
            .args(&step.arguments)
            .stdin(std::process::Stdio::null())
            .output();
        match output {
            Ok(output) if output.status.success() => {}
            Ok(output) => failures.push(format!(
                "{}: `{} {}` failed: {}",
                step.client.label(),
                step.program,
                step.arguments.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(error) => failures.push(format!(
                "{}: could not run {}: {error}",
                step.client.label(),
                step.program
            )),
        }
    }
    Ok(failures)
}

fn read_optional(path: &Path) -> Result<Option<String>, MmError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(MmError::Io {
            context: format!("reading {}", path.display()),
            source,
        }),
    }
}

fn push_file(
    plan: &mut AgentSetupPlan,
    client: Option<AgentClient>,
    path: &Path,
    existing: Option<&str>,
    contents: String,
    summary: &str,
) {
    if existing == Some(contents.as_str()) {
        return;
    }
    plan.files.push(FileChange {
        client,
        path: path.to_path_buf(),
        action: if existing.is_some() {
            FileAction::Update
        } else {
            FileAction::Create
        },
        summary: summary.to_owned(),
        contents,
    });
}

/// Sets `pointer` to `value` in the JSON object file at `path`, keeping
/// everything else. A file mm cannot parse is left alone, with a note.
fn merge_json(
    plan: &mut AgentSetupPlan,
    client: AgentClient,
    path: &Path,
    pointer: &[&str],
    value: serde_json::Value,
    summary: &str,
) -> Result<(), MmError> {
    let existing = read_optional(path)?;
    let mut document = match existing.as_deref().map(str::trim) {
        None | Some("") => serde_json::json!({}),
        Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(document @ serde_json::Value::Object(_)) => document,
            _ => {
                plan.notes.push(format!(
                    "{}: {} is not a plain JSON object, so mm left it alone; set {} to {value} yourself",
                    client.label(),
                    path.display(),
                    pointer.join("."),
                ));
                return Ok(());
            }
        },
    };
    let mut node = &mut document;
    for key in &pointer[..pointer.len() - 1] {
        let entry = node
            .as_object_mut()
            .map(|object| object.entry(*key).or_insert_with(|| serde_json::json!({})));
        match entry {
            Some(child @ serde_json::Value::Object(_)) => node = child,
            _ => {
                plan.notes.push(format!(
                    "{}: {} has an unexpected '{key}' value, so mm left it alone",
                    client.label(),
                    path.display(),
                ));
                return Ok(());
            }
        }
    }
    let key = pointer[pointer.len() - 1];
    let Some(object) = node.as_object_mut() else {
        return Ok(());
    };
    if object.get(key) == Some(&value) {
        plan.notes.push(format!(
            "{}: {} already has {}",
            client.label(),
            path.display(),
            pointer.join(".")
        ));
        return Ok(());
    }
    object.insert(key.to_owned(), value);
    let mut contents = serde_json::to_string_pretty(&document).map_err(|error| MmError::State {
        message: format!("serializing {}: {error}", path.display()),
    })?;
    contents.push('\n');
    push_file(
        plan,
        Some(client),
        path,
        existing.as_deref(),
        contents,
        summary,
    );
    Ok(())
}

/// Claude Code permission rules that stop it running mm directly.
const CLAUDE_DENY_RULES: [&str; 2] = ["Bash(mm:*)", "Bash(mmd:*)"];

fn deny_direct_commands(plan: &mut AgentSetupPlan, path: &Path) -> Result<(), MmError> {
    let existing = read_optional(path)?;
    let mut document = match existing.as_deref().map(str::trim) {
        None | Some("") => serde_json::json!({}),
        Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(document @ serde_json::Value::Object(_)) => document,
            _ => {
                plan.notes.push(format!(
                    "Claude Code: {} is not a plain JSON object, so mm left it alone; add {} to permissions.deny yourself",
                    path.display(),
                    CLAUDE_DENY_RULES.join(" and "),
                ));
                return Ok(());
            }
        },
    };
    let deny = document
        .as_object_mut()
        .map(|object| {
            object
                .entry("permissions")
                .or_insert_with(|| serde_json::json!({}))
        })
        .and_then(serde_json::Value::as_object_mut)
        .map(|permissions| {
            permissions
                .entry("deny")
                .or_insert_with(|| serde_json::json!([]))
        })
        .and_then(serde_json::Value::as_array_mut);
    let Some(deny) = deny else {
        plan.notes.push(format!(
            "Claude Code: {} has an unexpected permissions value, so mm left it alone",
            path.display()
        ));
        return Ok(());
    };
    let mut changed = false;
    for rule in CLAUDE_DENY_RULES {
        if !deny.iter().any(|existing| existing.as_str() == Some(rule)) {
            deny.push(serde_json::Value::String(rule.to_owned()));
            changed = true;
        }
    }
    if !changed {
        return Ok(());
    }
    let mut contents = serde_json::to_string_pretty(&document).map_err(|error| MmError::State {
        message: format!("serializing {}: {error}", path.display()),
    })?;
    contents.push('\n');
    push_file(
        plan,
        Some(AgentClient::ClaudeCode),
        path,
        existing.as_deref(),
        contents,
        "deny running mm and mmd directly",
    );
    Ok(())
}

/// Claude Code reads `CLAUDE.md`, not `AGENTS.md`: import one from the other.
fn claude_memory(plan: &mut AgentSetupPlan, root: &Path) -> Result<(), MmError> {
    let path = root.join("CLAUDE.md");
    match read_optional(&path)? {
        None => push_file(
            plan,
            Some(AgentClient::ClaudeCode),
            &path,
            None,
            "@AGENTS.md\n".to_owned(),
            "import AGENTS.md",
        ),
        Some(text) if text.contains("AGENTS.md") => {}
        Some(_) => managed_section(plan, Some(AgentClient::ClaudeCode), &path)?,
    }
    Ok(())
}

/// Adds or refreshes the managed instructions between their markers.
fn managed_section(
    plan: &mut AgentSetupPlan,
    client: Option<AgentClient>,
    path: &Path,
) -> Result<(), MmError> {
    let existing = read_optional(path)?;
    let section = agent_instructions();
    let contents = match existing.as_deref() {
        None => section,
        Some(text) => match (text.find(SECTION_START), text.find(SECTION_END)) {
            (Some(start), Some(end)) if start < end => {
                let after = &text[end + SECTION_END.len()..];
                format!(
                    "{}{}{}",
                    &text[..start],
                    section.trim_end_matches('\n'),
                    after
                )
            }
            _ => {
                let separator = if text.ends_with("\n\n") || text.is_empty() {
                    ""
                } else if text.ends_with('\n') {
                    "\n"
                } else {
                    "\n\n"
                };
                format!("{text}{separator}{section}")
            }
        },
    };
    push_file(
        plan,
        client,
        path,
        existing.as_deref(),
        contents,
        "tell agents to use the mm MCP tools",
    );
    Ok(())
}

fn opencode(plan: &mut AgentSetupPlan, directory: &Path, mm: &str) -> Result<(), MmError> {
    let server =
        serde_json::json!({ "type": "local", "command": [mm, "mcp", "serve"], "enabled": true });
    // OpenCode reads opencode.json and opencode.jsonc; use whichever exists.
    let plain = directory.join("opencode.json");
    let commented = directory.join("opencode.jsonc");
    let path = if plain.is_file() || !commented.is_file() {
        plain
    } else {
        commented
    };
    if read_optional(&path)?.is_none() {
        let mut contents = serde_json::to_string_pretty(&serde_json::json!({
            "$schema": "https://opencode.ai/config.json",
            "mcp": { "mm": server },
        }))
        .map_err(|error| MmError::State {
            message: format!("serializing {}: {error}", path.display()),
        })?;
        contents.push('\n');
        push_file(
            plan,
            Some(AgentClient::Opencode),
            &path,
            None,
            contents,
            "register the mm MCP server",
        );
        return Ok(());
    }
    merge_json(
        plan,
        AgentClient::Opencode,
        &path,
        &["mcp", "mm"],
        server,
        "register the mm MCP server",
    )
}

fn codex(plan: &mut AgentSetupPlan, path: &Path, mm: &str) -> Result<(), MmError> {
    let existing = read_optional(path)?;
    if existing
        .as_deref()
        .is_some_and(|text| text.lines().any(|line| line.trim() == "[mcp_servers.mm]"))
    {
        plan.notes.push(format!(
            "Codex: {} already has mcp_servers.mm",
            path.display()
        ));
        return Ok(());
    }
    // TOML basic strings escape backslashes and quotes.
    let quoted = mm.replace('\\', "\\\\").replace('"', "\\\"");
    let table = format!("[mcp_servers.mm]\ncommand = \"{quoted}\"\nargs = [\"mcp\", \"serve\"]\n");
    let contents = match existing.as_deref() {
        None | Some("") => table,
        Some(text) if text.ends_with('\n') => format!("{text}\n{table}"),
        Some(text) => format!("{text}\n\n{table}"),
    };
    push_file(
        plan,
        Some(AgentClient::Codex),
        path,
        existing.as_deref(),
        contents,
        "register the mm MCP server",
    );
    Ok(())
}

/// Claude Code owns its user configuration, so its own command registers
/// the server.
fn claude_user(plan: &mut AgentSetupPlan, home: &Path, mm: &str) {
    // Asked from the home directory, so a project's own entry does not count.
    let registered = std::process::Command::new("claude")
        .args(["mcp", "get", "mm"])
        .current_dir(home)
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|output| output.status.success());
    if registered {
        plan.notes
            .push("Claude Code: an MCP server named mm is already registered".to_owned());
        return;
    }
    plan.commands.push(CommandStep {
        client: AgentClient::ClaudeCode,
        program: "claude".to_owned(),
        arguments: [
            "mcp", "add", "--scope", "user", "mm", "--", mm, "mcp", "serve",
        ]
        .map(str::to_owned)
        .to_vec(),
        summary: "register the mm MCP server for every project".to_owned(),
    });
}

#[cfg(test)]
#[path = "agent_clients/tests.rs"]
mod tests;
