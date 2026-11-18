use super::*;

fn agents(yaml: &str) -> Result<AgentsConfig, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(yaml)
}

fn environments() -> BTreeSet<String> {
    BTreeSet::from(["local".to_owned(), "production".to_owned()])
}

#[test]
fn unlisted_environments_fall_back_to_the_wildcard_then_to_view()
-> Result<(), Box<dyn std::error::Error>> {
    let block = agents("\"*\": operate\nlocal: {level: deploy, exec: [api]}\n")?;
    assert!(validate_agents(&block, &environments()).is_empty());
    assert_eq!(
        agent_access_in(&block, "production").level,
        AccessLevel::Operate
    );
    let local = agent_access_in(&block, "local");
    assert_eq!(local.summary(), "deploy +exec[api]");
    assert_eq!(
        agent_access_in(&AgentsConfig::new(), "production"),
        AgentAccess::VIEW
    );
    Ok(())
}

#[test]
fn each_action_needs_its_level_and_exec_needs_the_add_on() -> Result<(), Box<dyn std::error::Error>>
{
    let view = AgentAccess::VIEW;
    for action in [AgentAction::Read, AgentAction::Logs, AgentAction::Plan] {
        assert!(view.permits(action, None));
    }
    for action in [
        AgentAction::JobRun,
        AgentAction::ScheduleManage,
        AgentAction::ConfigApply,
        AgentAction::BuildApply,
        AgentAction::DeployApply,
        AgentAction::Exec,
    ] {
        assert!(!view.permits(action, Some("api")), "{action:?}");
    }
    let operate = agent_access_in(&agents("local: operate\n")?, "local");
    assert!(operate.permits(AgentAction::JobRun, None));
    assert!(!operate.permits(AgentAction::DeployApply, None));
    // Even `deploy` does not include exec, and a list names every service.
    let deploy = agent_access_in(&agents("local: {level: deploy, exec: [api]}\n")?, "local");
    assert!(deploy.permits(AgentAction::DeployApply, None));
    assert!(deploy.permits(AgentAction::Exec, Some("api")));
    assert!(!deploy.permits(AgentAction::Exec, Some("postgres")));
    assert!(!deploy.permits(AgentAction::Exec, None));
    let all = agent_access_in(&agents("local: {level: view, exec: true}\n")?, "local");
    assert!(all.permits(AgentAction::Exec, Some("postgres")));
    Ok(())
}

#[test]
fn validation_rejects_admin_and_unknown_environments() -> Result<(), Box<dyn std::error::Error>> {
    let block = agents("qa: view\nproduction: admin\n")?;
    let problems = validate_agents(&block, &environments()).join("\n");
    assert!(problems.contains("unknown environment 'qa'"), "{problems}");
    assert!(problems.contains("cannot be admin"), "{problems}");
    assert!(agents("local: {level: view, forward: true}\n").is_err());
    Ok(())
}

#[test]
fn a_ceiling_narrows_level_and_exec() -> Result<(), Box<dyn std::error::Error>> {
    let granted = agent_access_in(&agents("local: {level: deploy, exec: true}\n")?, "local");
    let ceiling = agent_access_in(&agents("local: {level: operate, exec: [api]}\n")?, "local");
    assert_eq!(granted.within(&ceiling).summary(), "operate +exec[api]");
    assert_eq!(granted.within(&AgentAccess::VIEW), AgentAccess::VIEW);
    // The remote form never carries forward, secrets, or admin.
    let remote = granted.environment_access();
    assert_eq!(remote.level, AccessLevel::Deploy);
    assert!(remote.forward.is_none() && !remote.secrets);
    Ok(())
}

#[test]
fn the_policy_hash_follows_meaning_not_spelling() -> Result<(), Box<dyn std::error::Error>> {
    let short = agents("local: view\n")?;
    let long = agents("local: {level: view}\n")?;
    assert_eq!(agents_policy_hash(&short), agents_policy_hash(&long));
    let wider = agents("local: {level: view, exec: true}\n")?;
    assert_ne!(agents_policy_hash(&short), agents_policy_hash(&wider));
    assert_ne!(
        agents_policy_hash(&short),
        agents_policy_hash(&AgentsConfig::new())
    );
    Ok(())
}

#[test]
fn the_init_block_opens_the_development_environment_only() {
    let block = initial_agents("local");
    assert_eq!(agent_access_in(&block, "local").summary(), "deploy +exec");
    assert_eq!(agent_access_in(&block, "production"), AgentAccess::VIEW);
}

#[test]
fn agent_markers_need_a_value() {
    assert_eq!(detect_agent([("CLAUDECODE", "1")]), Some("CLAUDECODE"));
    assert_eq!(
        detect_agent([("AI_AGENT", "claude-code")]),
        Some("AI_AGENT")
    );
    assert_eq!(detect_agent([("CLAUDECODE", ""), ("PATH", "/bin")]), None);
    assert_eq!(detect_agent([("MM_AGENT", "0")]), None);
}
