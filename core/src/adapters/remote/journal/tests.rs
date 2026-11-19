use super::{RemoteJournal, create_private_directory, write_new_private};
use crate::domain::deployment::{DEPLOYMENT_PLAN_SCHEMA, DeploymentPlan, DeploymentRouteDiff};
use crate::domain::remote::{
    PROTOCOL_VERSION, RemoteDeploymentPlan, RemotePlanBinding, RemotePlanKind, RemoteScope,
    StoredRemotePlan,
};
use crate::domain::runtime::RuntimeKind;
use crate::domain::target::TargetId;
use crate::domain::traefik_dynamic::{PreparedTraefikDynamicConfig, TraefikDynamicConfigManifest};
use crate::domain::workspace::ProjectId;

#[test]
fn restart_preserves_replay_rejection_at_every_request_phase()
-> Result<(), Box<dyn std::error::Error>> {
    for (index, phase) in ["authorized", "running", "succeeded", "failed", "unknown"]
        .into_iter()
        .enumerate()
    {
        let temporary = crate::test_support::tempdir()?;
        let nonce = uuid::Uuid::from_u128(u128::try_from(index + 1)?);
        let request = uuid::Uuid::from_u128(u128::try_from(index + 100)?);
        let journal = RemoteJournal::open(temporary.path().join("journal"))?;
        journal.begin_request(nonce, request)?;
        if phase != "authorized" {
            journal.set_request_phase(nonce, phase)?;
        }
        drop(journal);
        let restarted = RemoteJournal::open(temporary.path().join("journal"))?;
        assert!(restarted.begin_request(nonce, request).is_err());
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(
            temporary
                .path()
                .join("journal/requests")
                .join(nonce.to_string()),
        )?)?;
        assert_eq!(value["phase"], phase);
    }
    Ok(())
}

#[test]
fn reviewed_plan_claim_rejects_scope_change_expiry_and_restart_replay()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let root = std::fs::canonicalize(temporary.path())?;
    let journal_root = root.join("journal");
    let scope = remote_scope(root.clone());
    let plan = remote_plan(&scope, 10_000, uuid::Uuid::from_u128(10))?;
    let journal = RemoteJournal::open(journal_root.clone())?;
    journal.save_plan(&StoredRemotePlan::Deployment(Box::new(plan.clone())))?;
    let mut changed = scope.clone();
    changed.environment = "staging".to_owned();
    assert!(
        journal
            .claim_plan(&plan.binding.plan_hash, &changed, 1)
            .is_err()
    );
    journal.claim_plan(&plan.binding.plan_hash, &scope, 1)?;
    drop(journal);

    let restarted = RemoteJournal::open(journal_root.clone())?;
    assert!(
        restarted
            .claim_plan(&plan.binding.plan_hash, &scope, 1)
            .is_err()
    );
    let expired = remote_plan(&scope, 10, uuid::Uuid::from_u128(11))?;
    restarted.save_plan(&StoredRemotePlan::Deployment(Box::new(expired.clone())))?;
    assert!(
        restarted
            .claim_plan(&expired.binding.plan_hash, &scope, 11)
            .is_err()
    );
    assert!(
        !journal_root
            .join("claims")
            .join(expired.binding.plan_hash)
            .exists()
    );
    Ok(())
}

#[test]
fn journal_rejects_missing_corrupt_oversized_and_unsafe_plan_or_request_state()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let temporary = crate::test_support::tempdir()?;
    let root = std::fs::canonicalize(temporary.path())?;
    let journal_root = root.join("journal");
    let journal = RemoteJournal::open(journal_root.clone())?;
    let scope = remote_scope(root);
    let plan = remote_plan(&scope, 10_000, uuid::Uuid::from_u128(20))?;
    assert!(journal.read_plan(&"a".repeat(64), &scope, 1).is_err());
    for hash in ["short", &"A".repeat(64), &"z".repeat(64)] {
        assert!(journal.read_plan(hash, &scope, 1).is_err());
    }

    journal.save_plan(&StoredRemotePlan::Deployment(Box::new(plan.clone())))?;
    let plan_path = journal_root.join("plans").join(&plan.binding.plan_hash);
    std::fs::write(&plan_path, b"not-json")?;
    assert!(
        journal
            .read_plan(&plan.binding.plan_hash, &scope, 1)
            .is_err()
    );
    std::fs::write(
        &plan_path,
        vec![b'x'; crate::domain::remote::MAX_FRAME_BYTES + 1],
    )?;
    assert!(
        journal
            .read_plan(&plan.binding.plan_hash, &scope, 1)
            .is_err()
    );
    std::fs::remove_file(&plan_path)?;
    std::fs::create_dir(&plan_path)?;
    assert!(
        journal
            .read_plan(&plan.binding.plan_hash, &scope, 1)
            .is_err()
    );

    let nonce = uuid::Uuid::from_u128(21);
    assert!(journal.set_request_phase(nonce, "invalid").is_err());
    assert!(journal.set_request_phase(nonce, "running").is_err());
    journal.begin_request(nonce, uuid::Uuid::from_u128(22))?;
    let request_path = journal_root.join("requests").join(nonce.to_string());
    std::fs::write(&request_path, b"not-json")?;
    assert!(journal.set_request_phase(nonce, "running").is_err());
    std::fs::write(&request_path, br#"{"phase":"authorized"}"#)?;
    assert!(journal.set_request_phase(nonce, "running").is_err());
    std::fs::remove_file(&request_path)?;
    std::fs::create_dir(&request_path)?;
    assert!(journal.set_request_phase(nonce, "running").is_err());

    journal.audit(&serde_json::json!({"event": "ok"}))?;
    assert!(std::fs::read_to_string(journal_root.join("audit.jsonl"))?.ends_with('\n'));
    std::fs::remove_file(journal_root.join("audit.jsonl"))?;
    std::fs::create_dir(journal_root.join("audit.jsonl"))?;
    assert!(
        journal
            .audit(&serde_json::json!({"event": "blocked"}))
            .is_err()
    );

    let duplicate = temporary.path().join("duplicate");
    write_new_private(&duplicate, b"one")?;
    assert!(write_new_private(&duplicate, b"two").is_err());
    let unsafe_directory = temporary.path().join("unsafe-directory");
    std::fs::create_dir(&unsafe_directory)?;
    std::fs::set_permissions(&unsafe_directory, std::fs::Permissions::from_mode(0o777))?;
    assert!(create_private_directory(&unsafe_directory).is_err());
    let file_directory = temporary.path().join("file-directory");
    std::fs::write(&file_directory, b"file")?;
    assert!(create_private_directory(&file_directory).is_err());
    Ok(())
}

fn remote_scope(_root: std::path::PathBuf) -> RemoteScope {
    RemoteScope {
        project_id: ProjectId(uuid::Uuid::from_u128(1)),
        environment: "production".to_owned(),
        target: "production".to_owned(),
        target_fingerprint: "a".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: None,
    }
}

fn remote_plan(
    scope: &RemoteScope,
    expires_at_unix_ms: i64,
    plan_nonce: uuid::Uuid,
) -> Result<RemoteDeploymentPlan, Box<dyn std::error::Error>> {
    let target_id = TargetId(uuid::Uuid::from_u128(2));
    let dynamic = PreparedTraefikDynamicConfig {
        root: std::path::PathBuf::from("/srv/project/dynamic"),
        path: std::path::PathBuf::from("/srv/project/dynamic/dynamic.yaml"),
        manifest: TraefikDynamicConfigManifest {
            target_id,
            claim_count: 0,
            router_count: 0,
            service_count: 0,
            digest: "b".repeat(64),
        },
    };
    let mut plan = DeploymentPlan {
        excluded_services: Vec::new(),
        schema: DEPLOYMENT_PLAN_SCHEMA.to_owned(),
        project_id: scope.project_id,
        project_name: "example".to_owned(),
        target_name: "local".to_owned(),
        target_id,
        target_fingerprint: "d".repeat(64),
        environment: scope.environment.clone(),
        runtime: Some(scope.runtime),
        runtime_source: Some("test".to_owned()),
        workload_intent_hash: Some("e".repeat(64)),
        networks: Vec::new(),
        expected_route_generation: 0,
        expected_route_config_digest: None,
        expected_previous_plan_hash: None,
        stacks: Vec::new(),
        removed_stacks: Vec::new(),
        preserved_volumes: Vec::new(),
        removed_preserved_volumes: Vec::new(),
        previous_owner_claims: Vec::new(),
        adopted_route_claims: Vec::new(),
        desired_route_claims: Vec::new(),
        complete_route_claims: Vec::new(),
        desired_middlewares: std::collections::BTreeMap::new(),
        environment_review: Default::default(),
        runtime_secret_plan: None,
        scheduler_reconciliation: None,
        environment_bundle_root: None,
        previous_dynamic_config: dynamic.clone(),
        dynamic_config: dynamic,
        local_certificates: Vec::new(),
        static_ingress: None,
        removed_ingress: None,
        route_diff: DeploymentRouteDiff::default(),
        ingress: Vec::new(),
        route_state_changed: false,
        build_receipt: None,
        local_build: None,
        registries: Default::default(),
        node: None,
        rollout: Default::default(),
        config_files: Vec::new(),
        runtime_changes: Vec::new(),
        unknown_config_keys: Vec::new(),
        secret_file_owners: Vec::new(),
        release: None,
        plan_hash: String::new(),
    };
    plan.plan_hash = plan.canonical_hash()?;
    let deployment_plan_hash = plan.plan_hash.clone();
    let access = crate::domain::users::AccessChange::between(
        None,
        crate::domain::users::reconcile_users(
            (
                &std::collections::BTreeMap::new(),
                &std::collections::BTreeMap::new(),
            ),
            None,
            scope.project_id,
            &scope.environment,
            &"c".repeat(40),
            1,
            "test",
        )?,
    );
    let mut binding = RemotePlanBinding {
        protocol_version: PROTOCOL_VERSION,
        daemon_version: "0.1.0".to_owned(),
        daemon_fingerprint: "f".repeat(64),
        target_fingerprint: scope.target_fingerprint.clone(),
        target: scope.target.clone(),
        node: None,
        kind: RemotePlanKind::Deployment,
        project_id: scope.project_id,
        environment: scope.environment.clone(),
        runtime: scope.runtime,
        commit: "c".repeat(40),
        deployment_plan_hash,
        access_change_hash: access.canonical_hash()?,
        principal_id: crate::domain::access::PrincipalId(uuid::Uuid::from_u128(9)),
        authorization_revision: 1,
        authorization_hash: "a".repeat(64),
        plan_nonce,
        expires_at_unix_ms,
        bundle_hash: None,
        plan_hash: String::new(),
    };
    binding.plan_hash = binding.canonical_hash()?;
    Ok(RemoteDeploymentPlan {
        binding,
        plan: Some(plan),
        access,
        receipt: None,
        unfinished: None,
        once: Default::default(),
        staged: Vec::new(),
        release: None,
    })
}
