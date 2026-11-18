//! `mmd state export` and `mmd state import`: a host's own records as one
//! tar archive, and back. What is in it, and what never is, is
//! `domain::node_state`'s; this writes and reads the files.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::{DaemonConfig, DeployedState};
use crate::domain::error::MmError;
use crate::domain::node_state::{
    HOST_POLICY, MANIFEST, NODE_ENV, STATE_FORMAT, STATE_PREFIX, StateManifest, is_archive_entry,
    is_exported_state,
};

/// The most an archive may hold in all: the audit log and its rotated
/// files, and some small records.
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
/// How deep under the state directory a record lies.
const MAX_DEPTH: usize = 4;

/// What an import did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StateImport {
    pub node: Option<String>,
    pub exported_at_unix_ms: i64,
    pub restored: Vec<String>,
    /// Entries left out, each with why.
    pub skipped: Vec<String>,
}

/// Writes the host's records to `out` and returns the manifest.
pub fn export_state(
    config: &DaemonConfig,
    out: impl Write,
    now_unix_ms: i64,
) -> Result<StateManifest, MmError> {
    let mut files = Vec::new();
    for (name, path) in [
        (HOST_POLICY, &config.host_policy_path),
        (NODE_ENV, &config.node_env_path),
    ] {
        if path.is_file() {
            files.push((name.to_owned(), path.clone()));
        }
    }
    let mut records = Vec::new();
    collect(&config.state_root, "", 0, &mut records)?;
    records.sort();
    files.extend(records.into_iter().map(|relative| {
        (
            format!("{STATE_PREFIX}{relative}"),
            config.state_root.join(relative),
        )
    }));
    let manifest = StateManifest {
        format: STATE_FORMAT,
        node: config.node_env()?.map(|node| node.name),
        mmd_version: env!("CARGO_PKG_VERSION").to_owned(),
        exported_at_unix_ms: now_unix_ms,
        files: files.iter().map(|(name, _)| name.clone()).collect(),
    };
    let mut archive = tar::Builder::new(out);
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| MmError::State {
        message: format!("serializing the state manifest: {error}"),
    })?;
    append(&mut archive, MANIFEST, &manifest_bytes, now_unix_ms)?;
    for (name, path) in &files {
        let bytes = fs::read(path).map_err(|source| io(path, "reading", source))?;
        append(&mut archive, name, &bytes, now_unix_ms)?;
    }
    archive
        .into_inner()
        .and_then(|mut out| out.flush())
        .map_err(|source| MmError::Io {
            context: "writing the state archive".to_owned(),
            source,
        })?;
    Ok(manifest)
}

/// The records under `directory`, by their path from the state directory.
/// Links are not followed and nothing else is read.
fn collect(
    directory: &Path,
    prefix: &str,
    depth: usize,
    found: &mut Vec<String>,
) -> Result<(), MmError> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io(directory, "listing", source)),
    };
    for entry in entries {
        let entry = entry.map_err(|source| io(directory, "listing", source))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let relative = format!("{prefix}{name}");
        let kind = entry
            .file_type()
            .map_err(|source| io(&entry.path(), "inspecting", source))?;
        if kind.is_dir() {
            collect(&entry.path(), &format!("{relative}/"), depth + 1, found)?;
        } else if kind.is_file() && is_exported_state(&relative) {
            found.push(relative);
        }
    }
    Ok(())
}

fn append(
    archive: &mut tar::Builder<impl Write>,
    name: &str,
    bytes: &[u8],
    now_unix_ms: i64,
) -> Result<(), MmError> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_mtime(u64::try_from(now_unix_ms / 1000).unwrap_or_default());
    header.set_entry_type(tar::EntryType::Regular);
    archive
        .append_data(&mut header, name, bytes)
        .map_err(|source| MmError::Io {
            context: format!("writing {name} to the state archive"),
            source,
        })
}

/// Reads an archive from `input` and puts its records in place. Refused,
/// with nothing written, while `mmd` answers on this host, for another
/// node's archive, and for an archive with anything an export never holds.
pub fn import_state(config: &DaemonConfig, input: impl Read) -> Result<StateImport, MmError> {
    if std::os::unix::net::UnixStream::connect(&config.socket_path).is_ok() {
        return Err(refused(
            "mmd is running on this host, and it reads these files while it runs. Stop it first (`systemctl stop mmd`), import, then start it (`systemctl start mmd`)",
        ));
    }
    let (manifest, entries) = read_archive(input)?;
    let here = config.node_env()?.map(|node| node.name);
    if let (Some(exported), Some(here)) = (&manifest.node, &here)
        && exported != here
    {
        return Err(refused(&format!(
            "this archive is of node '{exported}', and this host is node '{here}' (/etc/mm/node.env)"
        )));
    }
    if let Some((_, bytes)) = entries.iter().find(|(name, _)| name == HOST_POLICY) {
        check_policy(&config.host_policy_path, bytes)?;
    }
    let mut report = StateImport {
        node: manifest.node.clone(),
        exported_at_unix_ms: manifest.exported_at_unix_ms,
        restored: Vec::new(),
        skipped: Vec::new(),
    };
    for (name, bytes) in entries {
        if let Some(why) = skipped(&name, &bytes) {
            report.skipped.push(format!("{name}: {why}"));
            continue;
        }
        let (path, mode) = destination(config, &name);
        write_private(&path, &bytes, mode)?;
        report.restored.push(name);
    }
    Ok(report)
}

/// An archive entry: its name and its bytes.
type Entry = (String, Vec<u8>);

/// The manifest and every entry, checked before anything is written.
fn read_archive(input: impl Read) -> Result<(StateManifest, Vec<Entry>), MmError> {
    let mut archive = tar::Archive::new(input.take(MAX_ARCHIVE_BYTES + 1));
    let mut manifest = None::<StateManifest>;
    let mut entries = Vec::new();
    let reading = |source| MmError::Io {
        context: "reading the state archive from standard input".to_owned(),
        source,
    };
    for entry in archive.entries().map_err(reading)? {
        let mut entry = entry.map_err(reading)?;
        let name = entry
            .path()
            .map_err(reading)?
            .to_string_lossy()
            .into_owned();
        if entry.header().entry_type() != tar::EntryType::Regular {
            return Err(refused(&format!("'{name}' is not a plain file")));
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(reading)?;
        match &manifest {
            None if name == MANIFEST => {
                let read: StateManifest = serde_json::from_slice(&bytes)
                    .map_err(|error| refused(&format!("its manifest does not read: {error}")))?;
                if read.format != STATE_FORMAT {
                    return Err(refused(&format!(
                        "it is format {}, and this mmd reads format {STATE_FORMAT}",
                        read.format
                    )));
                }
                manifest = Some(read);
            }
            None => return Err(refused("it does not start with mm-state.json")),
            Some(_) if !is_archive_entry(&name) => {
                return Err(refused(&format!(
                    "it holds '{name}', which an export never does"
                )));
            }
            Some(_) => entries.push((name, bytes)),
        }
    }
    let manifest = manifest.ok_or_else(|| refused("it is empty"))?;
    Ok((manifest, entries))
}

/// A host policy that would not load is not put in place.
fn check_policy(path: &Path, bytes: &[u8]) -> Result<(), MmError> {
    let directory = path
        .parent()
        .ok_or_else(|| refused("the host policy path has no directory"))?;
    let trial = directory.join(format!(".host.yaml.import.{}", uuid::Uuid::new_v4()));
    write_private(&trial, bytes, 0o600)?;
    let checked = super::load_host_policy(&trial);
    if let Err(source) = fs::remove_file(&trial) {
        tracing::warn!(%source, "could not remove the trial copy of the host policy");
    }
    checked.map(|_| ()).map_err(|error| {
        refused(&format!(
            "the host policy in it does not load here: {error}"
        ))
    })
}

/// Why an entry is left out: a deployed record whose checkout this host
/// does not have would send every read of the environment to it.
fn skipped(name: &str, bytes: &[u8]) -> Option<String> {
    let deployed = name
        .strip_prefix(STATE_PREFIX)?
        .split('/')
        .nth(2)
        .is_some_and(|part| part == "deployed");
    if !deployed {
        return None;
    }
    match serde_json::from_slice::<DeployedState>(bytes) {
        Ok(state) if state.root.is_dir() => None,
        Ok(state) => Some(format!(
            "its checkout {} is not on this host; the next deployment writes the record again",
            state.root.display()
        )),
        Err(error) => Some(format!("it does not read: {error}")),
    }
}

fn destination(config: &DaemonConfig, name: &str) -> (PathBuf, u32) {
    match name {
        HOST_POLICY => (config.host_policy_path.clone(), 0o600),
        // Read by mm-mesh.service and the node's tools as well.
        NODE_ENV => (config.node_env_path.clone(), 0o644),
        _ => (
            config
                .state_root
                .join(name.strip_prefix(STATE_PREFIX).unwrap_or(name)),
            0o600,
        ),
    }
}

/// Writes `bytes` to `path` in one step, its directories root-only.
fn write_private(path: &Path, bytes: &[u8], mode: u32) -> Result<(), MmError> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

    let directory = path
        .parent()
        .ok_or_else(|| refused("a destination has no directory"))?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .map_err(|source| io(directory, "creating", source))?;
    let temporary = directory.join(format!(".mm-import.{}", uuid::Uuid::new_v4()));
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&temporary)
        .and_then(|mut file| {
            file.set_permissions(fs::Permissions::from_mode(mode))?;
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&temporary, path));
    if let Err(source) = written {
        if let Err(error) = fs::remove_file(&temporary) {
            tracing::debug!(%error, "no temporary file to remove");
        }
        return Err(io(path, "writing", source));
    }
    Ok(())
}

fn refused(why: &str) -> MmError {
    MmError::State {
        message: format!("mmd state import refused the archive: {why}; nothing was changed"),
    }
}

fn io(path: &Path, doing: &str, source: std::io::Error) -> MmError {
    MmError::Io {
        context: format!("{doing} {}", path.display()),
        source,
    }
}
