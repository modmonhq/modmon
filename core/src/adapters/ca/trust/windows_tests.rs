use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use zeroize::Zeroizing;

use super::*;
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

fn output(code: i32, stdout: &str) -> CommandOutput {
    CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(stdout.to_owned()),
        stderr: Zeroizing::new(String::new()),
        truncated: false,
    }
}

fn store(root: &Path, outputs: Vec<CommandOutput>, scope: TrustScope) -> PlatformTrustStore {
    PlatformTrustStore::for_platform(
        root.to_path_buf(),
        Arc::new(QueueProcess(Mutex::new(outputs.into()))),
        scope,
        super::super::Platform::Windows,
    )
}

#[tokio::test]
async fn powershell_actions_are_bounded_and_scope_specific()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let store = store(
        &temp.path().join("trust"),
        vec![
            output(0, "KEYS-PRIVATE\n"),
            output(0, "KEYS-PRIVATE\n"),
            output(0, ""),
            output(0, ""),
        ],
        TrustScope::CurrentUser,
    );
    secure_private_keys(&store).await?;
    assert!(private_keys_valid(&store).await?);
    let certificate = temp.path().join("ca.crt");
    std::fs::write(&certificate, "certificate")?;
    let fingerprint = format!("sha256:{}", "a".repeat(64));
    let status = install(&store, &certificate, &fingerprint).await?;
    assert!(!status.supported);
    uninstall(&store, &fingerprint).await?;
    assert_eq!(scope_name(TrustScope::CurrentUser), "CurrentUser");
    assert_eq!(scope_name(TrustScope::System), "LocalMachine");
    assert!(store.root.join("trust-store.ps1").is_file());
    Ok(())
}

#[tokio::test]
async fn powershell_failures_do_not_claim_security_or_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let store = store(
        &temp.path().join("trust"),
        vec![output(1, ""), output(1, ""), output(1, ""), output(1, "")],
        TrustScope::System,
    );
    assert!(secure_private_keys(&store).await.is_err());
    assert!(!private_keys_valid(&store).await?);
    let certificate = temp.path().join("ca.crt");
    std::fs::write(&certificate, "certificate")?;
    let fingerprint = format!("sha256:{}", "b".repeat(64));
    assert!(install(&store, &certificate, &fingerprint).await.is_err());
    assert!(uninstall(&store, &fingerprint).await.is_err());
    Ok(())
}
