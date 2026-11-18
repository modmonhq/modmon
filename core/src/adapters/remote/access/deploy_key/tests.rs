use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn project() -> ProjectId {
    ProjectId(uuid::Uuid::from_u128(
        0x019c_527b_6242_7b43_bfd4_bd60_e27c_1de3,
    ))
}

fn store(root: &Path) -> Result<HostAccessStore, Box<dyn std::error::Error>> {
    let config: crate::adapters::remote::DaemonConfig =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "socket_path": root.join("run/mm.sock"),
            "state_root": root.join("state"),
            "mm_home": root.join("home"),
            "socket_uid": 0,
            "socket_gid": 0,
            "host_policy_path": root.join("etc/host.yaml"),
            "relay_uid": 991,
            "authorized_keys_path": root.join("ssh/authorized_keys"),
            "projects_root": root.join("projects"),
        }))?;
    fs::create_dir_all(root.join("state"))?;
    fs::set_permissions(root.join("state"), fs::Permissions::from_mode(0o700))?;
    Ok(HostAccessStore::new(&config))
}

/// Generates an operator-side key pair; returns the private bytes and the
/// public line.
fn key_pair(root: &Path, name: &str) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>> {
    let path: PathBuf = root.join(name);
    run_ssh_keygen(&[
        "-q",
        "-t",
        "ed25519",
        "-N",
        "",
        "-C",
        "mm-deploy-integration",
        "-f",
        &path.display().to_string(),
    ])?;
    Ok((
        fs::read(&path)?,
        fs::read_to_string(path.with_extension("pub"))?
            .trim()
            .to_owned(),
    ))
}

#[test]
fn installed_keys_are_kept_by_enroll_and_reinstall_is_a_no_op() -> TestResult {
    let temp = crate::test_support::tempdir()?;
    let store = store(temp.path())?;
    let generated = store.ensure_deploy_key(project())?;
    let (private, public) = key_pair(temp.path(), "operator")?;
    assert_ne!(generated, public);

    assert!(store.install_deploy_key(project(), &private, &format!("{public}\n"))?);
    let directory = store.project_directory(project());
    assert_eq!(fs::read(directory.join("deploy-key"))?, private);
    let mode = |name: &str| -> Result<u32, std::io::Error> {
        Ok(fs::metadata(directory.join(name))?.permissions().mode() & 0o777)
    };
    assert_eq!(mode("deploy-key")?, 0o600);
    assert_eq!(mode("deploy-key.pub")?, 0o644);

    // `mmd access enroll` runs ensure_deploy_key again; it keeps the key.
    assert_eq!(store.ensure_deploy_key(project())?, public);
    assert_eq!(fs::read(directory.join("deploy-key"))?, private);
    assert!(!store.install_deploy_key(project(), &private, &public)?);

    let leftovers = fs::read_dir(&directory)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
        .count();
    assert_eq!(leftovers, 0);
    Ok(())
}

#[test]
fn rejects_a_private_key_that_does_not_match_its_public_line() -> TestResult {
    let temp = crate::test_support::tempdir()?;
    let store = store(temp.path())?;
    let generated = store.ensure_deploy_key(project())?;
    let directory = store.project_directory(project());
    let before = fs::read(directory.join("deploy-key"))?;
    let (private, _) = key_pair(temp.path(), "first")?;
    let (_, other_public) = key_pair(temp.path(), "second")?;

    let error = store
        .install_deploy_key(project(), &private, &other_public)
        .err()
        .ok_or("a mismatched key was installed")?;
    assert!(error.to_string().contains("does not match"), "{error}");
    assert!(
        store
            .install_deploy_key(project(), b"not a key", &other_public)
            .is_err()
    );
    assert!(
        store
            .install_deploy_key(project(), &private, "ssh-rsa AAAAB3NzaC1yc2E")
            .is_err()
    );
    assert_eq!(fs::read(directory.join("deploy-key"))?, before);
    assert_eq!(store.ensure_deploy_key(project())?, generated);
    let leftovers = fs::read_dir(&directory)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
        .count();
    assert_eq!(leftovers, 0);
    Ok(())
}

/// What `mm target setup deploy-key` generates since it stopped writing
/// the key to a file: mm's own rendering of an ed25519 key.
#[test]
fn a_key_generated_in_memory_by_the_client_installs_like_one_from_ssh_keygen() -> TestResult {
    use ring::signature::KeyPair as _;

    let temp = crate::test_support::tempdir()?;
    let store = store(temp.path())?;
    let seed = [0x5A_u8; 32];
    let pair = ring::signature::Ed25519KeyPair::from_seed_unchecked(&seed)
        .map_err(|error| error.to_string())?;
    let public: [u8; 32] = pair.public_key().as_ref().try_into()?;
    let (private, line) =
        crate::domain::key::ed25519_private_key(&seed, &public, "mm-deploy-shop-production", 11);

    assert!(store.install_deploy_key(project(), private.as_bytes(), &line)?);
    assert_eq!(store.ensure_deploy_key(project())?, line);
    assert!(!store.install_deploy_key(project(), private.as_bytes(), &line)?);
    Ok(())
}
