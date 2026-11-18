use super::*;
use crate::domain::build::{ImageBuildResult, ReusableImageRecord};
use crate::domain::target::TargetBootstrapResult;

#[test]
fn snapshot_parsers_reject_corrupt_missing_unsupported_and_incomplete_documents() {
    assert!(parse_image_build_plan("not-json").is_err());
    assert!(parse_image_build_plan(r#"{"project_id":"missing-schema"}"#).is_err());
    assert!(parse_image_build_plan(r#"{"schema":"pre-release-v0"}"#).is_err());
    assert!(parse_image_build_plan(&format!(r#"{{"schema":"{PLAN_SCHEMA}"}}"#)).is_err());
    assert!(parse_deployment_plan_snapshot("not-json").is_err());
}

#[cfg(unix)]
#[test]
fn empty_store_queries_and_invalid_results_fail_at_adapter_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = crate::test_support::tempdir()?;
    let store = SqliteStateStore::open(&temp.path().join("state.sqlite"))?;
    let operation_id = OperationId(Uuid::from_u128(501));
    let project_id = ProjectId(Uuid::from_u128(502));
    let workspace_id = WorkspaceId(Uuid::from_u128(503));
    let target_id = TargetId(Uuid::from_u128(504));

    let image_result = ImageBuildResult {
        operation_id,
        plan_hash: "a".repeat(64),
        images: Vec::new(),
        dependencies: Vec::new(),
    };
    assert!(
        store
            .save_image_build_result(operation_id, &image_result)
            .is_err()
    );
    assert!(
        store
            .latest_successful_image_build(project_id, temp.path(), "development",)?
            .is_none()
    );
    let non_unicode = temp.path().join(OsString::from_vec(vec![0xff]));
    assert!(
        store
            .latest_successful_image_build(project_id, &non_unicode, "development",)
            .is_err()
    );
    assert!(
        store
            .lookup_reusable_image(project_id, "development", "fingerprint", "api", None)?
            .is_none()
    );
    assert!(
        store
            .record_reusable_image(ReusableImageRecord {
                project_id,
                environment: "development",
                fingerprint: "fingerprint",
                module: "api",
                variant: None,
                repository: "example/api",
                digest: "not-a-digest",
                operation_id,
            })
            .is_err()
    );

    let bootstrap_result = TargetBootstrapResult {
        operation_id,
        plan_hash: "c".repeat(64),
        target: "local".to_owned(),
        target_id,
        verified_capabilities: Vec::new(),
        next_command: "mm deploy".to_owned(),
        access: None,
    };
    assert!(
        store
            .save_target_bootstrap_result(operation_id, &bootstrap_result)
            .is_err()
    );
    assert!(store.target_bootstrap_plan(operation_id)?.is_none());
    assert!(
        store
            .latest_successful_deployment(workspace_id, "development", target_id)?
            .is_none()
    );
    assert!(
        store
            .latest_successful_deployment_for_owner(project_id, "development", target_id)?
            .is_none()
    );
    assert!(
        store
            .deployment_candidates_for_owner(project_id, "development", target_id)?
            .is_empty()
    );
    Ok(())
}

#[test]
fn corrupt_ingress_rows_fail_at_each_typed_decoding_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::app::ports::IngressRouteRegistry as _;

    let temp = crate::test_support::tempdir()?;
    let store = SqliteStateStore::open(&temp.path().join("state.sqlite"))?;
    let target_id = TargetId(Uuid::from_u128(701));
    let project_id = ProjectId(Uuid::from_u128(702));
    let connection = store.connection()?;

    connection.execute(
        "INSERT INTO ingress_route_generations (target_id, generation, config_digest, updated_at_ms) VALUES (?1, '18446744073709551616', NULL, 0)",
        params![target_id.0.to_string()],
    )?;
    drop(connection);
    assert!(store.route_set(target_id).is_err());
    let connection = store.connection()?;
    connection.execute(
        "UPDATE ingress_route_generations SET generation = '0' WHERE target_id = ?1",
        params![target_id.0.to_string()],
    )?;

    connection.execute(
        "INSERT INTO ingress_middleware_definitions (target_id, project_id, environment_name, namespaced_name, definition_json, definition_digest, updated_at_ms) VALUES (?1, ?2, 'local', 'bad-middleware', 'not-json', 'digest', 0)",
        params![target_id.0.to_string(), project_id.0.to_string()],
    )?;
    drop(connection);
    assert!(store.route_set(target_id).is_err());
    let connection = store.connection()?;
    connection.execute(
        "UPDATE ingress_middleware_definitions SET definition_json = '{}' WHERE target_id = ?1",
        params![target_id.0.to_string()],
    )?;

    connection.execute(
        "INSERT INTO ingress_route_claims (target_id, target_name, hostname, path_prefix, project_id, project_name, environment_name, stack_name, route_name, service_name, backend_port, tls_enabled, router_name, backend_alias, artifact_hash, middlewares_json, updated_at_ms, route_provenance, certificate_reference_json) VALUES (?1, 'local', 'app.test', '/', 'invalid', 'project', 'local', 'app', 'web', 'api', 80, 1, 'router', 'backend', 'hash', '[]', 0, 'explicit_config', 'null')",
        params![target_id.0.to_string()],
    )?;
    drop(connection);
    assert!(store.route_set(target_id).is_err());
    let connection = store.connection()?;
    connection.execute(
        "UPDATE ingress_route_claims SET project_id = ?1, certificate_reference_json = 'not-json' WHERE target_id = ?2",
        params![project_id.0.to_string(), target_id.0.to_string()],
    )?;
    drop(connection);
    assert!(store.route_set(target_id).is_err());
    let connection = store.connection()?;
    connection.execute(
        "UPDATE ingress_route_claims SET certificate_reference_json = 'null', route_provenance = 'invalid' WHERE target_id = ?1",
        params![target_id.0.to_string()],
    )?;
    drop(connection);
    assert!(store.route_set(target_id).is_err());
    let connection = store.connection()?;
    connection.execute(
        "UPDATE ingress_route_claims SET route_provenance = 'explicit_config', middlewares_json = 'not-json' WHERE target_id = ?1",
        params![target_id.0.to_string()],
    )?;
    drop(connection);
    assert!(store.route_set(target_id).is_err());

    let connection = store.connection()?;
    connection.execute(
        "DELETE FROM ingress_route_claims WHERE target_id = ?1",
        params![target_id.0.to_string()],
    )?;
    connection.execute(
        "DELETE FROM ingress_middleware_definitions WHERE target_id = ?1",
        params![target_id.0.to_string()],
    )?;
    connection.execute(
        "UPDATE ingress_route_generations SET generation = ?1 WHERE target_id = ?2",
        params![u64::MAX.to_string(), target_id.0.to_string()],
    )?;
    drop(connection);
    assert!(
        store
            .replace_owner_claims(target_id, project_id, "local", &[], &BTreeMap::new(), 0,)
            .is_err()
    );
    Ok(())
}

#[test]
fn state_stored_for_the_removed_runtime_reads_as_absent() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = crate::test_support::tempdir()?;
    let path = temp.path().join("state.sqlite");
    let store = SqliteStateStore::open(&path)?;
    let project_id = ProjectId(Uuid::from_u128(601));

    // Rows as an earlier release wrote them, through the same database file.
    let earlier = Connection::open(&path)?;
    earlier.execute(
        "INSERT INTO local_project_preferences (project_id, runtime, target, updated_at_ms)
         VALUES (?1, 'docker-swarm', 'production', 1)",
        params![project_id.0.to_string()],
    )?;
    earlier.execute(
        "INSERT INTO local_environment_preferences
            (project_id, environment, runtime, target, updated_at_ms)
         VALUES (?1, 'production', 'docker-swarm', NULL, 1)",
        params![project_id.0.to_string()],
    )?;
    drop(earlier);

    let project = store
        .local_project_preference(project_id)?
        .ok_or("the stored project preference was dropped")?;
    assert_eq!(project.runtime, None, "no runtime preference");
    assert_eq!(project.target.as_deref(), Some("production"));
    let environment = store
        .local_environment_preference(project_id, "production")?
        .ok_or("the stored environment preference was dropped")?;
    assert_eq!(environment.runtime, None);
    assert_eq!(environment.target, None);

    // A fresh process reads the same, and a new choice replaces the old row.
    drop(store);
    let store = SqliteStateStore::open(&path)?;
    assert_eq!(
        store
            .local_project_preference(project_id)?
            .and_then(|preference| preference.runtime),
        None
    );
    store.set_local_project_preference(
        project_id,
        Some(crate::domain::runtime::RuntimeKind::DockerCompose),
        None,
        2,
    )?;
    assert_eq!(
        store
            .local_project_preference(project_id)?
            .and_then(|preference| preference.runtime),
        Some(crate::domain::runtime::RuntimeKind::DockerCompose)
    );

    // Any other unknown value is still a corrupt row.
    let connection = store.connection()?;
    connection.execute(
        "UPDATE local_environment_preferences SET runtime = 'kubernetes' WHERE project_id = ?1",
        params![project_id.0.to_string()],
    )?;
    drop(connection);
    assert!(
        store
            .local_environment_preference(project_id, "production")
            .is_err()
    );
    Ok(())
}

#[test]
fn a_deployment_snapshot_of_the_removed_runtime_is_no_previous_deployment() {
    for snapshot in [
        r#"{"runtime":"docker-swarm"}"#,
        r#"{"schema":"anything","runtime":"docker-swarm","swarm":{"cluster_id":"c"},"provided_tls":null,"plan_hash":"stale"}"#,
    ] {
        assert!(
            matches!(parse_deployment_plan_snapshot(snapshot), Ok(None)),
            "{snapshot}"
        );
    }
    // Every other snapshot is still parsed and verified.
    for snapshot in [
        r#"{"runtime":"docker-compose"}"#,
        r#"{"runtime":null}"#,
        r#"{"runtime":"kubernetes"}"#,
        r#"{"driver":"docker-swarm"}"#,
        r#""docker-swarm""#,
    ] {
        assert!(
            parse_deployment_plan_snapshot(snapshot).is_err(),
            "{snapshot}"
        );
    }
}
