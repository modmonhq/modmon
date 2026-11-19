//! State that 0.1.1 wrote, as this release reads it. The fixtures under
//! `tests/fixtures/state/0.1.1` are frozen: a change that breaks one needs a
//! migration, not a new fixture.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use mm::adapters::mcp::AgentTrustStore;
use mm::adapters::remote::{DaemonConfig, HostAccessStore, parse_host_policy};
use mm::adapters::services::FilesystemServiceStackStore;
use mm::adapters::state::SqliteStateStore;
use mm::adapters::target::{FilesystemTargetStore, ForwardShortcutStore, TargetPins};
use mm::adapters::user_namespace::FilesystemServiceUserIds;
use mm::app::ports::{
    EnvironmentValueStore, IngressRouteRegistry, ServiceStackStore, ServiceUserIds, StateStore,
    TargetStore,
};
use mm::domain::host_policy::HostPolicy;
use mm::domain::stored_state::unknown_keys;
use mm::domain::workspace::WorkspaceRegistration;

#[path = "stored_state/sql_dump.rs"]
mod sql_dump;
mod temp_dirs;
#[path = "stored_state/written.rs"]
mod written;

use written::{ENVIRONMENT, PIN, PROJECT, ROOT, TARGET, TestResult};

const RELEASE: &str = "0.1.1";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/state")
        .join(RELEASE)
}

/// Freezes the state this release writes under
/// `tests/fixtures/state/<version>`, when the release is cut. The files are
/// made in a private directory, as mm requires, then copied out with the
/// owner of `tests/fixtures`.
#[test]
#[ignore = "run once when a release is cut, then commit the files"]
fn write_fixtures() -> TestResult {
    use std::os::unix::fs::MetadataExt;

    let out = fixtures().with_file_name(env!("CARGO_PKG_VERSION"));
    if out.exists() {
        return Err(format!("{} is frozen", out.display()).into());
    }
    let staging = temp_dirs::tempdir()?;
    written::all(staging.path())?;
    let owner = fs::metadata(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"))?;
    let owner = (owner.uid(), owner.gid());
    let state = out
        .parent()
        .ok_or("the fixtures have no parent directory")?;
    if !state.exists() {
        fs::create_dir(state)?;
        std::os::unix::fs::chown(state, Some(owner.0), Some(owner.1))?;
    }
    publish(staging.path(), &out, owner)
}

fn publish(source: &Path, destination: &Path, owner: (u32, u32)) -> TestResult {
    fs::create_dir_all(destination)?;
    fs::set_permissions(destination, fs::Permissions::from_mode(0o755))?;
    std::os::unix::fs::chown(destination, Some(owner.0), Some(owner.1))?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let to = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            publish(&entry.path(), &to, owner)?;
        } else if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "lock")
        {
            continue;
        } else {
            fs::copy(entry.path(), &to)?;
            fs::set_permissions(&to, fs::Permissions::from_mode(0o644))?;
            std::os::unix::fs::chown(&to, Some(owner.0), Some(owner.1))?;
        }
    }
    Ok(())
}

/// A copy of `source` with the owner-only modes mm requires.
fn private_copy(source: &Path, destination: &Path) -> TestResult {
    fs::create_dir_all(destination)?;
    fs::set_permissions(destination, fs::Permissions::from_mode(0o700))?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let to = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            private_copy(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), &to)?;
            fs::set_permissions(&to, fs::Permissions::from_mode(0o600))?;
        }
    }
    Ok(())
}

fn database(directory: &Path) -> TestResult<PathBuf> {
    let path = directory.join("state.sqlite3");
    let sql = fs::read_to_string(fixtures().join("machine/state.sql"))?;
    rusqlite::Connection::open(&path)?.execute_batch(&sql)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    Ok(path)
}

fn registration() -> WorkspaceRegistration {
    WorkspaceRegistration {
        project_id: PROJECT,
        root: PathBuf::from(ROOT),
        config_path: PathBuf::from(ROOT).join(".config/mm.yaml"),
    }
}

#[test]
fn the_state_database_of_0_1_1_opens_and_every_record_reads() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&database(temporary.path())?)?;
    let workspace = store.register_workspace(&registration(), 9_000)?;

    let operations = store.recent_operations(workspace.id, 10)?;
    assert_eq!(operations.len(), 2, "{operations:?}");
    let project = store
        .local_project_preference(PROJECT)?
        .ok_or("no project preference")?;
    assert_eq!(project.target.as_deref(), Some("local"));
    assert!(
        store
            .local_environment_preference(PROJECT, ENVIRONMENT)?
            .is_some()
    );
    let overrides = store.list_environment_overrides(workspace.id, ENVIRONMENT)?;
    assert_eq!(overrides.len(), 1, "{overrides:?}");
    let (plan, result) = store
        .latest_successful_image_build(PROJECT, Path::new(ROOT), ENVIRONMENT)?
        .ok_or("no image build")?;
    assert_eq!(result.plan_hash, plan.plan_hash);
    let routes = store.route_set(TARGET)?;
    assert_eq!(routes.claims.len(), 1);
    assert_eq!(routes.middlewares.len(), 1);
    let deployed = store
        .latest_successful_deployment(workspace.id, ENVIRONMENT, TARGET)?
        .ok_or("no deployment")?;
    assert_eq!(deployed.canonical_hash()?, deployed.plan_hash);
    Ok(())
}

#[test]
fn a_later_migration_applies_to_0_1_1_state_and_0_1_1_then_refuses_it() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let path = database(temporary.path())?;
    let later = [
        ("0001_initial", ""),
        (
            "0002_later_release",
            "CREATE TABLE later_release (id INTEGER);",
        ),
    ];

    let migrated = SqliteStateStore::inspect_with(&path, &later)?.ok_or("no state")?;
    assert_eq!(
        migrated
            .recent_operations(migrated.register_workspace(&registration(), 9_000)?.id, 10)?
            .len(),
        2
    );
    drop(migrated);
    let connection = rusqlite::Connection::open(&path)?;
    let ledger = connection
        .prepare("SELECT version FROM schema_migrations ORDER BY version")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(ledger, ["0001_initial", "0002_later_release"]);
    drop(connection);

    let before = fs::read(&path)?;
    for refused in [
        SqliteStateStore::open(&path).err(),
        SqliteStateStore::inspect(&path).err(),
    ] {
        let message = refused.ok_or("an older mm opened newer state")?.to_string();
        assert!(
            message.contains("was written by a newer mm (state migration 0002_later_release)")
                && message.contains("`mm update`"),
            "{message}"
        );
    }
    assert_eq!(fs::read(&path)?, before);
    Ok(())
}

#[test]
fn the_files_0_1_1_keeps_on_a_machine_read() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let home = temporary.path().join("home");
    private_copy(&fixtures().join("machine"), &home)?;

    let services = FilesystemServiceStackStore::new(home.join("services"))
        .load_state()?
        .ok_or("no service stack state")?;
    assert_eq!(services.applied_plan.plan_hash, services.plan_hash);
    let targets = FilesystemTargetStore::new(home.join("targets.yaml")).list_targets()?;
    assert_eq!(targets.len(), 2, "{targets:?}");
    assert!(AgentTrustStore::new(&home).is_trusted(
        Path::new(ROOT),
        PROJECT,
        &mm::domain::agents::AgentsConfig::new()
    ));
    assert!(
        TargetPins::new(home.join("pinned-targets.json"))
            .pinned(PIN)
            .is_some()
    );
    let forwards =
        ForwardShortcutStore::new(home.join("forwards.json")).list(&PROJECT.0.to_string())?;
    assert!(forwards.contains_key("db"), "{forwards:?}");

    let receipt: mm::domain::build::BuildReceipt =
        serde_json::from_slice(&fs::read(fixtures().join("build-receipt.json"))?)?;
    receipt.validate()?;
    Ok(())
}

fn host_config(root: &Path) -> TestResult<DaemonConfig> {
    let path = root.join("etc/mm/mmd.json");
    let mut config = DaemonConfig::parse(&path, &fs::read(&path)?)?;
    config.state_root = root.join("var/lib/mm");
    config.mm_home = root.join("var/lib/mm-home");
    config.host_policy_path = root.join("etc/mm/host.yaml");
    Ok(config)
}

#[test]
fn the_files_0_1_1_keeps_on_a_host_read() -> TestResult {
    use mm::adapters::autoscale::store as scale;
    use mm::adapters::healing::store as health;

    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("host");
    private_copy(&fixtures().join("host"), &root)?;
    let config = host_config(&root)?;
    let state = config.state_root.clone();

    let policy = &config.host_policy_path;
    parse_host_policy(policy, &fs::read(policy)?)?;
    let mmd: serde_json::Value = serde_json::from_slice(&fs::read(root.join("etc/mm/mmd.json"))?)?;
    assert_eq!(unknown_keys::<DaemonConfig>(&mmd), Vec::<String>::new());
    let host: serde_json::Value = serde_yaml_ng::from_slice(&fs::read(policy)?)?;
    assert_eq!(unknown_keys::<HostPolicy>(&host), Vec::<String>::new());
    let access = HostAccessStore::new(&config);
    let snapshot = access.load()?;
    assert_eq!((snapshot.records.len(), snapshot.blocks.len()), (1, 1));
    assert!(access.deployed(PROJECT, ENVIRONMENT)?.is_some());
    let deployments = access.deployments(PROJECT, ENVIRONMENT)?;
    assert!(deployments.current.is_some() && deployments.rollout.is_some());
    assert!(deployments.once.contains_key("migrate"));
    let bundle = state.join(format!(
        "projects/{}/bundles/production/{}.json",
        PROJECT.0,
        written::COMMIT
    ));
    serde_json::from_slice::<mm::adapters::remote::BundleRecord>(&fs::read(bundle)?)?;

    assert_eq!(health::watched(&state.join("health"))?.len(), 1);
    let history = health::history(&state.join("health"), &written::scope())?;
    assert_eq!(history.containers.len(), 1);
    let scaling = state.join(mm::adapters::autoscale::SCALE_DIRECTORY);
    assert_eq!(scale::deployed(&scaling)?.len(), 1);
    assert!(scale::want(&scaling, "app/web")?.is_some());
    assert!(
        mm::adapters::cluster_network::NetworkStore::new(state.join("networks"))
            .state()?
            .is_some()
    );
    serde_json::from_slice::<mm::domain::mesh::MeshState>(&fs::read(
        state.join("mesh/mesh.json"),
    )?)?;
    let table = state.join(format!("ingress/routes/{}-{ENVIRONMENT}.json", PROJECT.0));
    serde_json::from_slice::<mm::domain::cluster_ingress::IngressTable>(&fs::read(table)?)?;
    assert_eq!(mm::adapters::leaving::leaving_records(&state).len(), 1);

    let ids = root.join("subordinate-ids");
    fs::write(
        &ids,
        format!(
            "{}:{}:{}\n",
            mm::domain::user_namespace::RANGE_OWNER,
            mm::domain::user_namespace::DEFAULT_FIRST,
            mm::domain::user_namespace::DEFAULT_COUNT
        ),
    )?;
    let owner = written::service_owner();
    let reserved = FilesystemServiceUserIds::at(
        config.mm_home.join("service-user-ids.json"),
        ids.clone(),
        ids,
    )
    .reserve(&std::collections::BTreeSet::from([owner.clone()]))?;
    assert!(reserved.contains_key(&owner));
    Ok(())
}

fn edit_json(path: &Path, change: impl FnOnce(&mut serde_json::Value)) -> TestResult {
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
    change(&mut value);
    fs::write(path, serde_json::to_vec_pretty(&value)?)?;
    Ok(())
}

fn refusal<T>(result: Result<T, mm::MmError>) -> TestResult<String> {
    match result {
        Ok(_) => Err("state a newer mm wrote was read".into()),
        Err(error) => Ok(error.to_string()),
    }
}

#[test]
fn state_a_newer_release_wrote_is_refused_with_the_command_that_updates() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let home = temporary.path().join("home");
    private_copy(&fixtures().join("machine"), &home)?;
    edit_json(&home.join("services/state.json"), |state| {
        state["schema"] = 2.into();
    })?;
    let message = refusal(FilesystemServiceStackStore::new(home.join("services")).load_state())?;
    assert!(
        message.contains(
            "services/state.json was written by a newer mm (schema 2); update with `mm update`"
        ),
        "{message}"
    );

    let root = temporary.path().join("host");
    private_copy(&fixtures().join("host"), &root)?;
    let config = host_config(&root)?;
    let mmd = root.join("etc/mm/mmd.json");
    edit_json(&mmd, |config| config["schema_version"] = 3.into())?;
    let message = refusal(DaemonConfig::parse(&mmd, &fs::read(&mmd)?))?;
    assert!(
        message.contains("mmd.json was written by a newer mm (schema 3); upgrade this host")
            && message.contains("mm target setup plan --upgrade"),
        "{message}"
    );

    let policy = root.join("etc/mm/host.yaml");
    let newer = fs::read_to_string(&policy)?
        .replace("schema-version: 1", "schema-version: 3\nadded-later: true");
    let message = refusal(parse_host_policy(&policy, newer.as_bytes()))?;
    assert!(
        message.contains("host.yaml was written by a newer mm (schema 3)"),
        "{message}"
    );

    let record = config
        .state_root
        .join(format!("projects/{}/access/{ENVIRONMENT}.json", PROJECT.0));
    edit_json(&record, |record| record["added-later"] = true.into())?;
    let deployments = config.state_root.join(format!(
        "projects/{}/deployments/{ENVIRONMENT}.json",
        PROJECT.0
    ));
    edit_json(&deployments, |stored| {
        stored["current"]["added_later"] = true.into();
    })?;
    let access = HostAccessStore::new(&config);
    access.load()?;
    access.deployments(PROJECT, ENVIRONMENT)?;
    edit_json(&record, |record| record["schema-version"] = 2.into())?;
    let message = refusal(access.load())?;
    assert!(
        message.contains("was written by a newer mm (schema 2)"),
        "{message}"
    );
    Ok(())
}

#[test]
fn keys_a_newer_release_added_are_found_and_ignored() -> TestResult {
    let mut mmd: serde_json::Value =
        serde_json::from_slice(&fs::read(fixtures().join("host/etc/mm/mmd.json"))?)?;
    mmd["added_later"] = true.into();
    assert_eq!(unknown_keys::<DaemonConfig>(&mmd), ["added_later"]);
    DaemonConfig::parse(Path::new("/etc/mm/mmd.json"), &serde_json::to_vec(&mmd)?)?;

    let path = fixtures().join("host/etc/mm/host.yaml");
    let mut host: serde_json::Value = serde_yaml_ng::from_slice(&fs::read(&path)?)?;
    host["projects"][PROJECT.0.to_string()]["added-later"] = true.into();
    host["break-glass"][0]["comment"] = "kept offline".into();
    assert_eq!(
        unknown_keys::<HostPolicy>(&host),
        [
            "break-glass[0].comment".to_owned(),
            format!("projects.{}.added-later", PROJECT.0)
        ]
    );
    parse_host_policy(&path, serde_yaml_ng::to_string(&host)?.as_bytes())?;
    Ok(())
}
