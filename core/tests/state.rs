use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use mm::adapters::state::{MmHome, SqliteStateStore};
use mm::app::ports::{EnvironmentValueStore, ExecAuditJournal, IngressRouteRegistry, StateStore};
use mm::domain::build::ReusableImageRecord;
use mm::domain::ingress::IngressRouteClaim;
use mm::domain::observability::ExecAuditRecord;
use mm::domain::operation::{
    NewOperation, OperationEvent, OperationId, OperationKind, OperationPhase, OperationState,
};
use mm::domain::runtime::RuntimeKind;
use mm::domain::target::TargetId;
use mm::domain::workspace::{ProjectId, WorkspaceRegistration};
use uuid::Uuid;

mod temp_dirs;

macro_rules! replace_owner_claims_cas {
    ($store:expr, $target_id:expr, $project_id:expr, $environment:expr, $generation:expr, $claims:expr, $middlewares:expr, $digest:expr, $now:expr $(,)?) => {
        IngressRouteRegistry::replace_owner_claims_cas(
            $store,
            mm::app::ports::IngressClaimsReplacement {
                target_id: $target_id,
                project_id: $project_id,
                environment: $environment,
                expected_generation: $generation,
                claims: $claims,
                middlewares: $middlewares,
                preserve_gate_attachments: true,
                config_digest: $digest,
                now_unix_ms: $now,
            },
        )
    };
}

#[test]
fn journals_operations_and_coordinates_locks() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let state_path = temporary.path().join("state.sqlite3");
    let store = SqliteStateStore::open(&state_path)?;
    let project_id = ProjectId(Uuid::parse_str("019c527b-6242-7b43-bfd4-bd60e27c1de3")?);
    let workspace = store.register_workspace(
        &WorkspaceRegistration {
            project_id,
            root: PathBuf::from("/work/minimal"),
            config_path: PathBuf::from("/work/minimal/.config/mm.yaml"),
        },
        1_000,
    )?;
    let operation_id = OperationId::new();
    store.begin_operation(&NewOperation {
        id: operation_id,
        workspace_id: workspace.id,
        kind: OperationKind::Deploy,
        environment: "local".to_owned(),
        requested_at_unix_ms: 1_100,
        plan_hash: Some("plan-hash".to_owned()),
    })?;
    store.append_event(
        operation_id,
        1_200,
        &OperationEvent::PhaseChanged {
            operation_id,
            phase: OperationPhase::Render,
        },
    )?;
    assert_eq!(store.running_operations(workspace.id)?.len(), 1);
    assert!(
        store
            .append_event(
                operation_id,
                1_250,
                &OperationEvent::PhaseChanged {
                    operation_id: OperationId::new(),
                    phase: OperationPhase::Validate,
                },
            )
            .is_err()
    );
    assert!(
        store
            .append_event(
                operation_id,
                1_260,
                &OperationEvent::Started {
                    operation_id,
                    kind: OperationKind::Deploy,
                },
            )
            .is_err()
    );
    assert!(
        store
            .finish_operation(operation_id, OperationState::Running, 1_300, None)
            .is_err()
    );

    assert!(store.try_acquire_lock(
        "runtime:local:minimal",
        operation_id,
        1_200,
        Duration::from_secs(30),
    )?);
    assert!(!store.try_acquire_lock(
        "runtime:local:minimal",
        OperationId::new(),
        1_300,
        Duration::from_secs(30),
    )?);

    store.finish_operation(operation_id, OperationState::Succeeded, 1_400, None)?;
    assert!(store.running_operations(workspace.id)?.is_empty());
    assert!(
        store
            .finish_operation(operation_id, OperationState::Failed, 1_500, Some("LATE"))
            .is_err()
    );
    assert!(
        store
            .append_event(
                operation_id,
                1_600,
                &OperationEvent::PhaseChanged {
                    operation_id,
                    phase: OperationPhase::Cleanup,
                },
            )
            .is_err()
    );
    store.release_lock("runtime:local:minimal", operation_id)?;
    let operations = store.recent_operations(workspace.id, 10)?;

    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].id, operation_id);
    assert_eq!(operations[0].state, OperationState::Succeeded);
    assert_eq!(operations[0].error_code, None);
    assert_eq!(operations[0].finished_at_unix_ms, Some(1_400));
    let connection = rusqlite::Connection::open(state_path)?;
    let event_count = connection.query_row(
        "SELECT COUNT(*) FROM operation_events WHERE operation_id = ?1",
        [operation_id.0.to_string()],
        |row| row.get::<_, i64>(0),
    )?;
    assert_eq!(event_count, 3);
    Ok(())
}

#[test]
fn local_selection_preferences_round_trip_update_and_unset_at_both_scopes()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let project_id = ProjectId(Uuid::from_u128(991));
    assert!(store.local_project_preference(project_id)?.is_none());
    assert!(
        store
            .local_environment_preference(project_id, "local")?
            .is_none()
    );
    assert!(!store.unset_local_project_preference(project_id)?);
    assert!(!store.unset_local_environment_preference(project_id, "local")?);

    store.set_local_project_preference(
        project_id,
        Some(RuntimeKind::PodmanCompose),
        Some("desktop"),
        10,
    )?;
    let project = store
        .local_project_preference(project_id)?
        .ok_or("project preference")?;
    assert_eq!(project.runtime, Some(RuntimeKind::PodmanCompose));
    assert_eq!(project.target.as_deref(), Some("desktop"));
    store.set_local_project_preference(project_id, Some(RuntimeKind::DockerCompose), None, 20)?;
    let project = store
        .local_project_preference(project_id)?
        .ok_or("updated preference")?;
    assert_eq!(project.runtime, Some(RuntimeKind::DockerCompose));
    assert!(project.target.is_none());

    store.set_local_environment_preference(
        project_id,
        "local",
        Some(RuntimeKind::PodmanCompose),
        Some("remote"),
        30,
    )?;
    let environment = store
        .local_environment_preference(project_id, "local")?
        .ok_or("environment preference")?;
    assert_eq!(environment.runtime, Some(RuntimeKind::PodmanCompose));
    assert_eq!(environment.target.as_deref(), Some("remote"));
    store.set_local_environment_preference(project_id, "local", None, None, 40)?;
    let environment = store
        .local_environment_preference(project_id, "local")?
        .ok_or("updated environment preference")?;
    assert!(environment.runtime.is_none());
    assert!(environment.target.is_none());

    assert!(store.unset_local_project_preference(project_id)?);
    assert!(!store.unset_local_project_preference(project_id)?);
    assert!(store.unset_local_environment_preference(project_id, "local")?);
    assert!(!store.unset_local_environment_preference(project_id, "local")?);
    Ok(())
}

#[test]
fn reusable_image_cache_upserts_exact_variant_keys_and_deployment_queries_are_owner_scoped()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let project_id = ProjectId(Uuid::from_u128(992));
    let workspace = store.register_workspace(
        &WorkspaceRegistration {
            project_id,
            root: PathBuf::from("/work/cache-project"),
            config_path: PathBuf::from("/work/cache-project/.config/mm.yaml"),
        },
        1,
    )?;
    let operation_id = OperationId::new();
    store.begin_operation(&NewOperation {
        id: operation_id,
        workspace_id: workspace.id,
        kind: OperationKind::Build,
        environment: "local".to_owned(),
        requested_at_unix_ms: 2,
        plan_hash: Some("a".repeat(64)),
    })?;
    assert!(
        store
            .lookup_reusable_image(project_id, "local", "fingerprint", "api", None)?
            .is_none()
    );
    assert!(
        store
            .record_reusable_image(ReusableImageRecord {
                project_id,
                environment: "local",
                fingerprint: "fingerprint",
                module: "api",
                variant: None,
                repository: "example.test/api",
                digest: "invalid",
                operation_id,
            })
            .is_err()
    );
    let first_digest = format!("sha256:{}", "a".repeat(64));
    store.record_reusable_image(ReusableImageRecord {
        project_id,
        environment: "local",
        fingerprint: "fingerprint",
        module: "api",
        variant: None,
        repository: "example.test/api",
        digest: &first_digest,
        operation_id,
    })?;
    let cached = store
        .lookup_reusable_image(project_id, "local", "fingerprint", "api", None)?
        .ok_or("default cache entry")?;
    assert_eq!(cached.repository, "example.test/api");
    assert_eq!(cached.digest, first_digest);

    let second_digest = format!("sha256:{}", "b".repeat(64));
    store.record_reusable_image(ReusableImageRecord {
        project_id,
        environment: "local",
        fingerprint: "fingerprint",
        module: "api",
        variant: None,
        repository: "mirror.test/api",
        digest: &second_digest,
        operation_id,
    })?;
    store.record_reusable_image(ReusableImageRecord {
        project_id,
        environment: "local",
        fingerprint: "fingerprint",
        module: "api",
        variant: Some("release"),
        repository: "example.test/api-release",
        digest: &first_digest,
        operation_id,
    })?;
    assert_eq!(
        store
            .lookup_reusable_image(project_id, "local", "fingerprint", "api", None)?
            .ok_or("updated cache entry")?
            .repository,
        "mirror.test/api"
    );
    assert_eq!(
        store
            .lookup_reusable_image(project_id, "local", "fingerprint", "api", Some("release"))?
            .ok_or("variant cache entry")?
            .repository,
        "example.test/api-release"
    );

    let target_id = TargetId(Uuid::from_u128(993));
    assert!(
        store
            .latest_successful_deployment(workspace.id, "local", target_id)?
            .is_none()
    );
    assert!(
        store
            .latest_successful_deployment_for_owner(project_id, "local", target_id)?
            .is_none()
    );
    assert!(
        store
            .deployment_candidates_for_owner(project_id, "local", target_id)?
            .is_empty()
    );
    Ok(())
}

#[test]
fn exec_audit_journals_only_policy_safe_metadata_and_rejects_rewrites()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let path = temporary.path().join("state.sqlite3");
    let store = SqliteStateStore::open(&path)?;
    let mut record = ExecAuditRecord {
        id: Uuid::new_v4(),
        principal: "developer".to_owned(),
        runtime: RuntimeKind::DockerCompose,
        project_id: ProjectId(Uuid::new_v4()),
        environment: "local".to_owned(),
        stack: "app".to_owned(),
        module: Some("backend".to_owned()),
        service: "api".to_owned(),
        container_id: None,
        executable: "printf".to_owned(),
        argv_hash: "a".repeat(64),
        started_at_unix_ms: 1_000,
        finished_at_unix_ms: None,
        exit_code: None,
        error_code: None,
    };
    store.append_exec_audit(&record)?;
    record.container_id = Some("0123456789abcdef".to_owned());
    store.append_exec_audit(&record)?;
    record.finished_at_unix_ms = Some(1_100);
    record.exit_code = Some(17);
    store.append_exec_audit(&record)?;
    assert!(store.append_exec_audit(&record).is_err());

    let connection = rusqlite::Connection::open(path)?;
    let values = connection.query_row(
        "SELECT principal, module_name, service_name, executable, argv_hash, exit_code, error_code FROM runtime_exec_audit WHERE id=?1",
        [record.id.to_string()],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i32>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        },
    )?;
    assert_eq!(values.0, "developer");
    assert_eq!(values.1.as_deref(), Some("backend"));
    assert_eq!(values.2, "api");
    assert_eq!(values.3, "printf");
    assert_eq!(values.4, "a".repeat(64));
    assert_eq!(values.5, 17);
    assert_eq!(values.6, None);
    let columns = connection
        .prepare("PRAGMA table_info(runtime_exec_audit)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    assert!(!columns.iter().any(|column| matches!(
        column.as_str(),
        "stdin" | "stdout" | "stderr" | "environment" | "argv"
    )));
    Ok(())
}

#[test]
fn does_not_reassign_history_when_a_root_changes_project() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let registration = WorkspaceRegistration {
        project_id: ProjectId(Uuid::new_v4()),
        root: PathBuf::from("/work/minimal"),
        config_path: PathBuf::from("/work/minimal/.config/mm.yaml"),
    };
    store.register_workspace(&registration, 1_000)?;
    let replacement = WorkspaceRegistration {
        project_id: ProjectId(Uuid::new_v4()),
        ..registration
    };

    assert!(store.register_workspace(&replacement, 2_000).is_err());
    Ok(())
}

#[test]
fn override_revisions_do_not_repeat_after_deletion() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let workspace = store.register_workspace(
        &WorkspaceRegistration {
            project_id: ProjectId(Uuid::new_v4()),
            root: PathBuf::from("/work/revisions"),
            config_path: PathBuf::from("/work/revisions/.config/mm.yaml"),
        },
        1_000,
    )?;
    let first =
        store.set_environment_override(workspace.id, "local", "DB.HOST", "first", 2_000, None)?;
    assert!(
        store
            .set_environment_override(
                workspace.id,
                "local",
                "DB.HOST",
                "invalid\nvalue",
                2_500,
                None,
            )
            .is_err()
    );
    assert!(store.unset_environment_override(workspace.id, "local", "DB.HOST")?);
    assert!(
        store
            .set_environment_override(
                workspace.id,
                "local",
                "DB.HOST",
                "stale",
                3_000,
                Some(first.revision),
            )
            .is_err()
    );
    let second =
        store.set_environment_override(workspace.id, "local", "DB.HOST", "second", 4_000, None)?;
    assert!(second.revision > first.revision);
    Ok(())
}

#[test]
fn ingress_route_set_starts_at_generation_zero() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let target_id = ingress_target_id();

    let route_set = store.route_set(target_id)?;

    assert_eq!(route_set.target_id, target_id);
    assert_eq!(route_set.generation, 0);
    assert_eq!(route_set.config_digest, None);
    assert!(route_set.claims.is_empty());
    Ok(())
}

#[test]
fn ingress_route_set_cas_returns_canonical_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let target_id = ingress_target_id();
    let project_id = ingress_project_id(1);
    let later = ingress_claim(target_id, project_id, "later", "z.example.test", "/");
    let earlier = ingress_claim(target_id, project_id, "earlier", "a.example.test", "/");

    let route_set = replace_owner_claims_cas!(
        &store,
        target_id,
        project_id,
        "production",
        0,
        &[later.clone(), earlier.clone()],
        &BTreeMap::new(),
        Some("Exact-Dynamic-Config-Digest"),
        1_000,
    )?;

    assert_eq!(route_set.generation, 1);
    assert_eq!(
        route_set.config_digest.as_deref(),
        Some("Exact-Dynamic-Config-Digest")
    );
    assert_eq!(route_set.claims, vec![earlier, later]);
    assert_eq!(store.route_set(target_id)?, route_set);
    Ok(())
}

#[test]
fn ingress_route_set_cas_rejects_stale_generation() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let target_id = ingress_target_id();
    let project_id = ingress_project_id(1);
    let first = ingress_claim(target_id, project_id, "first", "first.example.test", "/");
    let replacement = ingress_claim(
        target_id,
        project_id,
        "replacement",
        "replacement.example.test",
        "/",
    );
    let committed = replace_owner_claims_cas!(
        &store,
        target_id,
        project_id,
        "production",
        0,
        std::slice::from_ref(&first),
        &BTreeMap::new(),
        Some("first-digest"),
        1_000,
    )?;

    assert!(
        replace_owner_claims_cas!(
            &store,
            target_id,
            project_id,
            "production",
            0,
            &[replacement],
            &BTreeMap::new(),
            Some("stale-digest"),
            2_000,
        )
        .is_err()
    );
    assert_eq!(store.route_set(target_id)?, committed);
    Ok(())
}

#[test]
fn project_route_replacement_cannot_spoof_managed_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let target_id = ingress_target_id();
    let project_id = ingress_project_id(1);
    let mut claim = ingress_claim(target_id, project_id, "spoof", "mm.test", "/");
    claim.provenance = mm::domain::ingress::IngressRouteProvenance::MmManagedService;
    claim.ownership = mm::domain::ingress::IngressRouteOwnership::MmManaged {
        component: "dashboard".to_owned(),
    };

    assert!(
        replace_owner_claims_cas!(
            &store,
            target_id,
            project_id,
            "production",
            0,
            &[claim],
            &BTreeMap::new(),
            Some("spoofed-digest"),
            1_000,
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn ingress_route_set_replacement_preserves_other_projects() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let target_id = ingress_target_id();
    let first_project = ingress_project_id(1);
    let second_project = ingress_project_id(2);
    let first = ingress_claim(target_id, first_project, "first", "first.example.test", "/");
    let second = ingress_claim(
        target_id,
        second_project,
        "second",
        "second.example.test",
        "/",
    );
    let replacement = ingress_claim(
        target_id,
        first_project,
        "replacement",
        "third.example.test",
        "/",
    );
    replace_owner_claims_cas!(
        &store,
        target_id,
        first_project,
        "production",
        0,
        &[first],
        &BTreeMap::new(),
        Some("first-digest"),
        1_000,
    )?;
    replace_owner_claims_cas!(
        &store,
        target_id,
        second_project,
        "production",
        1,
        std::slice::from_ref(&second),
        &BTreeMap::new(),
        Some("second-digest"),
        2_000,
    )?;

    let route_set = replace_owner_claims_cas!(
        &store,
        target_id,
        first_project,
        "production",
        2,
        std::slice::from_ref(&replacement),
        &BTreeMap::new(),
        Some("replacement-digest"),
        3_000,
    )?;

    assert_eq!(route_set.generation, 3);
    assert_eq!(route_set.claims, vec![second, replacement]);
    Ok(())
}

#[test]
fn empty_ingress_route_set_replacement_removes_owner_claims()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let target_id = ingress_target_id();
    let project_id = ingress_project_id(1);
    let claim = ingress_claim(target_id, project_id, "route", "route.example.test", "/");
    replace_owner_claims_cas!(
        &store,
        target_id,
        project_id,
        "production",
        0,
        &[claim],
        &BTreeMap::new(),
        Some("populated-digest"),
        1_000,
    )?;

    let route_set = replace_owner_claims_cas!(
        &store,
        target_id,
        project_id,
        "production",
        1,
        &[],
        &BTreeMap::new(),
        Some("empty-digest"),
        2_000,
    )?;

    assert_eq!(route_set.generation, 2);
    assert_eq!(route_set.config_digest.as_deref(), Some("empty-digest"));
    assert!(route_set.claims.is_empty());
    Ok(())
}

#[test]
fn concurrent_ingress_route_set_cas_commits_one_generation()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let path = temporary.path().join("state.sqlite3");
    let stores = [
        SqliteStateStore::open(&path)?,
        SqliteStateStore::open(&path)?,
    ];
    let target_id = ingress_target_id();
    let claims = [
        ingress_claim(
            target_id,
            ingress_project_id(1),
            "first",
            "first.example.test",
            "/",
        ),
        ingress_claim(
            target_id,
            ingress_project_id(2),
            "second",
            "second.example.test",
            "/",
        ),
    ];
    let barrier = Arc::new(Barrier::new(2));
    let handles = stores
        .into_iter()
        .zip(claims)
        .enumerate()
        .map(|(index, (store, claim))| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                replace_owner_claims_cas!(
                    &store,
                    target_id,
                    claim.project_id,
                    "production",
                    0,
                    std::slice::from_ref(&claim),
                    &BTreeMap::new(),
                    Some(if index == 0 {
                        "first-digest"
                    } else {
                        "second-digest"
                    }),
                    1_000,
                )
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().map_err(|_| "route claimant panicked"))
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let verifier = SqliteStateStore::open(&path)?;
    let route_set = verifier.route_set(target_id)?;
    assert_eq!(route_set.generation, 1);
    assert_eq!(route_set.claims.len(), 1);
    let expected_digest = if route_set.claims[0].route == "first" {
        "first-digest"
    } else {
        "second-digest"
    };
    assert_eq!(route_set.config_digest.as_deref(), Some(expected_digest));
    Ok(())
}

#[test]
fn concurrent_first_opens_initialize_schema_once() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let path = Arc::new(temporary.path().join("state.sqlite3"));
    let barrier = Arc::new(Barrier::new(8));
    let handles = (0..8)
        .map(|_| {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                SqliteStateStore::open(&path).map(|_| ())
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().map_err(|_| "database opener panicked")??;
    }
    let connection = rusqlite::Connection::open(path.as_ref())?;
    let table_count = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'environment_value_overrides'",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    assert_eq!(table_count, 1);
    Ok(())
}

#[test]
fn fresh_database_uses_one_complete_baseline_and_reopens_idempotently()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let path = temporary.path().join("state.sqlite3");
    let store = SqliteStateStore::open(&path)?;
    store.register_workspace(
        &WorkspaceRegistration {
            project_id: ProjectId(Uuid::new_v4()),
            root: PathBuf::from("/work/baseline"),
            config_path: PathBuf::from("/work/baseline/.config/mm.yaml"),
        },
        1_000,
    )?;
    drop(store);
    SqliteStateStore::open(&path)?;

    let connection = rusqlite::Connection::open(&path)?;
    let migrations = connection
        .prepare("SELECT version FROM schema_migrations ORDER BY version")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(migrations, ["0001_initial"]);
    assert_eq!(
        connection.query_row("SELECT COUNT(*) FROM workspaces", [], |row| row
            .get::<_, i64>(0))?,
        1
    );
    for table in [
        "ingress_route_claims",
        "gate_policies",
        "runtime_exec_audit",
        "scheduled_jobs",
        "job_runs",
        "mcp_audit",
    ] {
        assert_eq!(
            connection.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get::<_, i64>(0),
            )?,
            1,
            "missing baseline table {table}"
        );
    }
    let route_columns = connection
        .prepare("PRAGMA table_info(ingress_route_claims)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    assert!(
        route_columns
            .iter()
            .any(|column| column == "route_provenance")
    );
    assert!(
        route_columns
            .iter()
            .any(|column| column == "certificate_reference_json")
    );
    for index in [
        "operations_workspace_requested",
        "ingress_route_claims_owner",
        "gate_sessions_policy_expiry",
        "runtime_exec_audit_owner_time",
        "schedules_due",
        "job_runs_runnable",
        "mcp_audit_scope_time",
    ] {
        assert_eq!(
            connection.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                [index],
                |row| row.get::<_, i64>(0),
            )?,
            1,
            "missing baseline index {index}"
        );
    }
    Ok(())
}

#[test]
fn corrupt_database_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let path = temporary.path().join("state.sqlite3");
    std::fs::write(&path, b"not a sqlite database")?;
    assert!(SqliteStateStore::open(&path).is_err());
    assert_eq!(std::fs::read(path)?, b"not a sqlite database");
    Ok(())
}

#[test]
fn baseline_enforces_foreign_keys_uniqueness_and_checks() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = temp_dirs::tempdir()?;
    let path = temporary.path().join("state.sqlite3");
    SqliteStateStore::open(&path)?;
    let connection = rusqlite::Connection::open(path)?;
    connection.execute_batch("PRAGMA foreign_keys = ON;")?;
    assert!(connection.execute(
        "INSERT INTO operations(id, workspace_id, kind, environment, state, requested_at_ms) VALUES('operation', 'missing', 'deploy', 'local', 'running', 1)",
        [],
    ).is_err());
    assert!(connection.execute(
        "INSERT INTO gate_policies(id, project_id, environment_name, policy_name, policy_json, revision, updated_at_ms) VALUES('policy', 'project', 'local', 'default', '{}', 0, 1)",
        [],
    ).is_err());
    connection.execute(
        "INSERT INTO workspaces(id, project_id, root, config_path, created_at_ms, last_seen_at_ms) VALUES('one', 'project', '/same', '/one', 1, 1)",
        [],
    )?;
    assert!(connection.execute(
        "INSERT INTO workspaces(id, project_id, root, config_path, created_at_ms, last_seen_at_ms) VALUES('two', 'project', '/same', '/two', 1, 1)",
        [],
    ).is_err());
    Ok(())
}

#[test]
fn pre_release_migration_ledger_is_rejected_before_baseline_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let path = temporary.path().join("state.sqlite3");
    let connection = rusqlite::Connection::open(&path)?;
    connection.execute_batch(
        "CREATE TABLE schema_migrations (version TEXT PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
         INSERT INTO schema_migrations(version) VALUES('0002_local_ingress_metadata');",
    )?;
    drop(connection);

    assert!(SqliteStateStore::open(&path).is_err());
    let connection = rusqlite::Connection::open(path)?;
    assert_eq!(
        connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='workspaces'",
            [],
            |row| row.get::<_, i64>(0),
        )?,
        0
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn explicit_mm_home_must_already_be_private_and_is_not_silently_chmodded()
-> Result<(), Box<dyn std::error::Error>> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let temporary = temp_dirs::tempdir()?;
    let shared = temporary.path().join("shared");
    fs::create_dir(&shared)?;
    fs::set_permissions(&shared, fs::Permissions::from_mode(0o755))?;
    fs::create_dir(shared.join("run"))?;
    fs::write(shared.join("unrelated.txt"), "keep")?;

    assert!(MmHome::resolve(Some(&shared)).is_err());
    assert_eq!(fs::metadata(&shared)?.permissions().mode() & 0o777, 0o755);
    Ok(())
}

fn ingress_target_id() -> TargetId {
    TargetId(Uuid::from_u128(0x419c527b62427b43bfd4bd60e27c1de3))
}

fn ingress_project_id(index: u128) -> ProjectId {
    ProjectId(Uuid::from_u128(index))
}

fn ingress_claim(
    target_id: TargetId,
    project_id: ProjectId,
    route: &str,
    host: &str,
    path_prefix: &str,
) -> IngressRouteClaim {
    IngressRouteClaim {
        target: "shared".to_owned(),
        target_id,
        project_id,
        project_name: format!("project-{}", project_id.0),
        environment: "production".to_owned(),
        stack: "app".to_owned(),
        route: route.to_owned(),
        host: host.to_owned(),
        path_prefix: path_prefix.to_owned(),
        service: "web".to_owned(),
        port: 8080,
        tls: true,
        provenance: mm::domain::ingress::IngressRouteProvenance::ExplicitConfig,
        certificate_reference: mm::domain::ingress::IngressCertificateReference::LocalCa {
            host: host.to_owned(),
        },
        ownership: mm::domain::ingress::IngressRouteOwnership::Project {
            project_id,
            environment: "production".to_owned(),
            stack: "app".to_owned(),
            service: "web".to_owned(),
        },
        priority: if path_prefix == "/" {
            1
        } else {
            1000 + path_prefix.len() as i64
        },
        router_name: format!("mm-{route}"),
        backend_alias: format!("mm-ingress-backend-{route}"),
        artifact_hash: format!("artifact-{route}"),
        middlewares: Vec::new(),
    }
}
