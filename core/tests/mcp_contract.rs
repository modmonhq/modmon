use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};

use mm::adapters::config::ConfigLoader;
use mm::adapters::mcp::{AgentTrustStore, McpPolicyStore};
use mm::adapters::state::SqliteStateStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{McpAuditJournal, TargetStore};
use mm::domain::mcp::{McpAuditRecord, McpServerConfig};
use mm::domain::runtime::RuntimeKind;
use mm::domain::target::{TargetBootstrap, TargetDefinition, TargetId, TargetTransport};
use uuid::Uuid;

mod temp_dirs;

#[test]
fn private_limits_are_optional_private_and_bounded() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let temporary = temp_dirs::tempdir()?;
    let path = temporary.path().join("mcp.json");
    assert_eq!(
        McpPolicyStore::load(&path, false)?,
        McpServerConfig::default()
    );
    fs::write(
        &path,
        br#"{"schema_version":2,"ceiling":{"production":"view"}}"#,
    )?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let loaded = McpPolicyStore::load(&path, true)?;
    assert_eq!(loaded.ceiling.len(), 1);

    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
    assert!(McpPolicyStore::load(&path, true).is_err());
    Ok(())
}

#[test]
fn baseline_audit_schema_persists_attribution_without_request_content()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let path = temporary.path().join("state.sqlite3");
    let store = SqliteStateStore::open(&path)?;
    let record = McpAuditRecord {
        id: Uuid::new_v4(),
        request_id: Uuid::new_v4(),
        principal: "coding-agent".to_owned(),
        source: "mcp-stdio".to_owned(),
        project_root: PathBuf::from("/project"),
        environment: "development".to_owned(),
        target: "workstation".to_owned(),
        operation: "config_apply".to_owned(),
        mutation: true,
        plan_hash: Some("a".repeat(64)),
        outcome_code: "OK".to_owned(),
        occurred_at_unix_ms: 1_000,
    };
    store.append_mcp_audit(&record)?;

    let connection = rusqlite::Connection::open(path)?;
    let values = connection.query_row(
        "SELECT principal, source, operation, mutation, plan_hash, outcome_code FROM mcp_audit WHERE id=?1",
        [record.id.to_string()],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
            ))
        },
    )?;
    assert_eq!(
        values,
        (
            record.principal,
            record.source,
            record.operation,
            true,
            record.plan_hash,
            record.outcome_code,
        )
    );
    let columns = connection
        .prepare("PRAGMA table_info(mcp_audit)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    for prohibited in [
        "arguments",
        "request_body",
        "response_body",
        "logs",
        "secret",
    ] {
        assert!(!columns.iter().any(|column| column == prohibited));
    }
    Ok(())
}

#[test]
fn mcp_adapter_starts_no_subprocess_and_offers_no_shell() -> Result<(), Box<dyn std::error::Error>>
{
    let presentation = rust_source_tree(Path::new("src/presentation/mcp"))?;
    let composition = format!(
        "{}{}",
        fs::read_to_string("src/composition/mcp.rs")?,
        rust_source_tree(Path::new("src/composition/mcp"))?
    );
    for prohibited in ["Command::new", "process::Command", "shell_wrapper: true"] {
        assert!(!presentation.contains(prohibited));
        assert!(!composition.contains(prohibited));
    }
    assert!(composition.contains("McpUseCases for McpApplicationAdapter"));
    assert!(composition.contains("onboarding_service().plan_mutation"));
    assert!(composition.contains("apply_reviewed"));
    // Exec goes through the audited application use case, without a TTY or stdin.
    assert!(composition.contains("ExecTtyMode::Disabled"));
    assert!(composition.contains("ExecStdinMode::Closed"));
    // A remote host is only ever reached as the agent.
    assert!(composition.contains("RemoteIdentity::Agent"));
    assert!(!composition.contains("RemoteIdentity::User"));
    assert!(!composition.contains("super::remote_transport_context("));
    Ok(())
}

fn rust_source_tree(root: &Path) -> Result<String, std::io::Error> {
    let mut source = String::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            source.push_str(&rust_source_tree(&path)?);
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && path.file_name().is_some_and(|name| name != "tests.rs")
        {
            source.push_str(&fs::read_to_string(path)?);
        }
    }
    Ok(source)
}

#[test]
fn mcp_startup_errors_never_corrupt_protocol_stdout() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let output = temp_dirs::mm()
        .args(["--mm-home"])
        .arg(home.path())
        .args(["mcp", "serve", "--config"])
        .arg(home.path().join("missing.json"))
        .output()?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MM_IO"));
    Ok(())
}

/// A project with `local` (deploy and exec, from `mm init`) and `production`
/// (view), an mm home with a local target, and the block trusted.
struct Project {
    root: tempfile::TempDir,
    home: tempfile::TempDir,
}

impl Project {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let root = temp_dirs::tempdir()?;
        let home = temp_dirs::tempdir()?;
        // Claim the home before anything else is written into it.
        mm::adapters::state::MmHome::resolve(Some(home.path()))?;
        fs::write(root.path().join("compose.yaml"), "services: {}\n")?;
        ConfigLoader.initialize(root.path(), Some("mcp-contract"))?;
        let project = Self { root, home };
        project.edit(|config| {
            config["environments"]["production"] = config["environments"]["local"].clone();
        })?;
        FilesystemTargetStore::new(project.home.path().join("targets.yaml")).insert_target(
            "local",
            TargetDefinition::local(TargetId(Uuid::from_u128(91))),
        )?;
        project.trust()?;
        Ok(project)
    }

    fn edit(
        &self,
        change: impl FnOnce(&mut serde_yaml_ng::Value),
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.root.path().join(".config/mm.yaml");
        let mut config: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(&fs::read_to_string(&path)?)?;
        change(&mut config);
        fs::write(path, serde_yaml_ng::to_string(&config)?)?;
        Ok(())
    }

    fn trust(&self) -> Result<(), Box<dyn std::error::Error>> {
        let document = ConfigLoader.load_from(self.root.path())?;
        AgentTrustStore::new(self.home.path()).trust(
            &document.root,
            document.config.project.id,
            &document.config.agents,
            1,
        )?;
        Ok(())
    }

    fn mm(&self) -> assert_cmd::Command {
        let mut command = temp_dirs::mm();
        command
            .arg("--root")
            .arg(self.root.path())
            .arg("--mm-home")
            .arg(self.home.path());
        command
    }
}

/// `mm mcp serve` as a coding tool runs it: a child process speaking
/// newline-delimited JSON-RPC, started inside the agent's environment.
struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Session {
    fn start(project: &Project) -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with(project, &[])
    }

    /// `start`, with these variables in the server's environment.
    fn start_with(
        project: &Project,
        environment: &[(&str, String)],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin!("mm"))
            .arg("--mm-home")
            .arg(project.home.path())
            .args(["mcp", "serve"])
            .current_dir(project.root.path())
            // The server itself must start for an agent.
            .env("CLAUDECODE", "1")
            .envs(environment.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().ok_or("stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("stdout")?);
        let mut session = Self {
            child,
            stdin,
            stdout,
            next_id: 0,
        };
        let initialized = session.request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "contract-test", "version": "0"},
            }),
        )?;
        assert!(
            initialized["instructions"]
                .as_str()
                .is_some_and(|text| text.contains("MM_AGENT_HANDOFF")),
            "{initialized}"
        );
        session.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }))?;
        Ok(session)
    }

    fn send(&mut self, message: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
        writeln!(self.stdin, "{message}")?;
        self.stdin.flush()?;
        Ok(())
    }

    fn request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        }))?;
        loop {
            let mut line = String::new();
            if self.stdout.read_line(&mut line)? == 0 {
                return Err("the MCP server closed its output".into());
            }
            let message: serde_json::Value = serde_json::from_str(&line)?;
            if message["id"] == id {
                if !message["error"].is_null() {
                    return Err(format!("{method} failed: {}", message["error"]).into());
                }
                return Ok(message["result"].clone());
            }
        }
    }

    /// Calls a tool and returns its structured result.
    fn call(
        &mut self,
        tool: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let result = self.request(
            "tools/call",
            serde_json::json!({"name": tool, "arguments": arguments}),
        )?;
        Ok(result["structuredContent"].clone())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _killed = self.child.kill();
        let _reaped = self.child.wait();
    }
}

#[test]
fn an_agent_session_is_limited_per_environment_and_hands_the_rest_to_the_user()
-> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new()?;
    let mut session = Session::start(&project)?;

    let tools = session.request("tools/list", serde_json::json!({}))?;
    let names = tools["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for expected in [
        "mm_access",
        "mm_read",
        "mm_logs",
        "mm_exec",
        "mm_deploy_plan",
    ] {
        assert!(names.iter().any(|name| name == expected), "{names:?}");
    }

    let access = session.call("mm_access", serde_json::json!({}))?;
    assert_eq!(access["policy_trusted"], true, "{access}");
    assert_eq!(
        access["environments"],
        serde_json::json!([
            {"environment": "local", "access": "deploy +exec", "level": "deploy", "exec": true},
            {"environment": "production", "access": "view", "level": "view", "exec": false},
        ])
    );

    // View works in production…
    let read = session.call(
        "mm_read",
        serde_json::json!({"scope": {"environment": "production"}, "kind": "project"}),
    )?;
    assert_eq!(read["selected_environment"], "production", "{read}");

    // …exec and deploy are the user's to run, with the exact commands.
    let exec = session.call(
        "mm_exec",
        serde_json::json!({
            "scope": {"environment": "production"},
            "service": "api",
            "argv": ["rails", "db:migrate"],
        }),
    )?;
    assert_eq!(exec["error"]["code"], "MM_AGENT_HANDOFF", "{exec}");
    assert_eq!(
        exec["error"]["handoff"]["user_commands"],
        serde_json::json!(["mm --environment production service exec api -- rails db:migrate"])
    );
    let deploy = session.call(
        "mm_deploy_apply",
        serde_json::json!({
            "scope": {"environment": "production"},
            "plan_hash": "a".repeat(64),
            "approval_token": "b".repeat(64),
            "confirmation": format!("apply:{}", "a".repeat(64)),
        }),
    )?;
    assert_eq!(deploy["error"]["code"], "MM_AGENT_HANDOFF", "{deploy}");
    assert_eq!(deploy["error"]["handoff"]["requires"], "level deploy");

    // In the development environment exec is granted: the call passes the
    // grant and fails later, at a runtime this test does not have.
    let local = session.call(
        "mm_exec",
        serde_json::json!({
            "scope": {"environment": "local", "runtime": "docker-compose"},
            "service": "api",
            "argv": ["true"],
            "timeout_seconds": 5,
        }),
    )?;
    assert_ne!(local["error"]["code"], "MM_AGENT_HANDOFF", "{local}");
    assert!(!local["error"]["code"].is_null(), "{local}");

    // The agent edits the block to give itself production. Nothing changes
    // except that it now has view everywhere.
    project.edit(|config| {
        config["agents"]["production"] =
            serde_yaml_ng::from_str("{level: deploy, exec: true}").unwrap_or_default();
    })?;
    let access = session.call("mm_access", serde_json::json!({}))?;
    assert_eq!(access["policy_trusted"], false, "{access}");
    let widened = session.call(
        "mm_exec",
        serde_json::json!({
            "scope": {"environment": "production"},
            "service": "api",
            "argv": ["true"],
        }),
    )?;
    assert_eq!(widened["error"]["code"], "MM_AGENT_HANDOFF", "{widened}");
    assert_eq!(widened["error"]["handoff"]["reason"], "policy_not_trusted");
    assert_eq!(
        widened["error"]["handoff"]["user_commands"],
        serde_json::json!(["mm agent trust"])
    );

    // Every refusal is in the audit journal.
    drop(session);
    let connection = rusqlite::Connection::open(project.home.path().join("state.sqlite3"))?;
    let handoffs: i64 = connection.query_row(
        "SELECT count(*) FROM mcp_audit WHERE outcome_code = 'MM_AGENT_HANDOFF'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(handoffs, 3);
    Ok(())
}

#[test]
fn mm_refuses_to_run_directly_for_an_agent() -> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new()?;
    // A person reads the status and trusts the block.
    let status = project.mm().args(["agent", "status"]).output()?;
    assert!(status.status.success(), "{status:?}");
    let text = String::from_utf8_lossy(&status.stdout);
    assert!(
        text.contains("deploy +exec") && text.contains("trusted on this device"),
        "{text}"
    );

    for (marker, arguments) in [
        ("CLAUDECODE", vec!["agent", "status"]),
        ("AI_AGENT", vec!["agent", "trust"]),
        ("OPENCODE", vec!["deploy", "plan"]),
        ("MM_AGENT", vec!["service", "exec", "api", "--", "true"]),
        ("CODEX_SANDBOX", vec!["config", "validate"]),
    ] {
        let output = project.mm().env(marker, "1").args(&arguments).output()?;
        assert_eq!(output.status.code(), Some(3), "{marker}: {output:?}");
        let message = String::from_utf8_lossy(&output.stderr);
        assert!(
            message.contains("mm_access") && message.contains(marker),
            "{message}"
        );
        assert!(output.stdout.is_empty());
    }
    // Help stays available, so the refusal is never a dead end for a person
    // who launched a shell from a coding tool.
    let help = project.mm().env("CLAUDECODE", "1").arg("--help").output()?;
    assert!(help.status.success());
    Ok(())
}

#[test]
fn trusting_takes_the_reviewed_hash_and_setup_writes_reviewable_files()
-> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new()?;
    project.edit(|config| {
        config["agents"]["production"] = "operate".into();
    })?;
    let review = project
        .mm()
        .args(["--output", "json", "agent", "trust"])
        .output()?;
    let status: serde_json::Value = serde_json::from_slice(&review.stdout)?;
    assert_eq!(status["trusted"], false, "{status}");
    assert_eq!(status["environments"][1]["granted"], "operate");
    assert_eq!(status["environments"][1]["in_force"], "view");
    let hash = status["policy_hash"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    // The review names its plan hash and the command that trusts it.
    assert_eq!(status["plan_hash"], hash.as_str(), "{status}");
    assert_eq!(
        status["next"],
        format!("mm agent trust --plan {hash} --yes").as_str(),
        "{status}"
    );

    // The block changes after the review: the reviewed hash no longer applies.
    project.edit(|config| {
        config["agents"]["production"] = "deploy".into();
    })?;
    let stale = project
        .mm()
        .args(["agent", "trust", "--plan", &hash, "--yes"])
        .output()?;
    assert!(!stale.status.success());
    project.edit(|config| {
        config["agents"]["production"] = "operate".into();
    })?;
    let trusted = project
        .mm()
        .args([
            "--output", "json", "agent", "trust", "--plan", &hash, "--yes",
        ])
        .output()?;
    let status: serde_json::Value = serde_json::from_slice(&trusted.stdout)?;
    assert_eq!(status["trusted"], true, "{status}");
    assert!(status.get("next").is_none(), "{status}");
    assert_eq!(status["environments"][1]["in_force"], "operate");

    // Setup shows its changes first and writes them only with --yes.
    let preview = project
        .mm()
        .args([
            "agent",
            "setup",
            "--client",
            "claude-code",
            "--client",
            "opencode",
        ])
        .output()?;
    assert!(preview.status.success(), "{preview:?}");
    assert!(String::from_utf8_lossy(&preview.stdout).contains("would create"));
    assert!(!project.root.path().join(".mcp.json").exists());
    let applied = project
        .mm()
        .args([
            "agent",
            "setup",
            "--client",
            "claude-code",
            "--client",
            "opencode",
            "--yes",
        ])
        .output()?;
    assert!(applied.status.success(), "{applied:?}");
    let server: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(project.root.path().join(".mcp.json"))?)?;
    assert_eq!(
        server["mcpServers"]["mm"],
        serde_json::json!({"command": "mm", "args": ["mcp", "serve"]})
    );
    let opencode: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        project.root.path().join("opencode.json"),
    )?)?;
    assert_eq!(
        opencode["mcp"]["mm"]["command"],
        serde_json::json!(["mm", "mcp", "serve"])
    );
    let settings = fs::read_to_string(project.root.path().join(".claude/settings.json"))?;
    assert!(settings.contains("Bash(mm:*)"));
    assert!(fs::read_to_string(project.root.path().join("AGENTS.md"))?.contains("mm_access"));
    assert_eq!(
        fs::read_to_string(project.root.path().join("CLAUDE.md"))?,
        "@AGENTS.md\n"
    );
    // A second run has nothing left to do.
    let again = project
        .mm()
        .args([
            "agent",
            "setup",
            "--client",
            "claude-code",
            "--client",
            "opencode",
            "--yes",
        ])
        .output()?;
    assert!(String::from_utf8_lossy(&again.stdout).contains("Nothing to change"));
    Ok(())
}

/// An `ssh` whose host refuses the key it is offered, in OpenSSH's words.
#[cfg(unix)]
const REFUSING_SSH: &str =
    "#!/bin/sh\necho 'mm@203.0.113.11: Permission denied (publickey).' >&2\nexit 255\n";

/// A host that refuses a key says so in mm's words, with the next step,
/// to the user on the command line and to an agent over MCP alike. What
/// `ssh` printed reaches neither the model nor the fixed sentence about
/// provider output.
#[cfg(unix)]
#[test]
fn a_host_that_refuses_the_key_is_named_with_the_next_step_for_the_user_and_the_agent()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt as _;

    let project = Project::new()?;
    FilesystemTargetStore::new(project.home.path().join("targets.yaml")).insert_target(
        "remote",
        TargetDefinition {
            id: TargetId(Uuid::from_u128(92)),
            transport: TargetTransport::Ssh {
                host: "203.0.113.11".to_owned(),
                port: 22,
                user: "mm".to_owned(),
                host_key_fingerprint: "SHA256:kmYcvdi2GkPeWxB6XLjrZB8JHsy2Hm8luHMFp9GMvqk"
                    .to_owned(),
                host_key: Some(
                    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
                        .to_owned(),
                ),
                identity_file: None,
                remote_root: None,
                daemon_fingerprint: Some("a".repeat(64)),
            },
            bootstrap: TargetBootstrap::Ansible,
            ingress: None,
            default_runtime: Some(RuntimeKind::DockerCompose),
            available_runtimes: vec![RuntimeKind::DockerCompose],
            runtime: None,
            nodes: Vec::new(),
        },
    )?;
    project.edit(|config| {
        config["environments"]["production"]["target"] = "remote".into();
    })?;
    project.trust()?;
    project.mm().args(["key", "create"]).assert().success();
    project
        .mm()
        .args(["key", "create", "--agent"])
        .assert()
        .success();

    let tools = temp_dirs::tempdir()?;
    let ssh = tools.path().join("ssh");
    fs::write(&ssh, REFUSING_SSH)?;
    fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755))?;
    let path = format!(
        "{}:{}",
        tools.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );

    // The user, with the device key.
    let whoami = project
        .mm()
        .args(["--runtime", "docker-compose", "--environment", "production"])
        .args(["team", "whoami"])
        .env("PATH", &path)
        .env("MM_SSH_REUSE", "off")
        .assert()
        .failure();
    let said = String::from_utf8_lossy(&whoami.get_output().stderr).into_owned();
    assert!(said.contains("MM_UNREACHABLE"), "{said}");
    assert!(
        said.contains("target 'remote' (203.0.113.11:22) refused this device's key."),
        "{said}"
    );
    assert!(said.contains("`mm key show`"), "{said}");
    assert!(said.contains("`mm team sync`"), "{said}");

    // The agent, with the agent key: the same error, named as its own key.
    let mut session = Session::start_with(
        &project,
        &[
            ("PATH", path),
            ("MM_KEY_VAULT", "file".to_owned()),
            ("MM_SSH_REUSE", "off".to_owned()),
        ],
    )?;
    let read = session.call(
        "mm_read",
        serde_json::json!({
            "scope": {"environment": "production", "runtime": "docker-compose"},
            "kind": "services",
        }),
    )?;
    let error = &read["error"];
    assert_eq!(error["code"], "MM_UNREACHABLE", "{read}");
    assert_eq!(
        error["message"], "target 'remote' (203.0.113.11:22) refused this device's agent key.",
        "{read}"
    );
    let next = error["next_action"].as_str().unwrap_or_default();
    for step in [
        "`mm key show --agent`",
        "`mm team join --agent`",
        "`mm team sync`",
        "tell the user",
    ] {
        assert!(next.contains(step), "{step}: {next}");
    }
    let answer = read.to_string();
    assert!(!answer.contains("Permission denied"), "{answer}");
    assert!(!answer.contains("publickey"), "{answer}");
    assert!(!answer.contains("provider output"), "{answer}");
    Ok(())
}

#[test]
fn an_unreachable_host_in_mms_own_words_leaves_out_what_another_program_printed() {
    let error = mm::MmError::Unreachable {
        subject: "node 'app-2' (203.0.113.12:22)".to_owned(),
        problem: "did not answer (ssh: connect to host 203.0.113.12 port 22: Connection timed out)"
            .to_owned(),
        help: "Check that the host is up and that this machine can reach its SSH port".to_owned(),
    };
    assert_eq!(
        error.own_words(),
        Some((
            "node 'app-2' (203.0.113.12:22) did not answer".to_owned(),
            "Check that the host is up and that this machine can reach its SSH port".to_owned(),
        ))
    );
    // The command line still prints the detail.
    assert!(error.to_string().contains("Connection timed out"));
    // No other error has words of its own to pass on.
    assert_eq!(
        mm::MmError::RuntimeResponse {
            message: "docker: no such container".to_owned(),
        }
        .own_words(),
        None
    );
}

// Found on the test VMs: an agent's `mm_deploy_apply` on a cluster ended
// with MM_STATE after the 15 s request bound, while the first node had
// begun the rollout; the rollout stopped there and was left unfinished.
#[test]
fn applying_a_deployment_runs_to_its_end_and_every_other_call_keeps_its_bound() {
    let config = McpServerConfig::default();
    // Found integrating: `mm_build_apply` on a local environment ended
    // MM_TIMEOUT after the bound and the build it had begun was dropped, so no
    // image was ever built and the operation stayed recorded as running.
    for operation in ["deploy_apply", "build_apply", "job_apply", "setup_apply"] {
        assert_eq!(config.call_bound(operation), None, "{operation}");
    }
    let bound = Some(std::time::Duration::from_millis(config.request_timeout_ms));
    for operation in [
        "deploy_plan",
        "build_plan",
        "config_apply",
        "read",
        "setup_plan",
    ] {
        assert_eq!(config.call_bound(operation), bound, "{operation}");
    }
}

/// An agent lists the optional units and turns one on or off only with
/// level deploy on the development environment; otherwise it hands the
/// user the `mm dev` command.
#[test]
fn optional_units_turn_on_and_off_only_with_deploy_on_the_development_environment()
-> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new()?;
    let root = project.root.path();
    fs::create_dir(root.join("status"))?;
    fs::write(
        root.join("compose.yaml"),
        "services:\n  status:\n    image: example/status\n",
    )?;
    project.edit(|config| {
        config["modules"] = serde_yaml_ng::from_str(
            "status: {path: status, development: optional, services: {status: {stack: app}}}",
        )
        .unwrap_or_default();
    })?;
    let mut session = Session::start(&project)?;

    let refused = session.call(
        "mm_dev_unit",
        serde_json::json!({"scope": {"environment": "production"}, "unit": "status", "on": true}),
    )?;
    assert_eq!(refused["error"]["code"], "MM_AGENT_HANDOFF", "{refused}");
    assert_eq!(
        refused["error"]["handoff"]["user_commands"],
        serde_json::json!(["mm --environment production dev enable status"])
    );
    assert!(!root.join(".config/mm.overrides.yaml").exists());

    let listed = session.call(
        "mm_read",
        serde_json::json!({"scope": {"environment": "local"}, "kind": "optional"}),
    )?;
    assert_eq!(listed["environment"], "local", "{listed}");
    assert_eq!(listed["units"][0]["name"], "status", "{listed}");
    assert_eq!(listed["units"][0]["on"], false, "{listed}");

    let on = session.call(
        "mm_dev_unit",
        serde_json::json!({"scope": {"environment": "local"}, "unit": "status", "on": true}),
    )?;
    assert_eq!(on["started"], serde_json::json!(["status/status"]), "{on}");
    assert!(fs::read_to_string(root.join(".config/mm.overrides.yaml"))?.contains("- status"));
    let off = session.call(
        "mm_dev_unit",
        serde_json::json!({"scope": {"environment": "local"}, "unit": "status", "on": false}),
    )?;
    assert_eq!(
        off["stopped"],
        serde_json::json!(["status/status"]),
        "{off}"
    );
    Ok(())
}

/// A refusal mm makes from the configuration and the request reaches the
/// agent in mm's own words and with its code; what a provider printed
/// still never does.
#[test]
fn mms_own_refusals_reach_the_agent_as_mm_says_them() -> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new()?;
    let mut session = Session::start(&project)?;

    // A rollback is for a remote environment; `local` runs on this machine.
    let rollback = session.call(
        "mm_deploy_plan",
        serde_json::json!({
            "scope": {"environment": "local", "runtime": "docker-compose"},
            "rollback": true,
        }),
    )?;
    assert_eq!(rollback["error"]["code"], "MM_ENVIRONMENT", "{rollback}");
    let message = rollback["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.starts_with("a rollback returns a remote environment to what its hosts ran before, and environment 'local' runs on this machine."),
        "{rollback}"
    );
    let next = rollback["error"]["next_action"]
        .as_str()
        .unwrap_or_default();
    assert!(
        !next.contains("mm init") && next.contains("names what to do instead"),
        "{rollback}"
    );

    // A bound of a tool, said as mm says it.
    let logs = session.call(
        "mm_logs",
        serde_json::json!({"scope": {"environment": "local"}, "service": "api", "tail": 0}),
    )?;
    assert_eq!(logs["error"]["code"], "MM_ENVIRONMENT", "{logs}");
    assert_eq!(
        logs["error"]["message"], "MCP log tail exceeds the finite log bound.",
        "{logs}"
    );

    // Schedules of a remote environment change with its deployment.
    let schedules = mm::domain::scheduler::remote_schedules("production");
    assert_eq!(schedules.code(), "MM_REMOTE_SCHEDULES");
    assert_eq!(schedules.exit_code(), 3);
    assert!(
        schedules.refusal().is_some_and(|text| text
            .starts_with("the schedules of 'production' are reconciled by its deployment")),
        "{schedules}"
    );
    // Only an error in mm's own words is a refusal; any other reaches the
    // agent as the command line prints it, with what to do next.
    let other = mm::domain::error::MmError::Environment {
        message: "provider said: something".to_owned(),
    };
    assert_eq!(other.refusal(), None);
    assert_eq!(other.agent_message(), other.to_string());
    Ok(())
}

/// An agent reads an error as the command line prints it, with secrets
/// redacted as the command log redacts them; what another program printed
/// never reaches it, and an error made of that names the step instead.
#[test]
fn an_agent_reads_the_cli_message_without_secrets_or_provider_output() {
    use mm::domain::diagnostic::Diagnostic;
    use mm::domain::error::{MmError, MutationStatus, printed};

    let leaky = MmError::Environment {
        message: "could not log in with password=SENTINEL-VALUE-4471 as ops".to_owned(),
    };
    assert_eq!(
        leaky.agent_message(),
        "environment error: could not log in with password=[REDACTED] as ops"
    );
    let key = MmError::Build {
        message: concat!(
            "-----BEGIN ",
            "OPENSSH PRIVATE KEY-----\nSENTINEL-KEY\n-----END ",
            "OPENSSH PRIVATE KEY-----\n"
        )
        .to_owned(),
    };
    assert!(
        !key.agent_message().contains("SENTINEL"),
        "{}",
        key.agent_message()
    );

    let failed = MmError::ProcessFailed {
        program: "docker".to_owned(),
        code: Some(1),
        stderr: "Error response from daemon: SENTINEL-STDERR".to_owned(),
    };
    assert!(failed.to_string().contains("SENTINEL-STDERR"));
    assert_eq!(
        failed.agent_message(),
        "docker failed with exit code 1; what it printed is left out here"
    );
    for withheld in [
        MmError::RuntimeResponse {
            message: "SENTINEL-ENGINE-ANSWER".to_owned(),
        },
        MmError::Remote {
            code: "MM_PROCESS_FAILED",
            message: "podman failed with exit code 125: SENTINEL-HOST-STDERR".to_owned(),
        },
        MmError::operation_failure(
            "op-1",
            MutationStatus::Unknown,
            failed,
            vec!["rm: SENTINEL-CLEANUP".to_owned()],
            "run `mm deploy` again",
        ),
    ] {
        let message = withheld.agent_message();
        assert!(!message.contains("SENTINEL"), "{message}");
        assert!(message.contains("left out here"), "{message}");
    }

    // What a program printed stays out wherever its text goes: quoted in
    // mm's message, in another error's message, or in the message a host
    // sent.
    let quoted = MmError::Environment {
        message: format!(
            "git merge-base failed: {}",
            printed("fatal: unable to access 'https://ci:SENTINEL-GIT@git.example.test/x.git/'")
        ),
    };
    let nested = MmError::Target {
        message: format!(
            "node app-2 could not be read ({})",
            MmError::ProcessFailed {
                program: "ssh".to_owned(),
                code: Some(255),
                stderr: "SENTINEL-SSH".to_owned(),
            }
        ),
    };
    let provider = MmError::Secret {
        message: format!(
            "the store did not start: {}",
            MmError::RuntimeResponse {
                message: "the engine answered with 500: SENTINEL-BODY".to_owned(),
            }
        ),
    };
    let hosted = MmError::Security {
        message: MmError::Build {
            message: format!("buildx said {}", printed("SENTINEL-HOST")),
        }
        .to_string(),
    };
    for error in [quoted, nested, provider, hosted] {
        let message = error.agent_message();
        assert!(!message.contains("SENTINEL"), "{message}");
        assert!(
            message.contains("[what it printed is left out]"),
            "{message}"
        );
        assert!(error.to_string().contains("SENTINEL"), "{error}");
    }
    let unmarked = MmError::Environment {
        message: "could not fetch https://ci:SENTINEL-URL@git.example.test/x.git with Bearer SENTINEL-BEARER as ghp_SENTINELTOKEN0000".to_owned(),
    };
    let message = unmarked.agent_message();
    assert!(!message.contains("SENTINEL"), "{message}");
    assert!(
        message.contains("https://[REDACTED]@git.example.test/x.git"),
        "{message}"
    );

    let invalid = MmError::invalid(&[
        Diagnostic::error("MM_FIRST", "modules.web.path does not exist").at_location(
            "/work/.config/mm.yaml",
            7,
            3,
        ),
        Diagnostic::warning("MM_ADVICE", "not an error"),
        Diagnostic::error("MM_SECOND", "second"),
        Diagnostic::error("MM_THIRD", "third"),
        Diagnostic::error("MM_FOURTH", "fourth"),
    ]);
    assert_eq!(invalid.code(), "MM_CONFIG_INVALID");
    assert_eq!(
        invalid.agent_message(),
        "configuration validation failed with 4 error(s): MM_FIRST at /work/.config/mm.yaml:7: modules.web.path does not exist; MM_SECOND: second; MM_THIRD: third; `mm config validate` lists the other 1"
    );
}

/// The project of the release tests: `web` runs everywhere, `worker` only
/// in production, and only `worker`'s template reads a release input.
fn release_project() -> Result<Project, Box<dyn std::error::Error>> {
    let project = Project::new()?;
    let root = project.root.path();
    fs::write(
        root.join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n  worker:\n    image: example/worker\n",
    )?;
    for module in ["web", "worker"] {
        fs::create_dir(root.join(module))?;
    }
    fs::write(
        root.join("worker/.env.example"),
        "BUILD=0 # mm:from=RELEASE.BUILD_ID\n",
    )?;
    project.edit(|config| {
        config["modules"] = serde_yaml_ng::from_str(
            "web: {path: web, services: {web: {stack: app}}}\nworker: {path: worker, env: {template: .env.example}, services: {worker: {stack: app, environments: [production]}}}",
        )
        .unwrap_or_default();
        config["release-inputs"] =
            serde_yaml_ng::from_str("BUILD_ID: {pattern: \"^[0-9]+$\"}").unwrap_or_default();
    })?;
    Ok(project)
}

fn engine_off() -> [(&'static str, String); 1] {
    [("MM_ENGINE_API", "off".to_owned())]
}

/// A local plan needs only the release inputs its own templates read, and
/// plans through MCP as `mm deploy plan` plans; the inputs it is given
/// become part of the plan. An environment whose template reads an input
/// names it, where it is read, and the option that gives it.
#[test]
fn a_plan_needs_only_the_release_inputs_its_environment_reads()
-> Result<(), Box<dyn std::error::Error>> {
    let project = release_project()?;
    let printed = project
        .mm()
        .args(["--runtime", "docker-compose", "--environment", "local"])
        .args(["--output", "json", "deploy", "plan"])
        .output()?;
    assert!(
        printed.status.success(),
        "{}",
        String::from_utf8_lossy(&printed.stderr)
    );
    let printed: serde_json::Value = serde_json::from_slice(&printed.stdout)?;

    let mut session = Session::start_with(&project, &engine_off())?;
    let scope = serde_json::json!({"environment": "local", "runtime": "docker-compose"});
    let planned = session.call("mm_deploy_plan", serde_json::json!({"scope": scope}))?;
    assert!(planned["error"].is_null(), "{planned}");
    assert_eq!(planned["plan_hash"], printed["plan_hash"], "{planned}");
    assert!(planned["plan"]["release"]["inputs"].is_null(), "{planned}");

    let released = session.call(
        "mm_deploy_plan",
        serde_json::json!({
            "scope": scope,
            "release": {"name": "v7", "inputs": {"BUILD_ID": "7"}},
        }),
    )?;
    assert_eq!(released["plan"]["release"]["name"], "v7", "{released}");
    assert_eq!(
        released["plan"]["release"]["inputs"]["BUILD_ID"], "7",
        "{released}"
    );
    assert_ne!(released["plan_hash"], planned["plan_hash"]);

    let refused = session.call(
        "mm_deploy_plan",
        serde_json::json!({"scope": {"environment": "production", "runtime": "docker-compose"}}),
    )?;
    assert_eq!(refused["error"]["code"], "MM_RELEASE_INPUT", "{refused}");
    let message = refused["error"]["message"].as_str().unwrap_or_default();
    for named in [
        "BUILD_ID (",
        "worker/.env.example line 1",
        "--release-input BUILD_ID=VALUE",
    ] {
        assert!(message.contains(named), "{named}: {message}");
    }
    let cli = project
        .mm()
        .args(["--runtime", "docker-compose", "--environment", "production"])
        .args(["deploy", "plan"])
        .output()?;
    let said = String::from_utf8_lossy(&cli.stderr);
    assert!(
        said.contains(&format!(
            "error[MM_RELEASE_INPUT]: {}",
            message.trim_end_matches('.')
        )),
        "{said}"
    );
    Ok(())
}

/// A configuration that does not validate is named finding by finding,
/// in the words and with the code the command line prints.
#[test]
fn a_validation_failure_names_the_finding_over_mcp_as_on_the_command_line()
-> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new()?;
    project.edit(|config| {
        config["targets"] =
            serde_yaml_ng::from_str("lab: {host: 203.0.113.20}").unwrap_or_default();
    })?;
    let mut session = Session::start_with(&project, &engine_off())?;
    let refused = session.call(
        "mm_deploy_plan",
        serde_json::json!({"scope": {"environment": "local", "runtime": "docker-compose"}}),
    )?;
    assert_eq!(refused["error"]["code"], "MM_CONFIG_INVALID", "{refused}");
    let message = refused["error"]["message"].as_str().unwrap_or_default();
    for named in [
        "MM_REPO_TARGET_INVALID at ",
        ".config/mm.yaml",
        "target 'lab'",
    ] {
        assert!(message.contains(named), "{named}: {message}");
    }
    assert!(!message.contains("provider output"), "{message}");
    let cli = project
        .mm()
        .args(["--runtime", "docker-compose", "--environment", "local"])
        .args(["deploy", "plan"])
        .output()?;
    let said = String::from_utf8_lossy(&cli.stderr);
    assert!(
        said.contains(&format!(
            "error[MM_CONFIG_INVALID]: {}",
            message.trim_end_matches('.')
        )),
        "{said}"
    );
    Ok(())
}

/// A `git` that fails the way it does outside a repository, with a line of
/// its own that must not reach the agent.
#[cfg(unix)]
const FAILING_GIT: &str = "#!/bin/sh\necho 'fatal: SENTINEL-GIT-STDERR' >&2\nexit 128\n";

#[cfg(unix)]
fn git(root: &Path, arguments: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=mm-test",
            "-c",
            "user.email=mm@example.test",
        ])
        .args(["-c", "commit.gpgsign=false"])
        .args(arguments)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned().into())
    }
}

/// A build context that would ship a sensitive file is refused over MCP
/// naming the file; a program that fails is named with its exit code, and
/// what it printed stays out.
#[cfg(unix)]
#[test]
fn a_build_refusal_names_the_file_and_a_failed_program_keeps_its_output()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt as _;

    let project = Project::new()?;
    let root = project.root.path();
    fs::write(
        root.join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n",
    )?;
    fs::create_dir(root.join("web"))?;
    fs::write(root.join("web/Dockerfile"), "FROM scratch\n")?;
    fs::write(root.join("web/.env"), "EXAMPLE=1\n")?;
    project.edit(|config| {
        config["modules"] = serde_yaml_ng::from_str(
            "web: {path: web, build: {image: example/web}, services: {web: {stack: app}}}",
        )
        .unwrap_or_default();
    })?;
    let scope = serde_json::json!({"environment": "local", "runtime": "docker-compose"});

    let tools = temp_dirs::tempdir()?;
    fs::write(tools.path().join("git"), FAILING_GIT)?;
    fs::set_permissions(tools.path().join("git"), fs::Permissions::from_mode(0o755))?;
    let path = format!(
        "{}:{}",
        tools.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut session = Session::start_with(
        &project,
        &[("PATH", path), ("MM_ENGINE_API", "off".to_owned())],
    )?;
    let failed = session.call("mm_build_plan", serde_json::json!({"scope": scope}))?;
    assert_eq!(failed["error"]["code"], "MM_PROCESS_FAILED", "{failed}");
    assert_eq!(
        failed["error"]["message"],
        "git failed with exit code 128; what it printed is left out here.",
        "{failed}"
    );
    assert!(!failed.to_string().contains("SENTINEL"), "{failed}");
    drop(session);

    git(root, &["init", "--quiet", "--initial-branch=main"])?;
    git(root, &["add", "--all"])?;
    git(root, &["commit", "--quiet", "--message", "fixture"])?;
    let mut session = Session::start_with(&project, &engine_off())?;
    let refused = session.call("mm_build_plan", serde_json::json!({"scope": scope}))?;
    assert_eq!(refused["error"]["code"], "MM_SECURITY", "{refused}");
    let message = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains(".env") && message.contains("build context"),
        "{refused}"
    );
    assert!(!message.contains("authorization"), "{refused}");
    Ok(())
}

/// `mm_read` of `health` on an environment whose status cannot be read
/// returns what `mm config validate` finds and the error that stopped the
/// status, rather than failing with it.
#[test]
fn health_of_a_broken_environment_returns_its_findings() -> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new()?;
    let root = project.root.path();
    fs::write(
        root.join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n",
    )?;
    fs::create_dir(root.join("web"))?;
    project.edit(|config| {
        config["modules"] = serde_yaml_ng::from_str(
            "web: {path: web, env: {template: .env.example}, services: {web: {stack: app}}}",
        )
        .unwrap_or_default();
        config["environments"]["production"]["target"] = "lab".into();
    })?;
    FilesystemTargetStore::new(project.home.path().join("targets.yaml")).insert_target(
        "lab",
        TargetDefinition {
            available_runtimes: vec![RuntimeKind::PodmanCompose],
            ..TargetDefinition::local(TargetId(Uuid::from_u128(93)))
        },
    )?;
    let mut session = Session::start_with(&project, &engine_off())?;
    let health = session.call(
        "mm_read",
        serde_json::json!({
            "scope": {"environment": "production", "runtime": "docker-compose"},
            "kind": "health",
        }),
    )?;
    assert!(health["error"].is_null(), "{health}");
    assert!(health["runtime"].is_null(), "{health}");
    let findings = health["validation"]["diagnostics"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let found = |code: &str, text: &str| {
        findings.iter().any(|finding| {
            finding["code"] == code
                && finding["message"]
                    .as_str()
                    .is_some_and(|message| message.contains(text))
        })
    };
    assert!(found("MM_TARGET", "does not offer runtime"), "{health}");
    assert!(found("MM_ENV_TEMPLATE_READ", ""), "{health}");
    Ok(())
}

#[test]
fn an_agent_reads_the_commands_run_in_the_project() -> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new()?;
    let validated = temp_dirs::mm()
        .arg("--root")
        .arg(project.root.path())
        .arg("--mm-home")
        .arg(project.home.path())
        .args(["config", "validate"])
        .output()?;
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );
    let mut session = Session::start(&project)?;
    let listed = session.call(
        "mm_read",
        serde_json::json!({
            "scope": {"environment": "production"},
            "kind": "commands",
            "filter": {"outcome": "succeeded"},
        }),
    )?;
    let commands = listed["commands"].as_array().ok_or("commands")?;
    assert_eq!(commands.len(), 1, "{listed}");
    let id = commands[0]["id"].as_str().ok_or("id")?;
    let read = session.call(
        "mm_read",
        serde_json::json!({
            "scope": {"environment": "production"},
            "kind": "commands",
            "command": id,
        }),
    )?;
    let output = read["command"]["output"].as_str().ok_or("output")?;
    assert!(!output.is_empty() && !output.contains('\u{1b}'), "{read}");
    assert_eq!(read["command"]["outcome"], "succeeded", "{read}");
    let none = session.call(
        "mm_read",
        serde_json::json!({
            "scope": {"environment": "production"},
            "kind": "commands",
            "filter": {"outcome": "failed"},
        }),
    )?;
    assert_eq!(none["commands"], serde_json::json!([]), "{none}");
    Ok(())
}

#[test]
fn an_agent_reads_what_mm_deploy_logs_prints() -> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new()?;
    let mut session = Session::start(&project)?;
    let read = session.call(
        "mm_read",
        serde_json::json!({
            "scope": {"environment": "local", "runtime": "docker-compose"},
            "kind": "deploy_logs",
        }),
    )?;
    assert_eq!(read["items"], serde_json::json!([]), "{read}");
    assert_eq!(read["total_items"], 0, "{read}");
    Ok(())
}

/// An agent reads the managed secret store of a cluster environment as
/// `mm secret provider status` prints it: the same report, member by member.
#[test]
fn an_agent_reads_the_managed_store_as_provider_status_prints_it()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let project = Project::new()?;
    let provider: serde_yaml_ng::Value = serde_yaml_ng::from_str(
        "{driver: openbao, mode: managed, scope: environment, release: \"2.4\"}",
    )?;
    let target: serde_yaml_ng::Value = serde_yaml_ng::from_str(&format!(
        "{{nodes: [{}]}}",
        [(1, "a"), (2, "b"), (3, "c")]
            .map(|(index, digit)| format!(
                "{{name: app-{index}, host: 203.0.113.2{index}, host-key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA, daemon-fingerprint: {}}}",
                digit.repeat(64)
            ))
            .join(", ")
    ))?;
    project.edit(|config| {
        config["providers"]["deploy-secrets"] = provider;
        config["targets"]["cluster"] = target;
        config["environments"]["production"]["target"] = "cluster".into();
    })?;
    project.trust()?;
    project
        .mm()
        .args(["key", "create", "--agent"])
        .assert()
        .success();
    let tools = temp_dirs::tempdir()?;
    let ssh = tools.path().join("ssh");
    fs::write(&ssh, REFUSING_SSH)?;
    fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755))?;
    let path = format!(
        "{}:{}",
        tools.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let printed = project
        .mm()
        .args(["--output", "json", "--environment", "production"])
        .args(["secret", "provider", "status", "deploy-secrets"])
        .env("PATH", &path)
        .env("MM_SSH_REUSE", "off")
        .output()?;
    let printed: serde_json::Value = serde_json::from_slice(&printed.stdout).map_err(|error| {
        format!(
            "no report ({error}): {}",
            String::from_utf8_lossy(&printed.stderr)
        )
    })?;
    let mut session = Session::start_with(
        &project,
        &[
            ("PATH", path),
            ("MM_KEY_VAULT", "file".to_owned()),
            ("MM_SSH_REUSE", "off".to_owned()),
        ],
    )?;
    let read = session.call(
        "mm_read",
        serde_json::json!({
            "scope": {"environment": "production"},
            "kind": "secret_store",
        }),
    )?;
    for key in ["provider", "environment", "target", "version"] {
        assert_eq!(read[key], printed[key], "{key}: {read} / {printed}");
    }
    assert_eq!(read["provider"], "deploy-secrets", "{read}");
    let members = |report: &serde_json::Value| {
        report["members"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|member| {
                (
                    member["node"].as_str().unwrap_or_default().to_owned(),
                    member["status"].is_null(),
                    member["problem"].is_string(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(members(&read), members(&printed));
    assert_eq!(
        members(&read),
        ["app-1", "app-2", "app-3"].map(|node| (node.to_owned(), true, true)),
        "{read}"
    );
    let codes = |report: &serde_json::Value| {
        report["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|diagnostic| diagnostic["code"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(codes(&read), codes(&printed));
    assert_eq!(
        codes(&read),
        [serde_json::json!("MM_SECRET_STORE_DEGRADED")]
    );
    Ok(())
}

/// The project of the setup tests: `local` keeps a generated password in
/// the project's age store, and agents have `secrets: true` there when
/// `secrets` is set.
fn setup_project(secrets: bool) -> Result<Project, Box<dyn std::error::Error>> {
    let project = Project::new()?;
    project.edit(|config| {
        config["secrets"] = serde_yaml_ng::from_str(
            "database.password: {storage: {provider: age, location: local/database-password}, generate: {preset: password, length: 40}}",
        )
        .unwrap_or_default();
        if secrets {
            config["agents"]["local"]["secrets"] = true.into();
        }
    })?;
    project.trust()?;
    Ok(project)
}

fn apply_setup(
    session: &mut Session,
    step: &serde_json::Value,
    plan: &serde_json::Value,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let hash = plan["plan_hash"].as_str().unwrap_or_default();
    session.call(
        "mm_setup_apply",
        serde_json::json!({
            "scope": {"environment": "local"},
            "step": step,
            "plan_hash": hash,
            "approval_token": plan["approval_token"],
            "confirmation": format!("apply:{hash}"),
        }),
    )
}

/// With `secrets: true` an agent creates the project's local secret store
/// and generates what is missing, each from a reviewed plan; a plan the
/// store moved past is refused, and no result holds a value.
#[test]
fn an_agent_sets_up_the_local_secret_store_within_its_grant()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::app::ports::SecretMaterialResolver as _;

    let project = setup_project(true)?;
    let mut session = Session::start_with(&project, &engine_off())?;
    let scope = serde_json::json!({"environment": "local"});
    let mut results = Vec::new();

    let init = serde_json::json!({"kind": "secret_init"});
    let planned = session.call(
        "mm_setup_plan",
        serde_json::json!({"scope": scope, "step": init}),
    )?;
    assert_eq!(planned["applied_by"], "agent", "{planned}");
    assert_eq!(planned["plan"]["store_initialized"], false, "{planned}");
    let initialized = apply_setup(&mut session, &init, &planned)?;
    assert!(
        initialized["recipient"]
            .as_str()
            .is_some_and(|recipient| recipient.starts_with("age1")),
        "{initialized}"
    );
    results.extend([planned, initialized.clone()]);

    let generate = serde_json::json!({"kind": "secret_generate"});
    let planned = session.call(
        "mm_setup_plan",
        serde_json::json!({"scope": scope, "step": generate}),
    )?;
    assert_eq!(planned["plan"]["store_initialized"], true, "{planned}");
    assert_eq!(
        planned["plan"]["current"][0]["key"], "database.password",
        "{planned}"
    );
    let generated = apply_setup(&mut session, &generate, &planned)?;
    assert_eq!(
        generated["generated"][0]["name"], "database.password",
        "{generated}"
    );
    assert_eq!(
        generated["generated"][0]["status"], "generated",
        "{generated}"
    );
    results.extend([planned, generated]);

    let path = |key: &str| PathBuf::from(initialized[key].as_str().unwrap_or_default());
    let identity = fs::read_to_string(path("identity_path"))?;
    let store =
        mm::adapters::secrets::AgeSecretStore::new(path("identity_path"), path("store_path"));
    let reference = mm::domain::secret::SecretReference::new(
        mm::domain::secret_contract::StorageProviderId::new("age")?,
        "local/database-password",
    )?;
    let resolved = tokio::runtime::Builder::new_current_thread()
        .build()?
        .block_on(store.resolve(&mm::domain::secret::SecretScope::new("local"), &reference))?;
    let value = resolved
        .material
        .map(|material| material.expose(str::to_owned))
        .unwrap_or_default();
    assert_eq!(value.len(), 40);
    for result in &results {
        let text = result.to_string();
        assert!(!text.contains(&value), "{text}");
        assert!(!text.contains(identity.trim()), "{text}");
    }

    let replanned = session.call(
        "mm_setup_plan",
        serde_json::json!({"scope": scope, "step": generate}),
    )?;
    project
        .mm()
        .args([
            "--environment",
            "local",
            "secret",
            "unset",
            "database.password",
            "--yes",
        ])
        .assert()
        .success();
    let stale = apply_setup(&mut session, &generate, &replanned)?;
    assert_eq!(stale["error"]["code"], "MM_STATE", "{stale}");
    assert!(
        stale["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("setup plan changed")),
        "{stale}"
    );
    Ok(())
}

/// Without the grant, a setup plan is the user's to apply, and applying it
/// hands the user the command line's commands for the step.
#[test]
fn setup_outside_the_grant_is_handed_to_the_user() -> Result<(), Box<dyn std::error::Error>> {
    let project = setup_project(false)?;
    let mut session = Session::start_with(&project, &engine_off())?;
    let scope = serde_json::json!({"environment": "local"});
    let init = serde_json::json!({"kind": "secret_init"});
    let planned = session.call(
        "mm_setup_plan",
        serde_json::json!({"scope": scope, "step": init}),
    )?;
    assert_eq!(planned["applied_by"], "user", "{planned}");
    assert!(planned["approval_token"].is_null(), "{planned}");
    assert_eq!(
        planned["user_commands"],
        serde_json::json!(["mm --environment local secret init age"])
    );

    for (environment, step, commands) in [
        (
            "local",
            init,
            serde_json::json!(["mm --environment local secret init age"]),
        ),
        (
            "local",
            serde_json::json!({"kind": "secret_generate", "name": "database.password"}),
            serde_json::json!(["mm --environment local secret generate database.password"]),
        ),
        (
            "production",
            serde_json::json!({"kind": "dev_services"}),
            serde_json::json!(["mm --environment production dev setup --services"]),
        ),
        (
            "local",
            serde_json::json!({"kind": "secret_provider", "name": "vault"}),
            serde_json::json!([
                "mm --environment local secret provider plan vault --operation install",
                "mm --environment local secret provider apply vault --operation install --plan HASH --yes",
            ]),
        ),
    ] {
        let refused = session.call(
            "mm_setup_apply",
            serde_json::json!({
                "scope": {"environment": environment},
                "step": step,
                "plan_hash": "0".repeat(64),
                "approval_token": "none",
                "confirmation": format!("apply:{}", "0".repeat(64)),
            }),
        )?;
        assert_eq!(refused["error"]["code"], "MM_AGENT_HANDOFF", "{refused}");
        assert_eq!(
            refused["error"]["handoff"]["user_commands"], commands,
            "{refused}"
        );
        if environment == "local" {
            assert_eq!(
                refused["error"]["handoff"]["requires"], "level deploy and secrets: true",
                "{refused}"
            );
        }
    }
    Ok(())
}
