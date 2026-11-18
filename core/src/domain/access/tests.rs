use std::collections::{BTreeMap, BTreeSet};

use uuid::Uuid;

use super::*;

const PROJECT: ProjectId = ProjectId(Uuid::from_u128(2));

fn key(fingerprint: &str) -> PrincipalKey {
    PrincipalKey {
        fingerprint: fingerprint.to_owned(),
        public_key: format!("ssh-ed25519 {}", fingerprint.trim_start_matches("SHA256:")),
        disabled: false,
        expires_at_unix_ms: None,
    }
}

fn principal(
    id: u128,
    name: &str,
    environment: Option<&str>,
    keys: Vec<PrincipalKey>,
) -> Principal {
    Principal {
        id: PrincipalId(Uuid::from_u128(id)),
        display_name: name.to_owned(),
        project_id: environment.map(|_| PROJECT),
        environment: environment.map(str::to_owned),
        disabled: false,
        keys,
    }
}

fn grant(
    id: u128,
    principal: u128,
    role: BuiltInRole,
    environment: &str,
    services: &[&str],
) -> Grant {
    Grant {
        id: Uuid::from_u128(id),
        principal_id: PrincipalId(Uuid::from_u128(principal)),
        role: RoleRef::BuiltIn(role),
        scope: ScopeMatcher {
            project_id: PROJECT,
            environments: BTreeSet::from([environment.to_owned()]),
            services: services
                .iter()
                .map(|service| (*service).to_owned())
                .collect(),
            jobs: BTreeSet::new(),
            gates: BTreeSet::new(),
        },
    }
}

fn policy() -> AccessPolicy {
    AccessPolicy {
        schema_version: ACCESS_POLICY_SCHEMA_VERSION,
        revision: 3,
        principals: vec![
            principal(1, "alice", Some("staging"), vec![key("SHA256:alice")]),
            principal(2, "alice", Some("production"), vec![key("SHA256:alice")]),
            principal(3, "break-glass:ops", None, vec![key("SHA256:ops")]),
        ],
        roles: BTreeMap::new(),
        grants: vec![
            grant(10, 1, BuiltInRole::Deployer, "staging", &[]),
            grant(11, 2, BuiltInRole::Viewer, "production", &["api"]),
            grant(12, 3, BuiltInRole::Deployer, "production", &[]),
        ],
        denials: Vec::new(),
    }
}

fn request<'a>(
    principal: u128,
    action: AccessAction,
    environment: &'a str,
    service: Option<&'a str>,
) -> AccessRequest<'a> {
    AccessRequest {
        principal_id: PrincipalId(Uuid::from_u128(principal)),
        capability: Capability::new(
            action,
            if service.is_some() {
                AccessResourceKind::Service
            } else {
                AccessResourceKind::Project
            },
        ),
        project_id: PROJECT,
        environment,
        service,
        job: None,
        gate: None,
    }
}

#[test]
fn authentication_picks_the_environment_principal_then_break_glass() -> Result<(), String> {
    let policy = policy();
    policy.validate()?;
    assert_eq!(
        policy
            .authenticate("SHA256:alice", PROJECT, "staging", 1)?
            .id
            .0,
        Uuid::from_u128(1)
    );
    assert_eq!(
        policy
            .authenticate("SHA256:alice", PROJECT, "production", 1)?
            .id
            .0,
        Uuid::from_u128(2)
    );
    assert!(
        policy
            .authenticate("SHA256:alice", PROJECT, "qa", 1)
            .is_err()
    );
    assert!(
        policy
            .authenticate("SHA256:alice", ProjectId(Uuid::from_u128(9)), "staging", 1)
            .is_err()
    );
    assert_eq!(
        policy
            .authenticate("SHA256:ops", PROJECT, "qa", 1)?
            .display_name,
        "break-glass:ops"
    );
    assert!(
        policy
            .authenticate("SHA256:unknown", PROJECT, "staging", 1)
            .is_err()
    );

    let mut expiring = policy.clone();
    expiring.principals[0].keys[0].expires_at_unix_ms = Some(10);
    assert!(
        expiring
            .authenticate("SHA256:alice", PROJECT, "staging", 9)
            .is_ok()
    );
    assert!(
        expiring
            .authenticate("SHA256:alice", PROJECT, "staging", 10)
            .is_err()
    );
    let mut disabled = policy;
    disabled.principals[0].disabled = true;
    assert!(
        disabled
            .authenticate("SHA256:alice", PROJECT, "staging", 1)
            .is_err()
    );
    Ok(())
}

#[test]
fn authorization_is_scoped_to_environment_and_service() {
    let policy = policy();
    assert!(
        policy
            .authorize(&request(1, AccessAction::DeployApply, "staging", None))
            .is_ok()
    );
    assert!(
        policy
            .authorize(&request(1, AccessAction::DeployApply, "production", None))
            .is_err()
    );
    assert!(
        policy
            .authorize(&request(2, AccessAction::Logs, "production", Some("api")))
            .is_ok()
    );
    assert!(
        policy
            .authorize(&request(2, AccessAction::Logs, "production", Some("db")))
            .is_err()
    );
    assert!(
        policy
            .authorize(&request(2, AccessAction::DeployApply, "production", None))
            .is_err()
    );

    let mut denied = policy.clone();
    denied.denials.push(Denial {
        id: Uuid::from_u128(20),
        principal_id: PrincipalId(Uuid::from_u128(1)),
        capabilities: BTreeSet::from([Capability::new(
            AccessAction::DeployApply,
            AccessResourceKind::Project,
        )]),
        scope: grant(0, 1, BuiltInRole::Viewer, "staging", &[]).scope,
    });
    assert!(
        denied
            .authorize(&request(1, AccessAction::DeployApply, "staging", None))
            .is_err()
    );
    assert!(
        denied
            .authorize(&request(1, AccessAction::Status, "staging", None))
            .is_ok()
    );

    let first = policy.authorize(&request(1, AccessAction::Status, "staging", None));
    let mut revised = policy;
    revised.revision = 4;
    let second = revised.authorize(&request(1, AccessAction::Status, "staging", None));
    assert_eq!(
        first.map(|authorization| authorization.authorization_hash),
        second.map(|authorization| authorization.authorization_hash),
        "a new revision that changes no grant keeps reviewed authorizations"
    );
}

#[test]
fn validation_rejects_ambiguous_or_dangling_policy() {
    let mut duplicate_key = policy();
    duplicate_key.principals[1].environment = Some("staging".to_owned());
    assert!(
        duplicate_key.validate().is_err(),
        "one key twice in one environment"
    );

    let mut bad_fingerprint = policy();
    bad_fingerprint.principals[0].keys[0].fingerprint = "MD5:aa".to_owned();
    assert!(bad_fingerprint.validate().is_err());

    let mut dangling = policy();
    dangling.grants[0].principal_id = PrincipalId(Uuid::from_u128(999));
    assert!(dangling.validate().is_err());

    let mut other_project = policy();
    other_project.grants[0].scope.project_id = ProjectId(Uuid::from_u128(9));
    assert!(
        other_project.validate().is_err(),
        "project users cannot hold grants elsewhere"
    );

    let mut unknown_role = policy();
    unknown_role.grants[0].role = RoleRef::Custom("missing".to_owned());
    assert!(unknown_role.validate().is_err());

    let mut no_environment = policy();
    no_environment.grants[0].scope.environments.clear();
    assert!(no_environment.validate().is_err());

    let mut old = policy();
    old.schema_version = 1;
    assert!(old.validate().is_err());
}

#[test]
fn authorized_keys_are_unique_and_skip_inactive_keys() {
    let mut policy = policy();
    policy.principals[1].keys[0].expires_at_unix_ms = Some(50);
    let keys = policy.authorized_keys(1);
    assert_eq!(keys.len(), 2, "alice's key appears once");
    assert_eq!(
        keys.get("ssh-ed25519 alice"),
        Some(&None),
        "the non-expiring entry wins"
    );
    policy.principals[2].keys[0].disabled = true;
    assert_eq!(policy.authorized_keys(1).len(), 1);
}

#[test]
fn built_in_roles_nest() {
    let viewer = BuiltInRole::Viewer.capabilities();
    let operator = BuiltInRole::Operator.capabilities();
    let deployer = BuiltInRole::Deployer.capabilities();
    assert!(viewer.is_subset(&operator) && operator.is_subset(&deployer));
    assert!(!deployer.contains(&Capability::new(
        AccessAction::Exec,
        AccessResourceKind::Service
    )));
}
