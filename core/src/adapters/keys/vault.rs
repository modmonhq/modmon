//! Where the passphrases of mm-managed keys are kept.
//!
//! What each vault is worth against a program running as the same user:
//!
//! - The Secret Service (GNOME Keyring, KWallet) encrypts its store with
//!   the login password and answers every program on the user's session
//!   bus while it is unlocked. It keeps the passphrase off the disk, out
//!   of backups and away from anything that cannot reach the session bus;
//!   a program on the bus can ask it as `mm` does.
//! - The macOS login keychain, reached here through `security`, releases
//!   the item to that tool, so to any program that runs it while the
//!   keychain is unlocked.
//! - A file beside the key is for machines with neither. It only keeps
//!   the key file from working on its own.
//!
//! None of them is a boundary against the user's own account. Each makes
//! the key file useless by itself, which is what a copy, a backup, or a
//! tool that is handed the file gets.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use crate::app::ports::{KeyEntry, KeyVault};
use crate::domain::error::MmError;
use crate::domain::key::{KeyPassphrase, KeyVaultKind};

/// The name every keychain entry of mm's carries.
const SERVICE: &str = "mm-ssh-key";
/// How long a keychain may take to answer; unlocking one can ask the user.
const PATIENCE: Duration = Duration::from_secs(60);
const PROBE_PATIENCE: Duration = Duration::from_secs(5);

/// The system keychain of this platform.
#[must_use]
pub fn system_keychain() -> std::sync::Arc<dyn KeyVault> {
    if cfg!(target_os = "macos") {
        std::sync::Arc::new(MacKeychainVault::default())
    } else if cfg!(unix) {
        std::sync::Arc::new(SecretServiceVault::default())
    } else {
        std::sync::Arc::new(NoKeychain)
    }
}

/// No keychain: for `MM_KEY_VAULT=file`, where none is ever asked.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoKeychain;

impl KeyVault for NoKeychain {
    fn kind(&self) -> KeyVaultKind {
        KeyVaultKind::Keychain
    }

    fn available(&self) -> bool {
        false
    }

    fn store(&self, _entry: &KeyEntry<'_>, _passphrase: &KeyPassphrase) -> Result<(), MmError> {
        Err(MmError::Environment {
            message: "no system keychain is used on this machine".to_owned(),
        })
    }

    fn fetch(&self, _entry: &KeyEntry<'_>) -> Result<Option<KeyPassphrase>, MmError> {
        Ok(None)
    }

    fn contains(&self, _entry: &KeyEntry<'_>) -> Result<bool, MmError> {
        Ok(false)
    }

    fn remove(&self, _entry: &KeyEntry<'_>) -> Result<(), MmError> {
        Ok(())
    }
}

/// Passphrases in `<key>.passphrase` beside the key: one line per key,
/// `<fingerprint> <passphrase>`, readable by the user alone.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileKeyVault;

impl FileKeyVault {
    fn file(entry: &KeyEntry<'_>) -> PathBuf {
        let mut name = entry
            .path
            .file_name()
            .map(std::ffi::OsStr::to_os_string)
            .unwrap_or_default();
        name.push(".passphrase");
        entry.path.with_file_name(name)
    }

    /// Every line of the file except the entry's own.
    fn others(entry: &KeyEntry<'_>) -> Result<(Vec<Zeroizing<String>>, bool), MmError> {
        let path = Self::file(entry);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => Zeroizing::new(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Vec::new(), false));
            }
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("reading {}", path.display()),
                    source,
                });
            }
        };
        let mut found = false;
        let others = text
            .lines()
            .filter(|line| {
                let own = line.split_once(' ').map(|(fingerprint, _)| fingerprint)
                    == Some(entry.fingerprint);
                found |= own;
                !own && !line.trim().is_empty()
            })
            .map(|line| Zeroizing::new(line.to_owned()))
            .collect();
        Ok((others, found))
    }

    fn write(entry: &KeyEntry<'_>, lines: &[Zeroizing<String>]) -> Result<(), MmError> {
        let path = Self::file(entry);
        let io = |source| MmError::Io {
            context: format!("writing {}", path.display()),
            source,
        };
        if lines.is_empty() {
            return match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(source) => Err(io(source)),
            };
        }
        let directory = path.parent().unwrap_or(Path::new("."));
        // A temporary file is created for the user alone, and replaces the
        // old file in one step.
        let mut file = tempfile::NamedTempFile::new_in(directory).map_err(io)?;
        for line in lines {
            file.write_all(line.as_bytes()).map_err(io)?;
            file.write_all(b"\n").map_err(io)?;
        }
        file.persist(&path).map_err(|error| io(error.error))?;
        Ok(())
    }
}

impl KeyVault for FileKeyVault {
    fn kind(&self) -> KeyVaultKind {
        KeyVaultKind::File
    }

    fn available(&self) -> bool {
        true
    }

    fn store(&self, entry: &KeyEntry<'_>, passphrase: &KeyPassphrase) -> Result<(), MmError> {
        let (mut lines, _) = Self::others(entry)?;
        lines.push(Zeroizing::new(format!(
            "{} {}",
            entry.fingerprint,
            passphrase.expose()
        )));
        Self::write(entry, &lines)
    }

    fn fetch(&self, entry: &KeyEntry<'_>) -> Result<Option<KeyPassphrase>, MmError> {
        let path = Self::file(entry);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => Zeroizing::new(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("reading {}", path.display()),
                    source,
                });
            }
        };
        text.lines()
            .find_map(|line| {
                let (fingerprint, passphrase) = line.split_once(' ')?;
                (fingerprint == entry.fingerprint).then_some(passphrase)
            })
            .map(|passphrase| {
                KeyPassphrase::from_vault(Zeroizing::new(passphrase.to_owned())).map_err(
                    |message| MmError::Secret {
                        message: format!("{}: {message}", path.display()),
                    },
                )
            })
            .transpose()
    }

    fn contains(&self, entry: &KeyEntry<'_>) -> Result<bool, MmError> {
        Self::others(entry).map(|(_, found)| found)
    }

    fn remove(&self, entry: &KeyEntry<'_>) -> Result<(), MmError> {
        let (lines, found) = Self::others(entry)?;
        if found {
            Self::write(entry, &lines)?;
        }
        Ok(())
    }
}

/// The freedesktop.org Secret Service, through libsecret's `secret-tool`.
/// The passphrase goes to it on standard input and comes back on standard
/// output; it is never an argument.
#[derive(Debug)]
pub struct SecretServiceVault {
    program: PathBuf,
    available: OnceLock<Availability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Availability {
    Answers,
    /// `secret-tool` ran and found no service to ask.
    NoService,
    /// There is no `secret-tool` to run.
    NoTool,
}

impl Default for SecretServiceVault {
    fn default() -> Self {
        Self::with_program("secret-tool")
    }
}

impl SecretServiceVault {
    /// With another `secret-tool`, for tests.
    #[must_use]
    pub fn with_program(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            available: OnceLock::new(),
        }
    }

    fn availability(&self) -> Availability {
        *self.available.get_or_init(|| {
            // A lookup that finds nothing ends with status 1 and says
            // nothing; with no service to ask, `secret-tool` complains.
            match run(
                self.command("lookup", "mm-probe"),
                None,
                Capture::Discard,
                PROBE_PATIENCE,
            ) {
                Ok(finished)
                    if finished.code == Some(0)
                        || (finished.code == Some(1) && finished.stderr.is_empty()) =>
                {
                    Availability::Answers
                }
                Ok(_) => Availability::NoService,
                Err(MmError::ProcessSpawn { source, .. })
                    if source.kind() == std::io::ErrorKind::NotFound =>
                {
                    Availability::NoTool
                }
                Err(error) => {
                    tracing::debug!(%error, "the Secret Service was not asked");
                    Availability::NoService
                }
            }
        })
    }

    fn command(&self, action: &str, entry_fingerprint: &str) -> Command {
        let mut command = Command::new(&self.program);
        command.arg(action);
        if action == "store" {
            command.arg("--label").arg("mm SSH key passphrase");
        }
        command.args(["service", SERVICE, "fingerprint", entry_fingerprint]);
        command
    }
}

impl KeyVault for SecretServiceVault {
    fn kind(&self) -> KeyVaultKind {
        KeyVaultKind::Keychain
    }

    fn available(&self) -> bool {
        self.availability() == Availability::Answers
    }

    fn remedy(&self) -> Option<String> {
        (self.availability() == Availability::NoTool).then(|| {
            "Install `secret-tool` (the package `libsecret-tools` on Debian and Ubuntu, `libsecret` on Fedora) so mm can reach the desktop keyring".to_owned()
        })
    }

    fn store(&self, entry: &KeyEntry<'_>, passphrase: &KeyPassphrase) -> Result<(), MmError> {
        let finished = run(
            self.command("store", entry.fingerprint),
            Some(passphrase.expose()),
            Capture::Discard,
            PATIENCE,
        )?;
        if finished.code == Some(0) {
            return Ok(());
        }
        Err(keychain_error("did not store the passphrase", &finished))
    }

    fn fetch(&self, entry: &KeyEntry<'_>) -> Result<Option<KeyPassphrase>, MmError> {
        let finished = run(
            self.command("lookup", entry.fingerprint),
            None,
            Capture::Keep,
            PATIENCE,
        )?;
        match finished.code {
            Some(0) if !finished.stdout.trim().is_empty() => {
                KeyPassphrase::from_vault(finished.stdout)
                    .map(Some)
                    .map_err(|message| MmError::Secret {
                        message: format!("the system keychain entry '{SERVICE}': {message}"),
                    })
            }
            Some(0 | 1) if finished.stderr.is_empty() => Ok(None),
            _ => Err(keychain_error("did not answer", &finished)),
        }
    }

    fn contains(&self, entry: &KeyEntry<'_>) -> Result<bool, MmError> {
        // Standard output is discarded: the passphrase never enters mm.
        match run(
            self.command("lookup", entry.fingerprint),
            None,
            Capture::Discard,
            PATIENCE,
        ) {
            Ok(finished) => Ok(finished.code == Some(0)),
            Err(error) => {
                tracing::debug!(%error, "the Secret Service was not asked");
                Ok(false)
            }
        }
    }

    fn remove(&self, entry: &KeyEntry<'_>) -> Result<(), MmError> {
        let finished = run(
            self.command("clear", entry.fingerprint),
            None,
            Capture::Discard,
            PATIENCE,
        )?;
        // Status 1 with nothing said: there was no such entry.
        if finished.code == Some(0) || (finished.code == Some(1) && finished.stderr.is_empty()) {
            return Ok(());
        }
        Err(keychain_error("did not remove the passphrase", &finished))
    }
}

/// The macOS login keychain, through `security`. Adding an item goes
/// through the tool's interactive mode, which reads the command from
/// standard input, so the passphrase is never an argument.
#[derive(Debug)]
pub struct MacKeychainVault {
    program: PathBuf,
    available: OnceLock<bool>,
}

/// `security` ends with this status when the keychain has no such item.
const MAC_NOT_FOUND: i32 = 44;

impl Default for MacKeychainVault {
    fn default() -> Self {
        Self::with_program("/usr/bin/security")
    }
}

impl MacKeychainVault {
    /// With another `security`, for tests.
    #[must_use]
    pub fn with_program(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            available: OnceLock::new(),
        }
    }

    fn command(&self, action: &str, fingerprint: &str) -> Command {
        let mut command = Command::new(&self.program);
        command.args([action, "-a", fingerprint, "-s", SERVICE]);
        command
    }
}

impl KeyVault for MacKeychainVault {
    fn kind(&self) -> KeyVaultKind {
        KeyVaultKind::Keychain
    }

    fn available(&self) -> bool {
        *self.available.get_or_init(|| {
            let mut command = Command::new(&self.program);
            command.arg("list-keychains");
            match run(command, None, Capture::Discard, PROBE_PATIENCE) {
                Ok(finished) => finished.code == Some(0),
                Err(error) => {
                    tracing::debug!(%error, "no keychain on this machine");
                    false
                }
            }
        })
    }

    fn store(&self, entry: &KeyEntry<'_>, passphrase: &KeyPassphrase) -> Result<(), MmError> {
        // A fingerprint and a passphrase hold no space and no quote, so
        // the line needs no quoting.
        if entry.fingerprint.contains(|character: char| {
            character.is_whitespace() || character == '"' || character == '\'' || character == '\\'
        }) {
            return Err(MmError::Security {
                message: "the key's fingerprint cannot name a keychain item".to_owned(),
            });
        }
        let line = Zeroizing::new(format!(
            "add-generic-password -U -a {} -s {SERVICE} -l mm-ssh-key-passphrase -w {}\n",
            entry.fingerprint,
            passphrase.expose()
        ));
        let mut command = Command::new(&self.program);
        command.arg("-i");
        let finished = run(command, Some(&line), Capture::Discard, PATIENCE)?;
        if finished.code == Some(0) {
            return Ok(());
        }
        Err(keychain_error("did not store the passphrase", &finished))
    }

    fn fetch(&self, entry: &KeyEntry<'_>) -> Result<Option<KeyPassphrase>, MmError> {
        let mut command = self.command("find-generic-password", entry.fingerprint);
        command.arg("-w");
        let finished = run(command, None, Capture::Keep, PATIENCE)?;
        match finished.code {
            Some(0) => KeyPassphrase::from_vault(finished.stdout)
                .map(Some)
                .map_err(|message| MmError::Secret {
                    message: format!("the keychain item '{SERVICE}': {message}"),
                }),
            Some(MAC_NOT_FOUND) => Ok(None),
            _ => Err(keychain_error("did not answer", &finished)),
        }
    }

    fn contains(&self, entry: &KeyEntry<'_>) -> Result<bool, MmError> {
        match run(
            self.command("find-generic-password", entry.fingerprint),
            None,
            Capture::Discard,
            PATIENCE,
        ) {
            Ok(finished) => Ok(finished.code == Some(0)),
            Err(error) => {
                tracing::debug!(%error, "the keychain was not asked");
                Ok(false)
            }
        }
    }

    fn remove(&self, entry: &KeyEntry<'_>) -> Result<(), MmError> {
        let finished = run(
            self.command("delete-generic-password", entry.fingerprint),
            None,
            Capture::Discard,
            PATIENCE,
        )?;
        match finished.code {
            Some(0 | MAC_NOT_FOUND) => Ok(()),
            _ => Err(keychain_error("did not remove the passphrase", &finished)),
        }
    }
}

fn keychain_error(problem: &str, finished: &Finished) -> MmError {
    let said = finished
        .stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| {
            line.chars()
                .filter(|character| !character.is_control())
                .take(200)
                .collect::<String>()
        });
    MmError::Secret {
        message: match said {
            Some(said) => format!(
                "the system keychain {problem}: {}",
                crate::domain::error::printed(&said)
            ),
            None => format!("the system keychain {problem}"),
        },
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    /// Standard output is read: it is the passphrase.
    Keep,
    /// Standard output goes nowhere.
    Discard,
}

struct Finished {
    code: Option<i32>,
    stdout: Zeroizing<String>,
    stderr: String,
}

/// Runs a keychain tool with `input` on its standard input, and ends it
/// when it takes longer than `patience`. Nothing it prints is logged.
fn run(
    mut command: Command,
    input: Option<&str>,
    capture: Capture,
    patience: Duration,
) -> Result<Finished, MmError> {
    let program = command.get_program().to_string_lossy().into_owned();
    let spawn_error = |source| MmError::ProcessSpawn {
        program: program.clone(),
        source,
    };
    let mut child = command
        .env("LC_ALL", "C")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(if capture == Capture::Keep {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(Stdio::piped())
        .spawn()
        .map_err(spawn_error)?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take())
        && let Err(error) = stdin.write_all(input.as_bytes())
    {
        tracing::debug!(%error, program, "a keychain tool did not read its input");
    }
    let deadline = Instant::now() + patience;
    let status = loop {
        match child.try_wait().map_err(spawn_error)? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                if let Err(error) = child.kill().and_then(|()| child.wait()) {
                    tracing::debug!(%error, program, "a keychain tool could not be ended");
                }
                return Err(MmError::ProcessTimeout {
                    program,
                    seconds: patience.as_secs(),
                });
            }
            None => std::thread::sleep(Duration::from_millis(2)),
        }
    };
    let mut stdout = Zeroizing::new(String::new());
    if let Some(mut pipe) = child.stdout.take()
        && let Err(error) = pipe.read_to_string(&mut stdout)
    {
        tracing::debug!(%error, program, "a keychain tool's answer was not text");
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take()
        && let Err(error) = pipe.read_to_string(&mut stderr)
    {
        tracing::debug!(%error, program, "a keychain tool's complaint was not text");
    }
    Ok(Finished {
        code: status.code(),
        stdout,
        stderr: stderr.trim().to_owned(),
    })
}
