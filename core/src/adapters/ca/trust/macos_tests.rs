use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use zeroize::Zeroizing;

use super::*;
use crate::adapters::ca::FilesystemLocalCaStore;
use crate::app::ports::{CommandOutput, CommandSpec, ProcessRunner};

struct QueueProcess(Mutex<VecDeque<CommandOutput>>);

#[async_trait::async_trait]
impl ProcessRunner for QueueProcess {
    async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
        self.0.lock().map_err(|_| MmError::State {
            message: "queue lock poisoned".to_owned(),
        })?.pop_front().ok_or_else(|| MmError::State {
            message: "missing queued output".to_owned(),
        })
    }
}

fn success() -> CommandOutput {
    CommandOutput {
        code: Some(0),
        stdout: Zeroizing::new(String::new()),
        stderr: Zeroizing::new(String::new()),
        truncated: false,
    }
}

fn output(code: i32, stdout: Vec<u8>) -> CommandOutput {
    CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(String::from_utf8_lossy(&stdout).into_owned()),
        stderr: Zeroizing::new(String::new()),
        truncated: false,
    }
}

#[tokio::test]
async fn user_keychain_commands_validate_owned_certificate_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let ca_root = temp.path().join("ca");
    let ca = FilesystemLocalCaStore::new(ca_root.clone());
    let installed = ca.install()?;
    let fingerprint = installed.fingerprint.ok_or("fingerprint missing")?;
    let pem = std::fs::read(ca_root.join("ca.crt"))?;
    let trust_root = temp.path().join("trust");
    std::fs::create_dir(&trust_root)?;
    std::fs::write(
        trust_root.join(format!("{}.crt", super::super::fingerprint_token(&fingerprint)?)),
        pem,
    )?;
    let store = PlatformTrustStore::for_platform(
        trust_root,
        Arc::new(QueueProcess(Mutex::new(vec![success(), success()].into()))),
        TrustScope::CurrentUser,
        super::super::Platform::Macos,
    );
    assert!(!status(&store, &fingerprint).await?.supported);
    let prepared = ca_root.join("ca.crt");
    assert!(!install(&store, &prepared, &fingerprint).await?.supported);
    uninstall(&store, &fingerprint).await?;
    assert!(keychain(TrustScope::CurrentUser)?.ends_with("Library/Keychains/login.keychain-db"));
    assert_eq!(keychain(TrustScope::System)?, PathBuf::from("/Library/Keychains/System.keychain"));
    Ok(())
}

#[tokio::test]
async fn available_security_tool_reports_installed_mismatch_and_absence()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(_security) = FixedFileGuard::create(SECURITY)? else {
        return Ok(());
    };
    let temp = crate::test_support::tempdir()?;
    let ca_root = temp.path().join("ca");
    let ca = FilesystemLocalCaStore::new(ca_root.clone());
    let installed = ca.install()?;
    let fingerprint = installed.fingerprint.ok_or("fingerprint missing")?;
    let pem = std::fs::read(ca_root.join("ca.crt"))?;
    let other_root = temp.path().join("other-ca");
    FilesystemLocalCaStore::new(other_root.clone()).install()?;
    let other_pem = std::fs::read(other_root.join("ca.crt"))?;
    let process = Arc::new(QueueProcess(Mutex::new(
        vec![
            output(0, pem.clone()),
            output(0, other_pem),
            output(1, Vec::new()),
        ]
        .into(),
    )));
    let store = PlatformTrustStore::for_platform(
        temp.path().join("trust"),
        process,
        TrustScope::CurrentUser,
        super::super::Platform::Macos,
    );
    let present = status(&store, &fingerprint).await?;
    assert!(present.supported && present.installed && !present.mismatch);
    let mismatched = status(&store, &fingerprint).await?;
    assert!(mismatched.supported && !mismatched.installed && mismatched.mismatch);
    let absent = status(&store, &fingerprint).await?;
    assert!(absent.supported && !absent.installed && !absent.mismatch);
    Ok(())
}

#[tokio::test]
async fn system_scope_uses_sudo_for_install_and_exact_removal()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(_security) = FixedFileGuard::create(SECURITY)? else {
        return Ok(());
    };
    let Some(_sudo) = FixedFileGuard::create("/usr/bin/sudo")? else {
        return Ok(());
    };
    let temp = crate::test_support::tempdir()?;
    let ca_root = temp.path().join("ca");
    let ca = FilesystemLocalCaStore::new(ca_root.clone());
    let installed = ca.install()?;
    let fingerprint = installed.fingerprint.ok_or("fingerprint missing")?;
    let pem = std::fs::read(ca_root.join("ca.crt"))?;
    let trust_root = temp.path().join("trust");
    std::fs::create_dir(&trust_root)?;
    std::fs::write(
        trust_root.join(format!(
            "{}.crt",
            super::super::fingerprint_token(&fingerprint)?
        )),
        &pem,
    )?;
    let process = Arc::new(QueueProcess(Mutex::new(
        vec![success(), success(), output(0, pem), success()].into(),
    )));
    let store = PlatformTrustStore::for_platform(
        trust_root,
        process,
        TrustScope::System,
        super::super::Platform::Macos,
    );
    let status = install(&store, &ca_root.join("ca.crt"), &fingerprint).await?;
    assert!(status.installed && status.needs_privilege);
    uninstall(&store, &fingerprint).await?;
    Ok(())
}

struct FixedFileGuard {
    path: &'static str,
}

impl FixedFileGuard {
    fn create(path: &'static str) -> Result<Option<Self>, std::io::Error> {
        if rustix::process::geteuid().as_raw() != 0 || Path::new(path).exists() {
            return Ok(None);
        }
        std::fs::write(path, b"test placeholder")?;
        Ok(Some(Self { path }))
    }
}

impl Drop for FixedFileGuard {
    fn drop(&mut self) {
        _ = std::fs::remove_file(self.path);
    }
}
