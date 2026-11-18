use std::fs;

#[cfg(unix)]
use std::os::unix::fs::{PermissionsExt as _, symlink};

use super::*;
use crate::domain::agents::{AgentAccessConfig, initial_agents};
use crate::domain::host_policy::AccessLevel;
use crate::domain::mcp::MCP_CONFIG_SCHEMA_VERSION;

fn config() -> McpServerConfig {
    McpServerConfig {
        principal: "automation".to_owned(),
        maximum_response_bytes: 65_536,
        request_timeout_ms: 5_000,
        ..McpServerConfig::default()
    }
}

fn write_policy(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    fs::write(path, bytes)?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[test]
fn private_limits_are_optional_unless_named() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let missing = McpPolicyStore::default_path(temp.path());
    assert_eq!(missing, temp.path().join("mcp.json"));
    assert_eq!(
        McpPolicyStore::load(&missing, false)?,
        McpServerConfig::default()
    );
    assert!(McpPolicyStore::load(&missing, true).is_err());

    let path = temp.path().join("policy.json");
    write_policy(&path, &serde_json::to_vec(&config())?)?;
    assert_eq!(McpPolicyStore::load(&path, true)?.principal, "automation");
    Ok(())
}

#[test]
fn rejects_directory_symlink_and_insecure_policy_files() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    assert!(McpPolicyStore::load(temp.path(), false).is_err());

    let policy = temp.path().join("policy.json");
    write_policy(&policy, &serde_json::to_vec(&config())?)?;
    #[cfg(unix)]
    {
        fs::set_permissions(&policy, fs::Permissions::from_mode(0o644))?;
        assert!(McpPolicyStore::load(&policy, false).is_err());
        fs::set_permissions(&policy, fs::Permissions::from_mode(0o600))?;
        let link = temp.path().join("link.json");
        symlink(&policy, &link)?;
        assert!(McpPolicyStore::load(&link, false).is_err());
    }
    Ok(())
}

#[test]
fn rejects_oversized_malformed_invalid_and_version_one_content()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let policy = temp.path().join("policy.json");
    write_policy(&policy, &vec![b'x'; MAX_CONFIG_BYTES + 1])?;
    assert!(McpPolicyStore::load(&policy, false).is_err());

    write_policy(&policy, b"not-json")?;
    assert!(McpPolicyStore::load(&policy, false).is_err());

    let mut invalid = config();
    invalid.maximum_page_size = 0;
    write_policy(&policy, &serde_json::to_vec(&invalid)?)?;
    assert!(McpPolicyStore::load(&policy, false).is_err());

    // The old private policy is refused with the way forward, never ignored.
    write_policy(
        &policy,
        br#"{"schema_version":1,"principal":"local-agent","project_root":"/p","environments":["local"],"targets":["local"],"capabilities":["inspect","deploy_apply"]}"#,
    )?;
    let error = McpPolicyStore::load(&policy, false)
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        error.contains("schema 1") && error.contains("agents:"),
        "{error}"
    );

    write_policy(
        &policy,
        format!(r#"{{"schema_version":{MCP_CONFIG_SCHEMA_VERSION},"capabilities":["inspect"]}}"#)
            .as_bytes(),
    )?;
    assert!(McpPolicyStore::load(&policy, false).is_err());
    Ok(())
}

#[test]
fn trust_follows_the_exact_block_the_checkout_and_the_project()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let store = AgentTrustStore::new(temp.path());
    let root = temp.path().join("project");
    let project = ProjectId(uuid::Uuid::from_u128(7));
    let block = initial_agents("local");
    assert!(!store.is_trusted(&root, project, &block));

    store.trust(&root, project, &block, 1_000)?;
    assert!(store.is_trusted(&root, project, &block));
    #[cfg(unix)]
    assert_eq!(
        fs::metadata(temp.path().join("agents-trust.json"))?
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    // An edit that widens the block is not the block the user trusted.
    let mut widened = block.clone();
    widened.insert(
        "production".to_owned(),
        AgentAccessConfig::Level(AccessLevel::Deploy),
    );
    assert!(!store.is_trusted(&root, project, &widened));
    // Neither is the same block in another checkout or another project.
    assert!(!store.is_trusted(&temp.path().join("other"), project, &block));
    assert!(!store.is_trusted(&root, ProjectId(uuid::Uuid::from_u128(8)), &block));

    // Trusting the new block replaces the old trust.
    store.trust(&root, project, &widened, 2_000)?;
    assert!(store.is_trusted(&root, project, &widened));
    assert!(!store.is_trusted(&root, project, &block));

    // A damaged trust file trusts nothing.
    fs::write(temp.path().join("agents-trust.json"), "{")?;
    assert!(!store.is_trusted(&root, project, &widened));
    Ok(())
}
