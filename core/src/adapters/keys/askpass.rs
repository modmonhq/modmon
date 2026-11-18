//! Answering OpenSSH's passphrase prompts for a key mm manages.
//!
//! OpenSSH takes a passphrase from a terminal or from the program named in
//! `SSH_ASKPASS`. mm gives it a small script that reads the answer from a
//! named pipe, and writes the answer into the pipe when the script opens
//! it. So the passphrase is never an argument, never in an environment
//! variable and never in a file: it passes through a pipe in a directory
//! only the user can enter, once, to the process that asked. The script
//! is the same for every connection and holds nothing; the directory with
//! the pipes is made for one process and removed after it.
//!
//! Each prompt has its own pipe, and each pipe answers once. A second
//! question on the same pipe gets an empty answer, so a passphrase that
//! does not open the key fails at once instead of being offered again.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::domain::error::MmError;
use crate::domain::key::KeyPassphrase;

/// The passphrase that opens the key as it is.
const CURRENT: &str = "current";
/// The passphrase of the new key a rotation made beside it (`<key>.next`).
const NEXT: &str = "next";
/// The passphrase a key is being created or encrypted under.
const NEW: &str = "new";
/// The same again: `ssh-keygen` asks twice.
const AGAIN: &str = "again";
const SCRIPT: &str = "askpass";

/// How often the pipes are looked at while a prompt may come.
const POLL: Duration = Duration::from_millis(2);

/// The answers one OpenSSH process may ask for. They are served for as
/// long as this value lives.
pub struct Askpass {
    /// The pipes of this one process.
    directory: tempfile::TempDir,
    /// The script every helper under the same home shares.
    script: PathBuf,
    stop: Arc<AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for Askpass {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Askpass").finish_non_exhaustive()
    }
}

impl Askpass {
    /// Starts answering in a private directory under `parent`. `current`
    /// opens the key as it is; `new` is the passphrase it gets.
    pub(crate) fn serve(
        parent: &Path,
        current: Option<KeyPassphrase>,
        new: Option<KeyPassphrase>,
    ) -> Result<Self, MmError> {
        Self::serve_keys(parent, (current, None), new)
    }

    /// As `serve`, for a connection that offers the key and the new key a
    /// rotation made beside it: each prompt names its key's file, and gets
    /// that key's passphrase.
    pub(crate) fn serve_keys(
        parent: &Path,
        (current, next): (Option<KeyPassphrase>, Option<KeyPassphrase>),
        new: Option<KeyPassphrase>,
    ) -> Result<Self, MmError> {
        let directory = private_directory(parent)?;
        let pipes = [CURRENT, NEW, AGAIN, NEXT].map(|name| directory.path().join(name));
        make_pipes(&pipes)?;
        // In `parent` (the mm home's run directory, which only the user
        // can enter), where every connection finds it. When the private
        // directory had to go elsewhere, the script goes inside it: a
        // shared temporary directory is no place for a program mm runs.
        let beside = (directory.path().parent() == Some(parent)).then_some(parent);
        let script = beside
            .map_or_else(|| script_in(directory.path()), script_in)
            .or_else(|error| {
                tracing::debug!(%error, "the passphrase helper's script goes into its own directory");
                script_in(directory.path())
            })
            .map_err(|source| MmError::Io {
                context: "writing the passphrase helper".to_owned(),
                source,
            })?;
        let stop = Arc::new(AtomicBool::new(false));
        let answers = [
            (pipes[0].clone(), current),
            (pipes[1].clone(), new.clone()),
            (pipes[2].clone(), new),
            (pipes[3].clone(), next),
        ];
        let stopped = Arc::clone(&stop);
        let server = std::thread::Builder::new()
            .name("mm-askpass".to_owned())
            .spawn(move || answer(answers, &stopped))
            .map_err(|source| MmError::Io {
                context: "starting the passphrase helper".to_owned(),
                source,
            })?;
        Ok(Self {
            directory,
            script,
            stop,
            server: Some(server),
        })
    }

    /// The environment that makes OpenSSH ask this helper and never a
    /// terminal. `SSH_ASKPASS_REQUIRE` needs OpenSSH 8.4. None of it is
    /// secret: it names a script and a directory.
    pub(crate) fn environment(&self) -> [(&'static str, String); 3] {
        [
            ("SSH_ASKPASS", self.script.display().to_string()),
            ("SSH_ASKPASS_REQUIRE", "force".to_owned()),
            (
                "MM_ASKPASS_DIR",
                self.directory.path().display().to_string(),
            ),
        ]
    }
}

impl Drop for Askpass {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(server) = self.server.take()
            && server.join().is_err()
        {
            tracing::debug!("the passphrase helper ended abnormally");
        }
    }
}

/// A directory only the user can enter, under `parent` when that can be
/// created, else under the system's temporary directory.
fn private_directory(parent: &Path) -> Result<tempfile::TempDir, MmError> {
    let under = |parent: &Path| super::private_directory_in(parent, "askpass-");
    under(parent)
        .or_else(|error| {
            tracing::debug!(%error, parent = %parent.display(), "the passphrase helper uses the temporary directory");
            under(&std::env::temp_dir())
        })
        .map_err(|source| MmError::Io {
            context: "creating a private directory for the passphrase helper".to_owned(),
            source,
        })
}

fn make_pipes(pipes: &[PathBuf]) -> Result<(), MmError> {
    let output = Command::new("mkfifo")
        .args(["-m", "600"])
        .args(pipes)
        .stdin(Stdio::null())
        .output()
        .map_err(|source| MmError::ProcessSpawn {
            program: "mkfifo".to_owned(),
            source,
        })?;
    if output.status.success() {
        return Ok(());
    }
    Err(MmError::Environment {
        message: format!(
            "mkfifo failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    })
}

/// The program OpenSSH runs with the prompt as its argument. It holds no
/// secret: it copies the pipe that belongs to the prompt, from the
/// directory named in its environment.
const SCRIPT_TEXT: &str = "#!/bin/sh
# Answers OpenSSH's passphrase prompts for a key mm manages. mm writes each
# answer into a pipe under $MM_ASKPASS_DIR when it is asked for; this file
# holds no secret.
case \"$1\" in
  *\"same passphrase\"*) exec cat \"$MM_ASKPASS_DIR/again\" ;;
  *\"new passphrase\"*|*\"empty for no passphrase\"*) exec cat \"$MM_ASKPASS_DIR/new\" ;;
  *\".next'\"*) exec cat \"$MM_ASKPASS_DIR/next\" ;;
  *[Pp]assphrase*) exec cat \"$MM_ASKPASS_DIR/current\" ;;
esac
exit 1
";

/// The script in `directory`, written when it is not there or is not what
/// mm would write. It is written once and kept, not once per connection:
/// a file that was just written can still be open in a process that is
/// being started elsewhere in mm, and the system refuses to run a program
/// that is open for writing.
fn script_in(directory: &Path) -> std::io::Result<PathBuf> {
    let path = directory.join(SCRIPT);
    if std::fs::read(&path).is_ok_and(|existing| existing == SCRIPT_TEXT.as_bytes()) && runs(&path)
    {
        return Ok(path);
    }
    let mut builder = tempfile::Builder::new();
    builder.prefix(".askpass-");
    #[cfg(unix)]
    builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o700));
    let mut file = builder.tempfile_in(directory)?;
    file.write_all(SCRIPT_TEXT.as_bytes())?;
    // In one step, so no `ssh` ever runs half a script.
    file.persist(&path).map_err(|error| error.error)?;
    Ok(path)
}

#[cfg(unix)]
fn runs(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o700 == 0o700)
}

#[cfg(not(unix))]
fn runs(_path: &Path) -> bool {
    false
}

/// Answers each pipe once, when something opens it for reading, and gives
/// every later reader an empty answer. Runs until `stop`.
fn answer(mut answers: [(PathBuf, Option<KeyPassphrase>); 4], stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        let mut asked = false;
        for (pipe, passphrase) in &mut answers {
            let Some(mut writer) = open_for_reader(pipe) else {
                continue;
            };
            asked = true;
            // Taken, so the pipe answers once.
            if let Some(passphrase) = passphrase.take() {
                let line = zeroize::Zeroizing::new(format!("{}\n", passphrase.expose()));
                if let Err(error) = writer.write_all(line.as_bytes()) {
                    tracing::debug!(%error, "the passphrase helper could not answer");
                }
            }
        }
        if !asked {
            std::thread::sleep(POLL);
        }
    }
}

/// The write end of `pipe` when a reader has it open, without waiting for
/// one.
#[cfg(unix)]
fn open_for_reader(pipe: &Path) -> Option<std::fs::File> {
    use rustix::fs::{Mode, OFlags};

    match rustix::fs::open(
        pipe,
        OFlags::WRONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(descriptor) => Some(std::fs::File::from(descriptor)),
        // Nobody is reading: nothing was asked.
        Err(rustix::io::Errno::NXIO) => None,
        Err(error) => {
            tracing::debug!(%error, pipe = %pipe.display(), "the passphrase helper could not open its pipe");
            None
        }
    }
}

#[cfg(not(unix))]
fn open_for_reader(_pipe: &Path) -> Option<std::fs::File> {
    None
}
