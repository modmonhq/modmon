use std::fs;
use std::path::{Path, PathBuf};

use crate::app::ports::CommandSpec;
use crate::domain::ca::{TrustScope, bundle_contains_fingerprint, certificate_fingerprint};
use crate::domain::error::MmError;

use super::{BackendStatus, PlatformTrustStore, bounded_detail, fingerprint_token, trust_error};

const MAX_BUNDLE_BYTES: u64 = 16 * 1024 * 1024;

struct LinuxMechanism {
    update: &'static str,
    anchors: &'static str,
    bundle: &'static str,
    update_args: &'static [&'static str],
}

const DEBIAN: LinuxMechanism = LinuxMechanism {
    update: "/usr/sbin/update-ca-certificates",
    anchors: "/usr/local/share/ca-certificates",
    bundle: "/etc/ssl/certs/ca-certificates.crt",
    update_args: &[],
};

const FEDORA: LinuxMechanism = LinuxMechanism {
    update: "/usr/bin/update-ca-trust",
    anchors: "/etc/pki/ca-trust/source/anchors",
    bundle: "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
    update_args: &["extract"],
};

pub(super) async fn status(fingerprint: &str) -> Result<BackendStatus, MmError> {
    let mechanism = match detect() {
        Ok(mechanism) => mechanism,
        Err(_) => return Ok(BackendStatus::unsupported()),
    };
    let destination = destination(mechanism, fingerprint)?;
    let mismatch = destination.is_file()
        && certificate_fingerprint(&read_bounded(&destination)?)
            .map_err(trust_error)?
            .to_string()
            != fingerprint;
    let installed = if Path::new(mechanism.bundle).is_file() {
        bundle_contains_fingerprint(&read_bounded(Path::new(mechanism.bundle))?, fingerprint)
            .map_err(trust_error)?
    } else {
        false
    };
    Ok(BackendStatus {
        supported: true,
        installed,
        mismatch,
        needs_privilege: !is_root(),
        entry: destination.display().to_string(),
    })
}

pub(super) async fn install(
    store: &PlatformTrustStore,
    certificate: &Path,
    fingerprint: &str,
) -> Result<BackendStatus, MmError> {
    if store.scope != TrustScope::System {
        return Err(MmError::Target {
            message: "Linux trust uses the system store; select system scope or use --no-trust"
                .to_owned(),
        });
    }
    let mechanism = detect()?;
    preflight_privilege(store).await?;
    let destination = destination(mechanism, fingerprint)?;
    if destination.is_file() {
        let actual = certificate_fingerprint(&read_bounded(&destination)?)
            .map_err(trust_error)?
            .to_string();
        if actual != fingerprint {
            return Err(MmError::Security {
                message: format!(
                    "Linux trust anchor '{}' contains fingerprint {actual}; refusing to replace it",
                    destination.display()
                ),
            });
        }
    } else {
        run_privileged(
            store,
            "/usr/bin/install",
            vec![
                "-m".to_owned(),
                "0644".to_owned(),
                "--".to_owned(),
                certificate.display().to_string(),
                destination.display().to_string(),
            ],
            "installing Linux trust anchor",
        )
        .await?;
    }
    run_privileged(
        store,
        mechanism.update,
        mechanism
            .update_args
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        "refreshing Linux trust store",
    )
    .await?;
    status(fingerprint).await
}

pub(super) async fn uninstall(
    store: &PlatformTrustStore,
    fingerprint: &str,
    entry: &str,
) -> Result<(), MmError> {
    let mechanism = detect()?;
    let expected = destination(mechanism, fingerprint)?;
    if Path::new(entry) != expected {
        return Err(MmError::Security {
            message:
                "Linux trust ownership path does not match the exact fingerprint-scoped anchor"
                    .to_owned(),
        });
    }
    if expected.is_file() {
        let actual = certificate_fingerprint(&read_bounded(&expected)?)
            .map_err(trust_error)?
            .to_string();
        if actual != fingerprint {
            return Err(MmError::Security {
                message:
                    "Linux trust anchor was replaced; refusing to delete mismatched certificate"
                        .to_owned(),
            });
        }
        preflight_privilege(store).await?;
        run_privileged(
            store,
            "/usr/bin/rm",
            vec!["--".to_owned(), expected.display().to_string()],
            "removing Linux trust anchor",
        )
        .await?;
        run_privileged(
            store,
            mechanism.update,
            mechanism
                .update_args
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            "refreshing Linux trust store",
        )
        .await?;
    }
    Ok(())
}

fn detect() -> Result<&'static LinuxMechanism, MmError> {
    if Path::new(DEBIAN.update).is_file() && Path::new(DEBIAN.anchors).is_dir() {
        Ok(&DEBIAN)
    } else if Path::new(FEDORA.update).is_file() && Path::new(FEDORA.anchors).is_dir() {
        Ok(&FEDORA)
    } else {
        Err(MmError::Target {
            message: "unsupported Linux trust store: expected Debian/Ubuntu update-ca-certificates or Fedora/RHEL update-ca-trust at allowlisted paths"
                .to_owned(),
        })
    }
}

fn destination(mechanism: &LinuxMechanism, fingerprint: &str) -> Result<PathBuf, MmError> {
    Ok(Path::new(mechanism.anchors).join(format!(
        "mm-local-ca-{}.crt",
        fingerprint_token(fingerprint)?
    )))
}

async fn preflight_privilege(store: &PlatformTrustStore) -> Result<(), MmError> {
    if is_root() {
        return Ok(());
    }
    if !Path::new("/usr/bin/sudo").is_file() {
        return Err(MmError::Target {
            message: "system trust installation requires root; /usr/bin/sudo is unavailable"
                .to_owned(),
        });
    }
    let output = store
        .run(CommandSpec::new("/usr/bin/sudo").args(["-n", "-v"]))
        .await?;
    if !output.success() {
        return Err(MmError::Target {
            message: format!(
                "system trust requires sudo authorization; run `sudo -v` and retry: {}",
                bounded_detail(&output.stderr)
            ),
        });
    }
    Ok(())
}

async fn run_privileged(
    store: &PlatformTrustStore,
    program: &str,
    args: Vec<String>,
    action: &str,
) -> Result<(), MmError> {
    let specification = if is_root() {
        CommandSpec::new(program).args(args)
    } else {
        CommandSpec::new("/usr/bin/sudo")
            .args(["-n", "--", program])
            .args_append(args)
    };
    store.run_checked(specification, action).await?;
    Ok(())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, MmError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting trust store {}", path.display()),
        source,
    })?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_BUNDLE_BYTES {
        return Err(MmError::Security {
            message: format!(
                "trust-store file '{}' is not a bounded regular file",
                path.display()
            ),
        });
    }
    fs::read(path).map_err(|source| MmError::Io {
        context: format!("reading trust store {}", path.display()),
        source,
    })
}

#[cfg(unix)]
fn is_root() -> bool {
    rustix::process::geteuid().is_root()
}

#[cfg(not(unix))]
fn is_root() -> bool {
    false
}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
