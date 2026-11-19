use std::collections::BTreeMap;

use super::*;
use crate::domain::host_policy::{
    ACCESS_RECORD_SCHEMA_VERSION, AccessLevel, EnvironmentAccess, ReconciledKey, ReconciledUser,
    parse_public_key,
};

const ALICE: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w alice@laptop";
const BOB: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJZYcbK/sLQnEppB74bVUmUQnMUahuREnHUmhtFYGcf7 bob@desk";
const PROJECT: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de3";

fn private_directory(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn store(root: &Path, host: &str) -> Result<HostAccessStore, Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let etc = root.join("etc");
    private_directory(&etc)?;
    let path = etc.join("host.yaml");
    fs::write(&path, host)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let config: crate::adapters::remote::DaemonConfig =
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "socket_path": root.join("run/mm.sock"),
            "state_root": root.join("state"),
            "mm_home": root.join("home"),
            "socket_uid": 0,
            "socket_gid": 0,
            "host_policy_path": path,
            "relay_uid": 991,
            "authorized_keys_path": root.join("ssh/authorized_keys"),
            "projects_root": root.join("projects"),
        }))?;
    Ok(HostAccessStore::new(&config))
}

fn host_yaml() -> String {
    format!(
        "schema-version: 1\nbreak-glass:\n  - name: ops\n    key: {BOB}\nprojects:\n  {PROJECT}:\n    name: shop\n    origin: /srv/git/shop.git\n    environments: [staging, production]\n    ceiling:\n      production: {{level: view}}\n"
    )
}

fn record(environment: &str, level: AccessLevel) -> Result<EnvironmentAccessRecord, String> {
    let key = parse_public_key(ALICE)?;
    Ok(EnvironmentAccessRecord {
        schema_version: ACCESS_RECORD_SCHEMA_VERSION,
        project_id: ProjectId(uuid::Uuid::parse_str(PROJECT).map_err(|error| error.to_string())?),
        environment: environment.to_owned(),
        commit: "b".repeat(40),
        users: BTreeMap::from([(
            "alice".to_owned(),
            ReconciledUser {
                machine: false,
                keys: vec![ReconciledKey {
                    public_key: key.openssh(),
                    fingerprint: key.fingerprint,
                    expires_at_unix_ms: Some(4_102_444_800_000),
                }],
                access: EnvironmentAccess::level(level),
                agent: None,
            },
        )]),
        deploy_from: None,
        reconciled_at_unix_ms: 1,
        reconciled_by: "test".to_owned(),
    })
}

#[tokio::test]
async fn loads_host_policy_records_and_renders_restricted_authorized_keys()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let store = store(temporary.path(), &host_yaml())?;
    let initial = store.load()?;
    assert_eq!(
        initial.policy.principals.len(),
        1,
        "only break-glass before any reconcile"
    );
    store.render_authorized_keys(&initial.policy)?;
    let keys = fs::read_to_string(temporary.path().join("ssh/authorized_keys"))?;
    assert_eq!(
        keys.lines()
            .filter(|line| line.starts_with("restrict,"))
            .count(),
        1
    );
    assert!(keys.contains("command=\"/usr/bin/mm relay serve --stdio"));

    let (staging, production) = (
        record("staging", AccessLevel::Deploy)?,
        record("production", AccessLevel::Admin)?,
    );
    store
        .locked("a test", |guard| {
            store.write_record(guard, &staging)?;
            store.write_record(guard, &production)
        })
        .await?;
    let snapshot = store.load()?;
    assert_ne!(snapshot.policy.revision, initial.policy.revision);
    assert_eq!(snapshot.records.len(), 2);
    let project = snapshot.records[0].project_id;
    let alice = parse_public_key(ALICE)?.fingerprint;
    let production = snapshot
        .policy
        .authenticate(&alice, project, "production", 10)
        .map_err(|message| message.clone())?;
    assert_eq!(production.display_name, "alice");
    store.render_authorized_keys(&snapshot.policy)?;
    let keys = fs::read_to_string(temporary.path().join("ssh/authorized_keys"))?;
    assert_eq!(
        keys.matches(&parse_public_key(ALICE)?.openssh()).count(),
        1,
        "one line per key"
    );
    assert!(keys.contains("expiry-time=\"21000101\""), "{keys}");

    let block = EmergencyBlock {
        project_id: project,
        environment: "staging".to_owned(),
        fingerprint: alice.clone(),
        blocked_by: "ops".to_owned(),
        blocked_at_unix_ms: 1,
    };
    store
        .locked("a test", |guard| {
            store.write_blocks(guard, project, &[block])
        })
        .await?;
    let blocked = store.load()?;
    assert!(
        blocked
            .policy
            .authenticate(&alice, project, "staging", 10)
            .is_err()
    );
    assert!(
        blocked
            .policy
            .authenticate(&alice, project, "production", 10)
            .is_ok()
    );

    let deployed = DeployedState {
        project_id: project,
        environment: "staging".to_owned(),
        commit: "b".repeat(40),
        root: temporary.path().join("projects/x"),
        deployed_at_unix_ms: 1,
        deployed_by: "alice".to_owned(),
        runs_services: true,
    };
    store
        .locked("a test", |guard| store.write_deployed(guard, &deployed))
        .await?;
    assert_eq!(store.deployed(project, "staging")?, Some(deployed.clone()));
    assert_eq!(store.deployed(project, "production")?, None);

    // A record an earlier release wrote carries one more key.
    let mut earlier = serde_json::to_value(&deployed)?;
    earlier["shared-sequence"] = serde_json::json!(7);
    fs::write(
        store.deployed_path(project, "staging"),
        serde_json::to_vec(&earlier)?,
    )?;
    assert_eq!(store.deployed(project, "staging")?, Some(deployed));
    Ok(())
}

#[test]
fn rejects_old_access_policies_unsafe_files_and_bad_policies()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let temporary = crate::test_support::tempdir()?;
    let old = store(
        temporary.path(),
        "schema_version: 1\nrevision: 1\nprincipals: []\n",
    )?;
    assert!(old.load().is_err());

    let missing_break_glass = store(temporary.path(), "schema-version: 1\nbreak-glass: []\n")?;
    assert!(missing_break_glass.load().is_err());

    let merge = store(
        temporary.path(),
        &format!(
            "schema-version: 1\nbase: &base {{name: ops, key: \"{BOB}\"}}\nbreak-glass: [{{<<: *base}}]\n"
        ),
    )?;
    assert!(
        merge
            .load()
            .is_err_and(|error| error.to_string().contains("merge keys"))
    );

    let readable = store(temporary.path(), &host_yaml())?;
    fs::set_permissions(
        temporary.path().join("etc/host.yaml"),
        fs::Permissions::from_mode(0o644),
    )?;
    assert!(readable.load().is_err(), "the host policy must be 0600");
    Ok(())
}

#[test]
fn expiry_times_use_utc_calendar_dates() {
    assert_eq!(expiry_time(0).as_deref(), Some("19700101"));
    assert_eq!(expiry_time(951_868_800_000).as_deref(), Some("20000301"));
    assert_eq!(expiry_time(1_709_164_800_000).as_deref(), Some("20240229"));
    assert_eq!(expiry_time(-1), None);
}

/// A rollout's client says every few seconds that it is still there; one
/// not heard from for longer than the silence is gone, while it holds the
/// lock or runs.
#[test]
fn a_rollout_client_is_silent_once_it_was_not_heard_from_for_the_silence() {
    use crate::domain::rollout::RolloutState;
    let mut rollout = StoredRollout {
        plan_hash: "f".repeat(64),
        commit: "c".repeat(40),
        principal_id: crate::domain::access::PrincipalId(uuid::Uuid::from_u128(1)),
        started_by: "alice".to_owned(),
        started_at_unix_ms: 1_000,
        heard_at_unix_ms: 10_000,
        expires_at_unix_ms: 1_000_000,
        state: RolloutState::Running,
        stacks: Vec::new(),
        failure: None,
        interrupted: None,
        interrupted_at_unix_ms: None,
        changes: None,
        begun_under: None,
    };
    assert!(!rollout.client_silent(130_000, 120_000), "heard 120 s ago");
    assert!(rollout.client_silent(130_001, 120_000));
    rollout.state = RolloutState::Held;
    assert!(rollout.client_silent(130_001, 120_000));
    for done in [RolloutState::Finished, RolloutState::RolledBack] {
        rollout.state = done;
        assert!(
            !rollout.client_silent(1_000_000, 120_000),
            "a rollout that ended has no client to hear from"
        );
    }
}
