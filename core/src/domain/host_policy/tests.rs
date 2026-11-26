use super::*;
use crate::domain::access::{AccessAction, AccessRequest, AccessResourceKind, Capability};

// `ssh-keygen -t ed25519` public keys with their `ssh-keygen -l` fingerprints.
const ALICE: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w alice@laptop";
const BOB: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJZYcbK/sLQnEppB74bVUmUQnMUahuREnHUmhtFYGcf7 bob@desk";

fn project() -> ProjectId {
    ProjectId(uuid::Uuid::from_u128(7))
}

fn host() -> HostPolicy {
    HostPolicy {
        schema_version: HOST_POLICY_SCHEMA_VERSION,
        break_glass: vec![BreakGlassKey {
            name: "ops".to_owned(),
            key: BOB.to_owned(),
        }],
        projects: BTreeMap::from([(
            project(),
            EnrolledProject {
                name: "shop".to_owned(),
                origin: "/srv/git/shop.git".to_owned(),
                origin_host_key: None,
                delivery: Default::default(),
                environments: BTreeSet::from(["staging".to_owned(), "production".to_owned()]),
                initial_deploy_from: BTreeMap::new(),
                ceiling: BTreeMap::from([(
                    "production".to_owned(),
                    EnvironmentAccess {
                        level: AccessLevel::Deploy,
                        exec: ServiceSelection::All(false),
                        forward: ServiceSelection::All(true),
                        secrets: false,
                    },
                )]),
                allow: Vec::new(),
            },
        )]),
        runtimes: BTreeSet::new(),
        exposed_runtime: None,
        metrics: None,
    }
}

fn record(environment: &str, access: EnvironmentAccess) -> Result<EnvironmentAccessRecord, String> {
    let key = parse_public_key(ALICE)?;
    Ok(EnvironmentAccessRecord {
        schema_version: ACCESS_RECORD_SCHEMA_VERSION,
        project_id: project(),
        environment: environment.to_owned(),
        commit: "0".repeat(40),
        users: BTreeMap::from([(
            "alice".to_owned(),
            ReconciledUser {
                machine: false,
                keys: vec![ReconciledKey {
                    public_key: key.openssh(),
                    fingerprint: key.fingerprint,
                    expires_at_unix_ms: None,
                }],
                access,
                agent: None,
            },
        )]),
        deploy_from: None,
        reconciled_at_unix_ms: 1,
        reconciled_by: "test".to_owned(),
    })
}

fn allowed(
    policy: &crate::domain::access::AccessPolicy,
    key: &str,
    environment: &str,
    action: AccessAction,
    resource: AccessResourceKind,
    service: Option<&str>,
) -> Result<bool, String> {
    let fingerprint = parse_public_key(key)?.fingerprint;
    let Ok(principal) = policy.authenticate(&fingerprint, project(), environment, 10) else {
        return Ok(false);
    };
    Ok(policy
        .authorize(&AccessRequest {
            principal_id: principal.id,
            capability: Capability::new(action, resource),
            project_id: project(),
            environment,
            service,
            job: None,
            gate: None,
        })
        .is_ok())
}

#[test]
fn public_keys_are_checked_and_fingerprinted_like_ssh_keygen() -> Result<(), String> {
    let key = parse_public_key(ALICE)?;
    assert_eq!(
        key.fingerprint,
        "SHA256:0ilAVEYiaM4+PCd17vMiY2bh4bv814hVSbQKrN45DMU"
    );
    assert_eq!(key.comment, "alice@laptop");
    assert!(parse_public_key("ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQ").is_err());
    assert!(parse_public_key("ssh-ed25519 not-base64!").is_err());
    // A blob whose embedded type disagrees with the declared type.
    assert!(parse_public_key("sk-ssh-ed25519@openssh.com AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w").is_err());
    assert!(valid_user_name("ci-bot") && !valid_user_name("Alice") && !valid_user_name("1x"));
    Ok(())
}

#[test]
fn host_policy_requires_break_glass_and_known_environments() {
    let mut policy = host();
    assert!(policy.validate().is_ok());
    policy.break_glass.clear();
    assert!(policy.validate().is_err());
    let mut policy = host();
    if let Some(project) = policy.projects.get_mut(&project()) {
        project
            .ceiling
            .insert("qa".to_owned(), EnvironmentAccess::level(AccessLevel::View));
    }
    assert!(policy.validate().is_err());
    let mut policy = host();
    policy.schema_version = 2;
    assert!(policy.validate().is_err());
}

#[test]
fn ceilings_cap_levels_and_add_ons() {
    let ceiling = host().projects[&project()].ceiling_for("production");
    let admin = EnvironmentAccess::level(AccessLevel::Admin).within(&ceiling);
    assert_eq!(admin.level, AccessLevel::Deploy);
    assert!(admin.exec.is_none(), "exec is above the production ceiling");
    assert_eq!(admin.forward, ServiceSelection::All(true));
    assert!(!admin.secrets);

    let only = EnvironmentAccess {
        level: AccessLevel::View,
        exec: ServiceSelection::All(false),
        forward: ServiceSelection::Only(BTreeSet::from(["postgres".to_owned()])),
        secrets: true,
    };
    let limited = only.within(&ceiling);
    assert_eq!(
        limited.forward,
        ServiceSelection::Only(BTreeSet::from(["postgres".to_owned()]))
    );
    assert!(!limited.secrets);
}

#[test]
fn effective_policy_grants_per_environment_and_honors_blocks() -> Result<(), String> {
    let staging = record("staging", EnvironmentAccess::level(AccessLevel::Deploy))?;
    let production = record(
        "production",
        EnvironmentAccess {
            level: AccessLevel::View,
            exec: ServiceSelection::All(false),
            forward: ServiceSelection::Only(BTreeSet::from(["postgres".to_owned()])),
            secrets: false,
        },
    )?;
    let policy = effective_policy(&host(), &[staging.clone(), production], &[], 9);
    policy.validate()?;
    use AccessAction as A;
    use AccessResourceKind as R;
    assert!(allowed(
        &policy,
        ALICE,
        "staging",
        A::DeployApply,
        R::Project,
        None
    )?);
    assert!(!allowed(
        &policy,
        ALICE,
        "production",
        A::DeployApply,
        R::Project,
        None
    )?);
    assert!(allowed(
        &policy,
        ALICE,
        "production",
        A::Status,
        R::Project,
        None
    )?);
    assert!(allowed(
        &policy,
        ALICE,
        "production",
        A::Forward,
        R::Service,
        Some("postgres")
    )?);
    assert!(!allowed(
        &policy,
        ALICE,
        "production",
        A::Forward,
        R::Service,
        Some("redis")
    )?);
    assert!(!allowed(
        &policy,
        ALICE,
        "staging",
        A::Exec,
        R::Service,
        Some("api")
    )?);
    // Break-glass is admin everywhere, including exec.
    assert!(allowed(
        &policy,
        BOB,
        "production",
        A::Exec,
        R::Service,
        Some("api")
    )?);
    assert!(allowed(
        &policy,
        BOB,
        "staging",
        A::Block,
        R::AccessPolicy,
        None
    )?);
    assert_eq!(policy.authorized_keys(10).len(), 2);

    // Only staging lists alice now: production no longer authenticates her.
    let policy = effective_policy(&host(), std::slice::from_ref(&staging), &[], 10);
    assert!(!allowed(
        &policy,
        ALICE,
        "production",
        A::Status,
        R::Project,
        None
    )?);

    let block = EmergencyBlock {
        project_id: project(),
        environment: "staging".to_owned(),
        fingerprint: parse_public_key(ALICE)?.fingerprint,
        blocked_by: "ops".to_owned(),
        blocked_at_unix_ms: 1,
    };
    let policy = effective_policy(&host(), &[staging], &[block], 11);
    assert!(!allowed(
        &policy,
        ALICE,
        "staging",
        A::Status,
        R::Project,
        None
    )?);
    assert_eq!(
        policy.authorized_keys(10).len(),
        1,
        "blocked keys leave authorized_keys"
    );
    Ok(())
}

#[test]
fn reading_a_target_needs_view_and_changing_it_needs_admin() -> Result<(), String> {
    use AccessAction as A;
    use AccessResourceKind as R;
    for (level, inspect, change) in [
        (AccessLevel::View, true, false),
        (AccessLevel::Operate, true, false),
        (AccessLevel::Deploy, true, false),
        (AccessLevel::Admin, true, true),
    ] {
        let policy = effective_policy(
            &host(),
            &[record("staging", EnvironmentAccess::level(level))?],
            &[],
            12,
        );
        assert_eq!(
            allowed(&policy, ALICE, "staging", A::Inspect, R::Target, None)?,
            inspect,
            "{level:?} reading the target"
        );
        assert_eq!(
            allowed(&policy, ALICE, "staging", A::TargetAdmin, R::Target, None)?,
            change,
            "{level:?} changing the target"
        );
    }
    // The host ceiling caps an admin grant, and break-glass is admin.
    let policy = effective_policy(
        &host(),
        &[record(
            "production",
            EnvironmentAccess::level(AccessLevel::Admin),
        )?],
        &[],
        13,
    );
    assert!(!allowed(
        &policy,
        ALICE,
        "production",
        A::TargetAdmin,
        R::Target,
        None
    )?);
    assert!(allowed(
        &policy,
        BOB,
        "production",
        A::TargetAdmin,
        R::Target,
        None
    )?);
    Ok(())
}

// The ed25519 host key github.com publishes.
const GITHUB_HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";

fn enrollment(project_id: ProjectId) -> ProjectEnrollment {
    ProjectEnrollment {
        project_id,
        name: "shop".to_owned(),
        origin: "git@github.com:acme/shop.git".to_owned(),
        origin_host_key: Some(GITHUB_HOST_KEY.to_owned()),
        delivery: Default::default(),
        environments: BTreeSet::from(["production".to_owned()]),
        break_glass: vec![BreakGlassKey {
            name: "ops".to_owned(),
            key: BOB.to_owned(),
        }],
    }
}

#[test]
fn enrolling_into_an_empty_policy_creates_a_valid_host_policy() -> Result<(), String> {
    let mut policy = HostPolicy::empty();
    assert!(policy.validate().is_err());
    let other = ProjectId(uuid::Uuid::from_u128(9));
    policy.enroll(&enrollment(other))?;
    assert_eq!(policy.break_glass.len(), 1);
    let enrolled = &policy.projects[&other];
    assert_eq!(enrolled.origin, "git@github.com:acme/shop.git");
    assert_eq!(enrolled.origin_host_key.as_deref(), Some(GITHUB_HOST_KEY));
    assert!(enrolled.ceiling.is_empty() && enrolled.allow.is_empty());
    Ok(())
}

#[test]
fn enrolling_again_keeps_limits_and_adds_environments_and_keys() -> Result<(), String> {
    let mut policy = host();
    let mut request = enrollment(project());
    request.environments = BTreeSet::from(["qa".to_owned()]);
    request.break_glass.push(BreakGlassKey {
        name: "spare".to_owned(),
        key: ALICE.to_owned(),
    });
    policy.enroll(&request)?;
    let enrolled = &policy.projects[&project()];
    assert_eq!(
        enrolled.environments,
        BTreeSet::from([
            "production".to_owned(),
            "qa".to_owned(),
            "staging".to_owned()
        ])
    );
    assert!(enrolled.ceiling.contains_key("production"));
    assert_eq!(enrolled.origin, "git@github.com:acme/shop.git");
    // `ops` already held BOB, so only `spare` is new.
    assert_eq!(
        policy
            .break_glass
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["ops", "spare"]
    );
    // The same key under another name is not added twice.
    let mut renamed = enrollment(project());
    renamed.break_glass[0].name = "recovery".to_owned();
    policy.enroll(&renamed)?;
    assert_eq!(policy.break_glass.len(), 2);
    Ok(())
}

#[test]
fn enrolling_never_replaces_a_break_glass_key() {
    let mut policy = host();
    let mut request = enrollment(project());
    request.break_glass[0].key = ALICE.to_owned();
    let error = policy.enroll(&request).err().unwrap_or_default();
    assert!(error.contains("already holds a different key"), "{error}");
    assert_eq!(policy, host());
}

#[test]
fn origins_need_a_host_key_exactly_when_they_use_ssh() {
    for origin in [
        "git@github.com:acme/shop.git",
        "ssh://git@github.com:22/acme/shop.git",
    ] {
        assert!(origin_uses_ssh(origin), "{origin}");
        assert!(validate_origin(origin, Some(GITHUB_HOST_KEY)).is_ok());
        assert!(validate_origin(origin, None).is_err());
    }
    for origin in [
        "https://github.com/acme/shop.git",
        "/srv/git/shop.git",
        "file:///srv/git/shop.git",
    ] {
        assert!(!origin_uses_ssh(origin), "{origin}");
        assert!(validate_origin(origin, None).is_ok());
        assert!(validate_origin(origin, Some(GITHUB_HOST_KEY)).is_err());
    }
    for origin in [
        "",
        "http://example.com/shop.git",
        "git://example.com/shop.git",
        "-uhack:x",
        "https://example.com/{{ x }}.git",
        "https://example.com/a b.git",
        "relative/path",
    ] {
        assert!(validate_origin(origin, None).is_err(), "{origin}");
    }
    let origin = "git@github.com:acme/shop.git";
    assert!(validate_origin(origin, Some(&format!("{GITHUB_HOST_KEY} comment"))).is_err());
    assert!(validate_origin(origin, Some("ssh-ed25519 !!!")).is_err());
}

#[test]
fn enrollment_rejects_bad_names_and_environments() {
    let mut policy = HostPolicy::empty();
    let mut request = enrollment(project());
    request.environments.clear();
    assert!(policy.enroll(&request).is_err());
    let mut request = enrollment(project());
    request.environments = BTreeSet::from(["prod uction".to_owned()]);
    assert!(policy.enroll(&request).is_err());
    let mut request = enrollment(project());
    request.name.clear();
    assert!(policy.enroll(&request).is_err());
    let mut request = enrollment(project());
    request.break_glass[0].key = "ssh-rsa AAAAB3NzaC1yc2E".to_owned();
    assert!(policy.enroll(&request).is_err());
}

const AGENT: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBPnHBZZYETwo334Dl5Vu8LQSzJiO4gxHeq5zQGE+eAL mm-agent:alice@laptop";

fn with_agent(
    environment: &str,
    user: EnvironmentAccess,
    agent: EnvironmentAccess,
) -> Result<EnvironmentAccessRecord, String> {
    let mut record = record(environment, user)?;
    let key = parse_public_key(AGENT)?;
    if let Some(alice) = record.users.get_mut("alice") {
        alice.agent = Some(ReconciledAgent {
            keys: vec![ReconciledKey {
                public_key: key.openssh(),
                fingerprint: key.fingerprint,
                expires_at_unix_ms: None,
            }],
            access: agent,
        });
    }
    Ok(record)
}

#[test]
fn an_agent_key_is_its_own_principal_with_only_what_agents_may_do() -> Result<(), String> {
    use AccessAction as A;
    use AccessResourceKind as R;
    let staging = with_agent(
        "staging",
        EnvironmentAccess::level(AccessLevel::Admin),
        EnvironmentAccess::level(AccessLevel::View),
    )?;
    let policy = effective_policy(&host(), std::slice::from_ref(&staging), &[], 3);
    policy.validate()?;

    let fingerprint = parse_public_key(AGENT)?.fingerprint;
    let principal = policy
        .authenticate(&fingerprint, project(), "staging", 10)
        .map_err(|_| "agent key does not authenticate".to_owned())?;
    assert_eq!(principal.display_name, "alice (agent)");
    // The agent looks; its user, an admin, does the rest.
    for (action, resource, service) in [
        (A::Status, R::Project, None),
        (A::Plan, R::Project, None),
        (A::Logs, R::Service, Some("api")),
    ] {
        assert!(allowed(
            &policy, AGENT, "staging", action, resource, service
        )?);
    }
    for (action, resource, service) in [
        (A::DeployApply, R::Project, None),
        (A::Restart, R::Service, Some("api")),
        (A::Exec, R::Service, Some("api")),
        (A::Forward, R::Service, Some("postgres")),
        (A::SecretSet, R::Secret, None),
        (A::TeamSync, R::Project, None),
        (A::Block, R::AccessPolicy, None),
    ] {
        assert!(
            !allowed(&policy, AGENT, "staging", action, resource, service)?,
            "agent {action:?}"
        );
        assert!(
            allowed(&policy, ALICE, "staging", action, resource, service)?,
            "user {action:?}"
        );
    }
    // `whoami` reports each key's own access.
    let ceiling = EnvironmentAccess::level(AccessLevel::Admin);
    let own = parse_public_key(ALICE)?.fingerprint;
    assert_eq!(
        staging
            .access_of_key(&own, &ceiling)
            .map(|access| access.level),
        Some(AccessLevel::Admin)
    );
    assert_eq!(
        staging
            .access_of_key(&fingerprint, &ceiling)
            .map(|access| access.summary()),
        Some("view".to_owned())
    );
    assert_eq!(staging.access_of_key("SHA256:unknown", &ceiling), None);

    // The agent has no access where its user's record does not apply.
    assert!(!allowed(
        &policy,
        AGENT,
        "production",
        A::Status,
        R::Project,
        None
    )?);

    // Blocking the agent key leaves its user working.
    let block = EmergencyBlock {
        project_id: project(),
        environment: "staging".to_owned(),
        fingerprint,
        blocked_by: "ops".to_owned(),
        blocked_at_unix_ms: 1,
    };
    let blocked = effective_policy(&host(), &[staging], &[block], 4);
    assert!(!allowed(
        &blocked,
        AGENT,
        "staging",
        A::Status,
        R::Project,
        None
    )?);
    assert!(allowed(
        &blocked,
        ALICE,
        "staging",
        A::Status,
        R::Project,
        None
    )?);
    Ok(())
}

#[test]
fn a_record_cannot_give_an_agent_more_than_its_user_or_admin() -> Result<(), String> {
    use AccessAction as A;
    use AccessResourceKind as R;
    // A record that claims the agent is an admin, for a user who is not.
    let user = EnvironmentAccess {
        level: AccessLevel::Deploy,
        exec: ServiceSelection::Only(BTreeSet::from(["api".to_owned()])),
        forward: ServiceSelection::All(true),
        secrets: true,
    };
    let staging = with_agent(
        "staging",
        user,
        EnvironmentAccess::level(AccessLevel::Admin),
    )?;
    let policy = effective_policy(&host(), &[staging], &[], 3);
    policy.validate()?;
    assert!(allowed(
        &policy,
        AGENT,
        "staging",
        A::DeployApply,
        R::Project,
        None
    )?);
    assert!(allowed(
        &policy,
        AGENT,
        "staging",
        A::Exec,
        R::Service,
        Some("api")
    )?);
    for (action, resource, service) in [
        (A::Exec, R::Service, Some("postgres")),
        (A::Forward, R::Service, Some("postgres")),
        (A::SecretSet, R::Secret, None),
        (A::Block, R::AccessPolicy, None),
        (A::TargetAdmin, R::Target, None),
    ] {
        assert!(
            !allowed(&policy, AGENT, "staging", action, resource, service)?,
            "{action:?}"
        );
    }

    // The host ceiling applies to agents too: production caps exec to none.
    let production = with_agent(
        "production",
        EnvironmentAccess::level(AccessLevel::Admin),
        EnvironmentAccess {
            level: AccessLevel::Deploy,
            exec: ServiceSelection::All(true),
            forward: ServiceSelection::All(false),
            secrets: false,
        },
    )?;
    let policy = effective_policy(&host(), &[production], &[], 3);
    assert!(allowed(
        &policy,
        AGENT,
        "production",
        A::DeployApply,
        R::Project,
        None
    )?);
    assert!(!allowed(
        &policy,
        AGENT,
        "production",
        A::Exec,
        R::Service,
        Some("api")
    )?);
    Ok(())
}
