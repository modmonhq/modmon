//! Files and engine commands of the keepers that run beside `mmd` on a
//! cluster node: each file is replaced whole, never half written, and the
//! engine setup installed on the node (`docker`, or `podman`) is asked
//! through its own command line.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use atomic_write_file::AtomicWriteFile;

use crate::app::ports::{CommandOutput, CommandSpec, ProcessRunner};
use crate::domain::error::MmError;
use crate::domain::runtime::RuntimeKind;

/// The largest file a keeper reads back.
const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;

pub(crate) fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// Replaces `path` with `bytes` in one step, readable as `mode` says.
pub(crate) fn write_whole(path: &Path, bytes: &[u8], mode: u32) -> Result<(), MmError> {
    use atomic_write_file::unix::OpenOptionsExt;
    use std::os::unix::fs::OpenOptionsExt as _;
    let io = |doing: &str, source| MmError::Io {
        context: format!("{doing} {}", path.display()),
        source,
    };
    let mut options = AtomicWriteFile::options();
    options
        .mode(mode)
        .preserve_mode(false)
        .preserve_owner(false);
    let mut file = options.open(path).map_err(|source| io("opening", source))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| io("writing", source))?;
    file.commit().map_err(|source| io("replacing", source))
}

/// The contents of `path`, or `None` when there is no such file.
pub(crate) fn read_whole(path: &Path) -> Result<Option<Vec<u8>>, MmError> {
    crate::domain::path_policy::read_regular_file(path, MAX_FILE_BYTES)
}

/// Creates `path` and its parents, owned by whoever runs `mmd`, with
/// `mode` on the last.
pub(crate) fn directory(path: &Path, mode: u32) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path)
        .and_then(|()| std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)))
        .map_err(|source| MmError::Io {
            context: format!("creating {}", path.display()),
            source,
        })
}

/// Removes `path`; a file that is not there is removed already.
pub(crate) fn remove(path: &Path) -> Result<bool, MmError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(MmError::Io {
            context: format!("removing {}", path.display()),
            source,
        }),
    }
}

/// The names of the files in `directory` that end in `suffix`, sorted. A
/// directory that is not there has none.
pub(crate) fn names(directory: &Path, suffix: &str) -> Result<Vec<String>, MmError> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(MmError::Io {
                context: format!("reading {}", directory.display()),
                source,
            });
        }
    };
    let mut names = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(suffix) && !name.starts_with('.'))
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

/// The command line of the engine `kind`, as on the node.
#[must_use]
pub(crate) const fn engine_program(kind: RuntimeKind) -> &'static str {
    match kind {
        RuntimeKind::DockerCompose => "docker",
        RuntimeKind::PodmanCompose => "podman",
    }
}

/// Runs the node's engine command line (`docker`, or `podman` on a node
/// setup made a Podman host) with `args`, with the engine's environment and
/// a deadline. The output is returned whatever the exit status.
pub(crate) async fn engine(
    process: &dyn ProcessRunner,
    kind: RuntimeKind,
    args: &[&str],
    timeout: Duration,
) -> Result<CommandOutput, MmError> {
    process
        .run(
            CommandSpec::new(engine_program(kind))
                .inherit_environment(kind.inherited_environment())
                .args(args.iter().copied())
                .timeout(timeout),
        )
        .await
}

/// Whether the engine answered that the container is not there: Docker
/// says "No such container", Podman "no such container".
pub(crate) fn no_such_container(output: &CommandOutput) -> bool {
    output
        .stderr
        .to_ascii_lowercase()
        .contains("no such container")
}

/// The engine's answer when a command failed, for an error message.
pub(crate) fn failure(output: &CommandOutput) -> String {
    let stderr = output.stderr.trim();
    if stderr.is_empty() {
        format!("exit {}", output.code.unwrap_or(-1))
    } else {
        crate::domain::error::printed(stderr)
    }
}
