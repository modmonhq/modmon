//! Registered workspaces whose folder is gone: listed as stale, skipped by
//! the dashboard, and forgotten through a reviewed plan.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm::adapters::config::ConfigLoader;
use mm::adapters::state::SqliteStateStore;
use mm::app::ports::{DashboardHostSource, StateStore};
use mm::app::{RegisteredDashboardHostSource, WorkspaceRegistry};
use mm::domain::error::MmError;
use mm::domain::operation::{NewOperation, OperationId, OperationKind, OperationState};
use mm::domain::workspace::{WorkspaceRecord, WorkspaceRegistration};
use mm::domain::workspace_registry::{WorkspacePresence, WorkspaceSelector};
use uuid::Uuid;

mod temp_dirs;

const NOW: i64 = 1_900_000_000_000;

fn project(parent: &Path, name: &str, id: u128) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let root = parent.join(name);
    std::fs::create_dir_all(root.join(".config"))?;
    std::fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: {}\n  name: {name}\ntarget: local\nstacks: {{}}\nenvironments:\n  local:\n    bindings: {{}}\n",
            Uuid::from_u128(id)
        ),
    )?;
    Ok(root)
}

fn register(
    state: &SqliteStateStore,
    root: &Path,
    at: i64,
) -> Result<WorkspaceRecord, Box<dyn std::error::Error>> {
    let document = ConfigLoader.load_from(root)?;
    Ok(state.register_workspace(
        &WorkspaceRegistration {
            project_id: document.config.project.id,
            root: document.root,
            config_path: document.path,
        },
        at,
    )?)
}

fn operation(
    state: &SqliteStateStore,
    workspace: &WorkspaceRecord,
    at: i64,
) -> Result<OperationId, MmError> {
    let id = OperationId::new();
    state.begin_operation(&NewOperation {
        id,
        workspace_id: workspace.id,
        kind: OperationKind::Deploy,
        environment: "local".to_owned(),
        requested_at_unix_ms: at,
        plan_hash: Some("plan".to_owned()),
    })?;
    state.finish_operation(id, OperationState::Succeeded, at + 1, None)?;
    Ok(id)
}

fn events(database: &Path, operation: OperationId) -> Result<i64, Box<dyn std::error::Error>> {
    let connection = rusqlite::Connection::open(database)?;
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM operation_events WHERE operation_id = ?1",
        [operation.0.to_string()],
        |row| row.get(0),
    )?)
}

struct Machine {
    _projects: tempfile::TempDir,
    home: tempfile::TempDir,
    state: Arc<SqliteStateStore>,
    kept: WorkspaceRecord,
    gone: WorkspaceRecord,
    gone_operations: Vec<OperationId>,
}

/// Two registered projects with operations; the folder of one is removed.
fn machine() -> Result<Machine, Box<dyn std::error::Error>> {
    let projects = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    let state = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let kept = register(&state, &project(projects.path(), "kept", 0x61)?, NOW)?;
    let gone_root = project(projects.path(), "gone", 0x62)?;
    let gone = register(&state, &gone_root, NOW - 5_000)?;
    operation(&state, &kept, NOW)?;
    let gone_operations = vec![
        operation(&state, &gone, NOW - 4_000)?,
        operation(&state, &gone, NOW - 3_000)?,
    ];
    std::fs::remove_dir_all(&gone_root)?;
    Ok(Machine {
        _projects: projects,
        home,
        state,
        kept,
        gone,
        gone_operations,
    })
}

#[test]
fn a_removed_checkout_is_listed_stale_and_does_not_stop_the_dashboard_hosts()
-> Result<(), Box<dyn std::error::Error>> {
    let machine = machine()?;
    let listed = WorkspaceRegistry::new(Some(machine.state.clone())).list()?;
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].root, machine.kept.root);
    assert_eq!(listed[0].presence, WorkspacePresence::Present);
    assert!(!listed[0].stale);
    assert_eq!(listed[0].operations, 1);
    assert_eq!(listed[1].root, machine.gone.root);
    assert_eq!(listed[1].presence, WorkspacePresence::RootMissing);
    assert!(listed[1].stale);
    assert_eq!(listed[1].operations, 2);
    assert_eq!(listed[1].last_used_at_unix_ms, NOW - 5_000);

    let hosts = RegisteredDashboardHostSource::new(Arc::new(ConfigLoader), machine.state.clone())
        .project_dashboard_hosts()?;
    assert_eq!(hosts.len(), 1);
    assert!(hosts.iter().all(|host| host.starts_with("mm.kept-")));

    std::fs::remove_file(machine.kept.config_path.clone())?;
    let listed = WorkspaceRegistry::new(Some(machine.state.clone())).list()?;
    assert_eq!(listed[0].presence, WorkspacePresence::ConfigMissing);
    assert!(
        RegisteredDashboardHostSource::new(Arc::new(ConfigLoader), machine.state.clone())
            .project_dashboard_hosts()?
            .is_empty()
    );
    Ok(())
}

#[test]
fn a_registered_root_whose_config_names_another_project_stays_a_security_error()
-> Result<(), Box<dyn std::error::Error>> {
    let machine = machine()?;
    let config = std::fs::read_to_string(&machine.kept.config_path)?.replace(
        &Uuid::from_u128(0x61).to_string(),
        &Uuid::from_u128(0x63).to_string(),
    );
    std::fs::write(&machine.kept.config_path, config)?;
    let refused = RegisteredDashboardHostSource::new(Arc::new(ConfigLoader), machine.state.clone())
        .project_dashboard_hosts();
    assert!(
        matches!(&refused, Err(MmError::Security { message })
            if message.contains("no longer matches its project owner")
                && message.contains("mm workspace forget")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn forgetting_the_stale_workspaces_is_reviewed_and_removes_the_row_and_its_operations()
-> Result<(), Box<dyn std::error::Error>> {
    let machine = machine()?;
    let state: Arc<dyn StateStore> = machine.state.clone();
    let registry = WorkspaceRegistry::new(Some(state));
    let plan = registry.forget_plan(&WorkspaceSelector::Stale)?;
    assert_eq!(plan.workspaces.len(), 1);
    assert_eq!(plan.workspaces[0].id, machine.gone.id);
    assert_eq!(plan.operations, 2);
    assert_eq!(
        registry.forget_plan(&WorkspaceSelector::Stale)?.plan_hash,
        plan.plan_hash
    );

    assert!(matches!(
        registry.forget(&WorkspaceSelector::Stale, "0".repeat(64).as_str()),
        Err(MmError::Security { message }) if message.contains("mm workspace forget --stale")
    ));
    assert_eq!(registry.list()?.len(), 2);

    operation(&machine.state, &machine.gone, NOW - 2_000)?;
    assert!(matches!(
        registry.forget(&WorkspaceSelector::Stale, &plan.plan_hash),
        Err(MmError::Security { .. })
    ));
    let plan = registry.forget_plan(&WorkspaceSelector::Stale)?;
    assert_eq!(plan.operations, 3);

    let forgotten = registry.forget(&WorkspaceSelector::Stale, &plan.plan_hash)?;
    assert_eq!(forgotten.forgotten.len(), 1);
    assert_eq!(forgotten.operations, 3);
    let listed = registry.list()?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, machine.kept.id);
    assert_eq!(listed[0].operations, 1);
    assert!(
        machine
            .state
            .recent_operations(machine.gone.id, 10)?
            .is_empty()
    );
    let database = machine.home.path().join("state.sqlite3");
    for operation in &machine.gone_operations {
        assert_eq!(events(&database, *operation)?, 0);
    }
    assert!(machine.kept.root.exists());

    let again = registry.forget_plan(&WorkspaceSelector::Stale)?;
    assert!(again.workspaces.is_empty());
    assert_eq!(
        registry
            .forget(&WorkspaceSelector::Stale, &again.plan_hash)?
            .operations,
        0
    );
    Ok(())
}

#[test]
fn forgetting_one_workspace_names_it_by_root_or_id_and_keeps_its_files()
-> Result<(), Box<dyn std::error::Error>> {
    let machine = machine()?;
    let registry = WorkspaceRegistry::new(Some(machine.state.clone() as Arc<dyn StateStore>));
    let by_root = WorkspaceSelector::One {
        given: machine.kept.root.display().to_string(),
        path: machine.kept.root.clone(),
    };
    let plan = registry.forget_plan(&by_root)?;
    assert_eq!(plan.workspaces.len(), 1);
    assert_eq!(plan.workspaces[0].id, machine.kept.id);
    let by_id = WorkspaceSelector::One {
        given: Uuid::from_u128(0x62).to_string(),
        path: PathBuf::from("/nonexistent"),
    };
    assert_eq!(
        registry.forget_plan(&by_id)?.workspaces[0].id,
        machine.gone.id
    );
    let unknown = WorkspaceSelector::One {
        given: "/nowhere".to_owned(),
        path: PathBuf::from("/nowhere"),
    };
    assert!(matches!(
        registry.forget_plan(&unknown),
        Err(MmError::Environment { message }) if message.contains("mm workspace list")
    ));

    registry.forget(&by_root, &plan.plan_hash)?;
    assert!(machine.kept.config_path.exists());
    assert_eq!(registry.list()?.len(), 1);
    Ok(())
}

fn mm(home: &Path) -> assert_cmd::Command {
    let mut command = temp_dirs::mm();
    command
        .env("HOME", home)
        .env("PATH", home)
        .env("MM_NO_HISTORY", "1")
        .arg("--mm-home")
        .arg(home);
    command
}

#[test]
fn the_cli_lists_stale_workspaces_and_forgets_them_with_the_reviewed_hash()
-> Result<(), Box<dyn std::error::Error>> {
    let machine = machine()?;
    let home = machine.home.path();

    let listed = mm(home)
        .args(["--output", "json", "workspace", "list"])
        .output()?;
    assert!(listed.status.success(), "{listed:?}");
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout)?;
    let workspaces = listed["workspaces"].as_array().ok_or("workspaces")?;
    assert_eq!(workspaces.len(), 2);
    assert_eq!(workspaces[1]["stale"], true);
    assert_eq!(workspaces[1]["presence"], "root_missing");

    let human = mm(home).args(["workspace", "list"]).output()?;
    let human = String::from_utf8(human.stdout)?;
    assert!(human.contains("stale: folder gone"), "{human}");
    assert!(human.contains("mm workspace forget --stale"), "{human}");

    let doctor = mm(home)
        .arg("--root")
        .arg(machine._projects.path())
        .args(["--output", "json", "doctor"])
        .output()?;
    let doctor: serde_json::Value = serde_json::from_slice(&doctor.stdout)?;
    let finding = doctor["validation"]["diagnostics"]
        .as_array()
        .ok_or("diagnostics")?
        .iter()
        .find(|diagnostic| diagnostic["code"] == "MM_WORKSPACE_STALE")
        .ok_or("no MM_WORKSPACE_STALE finding")?;
    assert!(
        finding["help"]
            .as_str()
            .is_some_and(|help| help.contains("mm workspace forget --stale")),
        "{finding}"
    );

    let review = mm(home).args(["workspace", "forget", "--stale"]).output()?;
    assert!(review.status.success(), "{review:?}");
    let review = String::from_utf8(review.stdout)?;
    let apply = review
        .lines()
        .find_map(|line| line.strip_prefix("Apply: "))
        .ok_or("no apply line")?;
    assert!(
        apply.contains("workspace forget --stale --plan "),
        "{review}"
    );
    let hash = apply
        .split_whitespace()
        .skip_while(|word| *word != "--plan")
        .nth(1)
        .ok_or("no hash")?;

    mm(home)
        .args(["workspace", "forget", "--stale", "--plan"])
        .arg("0".repeat(64))
        .arg("--yes")
        .assert()
        .failure();
    let applied = mm(home)
        .args([
            "--output",
            "json",
            "workspace",
            "forget",
            "--stale",
            "--plan",
            hash,
            "--yes",
        ])
        .output()?;
    assert!(applied.status.success(), "{applied:?}");
    let applied: serde_json::Value = serde_json::from_slice(&applied.stdout)?;
    assert_eq!(applied["operations"], 2);
    assert_eq!(applied["forgotten"].as_array().map(Vec::len), Some(1));

    let after = mm(home)
        .args(["--output", "json", "workspace", "list"])
        .output()?;
    let after: serde_json::Value = serde_json::from_slice(&after.stdout)?;
    assert_eq!(after["workspaces"].as_array().map(Vec::len), Some(1));
    Ok(())
}
