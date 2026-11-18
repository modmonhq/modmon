//! Shared storage on `mmd`'s own host.
//!
//! The storage node keeps every shared volume as a directory below the
//! storage root, which setup exports over NFSv4.2 on the node's mesh
//! address. This adapter creates those directories, says what is there and
//! what the host has mounted, removes one directory on the one explicit
//! command, and loads the packet filter rules that keep storage traffic
//! between the hosts.
//!
//! Everything mm adds to the packet filter for storage is in the chains
//! `MM-STORAGE` and `MM-STORAGE-IN` and the two rules that enter them. The
//! first is entered where the host's engine lets the host's own rules for
//! forwarded traffic in ([`forward_hook`]).

use std::net::Ipv4Addr;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use zeroize::Zeroizing;

use crate::app::ports::{
    CommandOutput, CommandSpec, MeshHost, ProcessRunner, SharedStorageHost, StoredVolumes,
};
use crate::domain::cluster_network::{ForwardHook, forward_hook};
use crate::domain::error::MmError;
use crate::domain::mesh::MeshApplied;
use crate::domain::shared_storage::{
    NFS_PORT, STORAGE_FORWARD_CHAIN, STORAGE_INPUT_CHAIN, STORAGE_REACH_TIMEOUT_MS, STORAGE_ROOT,
    StorageFirewall, StoredVolume, VolumeName, environment_directory, mounted_volumes,
};
use crate::domain::workspace::ProjectId;

mod keeper;
pub use keeper::StorageKeeper;

const TIMEOUT: Duration = Duration::from_secs(30);
/// `iptables` lives in an sbin directory, which the default search path of a
/// process started without an environment does not include.
const SEARCH_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// How long counting the files of an environment's volumes may take for
/// one status. A volume that is not done by then reports what was counted.
const COUNT_BUDGET: Duration = Duration::from_secs(3);
/// What a directory mm creates for a volume is: readable and searchable by
/// every user of a container, writable by root, like a new engine volume.
const DIRECTORY_MODE: u32 = 0o755;

/// Where the host keeps what this adapter reads and writes; tests point
/// them at a directory.
#[derive(Debug, Clone)]
pub struct StoragePaths {
    /// The directory the storage node exports.
    pub root: PathBuf,
    /// The export setup writes on the storage node.
    pub exports: PathBuf,
    /// The kernel's list of what is mounted.
    pub mountinfo: PathBuf,
}

impl Default for StoragePaths {
    fn default() -> Self {
        Self {
            root: PathBuf::from(STORAGE_ROOT),
            exports: PathBuf::from("/etc/exports.d/mm-storage.exports"),
            mountinfo: PathBuf::from("/proc/self/mountinfo"),
        }
    }
}

pub struct HostSharedStorage {
    paths: StoragePaths,
    process: Arc<dyn ProcessRunner>,
    mesh: Arc<dyn MeshHost>,
    hook: ForwardHook,
}

impl std::fmt::Debug for HostSharedStorage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostSharedStorage")
            .field("paths", &self.paths)
            .finish_non_exhaustive()
    }
}

fn io(context: String) -> impl FnOnce(std::io::Error) -> MmError {
    move |source| MmError::Io { context, source }
}

fn failed(program: &str, arguments: &[&str], output: &CommandOutput) -> MmError {
    MmError::ProcessFailed {
        program: format!("{program} {}", arguments.join(" ")),
        code: output.code,
        stderr: output.stderr.trim().to_owned(),
    }
}

/// What a walk over one volume's directory counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Counted {
    bytes: u64,
    files: u64,
    complete: bool,
}

/// The disk space and number of files below `directory`, until `deadline`.
/// Links are counted, never followed.
fn count(directory: &Path, deadline: Instant) -> Counted {
    let mut counted = Counted {
        complete: true,
        ..Counted::default()
    };
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            if Instant::now() >= deadline {
                counted.complete = false;
                return counted;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            counted.bytes = counted
                .bytes
                .saturating_add(metadata.blocks().saturating_mul(512));
            if metadata.is_dir() {
                pending.push(entry.path());
            } else {
                counted.files = counted.files.saturating_add(1);
            }
        }
    }
    counted
}

/// Gives a directory root created its owner's group. `mmd` runs with the
/// relay's group, which a volume's directory has no business carrying: a
/// new engine volume belongs to root and root's group.
fn own_group(path: &Path) -> Result<(), MmError> {
    let metadata = std::fs::metadata(path).map_err(io(format!("reading {}", path.display())))?;
    if metadata.uid() == 0 && metadata.gid() != 0 {
        std::os::unix::fs::chown(path, None, Some(0))
            .map_err(io(format!("setting the group of {}", path.display())))?;
    }
    Ok(())
}

/// The directories directly below `directory`, by name; a link is not one.
fn subdirectories(directory: &Path) -> Result<Vec<(String, PathBuf)>, MmError> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(io(format!("reading {}", directory.display()))(source)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io(format!("reading {}", directory.display())))?;
        let is_directory = entry
            .file_type()
            .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink());
        if let (true, Some(name)) = (is_directory, entry.file_name().to_str()) {
            found.push((name.to_owned(), entry.path()));
        }
    }
    found.sort();
    Ok(found)
}

impl HostSharedStorage {
    #[must_use]
    pub fn new(process: Arc<dyn ProcessRunner>, mesh: Arc<dyn MeshHost>) -> Self {
        Self {
            paths: StoragePaths::default(),
            process,
            mesh,
            hook: forward_hook(crate::domain::runtime::RuntimeKind::DockerCompose),
        }
    }

    /// The host runs `engine`, whose packet filter mm's chains enter.
    #[must_use]
    pub fn for_engine(mut self, engine: crate::domain::runtime::RuntimeKind) -> Self {
        self.hook = forward_hook(engine);
        self
    }

    #[must_use]
    pub fn with_paths(mut self, paths: StoragePaths) -> Self {
        self.paths = paths;
        self
    }

    /// `directory`, given below the storage root, as a path on this host.
    /// Only plain names lead anywhere: no `..`, no absolute part.
    fn below_root(&self, directory: &str) -> Result<PathBuf, MmError> {
        let plain = !directory.is_empty()
            && directory
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..");
        if !plain {
            return Err(MmError::Security {
                message: format!("'{directory}' is not a directory of shared storage"),
            });
        }
        Ok(self.paths.root.join(directory))
    }

    /// Creates `directory` below the root, one part at a time, and returns
    /// whether its last part was created. An existing part has to be a
    /// real directory.
    fn create(&self, directory: &str) -> Result<bool, MmError> {
        self.below_root(directory)?;
        let mut path = self.paths.root.clone();
        let mut created = false;
        for part in directory.split('/') {
            path.push(part);
            created = match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_dir() => false,
                Ok(_) => {
                    return Err(MmError::Security {
                        message: format!(
                            "{} is in the way of a shared volume and is not a directory; move it aside on the storage node",
                            path.display()
                        ),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir(&path)
                        .map_err(io(format!("creating {}", path.display())))?;
                    std::fs::set_permissions(
                        &path,
                        std::fs::Permissions::from_mode(DIRECTORY_MODE),
                    )
                    .map_err(io(format!("setting the mode of {}", path.display())))?;
                    own_group(&path)?;
                    true
                }
                Err(source) => return Err(io(format!("reading {}", path.display()))(source)),
            };
        }
        Ok(created)
    }

    async fn run(
        &self,
        program: &str,
        arguments: &[&str],
        stdin: Option<String>,
    ) -> Result<CommandOutput, MmError> {
        let mut specification = CommandSpec::new(program)
            .args(arguments.iter().copied())
            .environment("LC_ALL", "C")
            .environment("PATH", SEARCH_PATH)
            .timeout(TIMEOUT);
        if let Some(stdin) = stdin {
            specification = specification.sensitive_stdin(Zeroizing::new(stdin));
        }
        self.process
            .run(specification)
            .await
            .map_err(|error| match error {
                MmError::ProcessSpawn { program, .. } => MmError::Target {
                    message: format!(
                        "`{program}` is not installed on this host; keeping containers away from shared storage needs the iptables package: install it with `apt-get install iptables`"
                    ),
                },
                other => other,
            })
    }

    async fn iptables(&self, arguments: &[&str]) -> Result<(), MmError> {
        let output = self.run("iptables", arguments, None).await?;
        if output.success() {
            Ok(())
        } else {
            Err(failed("iptables", arguments, &output))
        }
    }

    /// The rules of one chain as the kernel holds them; `None` when the
    /// chain does not exist.
    async fn chain(&self, table: &str, chain: &str) -> Result<Option<Vec<String>>, MmError> {
        let output = self
            .run("iptables", &["-w", "-t", table, "-S", chain], None)
            .await?;
        Ok(output.success().then(|| {
            output
                .stdout
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with("-A "))
                .map(str::to_owned)
                .collect()
        }))
    }

    async fn enters(&self, table: &str, from: &str, chain: &str) -> Result<bool, MmError> {
        Ok(self
            .run(
                "iptables",
                &["-w", "-t", table, "-C", from, "-j", chain],
                None,
            )
            .await?
            .success())
    }

    /// Each chain with the table it lives in and the chain that enters it.
    fn hooks(&self) -> [(&'static str, &'static str, &'static str); 2] {
        [
            (self.hook.table, self.hook.chain, STORAGE_FORWARD_CHAIN),
            ("filter", "INPUT", STORAGE_INPUT_CHAIN),
        ]
    }

    /// Why the rules have to be loaded, in words; empty when the host
    /// holds exactly `wanted`.
    async fn drift(&self, wanted: &StorageFirewall) -> Result<Vec<String>, MmError> {
        let mut drift = Vec::new();
        for ((table, from, chain), rules) in self
            .hooks()
            .into_iter()
            .zip([&wanted.forward, &wanted.input])
        {
            match self.chain(table, chain).await? {
                None => drift.push(format!("the chain {chain} is missing")),
                Some(held) if held != *rules => {
                    drift.push(format!("the rules of {chain} differ"));
                }
                Some(_) => {}
            }
            if !self.enters(table, from, chain).await? {
                drift.push(format!("{from} does not enter {chain}"));
            }
        }
        Ok(drift)
    }

    /// Replaces both chains as a whole and makes sure they are entered
    /// first.
    async fn load(&self, wanted: &StorageFirewall) -> Result<(), MmError> {
        let arguments = ["--noflush", "--wait", "5"];
        let output = self
            .run(
                "iptables-restore",
                &arguments,
                Some(wanted.restore_input(self.hook)),
            )
            .await?;
        if !output.success() {
            return Err(failed("iptables-restore", &arguments, &output));
        }
        // Before the engine has started for the first time its chain does
        // not exist; the engine keeps a chain it finds.
        if self.hook.create
            && self
                .chain(self.hook.table, self.hook.chain)
                .await?
                .is_none()
        {
            self.iptables(&["-w", "-t", self.hook.table, "-N", self.hook.chain])
                .await?;
        }
        for (table, from, chain) in self.hooks() {
            if !self.enters(table, from, chain).await? {
                self.iptables(&["-w", "-t", table, "-I", from, "1", "-j", chain])
                    .await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl SharedStorageHost for HostSharedStorage {
    fn exported(&self) -> bool {
        self.paths.exports.is_file()
    }

    fn create_directories(&self, directories: &[String]) -> Result<Vec<String>, MmError> {
        if !self.paths.root.is_dir() {
            return Err(MmError::Target {
                message: format!(
                    "{} does not exist on this node, so it cannot keep shared volumes; run `mm target setup plan` and apply it",
                    self.paths.root.display()
                ),
            });
        }
        let mut created = Vec::new();
        for directory in directories {
            if self.create(directory)? {
                created.push(directory.clone());
            }
        }
        Ok(created)
    }

    async fn reach(&self, address: Ipv4Addr) -> Result<u64, String> {
        let started = Instant::now();
        let connecting = tokio::net::TcpStream::connect((address, NFS_PORT));
        match tokio::time::timeout(Duration::from_millis(STORAGE_REACH_TIMEOUT_MS), connecting)
            .await
        {
            Ok(Ok(_)) => Ok(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)),
            Ok(Err(error)) => Err(format!("{address}:{NFS_PORT} refused or failed: {error}")),
            Err(_) => Err(format!(
                "{address}:{NFS_PORT} did not answer within {} s",
                STORAGE_REACH_TIMEOUT_MS / 1_000
            )),
        }
    }

    fn mounted(&self, project: ProjectId, environment: &str) -> Result<Vec<VolumeName>, MmError> {
        let text = std::fs::read_to_string(&self.paths.mountinfo)
            .map_err(io(format!("reading {}", self.paths.mountinfo.display())))?;
        Ok(mounted_volumes(&text, project, environment))
    }

    async fn stored(
        &self,
        project: ProjectId,
        environment: &str,
        sizes: bool,
    ) -> Result<StoredVolumes, MmError> {
        let below = environment_directory(project, environment)
            .map_err(|message| MmError::Environment { message })?;
        let mut directories = Vec::new();
        for (stack, path) in subdirectories(&self.paths.root.join(below))? {
            for (volume, path) in subdirectories(&path)? {
                directories.push((stack.clone(), volume, path));
            }
        }
        let root = self.paths.root.clone();
        tokio::task::spawn_blocking(move || {
            let deadline = Instant::now() + COUNT_BUDGET;
            let volumes = directories
                .into_iter()
                .map(|(stack, volume, path)| {
                    let counted = sizes.then(|| count(&path, deadline));
                    StoredVolume {
                        stack,
                        volume,
                        path: path.to_string_lossy().into_owned(),
                        bytes: counted.map(|counted| counted.bytes),
                        files: counted.map(|counted| counted.files),
                        counted: counted.is_none_or(|counted| counted.complete),
                    }
                })
                .collect();
            // A disk that does not say how full it is leaves both unset.
            let (free_bytes, total_bytes) = match rustix::fs::statvfs(&root) {
                Ok(disk) => (
                    Some(disk.f_bavail.saturating_mul(disk.f_frsize)),
                    Some(disk.f_blocks.saturating_mul(disk.f_frsize)),
                ),
                Err(_) => (None, None),
            };
            StoredVolumes {
                volumes,
                free_bytes,
                total_bytes,
            }
        })
        .await
        .map_err(|error| MmError::State {
            message: format!("counting the shared volumes stopped: {error}"),
        })
    }

    async fn delete(&self, directory: &str) -> Result<(), MmError> {
        let path = self.below_root(directory)?;
        let metadata =
            std::fs::symlink_metadata(&path).map_err(io(format!("reading {}", path.display())))?;
        if !metadata.is_dir() {
            return Err(MmError::Security {
                message: format!("{} is not a directory", path.display()),
            });
        }
        let root = self.paths.root.clone();
        let removing = path.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::remove_dir_all(&removing)?;
            // The stack's, environment's, and project's directories go
            // with their last volume; one that still holds something stays.
            let mut parent = removing.parent();
            while let Some(directory) = parent.filter(|directory| *directory != root) {
                if std::fs::remove_dir(directory).is_err() {
                    break;
                }
                parent = directory.parent();
            }
            Ok(())
        })
        .await
        .map_err(|error| MmError::State {
            message: format!("deleting {} stopped: {error}", path.display()),
        })?
        .map_err(io(format!("deleting {}", path.display())))
    }

    async fn up(&self) -> Result<MeshApplied, MmError> {
        let Some(state) = self.mesh.stored()? else {
            return Ok(MeshApplied::default());
        };
        let wanted = StorageFirewall::of(&state);
        let drift = self.drift(&wanted).await?;
        if drift.is_empty() {
            return Ok(MeshApplied::default());
        }
        self.load(&wanted).await?;
        Ok(MeshApplied {
            changed: true,
            actions: vec![format!(
                "loaded the firewall chains {STORAGE_FORWARD_CHAIN} and {STORAGE_INPUT_CHAIN} ({})",
                drift.join("; ")
            )],
        })
    }

    async fn leave(&self) -> Result<MeshApplied, MmError> {
        let mut actions = Vec::new();
        for (table, from, chain) in self.hooks() {
            if self.chain(table, chain).await?.is_none() {
                continue;
            }
            while self.enters(table, from, chain).await? {
                self.iptables(&["-w", "-t", table, "-D", from, "-j", chain])
                    .await?;
            }
            self.iptables(&["-w", "-t", table, "-F", chain]).await?;
            self.iptables(&["-w", "-t", table, "-X", chain]).await?;
            actions.push(format!("removed the firewall chain {chain}"));
        }
        Ok(MeshApplied {
            changed: !actions.is_empty(),
            actions,
        })
    }
}
