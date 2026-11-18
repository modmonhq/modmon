use super::*;

const ALICE: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w alice@laptop";
const BOB: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJZYcbK/sLQnEppB74bVUmUQnMUahuREnHUmhtFYGcf7 bob@desk";

fn users(yaml: &str) -> Result<BTreeMap<String, UserConfig>, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(yaml)
}

fn environments() -> BTreeSet<String> {
    BTreeSet::from(["staging".to_owned(), "production".to_owned()])
}

#[test]
fn users_file_accepts_levels_add_ons_and_expiring_keys() -> Result<(), Box<dyn std::error::Error>> {
    let users = users(&format!(
        "alice:\n  keys: [\"{ALICE}\"]\n  access: {{staging: deploy, production: view}}\nbob:\n  keys:\n    - key: \"{BOB}\"\n      expires: 2099-01-31\n  access:\n    \"*\": operate\n    production: {{level: deploy, exec: true, forward: [postgres]}}\n"
    ))?;
    assert!(validate_users(&users, &environments()).is_empty());
    let bob = &users["bob"];
    assert_eq!(
        bob.access_in("staging").map(|access| access.level),
        Some(AccessLevel::Operate)
    );
    let production = bob.access_in("production").ok_or("bob production")?;
    assert_eq!(production.level, AccessLevel::Deploy);
    assert_eq!(
        production.forward.services(),
        Some(BTreeSet::from(["postgres".to_owned()]))
    );
    assert_eq!(users["alice"].access_in("qa"), None);
    Ok(())
}

#[test]
fn validation_names_each_problem() -> Result<(), Box<dyn std::error::Error>> {
    let users = users(&format!(
        "Alice:\n  keys: [\"ssh-rsa AAAA\"]\n  access: {{qa: view}}\nbob:\n  keys: []\ncarol:\n  keys: [\"{ALICE}\"]\ndave:\n  keys:\n    - key: \"{ALICE}\"\n      expires: tomorrow\n"
    ))?;
    let problems = validate_users(&users, &environments()).join("\n");
    for expected in [
        "user 'Alice' must match",
        "key type 'ssh-rsa' is not allowed",
        "unknown environment 'qa'",
        "user 'bob' needs at least one public key",
        "share key",
        "must be a YYYY-MM-DD date",
    ] {
        assert!(
            problems.contains(expected),
            "missing {expected:?} in:\n{problems}"
        );
    }
    Ok(())
}

#[test]
fn reconcile_keeps_this_environment_only_and_drops_expired_keys()
-> Result<(), Box<dyn std::error::Error>> {
    let users = users(&format!(
        "alice:\n  keys: [\"{ALICE}\"]\n  access: {{staging: deploy}}\nbob:\n  keys:\n    - key: \"{BOB}\"\n      expires: 2000-01-01\n  access: {{\"*\": view}}\n"
    ))?;
    let project = ProjectId(uuid::Uuid::from_u128(3));
    let commit = "a".repeat(40);
    let staging = reconcile_users(
        (&users, &AgentsConfig::new()),
        Some(vec!["main".to_owned()]),
        project,
        "staging",
        &commit,
        1_000_000_000_000,
        "ci",
    )?;
    assert_eq!(
        staging.users.keys().collect::<Vec<_>>(),
        ["alice"],
        "bob's only key expired"
    );
    assert_eq!(staging.deploy_from, Some(vec!["main".to_owned()]));
    let production = reconcile_users(
        (&users, &AgentsConfig::new()),
        None,
        project,
        "production",
        &commit,
        1_000_000_000_000,
        "ci",
    )?;
    assert!(production.users.is_empty());

    let change = AccessChange::between(None, staging.clone());
    assert_eq!(change.entries.len(), 1);
    assert_eq!(change.entries[0].user, "alice");
    assert!(change.entries[0].before.is_none());
    assert_eq!(change.entries[0].keys_added.len(), 1);
    let again = AccessChange::between(Some(&staging), staging.clone());
    assert!(again.entries.is_empty());
    let mut later = staging.clone();
    later.reconciled_at_unix_ms += 5;
    assert_eq!(
        AccessChange::between(None, later).canonical_hash()?,
        change.canonical_hash()?,
        "timestamps do not change the reviewed hash"
    );
    Ok(())
}

#[test]
fn expiry_dates_convert_to_utc_midnight() {
    assert_eq!(expiry_unix_ms("1970-01-01"), Some(0));
    assert_eq!(expiry_unix_ms("2000-03-01"), Some(951_868_800_000));
    assert_eq!(expiry_unix_ms("2023-02-29"), None);
    assert_eq!(expiry_unix_ms("2024-02-29"), Some(1_709_164_800_000));
    assert_eq!(expiry_unix_ms("2024-13-01"), None);
    assert_eq!(expiry_unix_ms("soon"), None);
}

const AGENT: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBPnHBZZYETwo334Dl5Vu8LQSzJiO4gxHeq5zQGE+eAL mm-agent:alice@laptop";

#[test]
fn an_agent_key_gets_its_user_s_access_limited_by_the_agents_block()
-> Result<(), Box<dyn std::error::Error>> {
    let users = users(&format!(
        "alice:\n  keys:\n    - \"{ALICE}\"\n    - key: \"{AGENT}\"\n      agent: true\n  access:\n    staging: admin\n    production: {{level: deploy, exec: [api, worker], forward: true, secrets: true}}\n"
    ))?;
    assert!(validate_users(&users, &environments()).is_empty());
    let agents: AgentsConfig = serde_yaml_ng::from_str(
        "\"*\": view\nstaging: {level: operate, exec: [api]}\nproduction: {level: deploy, exec: [api, postgres]}\n",
    )?;
    let project = ProjectId(uuid::Uuid::from_u128(3));
    let record = |environment: &str, agents: &AgentsConfig| {
        reconcile_users(
            (&users, agents),
            None,
            project,
            environment,
            &"a".repeat(40),
            1,
            "ci",
        )
    };

    let staging = record("staging", &agents)?;
    let alice = &staging.users["alice"];
    // The agent key is not one of the user's own keys.
    assert_eq!(alice.keys.len(), 1);
    assert_eq!(alice.access.level, AccessLevel::Admin);
    let agent = alice.agent.as_ref().ok_or("staging agent")?;
    assert_eq!(agent.keys.len(), 1);
    assert_eq!(agent.access.summary(), "operate +exec[api]");

    // Exec is the overlap of what the user has and what agents may have;
    // forward and secrets never pass to an agent.
    let production = record("production", &agents)?;
    let agent = production.users["alice"]
        .agent
        .as_ref()
        .ok_or("production agent")?;
    assert_eq!(agent.access.summary(), "deploy +exec[api]");

    // Without an agents: block, an agent key can only view.
    let unlisted = record("staging", &AgentsConfig::new())?;
    let agent = unlisted.users["alice"]
        .agent
        .as_ref()
        .ok_or("default agent")?;
    assert_eq!(agent.access.summary(), "view");

    // The plan shows the agent as its own line.
    let change = AccessChange::between(None, staging.clone());
    let entries = change
        .entries
        .iter()
        .map(|entry| (entry.user.as_str(), entry.keys_added.len()))
        .collect::<Vec<_>>();
    assert_eq!(entries, [("alice", 1), ("alice (agent)", 1)]);
    // Narrowing agents: changes the plan hash and only the agent's line.
    let narrowed = AccessChange::between(Some(&staging), unlisted);
    assert_eq!(narrowed.entries.len(), 1);
    assert_eq!(narrowed.entries[0].user, "alice (agent)");
    assert_eq!(
        narrowed.entries[0]
            .after
            .as_ref()
            .map(EnvironmentAccess::summary),
        Some("view".to_owned())
    );
    assert_ne!(change.canonical_hash()?, narrowed.canonical_hash()?);
    Ok(())
}

#[test]
fn agent_keys_need_a_person_with_a_key_of_their_own() -> Result<(), Box<dyn std::error::Error>> {
    let users = users(&format!(
        "alice:\n  keys:\n    - key: \"{AGENT}\"\n      agent: true\n  access: {{staging: deploy}}\nci:\n  machine: true\n  keys:\n    - \"{BOB}\"\n    - key: \"{ALICE}\"\n      agent: true\n  access: {{staging: deploy}}\n"
    ))?;
    let problems = validate_users(&users, &environments()).join("\n");
    assert!(
        problems.contains("user 'alice' has only agent keys"),
        "{problems}"
    );
    assert!(
        problems.contains("machine user 'ci' cannot have agent keys"),
        "{problems}"
    );

    // If the user's own key expires, the agent goes with it.
    let users = self::users(&format!(
        "alice:\n  keys:\n    - key: \"{ALICE}\"\n      expires: 2000-01-01\n    - key: \"{AGENT}\"\n      agent: true\n  access: {{staging: deploy}}\n"
    ))?;
    let record = reconcile_users(
        (&users, &AgentsConfig::new()),
        None,
        ProjectId(uuid::Uuid::from_u128(3)),
        "staging",
        &"a".repeat(40),
        1_000_000_000_000,
        "ci",
    )?;
    assert!(record.users.is_empty());
    Ok(())
}
