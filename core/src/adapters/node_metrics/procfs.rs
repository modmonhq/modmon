//! The host's numbers from its `/proc` files and `statvfs`, read in the
//! host's own view of the filesystem.
//!
//! `mmd` runs with a mount namespace and a `/proc` of its own
//! (`ProtectSystem=strict`, `ProcSubset=pid`), which show neither the
//! host's mounts nor its CPU, memory, disk and network counters. Through
//! `/proc/1/root`, the root of the host's first process, it sees what the
//! host sees.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use crate::app::ports::HostStats;
use crate::domain::node_metrics::procfs::{
    FilesystemSample, FilesystemUsage, HostSample, parse_diskstats, parse_loadavg, parse_meminfo,
    parse_mounts, parse_net_dev, parse_stat,
};

/// The most of one file that is read.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ProcHostStats {
    root: PathBuf,
}

impl ProcHostStats {
    /// The host as its first process sees it, else as this process does.
    #[must_use]
    pub fn host() -> Self {
        let init = Path::new("/proc/1/root");
        let root = if init.join("proc/stat").is_file() {
            init
        } else {
            Path::new("/")
        };
        Self::at(root)
    }

    /// The host whose root is `root`, which holds `proc/stat`,
    /// `proc/meminfo`, `proc/loadavg`, `proc/diskstats`, `proc/net/dev`
    /// and the mount table `proc/1/mounts`.
    #[must_use]
    pub fn at(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    fn text(&self, relative: &str) -> Option<String> {
        let mut text = String::new();
        std::fs::File::open(self.root.join(relative))
            .ok()?
            .take(MAX_FILE_BYTES)
            .read_to_string(&mut text)
            .ok()?;
        Some(text)
    }

    fn filesystems(&self) -> Vec<FilesystemSample> {
        let Some(table) = self
            .text("proc/1/mounts")
            .or_else(|| self.text("proc/self/mounts"))
        else {
            return Vec::new();
        };
        parse_mounts(&table)
            .into_iter()
            .filter_map(|mount| {
                let path = self.root.join(mount.mountpoint.trim_start_matches('/'));
                Some(FilesystemSample {
                    usage: usage(&path)?,
                    mount,
                })
            })
            .collect()
    }
}

fn usage(path: &Path) -> Option<FilesystemUsage> {
    let stat = rustix::fs::statvfs(path).ok()?;
    let block = stat.f_frsize;
    Some(FilesystemUsage {
        size: stat.f_blocks.saturating_mul(block),
        free: stat.f_bfree.saturating_mul(block),
        available: stat.f_bavail.saturating_mul(block),
        files: stat.f_files,
        files_free: stat.f_ffree,
    })
}

impl HostStats for ProcHostStats {
    fn read(&self) -> HostSample {
        HostSample {
            cpu: self.text("proc/stat").as_deref().and_then(parse_stat),
            memory: self.text("proc/meminfo").as_deref().and_then(parse_meminfo),
            load: self.text("proc/loadavg").as_deref().and_then(parse_loadavg),
            filesystems: self.filesystems(),
            disks: self
                .text("proc/diskstats")
                .as_deref()
                .map(parse_diskstats)
                .unwrap_or_default(),
            interfaces: self
                .text("proc/net/dev")
                .as_deref()
                .map(parse_net_dev)
                .unwrap_or_default(),
        }
    }
}
