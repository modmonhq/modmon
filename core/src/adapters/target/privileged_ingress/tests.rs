use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tempfile::TempDir;
use zeroize::Zeroizing;

use super::*;

use crate::adapters::target::{FilesystemTargetIngressArtifactStore, FilesystemTargetStore};
use crate::app::TargetIngressManager;
use crate::app::ports::{
    CommandOutput, CommandSpec, PrivilegedIngressInstaller, ProcessRunner, TargetStore as _,
};
use crate::domain::target::{TargetDefinition, TargetId};

#[test]
fn selinux_port_labels_only_reuse_mm_owned_tcp_mappings() {
    let mappings = "SELinux Port Type Proto Port Number\nhttp_port_t tcp 80, 443, 24773\nmm_ingress_port_t udp 24773\nmm_ingress_port_t tcp 24774, 25000-25002\n";
    assert!(!has_mm_port_label(mappings, 24773));
    assert!(has_mm_port_label(mappings, 24774));
    assert!(has_mm_port_label(mappings, 25001));
    assert!(!has_mm_port_label(mappings, 25003));
}

struct RecordingProcess {
    success: bool,
    commands: Mutex<Vec<CommandSpec>>,
}

#[async_trait]
impl ProcessRunner for RecordingProcess {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        self.commands
            .lock()
            .map_err(|_| MmError::State {
                message: "privileged ingress command record lock was poisoned".to_owned(),
            })?
            .push(specification);
        Ok(CommandOutput {
            code: Some(if self.success { 0 } else { 1 }),
            stdout: Zeroizing::new(String::new()),
            stderr: Zeroizing::new("injected sudo failure".to_owned()),
            truncated: false,
        })
    }
}

fn generated_plan() -> Result<(TempDir, TargetIngressPlan), MmError> {
    let temp = crate::test_support::tempdir().map_err(|source| MmError::Io {
        context: "creating privileged ingress test root".to_owned(),
        source,
    })?;
    let targets = Arc::new(FilesystemTargetStore::new(temp.path().join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(uuid::Uuid::from_u128(31))),
    )?;
    let manager = TargetIngressManager::new(
        targets,
        Arc::new(FilesystemTargetIngressArtifactStore::new(
            temp.path().join("ingress"),
        )),
    );
    let plan = manager.plan("local")?;
    Ok((temp, plan))
}

#[test]
fn generated_artifact_passes_complete_privileged_verification_chain()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, plan) = generated_plan()?;
    let uid = fs::metadata(&plan.artifact.root)?.uid();
    let verified =
        verify_source_artifact(&plan.artifact.root, plan.target_id, &plan.plan_hash, uid)?;
    verify_fixed_templates(plan.target_id, &verified)?;
    assert_eq!(
        verified.haproxy,
        haproxy_config(
            plan.artifact.manifest.proxy_http_port,
            plan.artifact.manifest.proxy_https_port,
        )
        .as_bytes()
    );
    assert_eq!(verified.unit, haproxy_service_unit().as_bytes());

    let mut tampered = verified;
    tampered.haproxy.push(b'!');
    assert!(verify_fixed_templates(plan.target_id, &tampered).is_err());
    assert!(
        verify_source_artifact(&plan.artifact.root, plan.target_id, &"0".repeat(64), uid,).is_err()
    );
    Ok(())
}

#[test]
fn manifest_support_set_and_file_protection_tampering_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, plan) = generated_plan()?;
    let uid = fs::metadata(&plan.artifact.root)?.uid();
    let manifest_path = plan.artifact.root.join("manifest.json");
    let original_manifest = fs::read(&manifest_path)?;
    let mut manifest = plan.artifact.manifest.clone();

    manifest.target_id = TargetId(uuid::Uuid::from_u128(32));
    fs::write(&manifest_path, serde_json::to_vec(&manifest)?)?;
    assert!(verified_manifest(&manifest_path, plan.target_id, &plan.plan_hash).is_err());
    fs::write(&manifest_path, b"not-json")?;
    assert!(verified_manifest(&manifest_path, plan.target_id, &plan.plan_hash).is_err());
    fs::write(&manifest_path, &original_manifest)?;

    let mut missing = plan.artifact.manifest.clone();
    missing.support_files.pop();
    assert!(verify_support_files(&plan.artifact.root, uid, &missing).is_err());

    let mut duplicate = plan.artifact.manifest.clone();
    duplicate.support_files[0].name = duplicate.support_files[1].name.clone();
    assert!(verify_support_files(&plan.artifact.root, uid, &duplicate).is_err());

    let support = plan.artifact.root.join("dynamic.yaml");
    let original_support = fs::read(&support)?;
    fs::write(&support, b"tampered")?;
    assert!(verify_support_files(&plan.artifact.root, uid, &plan.artifact.manifest).is_err());
    fs::write(&support, original_support)?;

    let mut permissions = fs::metadata(&support)?.permissions();
    permissions.set_mode(0o666);
    fs::set_permissions(&support, permissions)?;
    assert!(verify_source_file(&support, uid).is_err());
    fs::set_permissions(&support, fs::Permissions::from_mode(0o600))?;

    assert!(verify_source_file(&plan.artifact.root, uid).is_err());
    assert!(verify_source_file(&support, uid.saturating_add(1)).is_err());
    Ok(())
}

#[test]
fn exact_file_set_and_fixed_artifact_metadata_reject_ambiguity()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, plan) = generated_plan()?;
    let uid = fs::metadata(&plan.artifact.root)?.uid();
    let expected = BTreeSet::from([
        "compose.yaml".to_owned(),
        "dynamic.yaml".to_owned(),
        "haproxy.cfg".to_owned(),
        "manifest.json".to_owned(),
        "mm-ingress-forwarder.service".to_owned(),
        "mm-ingress.service".to_owned(),
    ]);
    verify_exact_file_set(&plan.artifact.root, &expected)?;
    fs::write(plan.artifact.root.join("unexpected"), b"extra")?;
    assert!(verify_exact_file_set(&plan.artifact.root, &expected).is_err());
    fs::remove_file(plan.artifact.root.join("unexpected"))?;

    let mut wrong_haproxy = plan.artifact.manifest.clone();
    let entry = wrong_haproxy
        .support_files
        .iter_mut()
        .find(|entry| entry.name == "haproxy.cfg")
        .ok_or("HAProxy manifest entry missing")?;
    entry.mode = "0600".to_owned();
    assert!(read_fixed_artifacts(&plan.artifact.root, uid, &wrong_haproxy).is_err());

    let mut wrong_unit = plan.artifact.manifest.clone();
    let entry = wrong_unit
        .support_files
        .iter_mut()
        .find(|entry| entry.name == "mm-ingress-forwarder.service")
        .ok_or("systemd manifest entry missing")?;
    entry.destination = "/tmp/wrong".to_owned();
    assert!(read_fixed_artifacts(&plan.artifact.root, uid, &wrong_unit).is_err());

    let mut wrong_compose = plan.artifact.manifest.clone();
    wrong_compose.compose_digest = "0".repeat(64);
    assert!(read_fixed_artifacts(&plan.artifact.root, uid, &wrong_compose).is_err());
    Ok(())
}

#[test]
fn validation_file_cleanup_and_restore_are_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let path = temp.path().join("fixed");
    remove_validation_file(&path)?;
    fs::write(&path, b"current")?;
    remove_validation_file(&path)?;
    assert!(!path.exists());

    restore_fixed(&path, Some(b"previous"), 0o600)?;
    assert_eq!(fs::read(&path)?, b"previous");
    restore_fixed(&path, None, 0o600)?;
    assert!(!path.exists());
    restore_fixed(&path, None, 0o600)?;
    Ok(())
}

#[tokio::test]
async fn sudo_installer_validates_plans_and_maps_process_results()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, plan) = generated_plan()?;
    // uutils coreutils installs /usr/bin/true as a symlink, and privileged
    // executables must be real files.
    let executable = std::fs::canonicalize("/usr/bin/true")?;
    let success = Arc::new(RecordingProcess {
        success: true,
        commands: Mutex::new(Vec::new()),
    });
    let installer = SudoPrivilegedIngressInstaller {
        process: success.clone(),
        executable: executable.clone(),
    };
    installer.install(&plan).await?;
    installer.uninstall(&plan).await?;
    {
        let commands = success.commands.lock().map_err(|_| "command lock")?;
        assert_eq!(commands.len(), 2);
        assert!(format!("{:?}", commands[0].redacted_record()).contains("__install-ingress"));
        assert!(format!("{:?}", commands[1].redacted_record()).contains("__uninstall-ingress"));
    }

    let failure = Arc::new(RecordingProcess {
        success: false,
        commands: Mutex::new(Vec::new()),
    });
    let installer = SudoPrivilegedIngressInstaller {
        process: failure,
        executable,
    };
    assert!(installer.install(&plan).await.is_err());
    assert!(installer.uninstall(&plan).await.is_err());

    let mut changed = plan.clone();
    changed.plan_hash = "0".repeat(64);
    assert!(installer.install(&changed).await.is_err());
    assert!(installer.uninstall(&changed).await.is_err());
    assert!(format!("{installer:?}").contains("SudoPrivilegedIngressInstaller"));
    Ok(())
}

#[test]
fn privileged_platform_helpers_validate_paths_hashes_and_atomic_publication()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let destination = temp.path().join("published");
    atomic_replace(&destination, b"first", 0o600)?;
    assert_eq!(fs::read(&destination)?, b"first");
    assert_eq!(
        fs::metadata(&destination)?.permissions().mode() & 0o777,
        0o600
    );
    atomic_replace(&destination, b"second", 0o644)?;
    assert_eq!(fs::read(&destination)?, b"second");
    let directory_destination = temp.path().join("directory-destination");
    fs::create_dir(&directory_destination)?;
    assert!(atomic_replace(&directory_destination, b"blocked", 0o600).is_err());
    assert!(atomic_replace(Path::new("/"), b"blocked", 0o600).is_err());

    assert_eq!(
        path_text(&destination, "fixture")?,
        destination.to_string_lossy()
    );
    assert!(path_text(Path::new("\u{fffd}"), "fixture").is_ok());
    assert!(validate_hash(&"a".repeat(64)).is_ok());
    assert!(validate_hash("short").is_err());
    assert!(validate_hash(&"z".repeat(64)).is_err());
    for name in ["dynamic.yaml", "name_with-dash.crt"] {
        validate_support_name(name)?;
    }
    for name in ["", ".hidden", "../escape", "a/b", "bad name"] {
        assert!(validate_support_name(name).is_err());
    }

    run_checked("/bin/true", &[])?;
    assert!(run_checked("/bin/false", &[]).is_err());
    assert!(run_checked("/definitely/missing/mm-command", &[]).is_err());
    verify_privileged_executable(&std::fs::canonicalize("/usr/bin/true")?)?;
    assert!(verify_privileged_executable(Path::new("relative-mm")).is_err());
    assert!(verify_privileged_executable(Path::new("/definitely/missing/mm")).is_err());
    assert!(verify_privileged_executable(temp.path()).is_err());
    Ok(())
}

#[test]
fn public_forwarder_probe_accepts_only_a_complete_http_404_response()
-> Result<(), Box<dyn std::error::Error>> {
    use std::net::{Ipv4Addr, SocketAddr, TcpListener};
    use std::thread;

    fn serve_once(response: &'static [u8]) -> Result<thread::JoinHandle<()>, std::io::Error> {
        let listener = TcpListener::bind(SocketAddr::from((
            Ipv4Addr::LOCALHOST,
            INGRESS_PUBLIC_HTTP_PORT,
        )))?;
        Ok(thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0_u8; 1024];
            _ = stream.read(&mut request);
            _ = stream.write_all(response);
        }))
    }

    let server = match serve_once(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n") {
        Ok(server) => server,
        // Unprivileged users cannot listen on loopback port 80.
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    probe_public_forwarder("probe.invalid")?;
    server.join().map_err(|_| "404 probe server panicked")?;

    for response in [
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".as_slice(),
        b"not-http\r\n\r\n".as_slice(),
    ] {
        let owned = response.to_vec();
        let listener = TcpListener::bind(SocketAddr::from((
            Ipv4Addr::LOCALHOST,
            INGRESS_PUBLIC_HTTP_PORT,
        )))?;
        let server = thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0_u8; 1024];
            _ = stream.read(&mut request);
            _ = stream.write_all(&owned);
        });
        assert!(probe_public_forwarder("probe.invalid").is_err());
        server.join().map_err(|_| "invalid probe server panicked")?;
    }
    assert!(probe_public_forwarder("probe.invalid").is_err());
    Ok(())
}

#[test]
fn source_root_manifest_and_file_checks_reject_aliases_and_missing_data()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let (_temp, plan) = generated_plan()?;
    let uid = fs::metadata(&plan.artifact.root)?.uid();
    assert!(verify_source_root(&plan.artifact.root, plan.target_id, &plan.plan_hash, uid).is_ok());
    assert!(
        verify_source_root(
            &plan.artifact.root,
            TargetId(uuid::Uuid::from_u128(99)),
            &plan.plan_hash,
            uid
        )
        .is_err()
    );
    assert!(verify_source_root(&plan.artifact.root, plan.target_id, "wrong", uid).is_err());
    assert!(
        verify_source_root(
            &plan.artifact.root.join("missing"),
            plan.target_id,
            &plan.plan_hash,
            uid
        )
        .is_err()
    );

    let manifest_path = plan.artifact.root.join("manifest.json");
    assert!(verified_manifest(&manifest_path, plan.target_id, &plan.plan_hash).is_ok());
    assert!(
        verified_manifest(
            &plan.artifact.root.join("missing.json"),
            plan.target_id,
            &plan.plan_hash
        )
        .is_err()
    );

    let support = plan.artifact.root.join("dynamic.yaml");
    let hardlink = plan.artifact.root.join("linked.yaml");
    fs::hard_link(&support, &hardlink)?;
    assert!(verify_source_file(&support, uid).is_err());
    fs::remove_file(&hardlink)?;
    let symlink_path = plan.artifact.root.join("symlink.yaml");
    symlink(&support, &symlink_path)?;
    assert!(verify_source_file(&symlink_path, uid).is_err());
    assert!(verify_source_file(&plan.artifact.root.join("missing"), uid).is_err());
    Ok(())
}

#[test]
fn exact_sets_private_files_and_cleanup_error_collection_cover_failure_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let path = temp.path().join("validation");
    write_validation_file(&path, b"validated")?;
    assert_eq!(fs::read(&path)?, b"validated");
    assert!(write_validation_file(&path, b"duplicate").is_err());
    assert!(remove_validation_file(temp.path()).is_err());

    let fixed = temp.path().join("fixed");
    assert!(read_existing_fixed(&fixed)?.is_none());
    fs::write(&fixed, b"owned")?;
    if fs::metadata(&fixed)?.uid() == 0 {
        assert_eq!(
            read_existing_fixed(&fixed)?.as_deref(),
            Some(b"owned".as_slice())
        );
        fs::set_permissions(&fixed, fs::Permissions::from_mode(0o666))?;
        assert!(read_existing_fixed(&fixed).is_err());
    }

    let mut failures = Vec::new();
    retain_cleanup_error(&mut failures, Ok(()));
    retain_cleanup_error(
        &mut failures,
        Err(MmError::State {
            message: "cleanup failed".to_owned(),
        }),
    );
    assert_eq!(failures.len(), 1);

    let expected = BTreeSet::from(["only".to_owned()]);
    assert!(verify_exact_file_set(temp.path(), &expected).is_err());
    assert!(verify_exact_file_set(&fixed, &expected).is_err());
    let fixed_directory = temp.path().join("fixed-directory");
    fs::create_dir(&fixed_directory)?;
    assert!(read_existing_fixed(&fixed_directory).is_err());
    assert!(write_validation_file(Path::new("/proc/mm-ingress-validation"), b"value").is_err());
    Ok(())
}

#[test]
fn root_owned_validation_lock_and_owner_checks_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Ok(());
    }

    ensure_fixed_directories()?;
    for path in [HOST_LOCK, OWNER_FILE, HAPROXY_CONFIG, SYSTEM_UNIT] {
        if Path::new(path).exists() {
            fs::remove_file(path)?;
        }
    }

    {
        let validation = ValidationFiles::new(b"haproxy", b"unit")?;
        assert_eq!(fs::read(&validation.haproxy)?, b"haproxy");
        assert_eq!(fs::read(&validation.unit)?, b"unit");
        assert_eq!(
            fs::metadata(&validation.haproxy)?.permissions().mode() & 0o777,
            0o600
        );
    }

    let lock = host_lock()?;
    assert!(host_lock().is_err());
    drop(lock);
    let lock = host_lock()?;
    drop(lock);
    fs::remove_file(HOST_LOCK)?;

    let target_id = TargetId(uuid::Uuid::from_u128(77));
    validate_owner(target_id, 1000)?;
    atomic_replace(Path::new(HAPROXY_CONFIG), b"unmanaged", 0o644)?;
    assert!(validate_owner(target_id, 1000).is_err());
    fs::remove_file(HAPROXY_CONFIG)?;

    let owner = HostIngressOwner {
        target_id,
        source_uid: 1000,
        artifact_hash: "a".repeat(64),
    };
    atomic_replace(Path::new(OWNER_FILE), &serde_json::to_vec(&owner)?, 0o600)?;
    validate_owner(target_id, 1000)?;
    assert!(validate_owner(TargetId(uuid::Uuid::from_u128(78)), 1000).is_err());
    assert!(validate_owner(target_id, 1001).is_err());

    fs::write(OWNER_FILE, b"not-json")?;
    assert!(validate_owner(target_id, 1000).is_err());
    if Path::new(OWNER_FILE).is_file() {
        fs::remove_file(OWNER_FILE)?;
    }
    Ok(())
}

#[test]
fn root_entry_points_require_non_root_sudo_attribution() {
    if rustix::process::geteuid().as_raw() != 0 || std::env::var_os("SUDO_UID").is_some() {
        return;
    }
    let target_id = TargetId(uuid::Uuid::from_u128(79));
    assert!(
        install_privileged_ingress_artifact(Path::new("/tmp/missing"), target_id, &"a".repeat(64))
            .is_err()
    );
    assert!(uninstall_privileged_ingress(target_id, &"a".repeat(64)).is_err());
}

#[test]
fn source_validation_covers_owner_names_and_missing_fixed_files()
-> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    let (_temp, plan) = generated_plan()?;
    let uid = fs::metadata(&plan.artifact.root)?.uid();
    assert!(
        verify_source_root(
            &plan.artifact.root,
            plan.target_id,
            &plan.plan_hash,
            uid.saturating_add(1),
        )
        .is_err()
    );

    let extra_expected = BTreeSet::from([
        "compose.yaml".to_owned(),
        "dynamic.yaml".to_owned(),
        "haproxy.cfg".to_owned(),
        "manifest.json".to_owned(),
        "mm-ingress-forwarder.service".to_owned(),
        "mm-ingress.service".to_owned(),
        "expected-but-missing".to_owned(),
    ]);
    assert!(verify_exact_file_set(&plan.artifact.root, &extra_expected).is_err());
    let non_unicode = plan.artifact.root.join(OsString::from_vec(vec![0xff]));
    fs::write(&non_unicode, b"unexpected")?;
    assert!(verify_exact_file_set(&plan.artifact.root, &extra_expected).is_err());
    fs::remove_file(non_unicode)?;

    let haproxy = plan.artifact.root.join("haproxy.cfg");
    let haproxy_bytes = fs::read(&haproxy)?;
    fs::remove_file(&haproxy)?;
    assert!(read_fixed_artifacts(&plan.artifact.root, uid, &plan.artifact.manifest).is_err());
    fs::write(&haproxy, haproxy_bytes)?;

    let unit = plan.artifact.root.join("mm-ingress-forwarder.service");
    fs::remove_file(&unit)?;
    assert!(read_fixed_artifacts(&plan.artifact.root, uid, &plan.artifact.manifest).is_err());
    Ok(())
}

#[test]
fn fixed_file_restoration_reports_non_file_destinations() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = crate::test_support::tempdir()?;
    let directory = temp.path().join("directory");
    fs::create_dir(&directory)?;
    assert!(restore_fixed(&directory, None, 0o600).is_err());
    Ok(())
}

#[test]
fn root_helpers_validate_an_attributed_artifact_before_host_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Ok(());
    }
    if rerun_with_sudo_uid("root_helpers_validate_an_attributed_artifact_before_host_mutation")? {
        return Ok(());
    }
    let (_temp, plan) = generated_plan()?;
    for entry in fs::read_dir(&plan.artifact.root)? {
        let path = entry?.path();
        std::os::unix::fs::chown(&path, Some(1000), Some(1000))?;
    }
    std::os::unix::fs::chown(&plan.artifact.root, Some(1000), Some(1000))?;
    for path in [HOST_LOCK, OWNER_FILE, HAPROXY_CONFIG, SYSTEM_UNIT] {
        if Path::new(path).is_file() {
            fs::remove_file(path)?;
        }
    }
    let _haproxy = FixedExecutableGuard::create("/usr/sbin/haproxy")?;

    let result =
        install_privileged_ingress_artifact(&plan.artifact.root, plan.target_id, &plan.plan_hash);
    assert!(result.is_err());
    assert!(!Path::new(OWNER_FILE).exists());
    assert!(!Path::new(HAPROXY_CONFIG).exists());
    assert!(!Path::new(SYSTEM_UNIT).exists());
    if Path::new(HOST_LOCK).is_file() {
        fs::remove_file(HOST_LOCK)?;
    }
    Ok(())
}

#[test]
fn attributed_uninstall_handles_absent_invalid_and_foreign_owners()
-> Result<(), Box<dyn std::error::Error>> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Ok(());
    }
    if rerun_with_sudo_uid("attributed_uninstall_handles_absent_invalid_and_foreign_owners")? {
        return Ok(());
    }
    ensure_fixed_directories()?;
    for path in [HOST_LOCK, OWNER_FILE, HAPROXY_CONFIG, SYSTEM_UNIT] {
        if Path::new(path).is_file() {
            fs::remove_file(path)?;
        }
    }
    let target_id = TargetId(uuid::Uuid::from_u128(801));
    let artifact_hash = "a".repeat(64);
    uninstall_privileged_ingress(target_id, &artifact_hash)?;

    atomic_replace(Path::new(OWNER_FILE), b"not-json", 0o600)?;
    assert!(uninstall_privileged_ingress(target_id, &artifact_hash).is_err());
    let foreign = HostIngressOwner {
        target_id: TargetId(uuid::Uuid::from_u128(802)),
        source_uid: 1000,
        artifact_hash: artifact_hash.clone(),
    };
    atomic_replace(Path::new(OWNER_FILE), &serde_json::to_vec(&foreign)?, 0o600)?;
    assert!(uninstall_privileged_ingress(target_id, &artifact_hash).is_err());

    let owner = HostIngressOwner {
        target_id,
        source_uid: 1000,
        artifact_hash: artifact_hash.clone(),
    };
    atomic_replace(Path::new(OWNER_FILE), &serde_json::to_vec(&owner)?, 0o600)?;
    atomic_replace(Path::new(HAPROXY_CONFIG), b"owned config", 0o644)?;
    atomic_replace(Path::new(SYSTEM_UNIT), b"owned unit", 0o644)?;
    let removal = uninstall_privileged_ingress(target_id, &artifact_hash);
    if removal.is_err() {
        assert_eq!(fs::read(HAPROXY_CONFIG)?, b"owned config");
        assert_eq!(fs::read(SYSTEM_UNIT)?, b"owned unit");
    } else {
        assert!(!Path::new(HAPROXY_CONFIG).exists());
        assert!(!Path::new(SYSTEM_UNIT).exists());
    }
    for path in [HAPROXY_CONFIG, SYSTEM_UNIT] {
        if Path::new(path).is_file() {
            fs::remove_file(path)?;
        }
    }
    if Path::new(OWNER_FILE).is_file() {
        fs::remove_file(OWNER_FILE)?;
    }
    if Path::new(HOST_LOCK).is_file() {
        fs::remove_file(HOST_LOCK)?;
    }
    Ok(())
}

fn rerun_with_sudo_uid(test_name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    if std::env::var("SUDO_UID").as_deref() == Ok("1000") {
        return Ok(false);
    }

    let module = module_path!()
        .split_once("::")
        .map_or(module_path!(), |(_, module)| module);
    let output = std::process::Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            &format!("{module}::{test_name}"),
            "--test-threads=1",
        ])
        .env("SUDO_UID", "1000")
        .output()?;
    if !output.status.success() || !String::from_utf8_lossy(&output.stdout).contains("1 passed") {
        return Err(std::io::Error::other(format!(
            "attributed root test subprocess failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(true)
}

struct FixedExecutableGuard(Option<&'static str>);

impl FixedExecutableGuard {
    fn create(path: &'static str) -> Result<Self, std::io::Error> {
        if Path::new(path).exists() {
            return Ok(Self(None));
        }
        fs::write(path, b"#!/bin/sh\nexit 0\n")?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        Ok(Self(Some(path)))
    }
}

impl Drop for FixedExecutableGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0 {
            _ = fs::remove_file(path);
        }
    }
}

/// The pre-HAProxy-3.3 template reused frontend names for backends.
fn legacy_haproxy_config(proxy_http_port: u16, proxy_https_port: u16) -> String {
    haproxy_config(proxy_http_port, proxy_https_port)
        .replace("mm-http-traefik", "mm-http")
        .replace("mm-https-traefik", "mm-https")
}

#[test]
fn haproxy_proxy_names_are_unique_across_sections() {
    let config = haproxy_config(24_000, 24_001);
    let names = config
        .lines()
        .filter_map(|line| {
            line.strip_prefix("frontend ")
                .or_else(|| line.strip_prefix("backend "))
        })
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 4);
    assert_eq!(names.iter().collect::<BTreeSet<_>>().len(), names.len());
}

#[test]
fn legacy_installed_forwarder_files_require_reviewed_reinstall()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, plan) = generated_plan()?;
    let (http, https) = (
        plan.artifact.manifest.proxy_http_port,
        plan.artifact.manifest.proxy_https_port,
    );
    let unit = haproxy_service_unit();
    let current = haproxy_config(http, https);
    assert!(installed_files_match(
        plan.target_id,
        Some(current.as_bytes()),
        Some(unit.as_bytes())
    ));
    let legacy = legacy_haproxy_config(http, https);
    assert_ne!(legacy, current);
    assert!(!installed_files_match(
        plan.target_id,
        Some(legacy.as_bytes()),
        Some(unit.as_bytes())
    ));
    assert!(!installed_files_match(
        plan.target_id,
        None,
        Some(unit.as_bytes())
    ));

    // The helper replaces legacy files under the same owner; only the template
    // check gates the new artifact.
    let uid = fs::metadata(&plan.artifact.root)?.uid();
    let verified =
        verify_source_artifact(&plan.artifact.root, plan.target_id, &plan.plan_hash, uid)?;
    verify_fixed_templates(plan.target_id, &verified)?;
    Ok(())
}

/// Runs only where HAProxy is installed (CI images, workstations).
#[test]
fn rendered_haproxy_config_passes_haproxy_validation() -> Result<(), Box<dyn std::error::Error>> {
    let Some(haproxy) = [
        "/usr/sbin/haproxy",
        "/opt/homebrew/bin/haproxy",
        "/usr/local/bin/haproxy",
    ]
    .into_iter()
    .map(Path::new)
    .find(|path| path.is_file()) else {
        return Ok(());
    };
    let temp = crate::test_support::tempdir()?;
    let path = temp.path().join("haproxy.cfg");
    fs::write(&path, haproxy_config(24_000, 24_001))?;
    let output = std::process::Command::new(haproxy)
        .arg("-c")
        .arg("-f")
        .arg(&path)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
