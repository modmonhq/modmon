//! The project's read-only git deploy key, which `mmd` fetches the origin
//! with: `<state_root>/projects/<id>/deploy-key` (mode 0600) and its public
//! line beside it in `deploy-key.pub` (mode 0644).

use std::fs;
use std::path::Path;

use zeroize::Zeroizing;

use super::{
    HostAccessStore, create_private_directory, read_optional, security, validate_private_file,
    write_atomic,
};
use crate::domain::error::MmError;
use crate::domain::host_policy::{PublicKey, parse_public_key};
use crate::domain::workspace::ProjectId;

const DEPLOY_KEY_ALGORITHM: &str = "ssh-ed25519";

impl HostAccessStore {
    /// The project's read-only git deploy key, created on first use. A key
    /// already in place, generated or installed, is kept. Returns its public
    /// line, to add to the git host.
    pub fn ensure_deploy_key(&self, project: ProjectId) -> Result<String, MmError> {
        let directory = self.project_directory(project);
        create_private_directory(&directory)?;
        let key = directory.join("deploy-key");
        let public = directory.join("deploy-key.pub");
        let comment = format!("mmd-deploy-{}", project.0);
        if fs::symlink_metadata(&key).is_err() {
            if fs::symlink_metadata(&public).is_ok() {
                fs::remove_file(&public).map_err(|source| MmError::Io {
                    context: format!("removing stale {}", public.display()),
                    source,
                })?;
            }
            run_ssh_keygen(&[
                "-q",
                "-t",
                "ed25519",
                "-N",
                "",
                "-C",
                &comment,
                "-f",
                &key.display().to_string(),
            ])?;
        } else if fs::symlink_metadata(&public).is_err() {
            let line = run_ssh_keygen(&["-y", "-f", &key.display().to_string()])?;
            write_atomic(&public, format!("{} {comment}\n", line.trim()), 0o644)?;
        }
        validate_private_file(&key)?;
        let line = read_optional(&public)?
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .ok_or_else(|| security("deploy key public half is missing or not UTF-8"))?;
        let line = line.trim().to_owned();
        parse_public_key(&line).map_err(|message| MmError::Security {
            message: format!("deploy key {}: {message}", public.display()),
        })?;
        Ok(line)
    }

    /// Installs a deploy key generated elsewhere, replacing the project's
    /// current key, at the paths and modes [`Self::ensure_deploy_key`] uses.
    /// `private_key` is the raw OpenSSH private key and `public_key` its
    /// `ssh-ed25519` line; the key is refused unless its private half derives
    /// that public key. Returns whether anything changed.
    pub fn install_deploy_key(
        &self,
        project: ProjectId,
        private_key: &[u8],
        public_key: &str,
    ) -> Result<bool, MmError> {
        let line = public_key.trim();
        let expected = deploy_public_key(line)?;
        if private_key.is_empty() {
            return Err(security("deploy key private half is empty"));
        }
        let directory = self.project_directory(project);
        create_private_directory(&directory)?;
        let key = directory.join("deploy-key");
        let public = directory.join("deploy-key.pub");
        if installed(&key, &public, private_key, line)? {
            return Ok(false);
        }

        let staged = directory.join(format!(".deploy-key.{}", uuid::Uuid::new_v4()));
        write_atomic(&staged, private_key, 0o600)?;
        let result = publish_staged(&staged, &key, &public, line, &expected);
        if result.is_err() {
            match fs::remove_file(&staged) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => tracing::warn!(%error, "failed to remove a staged deploy key"),
            }
        }
        result.map(|()| true)
    }
}

impl HostAccessStore {
    /// Removes what fetching left for a project that is now pushed to: the
    /// deploy key and its public half, the origin's pinned host key, and
    /// the clone. Nothing reads them any more, and a host that is meant to
    /// have no way to the git server keeps no key for it. Returns what was
    /// removed, by name; nothing when it was never fetched.
    pub fn remove_fetch_material(&self, project: ProjectId) -> Result<Vec<String>, MmError> {
        let directory = self.project_directory(project);
        let mut removed = Vec::new();
        for name in [
            "deploy-key",
            "deploy-key.pub",
            "origin_known_hosts",
            "repo.git",
        ] {
            let path = directory.join(name);
            let gone = match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(&path),
                Ok(_) => fs::remove_file(&path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => Err(error),
            };
            gone.map_err(|source| MmError::Io {
                context: format!("removing {} that fetching left", path.display()),
                source,
            })?;
            removed.push(name.to_owned());
        }
        Ok(removed)
    }
}

fn deploy_public_key(line: &str) -> Result<PublicKey, MmError> {
    let key = parse_public_key(line).map_err(|message| MmError::Security {
        message: format!("deploy key public half: {message}"),
    })?;
    if key.algorithm != DEPLOY_KEY_ALGORITHM {
        return Err(security("deploy keys must be ssh-ed25519"));
    }
    Ok(key)
}

/// Whether exactly this key pair is already installed. A symlink or any
/// other difference counts as not installed, and is replaced.
fn installed(key: &Path, public: &Path, private_key: &[u8], line: &str) -> Result<bool, MmError> {
    let regular = |path: &Path| {
        fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
    };
    if !regular(key) || !regular(public) {
        return Ok(false);
    }
    let current = Zeroizing::new(read_optional(key)?.unwrap_or_default());
    let current_public = read_optional(public)?
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|text| text.trim().to_owned());
    Ok(current.as_slice() == private_key
        && current_public.as_deref() == Some(line)
        && validate_private_file(key).is_ok())
}

fn publish_staged(
    staged: &Path,
    key: &Path,
    public: &Path,
    line: &str,
    expected: &PublicKey,
) -> Result<(), MmError> {
    // `-P ''` keeps ssh-keygen from prompting: a passphrase-protected key
    // fails here, since mmd could not use it either.
    let derived = run_ssh_keygen(&["-y", "-P", "", "-f", &staged.display().to_string()])?;
    let derived = parse_public_key(derived.trim()).map_err(|message| MmError::Security {
        message: format!("deploy key private half: {message}"),
    })?;
    if derived.algorithm != expected.algorithm || derived.blob != expected.blob {
        return Err(security(
            "deploy key private half does not match the given public key",
        ));
    }
    fs::rename(staged, key).map_err(|source| MmError::Io {
        context: format!("installing deploy key {}", key.display()),
        source,
    })?;
    write_atomic(public, format!("{line}\n"), 0o644)?;
    validate_private_file(key)
}

fn run_ssh_keygen(arguments: &[&str]) -> Result<String, MmError> {
    let output = std::process::Command::new("ssh-keygen")
        .args(arguments)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|source| MmError::Io {
            context: "running ssh-keygen for the deploy key".to_owned(),
            source,
        })?;
    if !output.status.success() {
        return Err(MmError::ProcessFailed {
            program: "ssh-keygen".to_owned(),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    String::from_utf8(output.stdout).map_err(|_| security("ssh-keygen printed non-UTF-8 output"))
}

#[cfg(test)]
mod tests;
