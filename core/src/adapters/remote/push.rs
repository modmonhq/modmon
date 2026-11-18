//! Building a commit's bundle on the client, from the repository mm runs
//! in. See [`crate::domain::delivery`] for what a bundle is.
//!
//! The files come out of git's object store, never out of the working
//! tree: a bundle holds exactly what the commit tracks, byte for byte as
//! the commit stores it. A file git does not track at that commit (a
//! `.env`, a build output, anything ignored or not yet added) cannot get
//! in, and neither can a change that is not committed.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;

use super::git_tree;
use crate::app::ports::{CommitBundles, PreparedBundle};
use crate::domain::delivery::{
    BundleOffer, MAX_BUNDLE_BYTES, MAX_BUNDLE_FILES, MAX_COMMIT_OBJECT_BYTES, OfferedBundle,
    human_size, withheld, withheld_help,
};
use crate::domain::error::MmError;

/// The longest target a tracked symbolic link may have.
const MAX_LINK_BYTES: u64 = 4096;

/// Bundles of the commits in one repository.
#[derive(Debug, Clone)]
pub struct GitCommitBundles {
    root: PathBuf,
}

impl GitCommitBundles {
    /// `root` is the project's root, which is the repository's.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Builds the bundle of `commit` into a private temporary file.
    pub fn build(&self, commit: &str) -> Result<PreparedBundle, MmError> {
        if !crate::domain::remote::is_commit_sha(commit) {
            return Err(MmError::Environment {
                message: format!("'{commit}' is not a full 40-character commit SHA"),
            });
        }
        let branches = self.origin_branches(commit)?;
        let Some(commit_object) = self.commit_object(commit)? else {
            // A rollback from a clone that does not reach back that far:
            // the hosts answer from what they hold.
            return Ok(PreparedBundle {
                offer: BundleOffer {
                    commit: commit.to_owned(),
                    branches,
                    bundle: None,
                },
                files: 0,
                path: None,
                keep: None,
            });
        };
        let entries = self.tracked(commit)?;
        let file = tempfile::Builder::new()
            .prefix("mm-bundle-")
            .suffix(".tar")
            .tempfile()
            .map_err(|source| MmError::Io {
                context: "creating a temporary file for the bundle".to_owned(),
                source,
            })?;
        let (hash, bytes) = self.archive(&entries, file.as_file())?;
        let path = file.into_temp_path();
        Ok(PreparedBundle {
            offer: BundleOffer {
                commit: commit.to_owned(),
                branches,
                bundle: Some(OfferedBundle {
                    hash,
                    bytes,
                    commit_object: base64::engine::general_purpose::STANDARD.encode(commit_object),
                }),
            },
            files: entries.len() as u64,
            path: Some(path.to_path_buf()),
            keep: Some(Arc::new(path)),
        })
    }

    fn git(&self) -> Command {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(&self.root)
            .env("LC_ALL", "C")
            .stdin(Stdio::null());
        command
    }

    fn output(&self, arguments: &[&str]) -> Result<std::process::Output, MmError> {
        self.git()
            .args(arguments)
            .output()
            .map_err(|source| MmError::ProcessSpawn {
                program: "git".to_owned(),
                source,
            })
    }

    /// The commit's object, or `None` when this repository does not hold
    /// the commit.
    fn commit_object(&self, commit: &str) -> Result<Option<Vec<u8>>, MmError> {
        let output = self.output(&["cat-file", "commit", commit])?;
        if !output.status.success() {
            return Ok(None);
        }
        let object = output.stdout;
        let named = git_tree::commit_tree(&object).map(|(named, _)| named);
        if object.len() > MAX_COMMIT_OBJECT_BYTES || named.as_deref() != Some(commit) {
            return Err(MmError::Environment {
                message: format!(
                    "commit {commit} cannot be delivered by push: its object is larger than {} KiB or is not named by its SHA-1 (a SHA-256 repository). Deploy this target with the default delivery, which fetches",
                    MAX_COMMIT_OBJECT_BYTES / 1024
                ),
            });
        }
        Ok(Some(object))
    }

    /// The origin's branches that contain the commit, as this repository
    /// last saw them.
    fn origin_branches(&self, commit: &str) -> Result<Vec<String>, MmError> {
        let output = self.output(&[
            "branch",
            "--remotes",
            "--contains",
            commit,
            "--format=%(refname)",
        ])?;
        if !output.status.success() {
            return Ok(Vec::new());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.trim().strip_prefix("refs/remotes/origin/"))
            .filter(|branch| *branch != "HEAD")
            .map(str::to_owned)
            .collect())
    }

    /// Every file the commit tracks, in git's order.
    fn tracked(&self, commit: &str) -> Result<Vec<Tracked>, MmError> {
        let output = self.output(&["ls-tree", "-r", "-z", "-l", "--full-tree", commit])?;
        if !output.status.success() {
            return Err(MmError::RuntimeResponse {
                message: format!(
                    "git ls-tree failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }
        let refused = |message: String| MmError::Environment { message };
        let mut entries = Vec::new();
        for line in output.stdout.split(|byte| *byte == 0) {
            if line.is_empty() {
                continue;
            }
            let entry = Tracked::parse(line).ok_or_else(|| MmError::RuntimeResponse {
                message: "git ls-tree printed a line mm does not understand".to_owned(),
            })?;
            if entry.mode == "160000" {
                return Err(refused(
                    "git submodules are not supported for remote deployments".to_owned(),
                ));
            }
            if let Some(reason) = withheld(&entry.path) {
                return Err(refused(withheld_help(&entry.path, reason)));
            }
            entries.push(entry);
        }
        if let Some(attributes) = entries.iter().find(|entry| entry.path == ".gitattributes") {
            let content = self.output(&["cat-file", "blob", &attributes.object])?;
            if String::from_utf8_lossy(&content.stdout).contains("filter=lfs") {
                return Err(refused(
                    "Git LFS files are not supported for remote deployments".to_owned(),
                ));
            }
        }
        let total = entries.iter().map(|entry| entry.size).sum::<u64>();
        if total > MAX_BUNDLE_BYTES || entries.len() as u64 > MAX_BUNDLE_FILES {
            let mut largest = entries.iter().collect::<Vec<_>>();
            largest.sort_by_key(|entry| std::cmp::Reverse(entry.size));
            let largest = largest
                .iter()
                .take(3)
                .map(|entry| format!("{} ({})", entry.path, human_size(entry.size)))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(refused(format!(
                "commit {commit} tracks {} files of {} together, and a push delivery carries at most {MAX_BUNDLE_FILES} files and {}. The largest: {largest}. Keep large files out of the repository, or deploy this target with the default delivery, which fetches",
                entries.len(),
                human_size(total),
                human_size(MAX_BUNDLE_BYTES)
            )));
        }
        Ok(entries)
    }

    /// Writes the entries' blobs as a tar archive: one `git cat-file`
    /// answers for every object, and each file goes from its pipe into the
    /// archive without being held in memory. Returns the archive's BLAKE3
    /// and size.
    fn archive(&self, entries: &[Tracked], file: &std::fs::File) -> Result<(String, u64), MmError> {
        let failed = |source: std::io::Error| MmError::Io {
            context: "building the bundle".to_owned(),
            source,
        };
        let mut child = self
            .git()
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| MmError::ProcessSpawn {
                program: "git".to_owned(),
                source,
            })?;
        let pipes = child.stdin.take().zip(child.stdout.take());
        let (mut stdin, stdout) = pipes.ok_or_else(|| MmError::State {
            message: "git cat-file started without its pipes".to_owned(),
        })?;
        let objects = entries
            .iter()
            .map(|entry| format!("{}\n", entry.object))
            .collect::<String>();
        // Asked from a thread of its own: git answers while it is asked,
        // and neither side may wait for the other with a full pipe.
        let asking = std::thread::spawn(move || stdin.write_all(objects.as_bytes()));
        let mut objects = BufReader::new(stdout);
        let mut builder = tar::Builder::new(Hashing::new(file));
        builder.mode(tar::HeaderMode::Deterministic);
        let written = entries
            .iter()
            .try_for_each(|entry| append(&mut builder, &mut objects, entry));
        let finished = written.and_then(|()| builder.into_inner());
        drop(objects);
        if let Err(error) = child.wait() {
            tracing::debug!(%error, "git cat-file did not end cleanly");
        }
        if asking.join().is_err() {
            return Err(MmError::State {
                message: "asking git for the commit's files stopped".to_owned(),
            });
        }
        let mut hashing = finished.map_err(failed)?;
        hashing.flush().map_err(failed)?;
        Ok((
            hashing.hasher.finalize().to_hex().to_string(),
            hashing.written,
        ))
    }
}

#[async_trait]
impl CommitBundles for GitCommitBundles {
    async fn bundle(&self, commit: &str) -> Result<PreparedBundle, MmError> {
        let (bundles, commit) = (self.clone(), commit.to_owned());
        tokio::task::spawn_blocking(move || bundles.build(&commit))
            .await
            .map_err(|error| MmError::State {
                message: format!("building the bundle stopped: {error}"),
            })?
    }
}

/// One file of a commit, as `git ls-tree -l` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tracked {
    mode: String,
    object: String,
    size: u64,
    path: String,
}

impl Tracked {
    /// `<mode> SP <type> SP <object> SP+ <size> TAB <path>`.
    fn parse(line: &[u8]) -> Option<Self> {
        let tab = line.iter().position(|byte| *byte == b'\t')?;
        let Ok(facts) = std::str::from_utf8(&line[..tab]) else {
            return None;
        };
        let Ok(path) = std::str::from_utf8(&line[tab + 1..]) else {
            return None;
        };
        let mut facts = facts.split_ascii_whitespace();
        let (mode, _kind, object, size) =
            (facts.next()?, facts.next()?, facts.next()?, facts.next()?);
        Some(Self {
            mode: mode.to_owned(),
            object: object.to_owned(),
            // A submodule has no size; it is refused by its mode.
            size: size.parse().unwrap_or_default(),
            path: path.to_owned(),
        })
    }
}

/// Appends one object from `git cat-file --batch` to the archive.
fn append(
    builder: &mut tar::Builder<Hashing<'_>>,
    objects: &mut BufReader<std::process::ChildStdout>,
    entry: &Tracked,
) -> std::io::Result<()> {
    let wrong = |what: &str| std::io::Error::other(format!("git answered '{}' {what}", entry.path));
    let mut answer = String::new();
    objects.read_line(&mut answer)?;
    let mut facts = answer.split_ascii_whitespace();
    let (object, kind, size) = (facts.next(), facts.next(), facts.next());
    let size = size.and_then(|size| size.parse::<u64>().ok());
    if object != Some(entry.object.as_str()) || kind != Some("blob") || size != Some(entry.size) {
        return Err(wrong("with another object"));
    }
    let mut header = tar::Header::new_gnu();
    let mut content = objects.by_ref().take(entry.size);
    if entry.mode == "120000" {
        if entry.size > MAX_LINK_BYTES {
            return Err(wrong("as a link with a target that is too long"));
        }
        let mut target = Vec::new();
        content.read_to_end(&mut target)?;
        let target = String::from_utf8(target).map_err(|_| wrong("as a link that is not UTF-8"))?;
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        builder.append_link(&mut header, &entry.path, target)?;
    } else {
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(entry.size);
        header.set_mode(if entry.mode == "100755" { 0o755 } else { 0o644 });
        builder.append_data(&mut header, &entry.path, &mut content)?;
        if content.limit() != 0 {
            return Err(wrong("with fewer bytes than it stated"));
        }
    }
    // Each object is followed by a line feed.
    let mut end = [0_u8; 1];
    objects.read_exact(&mut end)?;
    Ok(())
}

/// A writer that hashes and counts what goes through it.
struct Hashing<'a> {
    file: &'a std::fs::File,
    hasher: blake3::Hasher,
    written: u64,
}

impl<'a> Hashing<'a> {
    fn new(file: &'a std::fs::File) -> Self {
        Self {
            file,
            hasher: blake3::Hasher::new(),
            written: 0,
        }
    }
}

impl Write for Hashing<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = self.file.write(bytes)?;
        self.hasher.update(&bytes[..count]);
        self.written = self.written.saturating_add(count as u64);
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
