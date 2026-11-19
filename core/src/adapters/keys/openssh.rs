//! This device's key files, made and changed with OpenSSH's own
//! `ssh-keygen`. mm never reads `~/.ssh`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use zeroize::Zeroizing;

use super::askpass::Askpass;
use crate::app::ports::{KeyFileFacts, KeyFiles, NewKeyProtection};
use crate::domain::error::MmError;
use crate::domain::key::{KeyPassphrase, PrivateKeyHeader, openssh_version};

/// The rounds of the key-derivation function that turns the passphrase
/// into the key's cipher key. Rounds make a guessable passphrase costly to
/// try; mm's passphrase is 256 random bits, which nothing can try, and
/// every connection pays for the rounds (150 ms at OpenSSH's default 16).
const KDF_ROUNDS: &str = "1";

/// The first OpenSSH that takes a passphrase from a helper while a
/// terminal is attached (`SSH_ASKPASS_REQUIRE`).
const ASKPASS_SINCE: (u32, u32) = (8, 4);

pub struct OpenSshKeyFiles {
    /// Where the passphrase helper keeps its pipes: `MM_HOME/run`.
    run_directory: PathBuf,
    cannot_protect: OnceLock<Option<String>>,
}

impl OpenSshKeyFiles {
    #[must_use]
    pub fn new(run_directory: PathBuf) -> Self {
        Self {
            run_directory,
            cannot_protect: OnceLock::new(),
        }
    }

    fn keygen(
        &self,
        configure: impl FnOnce(&mut Command),
        askpass: Option<&Askpass>,
        stdin: Stdio,
    ) -> Result<(), MmError> {
        let mut command = Command::new("ssh-keygen");
        command.env("LC_ALL", "C");
        configure(&mut command);
        if let Some(askpass) = askpass {
            command.envs(askpass.environment());
        }
        let output = command
            .stdin(stdin)
            .output()
            .map_err(|source| MmError::ProcessSpawn {
                program: "ssh-keygen".to_owned(),
                source,
            })?;
        if output.status.success() {
            return Ok(());
        }
        Err(MmError::Environment {
            message: format!(
                "ssh-keygen failed: {}",
                crate::domain::error::printed(String::from_utf8_lossy(&output.stderr).trim())
            ),
        })
    }
}

/// `path.pub`, whatever extension `path` has.
fn public_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(".pub");
    path.with_file_name(name)
}

/// `path.previous.pub`: the public keys the key at `path` replaced, one
/// line each.
fn replaced_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(".previous.pub");
    path.with_file_name(name)
}

fn read(path: &Path) -> Result<Option<Zeroizing<String>>, MmError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(Zeroizing::new(text))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(MmError::Io {
            context: format!("reading {}", path.display()),
            source,
        }),
    }
}

impl KeyFiles for OpenSshKeyFiles {
    fn inspect(&self, path: &Path) -> Result<Option<KeyFileFacts>, MmError> {
        let public = public_path(path);
        let Some(private) = read(path)? else {
            return match read(&public)? {
                None => Ok(None),
                Some(_) => Err(MmError::State {
                    message: format!(
                        "{} exists and its private key {} does not; delete the public file to start over",
                        public.display(),
                        path.display()
                    ),
                }),
            };
        };
        let header = PrivateKeyHeader::parse(&private).ok_or_else(|| MmError::Security {
            message: format!("{} is not an OpenSSH private key", path.display()),
        })?;
        // The line beside the key carries the comment; the key file is the
        // authority on the key itself.
        let derived = header.public_line();
        let public_key = read(&public)?
            .map(|line| line.trim().to_owned())
            .filter(|line| {
                line.split_whitespace()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ")
                    == derived
            })
            .unwrap_or_else(|| derived.clone());
        let parsed = crate::domain::host_policy::parse_public_key(&public_key)
            .map_err(|message| MmError::Security { message })?;
        Ok(Some(KeyFileFacts {
            public_key,
            fingerprint: parsed.fingerprint,
            encrypted: header.encrypted,
            security_key: header.is_security_key(),
        }))
    }

    fn cannot_protect(&self) -> Option<String> {
        self.cannot_protect
            .get_or_init(|| {
                let output = Command::new("ssh")
                    .arg("-V")
                    .stdin(Stdio::null())
                    .output();
                let said = match output {
                    Ok(output) => String::from_utf8_lossy(&output.stderr).into_owned(),
                    Err(error) => {
                        tracing::debug!(%error, "could not ask ssh for its version");
                        return None;
                    }
                };
                match openssh_version(&said) {
                    Some(version) if version < ASKPASS_SINCE => Some(format!(
                        "OpenSSH {}.{} is installed, and using an encrypted key with nobody present needs {}.{} or newer",
                        version.0, version.1, ASKPASS_SINCE.0, ASKPASS_SINCE.1
                    )),
                    _ => None,
                }
            })
            .clone()
    }

    fn new_passphrase(&self) -> Result<KeyPassphrase, MmError> {
        let mut bytes = Zeroizing::new([0_u8; 32]);
        getrandom::fill(bytes.as_mut_slice()).map_err(|error| MmError::Security {
            message: format!("the system's random source failed: {error}"),
        })?;
        Ok(KeyPassphrase::from_random(&bytes))
    }

    fn generate(
        &self,
        path: &Path,
        comment: &str,
        protection: NewKeyProtection<'_>,
    ) -> Result<(), MmError> {
        if path.exists() || public_path(path).exists() {
            return Err(MmError::Environment {
                message: format!("{} already exists", path.display()),
            });
        }
        let directory = path.parent().ok_or_else(|| MmError::State {
            message: "a key has no directory".to_owned(),
        })?;
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(directory).map_err(|source| MmError::Io {
            context: format!("creating {}", directory.display()),
            source,
        })?;
        let askpass = match protection {
            NewKeyProtection::Passphrase(passphrase) => Some(Askpass::serve(
                &self.run_directory,
                None,
                Some(passphrase.clone()),
            )?),
            NewKeyProtection::SecurityKey | NewKeyProtection::None => None,
        };
        let security_key = matches!(protection, NewKeyProtection::SecurityKey);
        self.keygen(
            |command| {
                command.args(["-q", "-t"]);
                command.arg(if security_key {
                    "ed25519-sk"
                } else {
                    "ed25519"
                });
                if askpass.is_some() {
                    // With no -N, ssh-keygen asks for the passphrase, and
                    // the helper answers.
                    command.args(["-a", KDF_ROUNDS]);
                } else {
                    command.args(["-N", ""]);
                }
                command.arg("-C").arg(comment).arg("-f").arg(path);
            },
            askpass.as_ref(),
            // A security key asks for a touch, and sometimes a PIN.
            if security_key {
                Stdio::inherit()
            } else {
                Stdio::null()
            },
        )
    }

    fn encrypt(
        &self,
        path: &Path,
        old: Option<&KeyPassphrase>,
        new: &KeyPassphrase,
    ) -> Result<(), MmError> {
        let askpass = Askpass::serve(&self.run_directory, old.cloned(), Some(new.clone()))?;
        self.keygen(
            |command| {
                command.args(["-p", "-q", "-a", KDF_ROUNDS, "-f"]).arg(path);
            },
            Some(&askpass),
            Stdio::null(),
        )
    }

    fn replace(&self, staged: &Path, path: &Path) -> Result<(), MmError> {
        for (from, to) in [
            (staged.to_path_buf(), path.to_path_buf()),
            (public_path(staged), public_path(path)),
        ] {
            std::fs::rename(&from, &to).map_err(|source| MmError::Io {
                context: format!("moving {} to {}", from.display(), to.display()),
                source,
            })?;
        }
        Ok(())
    }

    fn delete(&self, path: &Path) -> Result<(), MmError> {
        for file in [path.to_path_buf(), public_path(path)] {
            match std::fs::remove_file(&file) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(MmError::Io {
                        context: format!("deleting {}", file.display()),
                        source,
                    });
                }
            }
        }
        Ok(())
    }

    fn replaced(&self, path: &Path) -> Result<Vec<String>, MmError> {
        Ok(read(&replaced_path(path))?
            .map(|text| {
                text.lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default())
    }

    fn remember_replaced(&self, path: &Path, public_key: &str) -> Result<(), MmError> {
        let mut lines = self.replaced(path)?;
        lines.push(public_key.trim().to_owned());
        let file = replaced_path(path);
        std::fs::write(&file, lines.join("\n") + "\n").map_err(|source| MmError::Io {
            context: format!("writing {}", file.display()),
            source,
        })
    }

    fn forget_replaced(&self, path: &Path) -> Result<(), MmError> {
        let file = replaced_path(path);
        match std::fs::remove_file(&file) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(MmError::Io {
                context: format!("deleting {}", file.display()),
                source,
            }),
        }
    }
}
