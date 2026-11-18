use std::sync::Arc;

use zeroize::Zeroizing;

use super::*;
use crate::adapters::ca::FilesystemLocalCaStore;
use crate::app::ports::{CommandOutput, CommandSpec, ProcessRunner};

struct SuccessProcess;

#[async_trait::async_trait]
impl ProcessRunner for SuccessProcess {
    async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(String::new()),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

#[tokio::test]
async fn linux_helpers_reject_wrong_scope_paths_and_unbounded_files()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let store = PlatformTrustStore::for_platform(
        temp.path().join("trust"),
        Arc::new(SuccessProcess),
        TrustScope::CurrentUser,
        super::super::Platform::Linux,
    );
    let fingerprint = format!("sha256:{}", "c".repeat(64));
    assert!(install(&store, Path::new("/tmp/unused"), &fingerprint).await.is_err());
    assert!(destination(&DEBIAN, &fingerprint)?.ends_with(format!("mm-local-ca-{}.crt", "c".repeat(64))));
    assert!(destination(&FEDORA, "invalid").is_err());
    let regular = temp.path().join("bundle");
    std::fs::write(&regular, "bounded")?;
    assert_eq!(read_bounded(&regular)?, b"bounded");
    assert!(read_bounded(temp.path()).is_err());
    preflight_privilege(&store).await?;
    run_privileged(&store, "/bin/true", Vec::new(), "testing trust command").await?;
    if let Ok(mechanism) = detect() {
        assert!(uninstall(&store, &fingerprint, "/wrong/entry").await.is_err());
        assert!(!mechanism.anchors.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn system_scope_install_and_exact_absent_uninstall_use_only_allowlisted_commands()
-> Result<(), Box<dyn std::error::Error>> {
    let Ok(mechanism) = detect() else {
        return Ok(());
    };
    let temp = crate::test_support::tempdir()?;
    let store = PlatformTrustStore::for_platform(
        temp.path().join("trust"),
        Arc::new(SuccessProcess),
        TrustScope::System,
        super::super::Platform::Linux,
    );
    let certificate = temp.path().join("ca.crt");
    std::fs::write(&certificate, b"test certificate")?;
    let fingerprint = format!("sha256:{}", "d".repeat(64));
    let status = install(&store, &certificate, &fingerprint).await?;
    assert!(status.supported);
    assert!(!status.installed);
    let entry = destination(mechanism, &fingerprint)?;
    uninstall(&store, &fingerprint, &entry.to_string_lossy()).await?;
    assert!(read_bounded(&temp.path().join("missing")).is_err());
    Ok(())
}

#[tokio::test]
async fn existing_anchor_requires_the_exact_reviewed_certificate()
-> Result<(), Box<dyn std::error::Error>> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Ok(());
    }
    let Ok(mechanism) = detect() else {
        return Ok(());
    };
    let temp = crate::test_support::tempdir()?;
    let ca_root = temp.path().join("ca");
    let ca = FilesystemLocalCaStore::new(ca_root.clone());
    let installed = ca.install()?;
    let fingerprint = installed.fingerprint.ok_or("fingerprint missing")?;
    let certificate = ca_root.join("ca.crt");
    let other_root = temp.path().join("other-ca");
    FilesystemLocalCaStore::new(other_root.clone()).install()?;
    let destination = destination(mechanism, &fingerprint)?;
    if destination.exists() {
        return Ok(());
    }
    let _anchor = AnchorGuard(destination.clone());
    std::fs::write(&destination, std::fs::read(other_root.join("ca.crt"))?)?;
    let store = PlatformTrustStore::for_platform(
        temp.path().join("trust"),
        Arc::new(SuccessProcess),
        TrustScope::System,
        super::super::Platform::Linux,
    );
    let observed = status(&fingerprint).await?;
    assert!(observed.supported && observed.mismatch);
    assert!(install(&store, &certificate, &fingerprint).await.is_err());
    assert!(
        uninstall(&store, &fingerprint, &destination.to_string_lossy())
            .await
            .is_err()
    );

    std::fs::write(&destination, std::fs::read(&certificate)?)?;
    let observed = install(&store, &certificate, &fingerprint).await?;
    assert!(!observed.mismatch);
    uninstall(&store, &fingerprint, &destination.to_string_lossy()).await?;
    Ok(())
}

struct AnchorGuard(PathBuf);

impl Drop for AnchorGuard {
    fn drop(&mut self) {
        _ = std::fs::remove_file(&self.0);
    }
}
