#[cfg(unix)]
pub(crate) fn verify_public_forwarder() -> Result<(), MmError> {
    let host = format!("mm-probe-{}.invalid", uuid::Uuid::new_v4().simple());
    let mut last_error = None;
    for attempt in 0..20 {
        match probe_public_forwarder(&host) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        if attempt < 19 {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Err(MmError::RuntimeResponse {
        message: format!("the privileged ingress forwarder did not produce Traefik's loopback HTTP response: {}. Inspect `journalctl -u mm-ingress-forwarder.service` for bind or security-policy failures",
            last_error.map_or_else(|| "no response".to_owned(), |error| error.to_string())),
    })
}

#[cfg(unix)]
fn probe_public_forwarder(host: &str) -> Result<(), MmError> {
    let timeout = Duration::from_millis(500);
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, INGRESS_PUBLIC_HTTP_PORT));
    let mut stream = TcpStream::connect_timeout(&address, timeout).map_err(|source| {
        MmError::RuntimeResponse {
            message: format!("could not connect to public ingress loopback: {source}"),
        }
    })?;
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|()| stream.set_write_timeout(Some(timeout)))
        .map_err(|source| MmError::Io {
            context: "configuring the public ingress verification socket".to_owned(),
            source,
        })?;
    let request =
        format!("GET /.mm-forwarder-check HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|source| MmError::Io {
            context: "writing the public ingress verification request".to_owned(),
            source,
        })?;
    let mut response = [0_u8; 4096];
    let count = stream.read(&mut response).map_err(|source| MmError::Io {
        context: "reading the public ingress verification response".to_owned(),
        source,
    })?;
    let status = std::str::from_utf8(&response[..count])
        .ok()
        .and_then(|response| response.lines().next())
        .and_then(|line| line.split_ascii_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok());
    if status == Some(404) {
        Ok(())
    } else {
        Err(MmError::RuntimeResponse {
            message: format!(
                "public ingress loopback returned HTTP {} instead of Traefik's expected 404",
                status.map_or_else(|| "unknown".to_owned(), |status| status.to_string())
            ),
        })
    }
}

#[cfg(unix)]
fn atomic_replace(path: &Path, bytes: &[u8], mode: u32) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;

    ensure_no_symlink_components(path)?;
    let parent = path.parent().ok_or_else(|| MmError::Security {
        message: "privileged ingress destination has no parent".to_owned(),
    })?;
    let temporary = parent.join(format!(".mm-ingress-{}.tmp", uuid::Uuid::new_v4()));
    ensure_no_symlink_components(&temporary)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|source| MmError::Io {
            context: format!("creating staged ingress file {}", temporary.display()),
            source,
        })?;
    let result = (|| {
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()
    })()
    .map_err(|source| MmError::Io {
        context: format!("publishing privileged ingress file {}", path.display()),
        source,
    });
    let cleanup = if temporary.exists() {
        fs::remove_file(&temporary).map_err(|source| MmError::Io {
            context: format!("removing staged ingress file {}", temporary.display()),
            source,
        })
    } else {
        Ok(())
    };
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => Err(MmError::operation_failure(
            temporary.display().to_string(),
            crate::domain::error::MutationStatus::NotStarted,
            primary,
            vec![cleanup.to_string()],
            "remove the privileged ingress staging file before retrying",
        )),
    }
}

#[cfg(unix)]
fn run_checked(program: &str, args: &[&str]) -> Result<(), MmError> {
    let output = Command::new(program)
        .args(args)
        .env_clear()
        .output()
        .map_err(|source| MmError::ProcessSpawn {
            program: program.to_owned(),
            source,
        })?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(MmError::ProcessFailed {
            program: program.to_owned(),
            code: output.status.code(),
            stderr: stderr.chars().take(4096).collect(),
        })
    }
}

fn path_text(path: &Path, kind: &str) -> Result<String, MmError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| MmError::Security {
            message: format!("{kind} must be a Unicode path"),
        })
}

#[cfg(unix)]
fn verify_privileged_executable(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !path.is_absolute() {
        return Err(MmError::Security {
            message: "the mm executable used with sudo must have an absolute path".to_owned(),
        });
    }
    ensure_no_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting privileged mm executable {}", path.display()),
        source,
    })?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.permissions().mode() & 0o6022 != 0 {
        return Err(MmError::Security {
            message: "public ingress installation requires mm to be installed as a protected root-owned executable"
                .to_owned(),
        });
    }
    for ancestor in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(ancestor).map_err(|source| MmError::Io {
            context: format!(
                "inspecting privileged executable path {}",
                ancestor.display()
            ),
            source,
        })?;
        if !metadata.is_dir() || metadata.uid() != 0 || metadata.permissions().mode() & 0o022 != 0 {
            return Err(MmError::Security {
                message: format!(
                    "privileged executable ancestor '{}' must be a protected root-owned directory",
                    ancestor.display()
                ),
            });
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_privileged_executable(_path: &Path) -> Result<(), MmError> {
    Err(MmError::Security {
        message: "privileged ingress installation requires Unix permission semantics".to_owned(),
    })
}

fn validate_support_name(name: &str) -> Result<(), MmError> {
    let path = Path::new(name);
    if name.is_empty()
        || name.starts_with('.')
        || path.components().count() != 1
        || !name
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'.' | b'_' | b'-'))
    {
        return Err(MmError::Security {
            message: format!("invalid ingress support-file name '{name}'"),
        });
    }
    Ok(())
}

fn validate_hash(value: &str) -> Result<(), MmError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(MmError::Security {
            message: "invalid ingress artifact hash".to_owned(),
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use tempfile::TempDir;
    use uuid::Uuid;

    use super::{verify_fixed_templates, verify_privileged_executable, verify_source_artifact};
    use crate::adapters::target::{FilesystemTargetIngressArtifactStore, FilesystemTargetStore};
    use crate::app::TargetIngressManager;
    use crate::app::ports::TargetStore;
    use crate::domain::target::{TargetDefinition, TargetId};

    #[test]
    fn accepts_only_a_digest_verified_fixed_privileged_artifact()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_temporary, plan) = fixture()?;
        let artifact = verify_source_artifact(
            &plan.artifact.root,
            plan.target_id,
            &plan.plan_hash,
            rustix::process::geteuid().as_raw(),
        )?;

        verify_fixed_templates(plan.target_id, &artifact)?;
        Ok(())
    }

    #[test]
    fn rejects_a_support_file_changed_after_publication() -> Result<(), Box<dyn std::error::Error>>
    {
        let (_temporary, plan) = fixture()?;
        let haproxy = plan.artifact.root.join("haproxy.cfg");
        fs::write(&haproxy, "global\n  lua-load /tmp/attacker.lua\n")?;

        assert!(
            verify_source_artifact(
                &plan.artifact.root,
                plan.target_id,
                &plan.plan_hash,
                rustix::process::geteuid().as_raw(),
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn rejects_a_user_owned_executable_at_the_sudo_boundary()
    -> Result<(), Box<dyn std::error::Error>> {
        if rustix::process::geteuid().as_raw() == 0 {
            return Ok(());
        }
        let temporary = crate::test_support::tempdir()?;
        let executable = temporary.path().join("mm");
        fs::write(&executable, "test executable")?;

        assert!(verify_privileged_executable(&executable).is_err());
        Ok(())
    }

    fn fixture() -> Result<
        (TempDir, crate::domain::target_ingress::TargetIngressPlan),
        Box<dyn std::error::Error>,
    > {
        let temporary = crate::test_support::tempdir()?;
        let target_id = TargetId(Uuid::from_u128(0x319c527b62427b43bfd4bd60e27c1de3));
        let targets = Arc::new(FilesystemTargetStore::new(
            temporary.path().join("targets.yaml"),
        ));
        targets.ensure_target("local", TargetDefinition::local(target_id))?;
        let artifacts = Arc::new(FilesystemTargetIngressArtifactStore::new(
            temporary.path().join("artifacts"),
        ));
        let plan = TargetIngressManager::new(targets, artifacts).plan("local")?;
        Ok((temporary, plan))
    }
}
