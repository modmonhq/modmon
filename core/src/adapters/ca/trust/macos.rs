use std::path::{Path, PathBuf};

use crate::app::ports::CommandSpec;
use crate::domain::ca::{
    TrustScope, bundle_contains_fingerprint, certificate_der, certificate_fingerprint,
};
use crate::domain::error::MmError;

use super::{BackendStatus, PlatformTrustStore, trust_error};

const SECURITY: &str = "/usr/bin/security";

pub(super) async fn status(
    store: &PlatformTrustStore,
    fingerprint: &str,
) -> Result<BackendStatus, MmError> {
    if !Path::new(SECURITY).is_file() {
        return Ok(BackendStatus::unsupported());
    }
    let keychain = keychain(store.scope)?;
    let output = store
        .run(CommandSpec::new(SECURITY).args([
            "find-certificate",
            "-a",
            "-c",
            "mm local CA",
            "-p",
            keychain.to_string_lossy().as_ref(),
        ]))
        .await?;
    let installed = output.success()
        && bundle_contains_fingerprint(output.stdout.as_bytes(), fingerprint)
            .map_err(trust_error)?;
    let mismatch = output.success() && !output.stdout.trim().is_empty() && !installed;
    Ok(BackendStatus {
        supported: true,
        installed,
        mismatch,
        needs_privilege: store.scope == TrustScope::System,
        entry: keychain.display().to_string(),
    })
}

pub(super) async fn install(
    store: &PlatformTrustStore,
    certificate: &Path,
    fingerprint: &str,
) -> Result<BackendStatus, MmError> {
    let keychain = keychain(store.scope)?;
    let mut args = vec![
        "add-trusted-cert".to_owned(),
        "-r".to_owned(),
        "trustRoot".to_owned(),
        "-k".to_owned(),
        keychain.display().to_string(),
        certificate.display().to_string(),
    ];
    let specification = if store.scope == TrustScope::System {
        if !Path::new("/usr/bin/sudo").is_file() {
            return Err(MmError::Target {
                message:
                    "macOS system trust requires /usr/bin/sudo; use the default current-user scope"
                        .to_owned(),
            });
        }
        args.insert(1, "-d".to_owned());
        let preflight = store
            .run(CommandSpec::new("/usr/bin/sudo").args(["-n", "-v"]))
            .await?;
        if !preflight.success() {
            return Err(MmError::Target {
                message: "macOS system trust requires sudo authorization; run `sudo -v` and retry"
                    .to_owned(),
            });
        }
        CommandSpec::new("/usr/bin/sudo")
            .args(["-n", "--", SECURITY])
            .args_append(args)
    } else {
        CommandSpec::new(SECURITY).args(args)
    };
    store
        .run_checked(specification, "installing macOS trusted root")
        .await?;
    status(store, fingerprint).await
}

pub(super) async fn uninstall(
    store: &PlatformTrustStore,
    fingerprint: &str,
) -> Result<(), MmError> {
    let keychain = keychain(store.scope)?;
    let certificate = store
        .root
        .join(format!("{}.crt", super::fingerprint_token(fingerprint)?));
    let bytes = std::fs::read(&certificate).map_err(|source| MmError::Io {
        context: format!(
            "reading owned macOS trust certificate {}",
            certificate.display()
        ),
        source,
    })?;
    if certificate_fingerprint(&bytes)
        .map_err(trust_error)?
        .to_string()
        != fingerprint
    {
        return Err(MmError::Security {
            message: "owned macOS trust certificate no longer matches its SHA-256 fingerprint"
                .to_owned(),
        });
    }
    use sha1::Digest;
    let hash = sha1::Sha1::digest(certificate_der(&bytes).map_err(trust_error)?)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>();
    let args = vec![
        "delete-certificate".to_owned(),
        "-Z".to_owned(),
        hash,
        keychain.display().to_string(),
    ];
    let specification = if store.scope == TrustScope::System {
        CommandSpec::new("/usr/bin/sudo")
            .args(["-n", "--", SECURITY])
            .args_append(args)
    } else {
        CommandSpec::new(SECURITY).args(args)
    };
    store
        .run_checked(specification, "removing exact macOS trusted root")
        .await?;
    Ok(())
}

fn keychain(scope: TrustScope) -> Result<PathBuf, MmError> {
    match scope {
        TrustScope::CurrentUser => std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Keychains/login.keychain-db"))
            .ok_or_else(|| MmError::Target {
                message: "HOME is required to select the macOS current-user keychain".to_owned(),
            }),
        TrustScope::System => Ok(PathBuf::from("/Library/Keychains/System.keychain")),
    }
}

#[cfg(test)]
#[path = "macos_tests.rs"]
mod tests;
