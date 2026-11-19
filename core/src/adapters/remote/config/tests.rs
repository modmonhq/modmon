use std::fs;
use std::os::unix::fs::{PermissionsExt as _, symlink};

use super::*;

fn config(root: &Path) -> DaemonConfig {
    DaemonConfig {
        schema_version: 2,
        socket_path: root.join("daemon.sock"),
        state_root: root.join("state"),
        mm_home: root.join("home"),
        socket_uid: 0,
        socket_gid: 0,
        idle_timeout_ms: 1_000,
        host_policy_path: root.join("host.yaml"),
        relay_uid: 991,
        authorized_keys_path: root.join("ssh/authorized_keys"),
        relay_command: "/usr/bin/mm relay serve --stdio".to_owned(),
        projects_root: root.join("projects"),
        node_env_path: root.join("node.env"),
    }
}

fn write_config(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    fs::write(path, bytes)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[test]
fn loads_secure_configuration_and_applies_default_timeout() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = crate::test_support::tempdir()?;
    fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o700))?;
    let path = temporary.path().join("daemon.json");
    let value = serde_json::json!({
        "schema_version": 2,
        "socket_path": temporary.path().join("daemon.sock"),
        "state_root": temporary.path().join("state"),
        "mm_home": temporary.path().join("home"),
        "socket_uid": 0,
        "socket_gid": 0,
        "host_policy_path": temporary.path().join("host.yaml"),
        "relay_uid": 991
    });
    write_config(&path, &serde_json::to_vec(&value)?)?;
    // The daemon accepts only root-owned configuration.
    if rustix::process::geteuid().as_raw() != 0 {
        assert!(matches!(
            DaemonConfig::load(&path),
            Err(MmError::Security { .. })
        ));
        return Ok(());
    }
    let loaded = DaemonConfig::load(&path)?;
    assert_eq!(loaded.idle_timeout_ms, 30_000);
    assert_eq!(
        loaded.host_policy_path(),
        temporary.path().join("host.yaml")
    );
    assert_eq!(
        loaded.authorized_keys_path,
        Path::new("/etc/ssh/mm_authorized_keys")
    );
    assert_eq!(loaded.projects_root, Path::new("/srv/mm/projects"));
    Ok(())
}

#[test]
fn validates_schema_absolute_paths_and_timeout_bounds() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let valid = config(temporary.path());
    assert!(valid.validate().is_ok());

    let mut invalid = valid.clone();
    invalid.schema_version = 1;
    assert!(invalid.validate().is_err());
    for mutate in [
        |value: &mut DaemonConfig| value.socket_path = "relative.sock".into(),
        |value: &mut DaemonConfig| value.state_root = "relative".into(),
        |value: &mut DaemonConfig| value.mm_home = "relative".into(),
        |value: &mut DaemonConfig| value.host_policy_path = "relative".into(),
        |value: &mut DaemonConfig| value.projects_root = "relative".into(),
        |value: &mut DaemonConfig| value.relay_command = "mm \"quoted\"".to_owned(),
        |value: &mut DaemonConfig| value.idle_timeout_ms = 0,
        |value: &mut DaemonConfig| value.idle_timeout_ms = 86_400_001,
    ] {
        let mut invalid = valid.clone();
        mutate(&mut invalid);
        assert!(invalid.validate().is_err());
    }
    Ok(())
}

#[test]
fn rejects_missing_non_file_symlink_and_insecure_configuration_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o700))?;
    let missing = temporary.path().join("missing.json");
    assert!(DaemonConfig::load(&missing).is_err());
    assert!(DaemonConfig::load(Path::new("/")).is_err());

    let path = temporary.path().join("daemon.json");
    write_config(&path, &serde_json::to_vec(&config(temporary.path()))?)?;
    let link = temporary.path().join("link.json");
    symlink(&path, &link)?;
    assert!(DaemonConfig::load(&link).is_err());

    fs::set_permissions(&path, fs::Permissions::from_mode(0o622))?;
    assert!(DaemonConfig::load(&path).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o722))?;
    assert!(DaemonConfig::load(&path).is_err());

    let outer = crate::test_support::tempdir()?;
    let linked_parent = outer.path().join("linked");
    symlink(temporary.path(), &linked_parent)?;
    assert!(DaemonConfig::load(&linked_parent.join("daemon.json")).is_err());
    Ok(())
}

#[test]
fn rejects_oversized_malformed_newer_and_semantically_invalid_content()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o700))?;
    let path = temporary.path().join("daemon.json");
    write_config(&path, &vec![b'x'; 1024 * 1024 + 1])?;
    assert!(DaemonConfig::load(&path).is_err());
    write_config(&path, b"not-json")?;
    assert!(DaemonConfig::load(&path).is_err());

    let mut unknown = serde_json::to_value(config(temporary.path()))?;
    unknown["added_later"] = serde_json::json!([]);
    assert!(DaemonConfig::parse(&path, &serde_json::to_vec(&unknown)?).is_ok());
    unknown["schema_version"] = serde_json::json!(3);
    let newer = DaemonConfig::parse(&path, &serde_json::to_vec(&unknown)?)
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        newer.contains("written by a newer mm (schema 3)"),
        "{newer}"
    );

    let mut invalid = config(temporary.path());
    invalid.schema_version = 1;
    write_config(&path, &serde_json::to_vec(&invalid)?)?;
    assert!(DaemonConfig::load(&path).is_err());
    Ok(())
}
