//! Commits a client pushed to this host: receiving a bundle, checking that
//! it is the commit it claims to be, and keeping it as the commit's
//! checkout. See [`crate::domain::delivery`] for what a bundle is.
//!
//! A bundle is written to a file under `projects/<id>/incoming` as it
//! arrives, never held in memory, and is refused once it is larger than
//! the host takes. An upload that stops half way leaves that one file,
//! which is removed when the receiver is dropped; a file a killed `mmd`
//! left behind is removed by the next delivery.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use base64::Engine;
use serde::{Deserialize, Serialize};

use super::git::{CHECKOUT_MARKER, ProjectRepositories};
use super::git_tree::{self, FileMode, ObjectId};
use crate::domain::delivery::{
    BundleOffer, Delivered, MAX_BUNDLE_FILES, MAX_COMMIT_OBJECT_BYTES, human_size, withheld,
};
use crate::domain::error::MmError;
use crate::domain::workspace::ProjectId;

/// A partly received bundle older than this belongs to no upload that
/// still runs: a delivery's own deadline is shorter.
const ABANDONED_AFTER: Duration = Duration::from_secs(2 * 60 * 60);

/// What a host keeps about the bundle a commit's checkout was made from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BundleRecord {
    pub commit: String,
    /// BLAKE3 of the bundle's bytes.
    pub bundle_hash: String,
    pub bytes: u64,
    pub files: u64,
    /// The origin's branches the delivering client said contain the commit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<String>,
    pub delivered_by: String,
    pub delivered_at_unix_ms: i64,
}

impl BundleRecord {
    #[must_use]
    pub fn delivered(&self, received: bool) -> Delivered {
        Delivered {
            commit: self.commit.clone(),
            bundle_hash: self.bundle_hash.clone(),
            bytes: self.bytes,
            files: self.files,
            received,
        }
    }
}

/// Who delivers a bundle, and where it goes.
#[derive(Debug, Clone)]
pub struct DeliveryScope<'a> {
    pub project: ProjectId,
    pub environment: &'a str,
    pub principal: &'a str,
    pub now_unix_ms: i64,
}

impl ProjectRepositories {
    fn record_path(&self, project: ProjectId, environment: &str, commit: &str) -> PathBuf {
        self.project_state(project)
            .join("bundles")
            .join(environment)
            .join(format!("{commit}.json"))
    }

    fn incoming(&self, project: ProjectId) -> PathBuf {
        self.project_state(project).join("incoming")
    }

    /// The bundle this host holds for `commit`, when its checkout is there.
    pub fn delivered(
        &self,
        project: ProjectId,
        environment: &str,
        commit: &str,
    ) -> Result<Option<BundleRecord>, MmError> {
        let path = self.record_path(project, environment, commit);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(io("reading the bundle record", &path, source)),
        };
        let record: BundleRecord =
            serde_json::from_slice(&bytes).map_err(|error| MmError::State {
                message: format!(
                    "the bundle record {} is not readable: {error}",
                    path.display()
                ),
            })?;
        let checkout = self.checkout_path(project, environment, commit);
        if record.commit != commit || !holds_commit(&checkout, commit)? {
            return Ok(None);
        }
        Ok(Some(record))
    }

    /// The checkout of a commit that was pushed here, or what to run when
    /// it was not.
    pub fn pushed_checkout(
        &self,
        project: ProjectId,
        environment: &str,
        commit: &str,
    ) -> Result<(PathBuf, BundleRecord), MmError> {
        if !crate::domain::remote::is_commit_sha(commit) {
            return Err(MmError::Environment {
                message: format!("'{commit}' is not a full 40-character commit SHA"),
            });
        }
        let record = self.delivered(project, environment, commit)?.ok_or_else(|| {
            MmError::Environment {
                message: format!(
                    "commit {commit} was not delivered to this host, which takes this project's files from the client and fetches nothing. `mm deploy plan --environment {environment}` sends it; with an mm that does not, set `delivery: push` on the target in mm.yaml"
                ),
            }
        })?;
        Ok((self.checkout_path(project, environment, commit), record))
    }

    /// Keeps the branches a client states for a commit the host holds.
    pub fn state_branches(
        &self,
        scope: &DeliveryScope<'_>,
        mut record: BundleRecord,
        branches: &[String],
    ) -> Result<BundleRecord, MmError> {
        if branches.is_empty() || record.branches == branches {
            return Ok(record);
        }
        record.branches = branches.to_vec();
        self.write_record(scope.project, scope.environment, &record)?;
        Ok(record)
    }

    fn write_record(
        &self,
        project: ProjectId,
        environment: &str,
        record: &BundleRecord,
    ) -> Result<(), MmError> {
        let path = self.record_path(project, environment, &record.commit);
        let parent = path.parent().ok_or_else(|| MmError::State {
            message: "the bundle record has no directory".to_owned(),
        })?;
        std::fs::create_dir_all(parent)
            .map_err(|source| io("creating the bundle records of", parent, source))?;
        let bytes = serde_json::to_vec_pretty(record).map_err(|error| MmError::State {
            message: format!("could not write a bundle record: {error}"),
        })?;
        let staged = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&staged, bytes)
            .and_then(|()| std::fs::rename(&staged, &path))
            .map_err(|source| io("writing the bundle record", &path, source))
    }

    /// Starts receiving the bundle `offer` describes. Nothing of the
    /// project changes before [`BundleReceiver::finish`].
    pub fn receive(
        &self,
        scope: &DeliveryScope<'_>,
        offer: &BundleOffer,
    ) -> Result<BundleReceiver, MmError> {
        offer
            .validate()
            .map_err(|message| MmError::Security { message })?;
        let offered = offer.bundle.as_ref().ok_or_else(|| MmError::State {
            message: "a delivery without a bundle has nothing to receive".to_owned(),
        })?;
        if offered.bytes > self.bundle_limit {
            return Err(MmError::Environment {
                message: format!(
                    "the files of commit {} are {} and this host takes a bundle of at most {}. Keep large files out of the repository (an image, a volume, or object storage carries them), or deploy this target with the default delivery, which fetches",
                    offer.commit,
                    human_size(offered.bytes),
                    human_size(self.bundle_limit)
                ),
            });
        }
        let tree = stated_tree(&offer.commit, &offered.commit_object)?;
        let incoming = self.incoming(scope.project);
        std::fs::create_dir_all(&incoming)
            .map_err(|source| io("creating the directory for bundles of", &incoming, source))?;
        remove_abandoned(&incoming);
        let path = incoming.join(format!("{}.tar", uuid::Uuid::new_v4()));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|source| io("creating the bundle file", &path, source))?;
        Ok(BundleReceiver {
            repositories: self.clone(),
            project: scope.project,
            environment: scope.environment.to_owned(),
            record: BundleRecord {
                commit: offer.commit.clone(),
                bundle_hash: offered.hash.clone(),
                bytes: offered.bytes,
                files: 0,
                branches: offer.branches.clone(),
                delivered_by: scope.principal.to_owned(),
                delivered_at_unix_ms: scope.now_unix_ms,
            },
            tree,
            path,
            file: Some(file),
            hasher: blake3::Hasher::new(),
            received: 0,
        })
    }
}

/// A bundle on its way in. Dropping it before `finish` removes what
/// arrived; nothing else was touched.
pub struct BundleReceiver {
    repositories: ProjectRepositories,
    project: ProjectId,
    environment: String,
    record: BundleRecord,
    /// The tree the commit object names.
    tree: String,
    path: PathBuf,
    file: Option<std::fs::File>,
    hasher: blake3::Hasher,
    received: u64,
}

impl Drop for BundleReceiver {
    fn drop(&mut self) {
        self.file = None;
        if let Err(error) = std::fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, path = %self.path.display(), "could not remove a bundle file");
        }
    }
}

impl BundleReceiver {
    /// How many bytes of the bundle are still to come.
    #[must_use]
    pub const fn remaining(&self) -> u64 {
        self.record.bytes.saturating_sub(self.received)
    }

    /// Appends the next bytes. More than the offer stated is refused.
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), MmError> {
        if bytes.len() as u64 > self.remaining() {
            return Err(MmError::Security {
                message: format!(
                    "the client sent more than the {} it stated for the bundle; nothing was kept",
                    human_size(self.record.bytes)
                ),
            });
        }
        let file = self.file.as_mut().ok_or_else(|| MmError::State {
            message: "the bundle file is closed".to_owned(),
        })?;
        file.write_all(bytes)
            .map_err(|source| io("writing the bundle file", &self.path, source))?;
        self.hasher.update(bytes);
        self.received = self.received.saturating_add(bytes.len() as u64);
        Ok(())
    }

    /// Checks the bundle and keeps it as the commit's checkout. This reads
    /// and writes every file of the commit: call it off the request
    /// threads.
    pub fn finish(mut self) -> Result<Delivered, MmError> {
        if self.remaining() != 0 {
            return Err(MmError::Security {
                message: format!(
                    "the bundle ended after {} of {}; nothing was kept",
                    human_size(self.received),
                    human_size(self.record.bytes)
                ),
            });
        }
        if let Some(file) = self.file.take() {
            file.sync_all()
                .map_err(|source| io("writing the bundle file", &self.path, source))?;
        }
        let hash = self.hasher.finalize().to_hex().to_string();
        if hash != self.record.bundle_hash {
            return Err(MmError::Security {
                message: format!(
                    "the bundle that arrived hashes to {hash}, and the client stated {}; nothing was kept. Run the command again",
                    self.record.bundle_hash
                ),
            });
        }
        let commit = self.record.commit.clone();
        let target = self
            .repositories
            .checkout_path(self.project, &self.environment, &commit);
        let parent = target.parent().ok_or_else(|| MmError::State {
            message: "checkout has no parent directory".to_owned(),
        })?;
        std::fs::create_dir_all(parent)
            .map_err(|source| io("creating the checkout directory", parent, source))?;
        let staging = parent.join(format!(".staging-{}", uuid::Uuid::new_v4()));
        let kept = self.unpack_verified(&staging).and_then(|files| {
            self.record.files = files;
            publish(&staging, &target, &commit)
        });
        if let Err(cleanup) = std::fs::remove_dir_all(&staging)
            && cleanup.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%cleanup, path = %staging.display(), "could not remove checkout staging");
        }
        kept?;
        self.repositories
            .write_record(self.project, &self.environment, &self.record)?;
        Ok(self.record.delivered(true))
    }

    /// Unpacks the bundle into `staging` and checks that its files are the
    /// tree of the commit. Returns how many files it holds.
    fn unpack_verified(&self, staging: &Path) -> Result<u64, MmError> {
        std::fs::create_dir(staging)
            .map_err(|source| io("creating the checkout staging directory", staging, source))?;
        let refused = |reason: String| MmError::Security {
            message: format!(
                "the bundle for commit {} was refused: {reason}; nothing was kept",
                self.record.commit
            ),
        };
        let file = std::fs::File::open(&self.path)
            .map_err(|source| io("reading the bundle file", &self.path, source))?;
        let files = unpack(file, staging).map_err(&refused)?;
        let tree = git_tree::tree_id(&files).map_err(&refused)?;
        if git_tree::hex(&tree) != self.tree {
            return Err(refused(format!(
                "its files are not the files of that commit (they hash to tree {}, the commit's tree is {})",
                git_tree::hex(&tree),
                self.tree
            )));
        }
        Ok(files.len() as u64)
    }
}

/// The tree `commit`'s object names, once the object hashes to `commit`.
fn stated_tree(commit: &str, commit_object: &str) -> Result<String, MmError> {
    let refused = |reason: &str| MmError::Security {
        message: format!("the delivery of commit {commit} was refused: {reason}"),
    };
    let object = base64::engine::general_purpose::STANDARD
        .decode(commit_object)
        .map_err(|_| refused("its commit object is not base64"))?;
    if object.len() > MAX_COMMIT_OBJECT_BYTES {
        return Err(refused("its commit object is too large"));
    }
    match git_tree::commit_tree(&object) {
        Some((named, tree)) if named == commit => Ok(tree),
        Some(_) => Err(refused(
            "the commit object the client sent is another commit's",
        )),
        None => Err(refused("what the client sent is not a commit object")),
    }
}

/// Unpacks a bundle: regular files and symbolic links only, each inside
/// `destination`, with ownership and set-id bits never applied. Returns
/// every file with its mode and the blob its content on disk is.
fn unpack(
    reader: impl std::io::Read,
    destination: &Path,
) -> Result<Vec<(String, FileMode, ObjectId)>, String> {
    let mut archive = tar::Archive::new(reader);
    archive.set_preserve_permissions(false);
    archive.set_preserve_ownerships(false);
    archive.set_unpack_xattrs(false);
    archive.set_overwrite(false);
    let mut files = Vec::new();
    let entries = archive
        .entries()
        .map_err(|error| format!("it is not a tar archive ({error})"))?;
    for entry in entries {
        let mut entry = entry.map_err(|error| format!("it is not a tar archive ({error})"))?;
        let path = entry
            .path()
            .map_err(|error| format!("an entry has no path ({error})"))?
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| "a file name is not UTF-8".to_owned())?;
        if let Some(reason) = withheld(&path) {
            return Err(format!("it carries '{path}', {reason}"));
        }
        if files.len() as u64 >= MAX_BUNDLE_FILES {
            return Err(format!("it holds more than {MAX_BUNDLE_FILES} files"));
        }
        let kind = entry.header().entry_type();
        let mode = if kind.is_file() {
            let executable = entry.header().mode().is_ok_and(|mode| mode & 0o111 != 0);
            if executable {
                FileMode::Executable
            } else {
                FileMode::Regular
            }
        } else if kind.is_symlink() {
            FileMode::Symlink
        } else {
            return Err(format!("'{path}' is neither a file nor a symbolic link"));
        };
        let unpacked = entry
            .unpack_in(destination)
            .map_err(|error| format!("'{path}' could not be unpacked ({error})"))?;
        if !unpacked {
            return Err(format!("'{path}' is not a path inside the project"));
        }
        let written = destination.join(&path);
        let id = if mode == FileMode::Symlink {
            let link = std::fs::read_link(&written)
                .map_err(|error| format!("'{path}' could not be read back ({error})"))?;
            git_tree::blob_id(link.as_os_str().as_encoded_bytes())
        } else {
            git_tree::file_blob_id(&written)
                .map_err(|error| format!("'{path}' could not be read back ({error})"))?
        };
        files.push((path, mode, id));
    }
    Ok(files)
}

/// Makes `staging` the checkout of `commit`, as a fetch does: without the
/// checkout's local overrides, with the marker file. A checkout that is
/// already there stays as it is; containers may have its files mounted.
fn publish(staging: &Path, target: &Path, commit: &str) -> Result<(), MmError> {
    if holds_commit(target, commit)? {
        return Ok(());
    }
    let overrides = staging.join(crate::adapters::config::OVERRIDES_RELATIVE_PATH);
    match std::fs::remove_file(&overrides) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => return Err(io("removing committed local overrides", &overrides, source)),
    }
    std::fs::write(staging.join(CHECKOUT_MARKER), format!("{commit}\n"))
        .map_err(|source| io("writing the checkout marker", staging, source))?;
    std::fs::rename(staging, target).map_err(|source| io("publishing the checkout", target, source))
}

/// Whether `checkout` is the checkout of `commit`. One of another commit
/// at the same place is refused.
fn holds_commit(checkout: &Path, commit: &str) -> Result<bool, MmError> {
    let marker = checkout.join(CHECKOUT_MARKER);
    match std::fs::read_to_string(&marker) {
        Ok(recorded) if recorded.trim() == commit => Ok(true),
        Ok(_) => Err(MmError::Security {
            message: format!("{} belongs to another commit", checkout.display()),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(io("reading the checkout marker", &marker, source)),
    }
}

/// Removes bundle files no upload is writing any more.
fn remove_abandoned(incoming: &Path) {
    let Ok(entries) = std::fs::read_dir(incoming) else {
        return;
    };
    for entry in entries.flatten() {
        let abandoned = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| {
                SystemTime::now()
                    .duration_since(modified)
                    .is_ok_and(|age| age > ABANDONED_AFTER)
            });
        if abandoned && let Err(error) = std::fs::remove_file(entry.path()) {
            tracing::warn!(%error, path = %entry.path().display(), "could not remove an abandoned bundle file");
        }
    }
}

fn io(context: &str, path: &Path, source: std::io::Error) -> MmError {
    MmError::Io {
        context: format!("{context} {}", path.display()),
        source,
    }
}
