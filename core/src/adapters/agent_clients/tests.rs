use super::*;

fn read_json(path: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

#[test]
fn project_setup_writes_every_tool_and_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let project = crate::test_support::tempdir()?;
    let root = project.path();
    let plan = plan_project_setup(root, &AgentClient::ALL)?;
    let paths = plan
        .files
        .iter()
        .filter_map(|change| change.path.strip_prefix(root).ok())
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            ".mcp.json",
            ".claude/settings.json",
            "CLAUDE.md",
            "opencode.json",
            ".codex/config.toml",
            ".cursor/mcp.json",
            "AGENTS.md",
        ]
    );
    assert!(
        plan.files
            .iter()
            .all(|change| change.action == FileAction::Create)
    );
    assert!(plan.commands.is_empty());
    // Planning writes nothing.
    assert!(!root.join(".mcp.json").exists());

    assert!(apply_setup(&plan)?.is_empty());
    let server = serde_json::json!({"command": "mm", "args": ["mcp", "serve"]});
    assert_eq!(
        read_json(&root.join(".mcp.json"))?["mcpServers"]["mm"],
        server
    );
    assert_eq!(
        read_json(&root.join(".cursor/mcp.json"))?["mcpServers"]["mm"],
        server
    );
    assert_eq!(
        read_json(&root.join("opencode.json"))?["mcp"]["mm"],
        serde_json::json!({"type": "local", "command": ["mm", "mcp", "serve"], "enabled": true})
    );
    assert_eq!(
        read_json(&root.join(".claude/settings.json"))?["permissions"]["deny"],
        serde_json::json!(["Bash(mm:*)", "Bash(mmd:*)"])
    );
    assert_eq!(
        fs::read_to_string(root.join(".codex/config.toml"))?,
        "[mcp_servers.mm]\ncommand = \"mm\"\nargs = [\"mcp\", \"serve\"]\n"
    );
    assert_eq!(fs::read_to_string(root.join("CLAUDE.md"))?, "@AGENTS.md\n");
    let instructions = fs::read_to_string(root.join("AGENTS.md"))?;
    assert!(instructions.contains("MM_AGENT_HANDOFF") && instructions.contains("Never run `mm`"));

    let again = plan_project_setup(root, &AgentClient::ALL)?;
    assert!(again.is_empty(), "{:?}", again.files);
    Ok(())
}

#[test]
fn existing_configuration_is_merged_not_replaced() -> Result<(), Box<dyn std::error::Error>> {
    let project = crate::test_support::tempdir()?;
    let root = project.path();
    fs::write(
        root.join(".mcp.json"),
        r#"{"mcpServers": {"other": {"command": "other"}, "mm": {"command": "old"}}}"#,
    )?;
    fs::create_dir(root.join(".claude"))?;
    fs::write(
        root.join(".claude/settings.json"),
        r#"{"model": "x", "permissions": {"allow": ["Bash(ls:*)"], "deny": ["Bash(mm:*)"]}}"#,
    )?;
    fs::write(
        root.join("CLAUDE.md"),
        "# Notes\n\nSee AGENTS.md for the rest.\n",
    )?;
    fs::write(
        root.join("AGENTS.md"),
        format!("# Project\n\n{SECTION_START}\nstale text\n{SECTION_END}\n\n## After\n"),
    )?;
    fs::create_dir(root.join(".codex"))?;
    fs::write(root.join(".codex/config.toml"), "model = \"x\"")?;

    let plan = plan_project_setup(root, &[AgentClient::ClaudeCode, AgentClient::Codex])?;
    assert!(
        plan.files
            .iter()
            .all(|change| change.action == FileAction::Update)
    );
    apply_setup(&plan)?;

    let servers = read_json(&root.join(".mcp.json"))?;
    assert_eq!(servers["mcpServers"]["other"]["command"], "other");
    assert_eq!(servers["mcpServers"]["mm"]["command"], "mm");
    let settings = read_json(&root.join(".claude/settings.json"))?;
    assert_eq!(settings["model"], "x");
    assert_eq!(
        settings["permissions"]["allow"],
        serde_json::json!(["Bash(ls:*)"])
    );
    assert_eq!(
        settings["permissions"]["deny"],
        serde_json::json!(["Bash(mm:*)", "Bash(mmd:*)"])
    );
    // CLAUDE.md already points at AGENTS.md, so it is left as it was.
    assert_eq!(
        fs::read_to_string(root.join("CLAUDE.md"))?,
        "# Notes\n\nSee AGENTS.md for the rest.\n"
    );
    let instructions = fs::read_to_string(root.join("AGENTS.md"))?;
    assert!(instructions.starts_with("# Project\n\n"));
    assert!(instructions.ends_with("\n\n## After\n"));
    assert!(!instructions.contains("stale text"));
    assert_eq!(instructions.matches(SECTION_START).count(), 1);
    assert_eq!(
        fs::read_to_string(root.join(".codex/config.toml"))?,
        "model = \"x\"\n\n[mcp_servers.mm]\ncommand = \"mm\"\nargs = [\"mcp\", \"serve\"]\n"
    );
    assert!(plan_project_setup(root, &[AgentClient::ClaudeCode, AgentClient::Codex])?.is_empty());
    Ok(())
}

#[test]
fn files_mm_cannot_parse_are_left_alone_with_a_note() -> Result<(), Box<dyn std::error::Error>> {
    let project = crate::test_support::tempdir()?;
    let root = project.path();
    fs::write(root.join(".mcp.json"), "// comments\n{}")?;
    // Only a commented opencode.jsonc exists, and it does not parse as JSON.
    fs::write(root.join("opencode.jsonc"), "{ /* mine */ }")?;
    fs::create_dir(root.join(".cursor"))?;
    fs::write(root.join(".cursor/mcp.json"), r#"{"mcpServers": []}"#)?;
    let plan = plan_project_setup(
        root,
        &[
            AgentClient::ClaudeCode,
            AgentClient::Opencode,
            AgentClient::Cursor,
        ],
    )?;
    let touched = plan
        .files
        .iter()
        .filter_map(|change| change.path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(
        !touched
            .iter()
            .any(|name| name == ".mcp.json" || name.starts_with("opencode"))
    );
    assert!(
        !plan
            .files
            .iter()
            .any(|change| change.path.ends_with(".cursor/mcp.json"))
    );
    assert_eq!(plan.notes.len(), 3, "{:?}", plan.notes);
    assert!(plan.notes.iter().all(|note| note.contains("left it alone")));
    Ok(())
}

#[test]
fn user_setup_uses_the_absolute_binary_and_each_tool_s_own_file()
-> Result<(), Box<dyn std::error::Error>> {
    let home = crate::test_support::tempdir()?;
    let mm = Path::new("/opt/mm/bin/mm");
    let plan = plan_user_setup(home.path(), &[AgentClient::Codex, AgentClient::Cursor], mm)?;
    apply_setup(&plan)?;
    assert_eq!(
        read_json(&home.path().join(".cursor/mcp.json"))?["mcpServers"]["mm"],
        serde_json::json!({"command": "/opt/mm/bin/mm", "args": ["mcp", "serve"]})
    );
    assert!(
        fs::read_to_string(home.path().join(".codex/config.toml"))?
            .contains("command = \"/opt/mm/bin/mm\"")
    );
    // No instructions file is written outside a project.
    assert!(!home.path().join("AGENTS.md").exists());
    Ok(())
}

#[test]
fn opencode_prefers_the_plain_file_and_merges_a_parsable_jsonc()
-> Result<(), Box<dyn std::error::Error>> {
    let both = crate::test_support::tempdir()?;
    fs::write(
        both.path().join("opencode.json"),
        r#"{"model": "x", "mcp": {"other": {}}}"#,
    )?;
    fs::write(both.path().join("opencode.jsonc"), "{ /* mine */ }")?;
    let plan = plan_project_setup(both.path(), &[AgentClient::Opencode])?;
    apply_setup(&plan)?;
    let config = read_json(&both.path().join("opencode.json"))?;
    assert_eq!(config["model"], "x");
    assert!(config["mcp"]["other"].is_object());
    assert_eq!(config["mcp"]["mm"]["type"], "local");
    assert_eq!(
        fs::read_to_string(both.path().join("opencode.jsonc"))?,
        "{ /* mine */ }"
    );

    let only = crate::test_support::tempdir()?;
    fs::write(only.path().join("opencode.jsonc"), r#"{"permission": {}}"#)?;
    let plan = plan_project_setup(only.path(), &[AgentClient::Opencode])?;
    apply_setup(&plan)?;
    assert!(!only.path().join("opencode.json").exists());
    assert_eq!(
        read_json(&only.path().join("opencode.jsonc"))?["mcp"]["mm"]["enabled"],
        true
    );
    Ok(())
}
