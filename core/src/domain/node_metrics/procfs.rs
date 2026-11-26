//! The host's own numbers, read from the text of its `/proc` files.

use super::MAX_DEVICES;

/// Ticks of the kernel's CPU accounting per second (`USER_HZ`), which is
/// 100 on every Linux architecture mm runs on.
pub const TICKS_PER_SECOND: f64 = 100.0;
/// The unit of `/proc/diskstats` sectors, whatever the device's own.
const SECTOR_BYTES: u64 = 512;

const CPU_MODES: [&str; 8] = [
    "user", "nice", "system", "idle", "iowait", "irq", "softirq", "steal",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuSample {
    /// Seconds spent in each mode over every CPU since boot.
    pub modes: Vec<(&'static str, f64)>,
    pub cpus: u32,
    pub boot_time: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemorySample {
    pub total: u64,
    pub free: u64,
    pub available: u64,
    pub buffers: u64,
    pub cached: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

/// A filesystem the host has mounted, from its mount table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub device: String,
    pub mountpoint: String,
    pub fstype: String,
    pub readonly: bool,
}

/// What `statvfs` says about one filesystem, in bytes and inodes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilesystemUsage {
    pub size: u64,
    pub free: u64,
    pub available: u64,
    pub files: u64,
    pub files_free: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemSample {
    pub mount: Mount,
    pub usage: FilesystemUsage,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskSample {
    pub device: String,
    pub reads: u64,
    pub read_bytes: u64,
    pub writes: u64,
    pub written_bytes: u64,
    pub io_milliseconds: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InterfaceSample {
    pub interface: String,
    pub receive_bytes: u64,
    pub receive_packets: u64,
    pub receive_errors: u64,
    pub receive_drops: u64,
    pub transmit_bytes: u64,
    pub transmit_packets: u64,
    pub transmit_errors: u64,
    pub transmit_drops: u64,
}

/// Everything read from the host in one collection. A part the host did
/// not show is empty.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostSample {
    pub cpu: Option<CpuSample>,
    pub memory: Option<MemorySample>,
    pub load: Option<[f64; 3]>,
    pub filesystems: Vec<FilesystemSample>,
    pub disks: Vec<DiskSample>,
    pub interfaces: Vec<InterfaceSample>,
}

/// `/proc/stat`: the time spent in each mode over every CPU, the number of
/// CPUs, and when the host booted.
#[must_use]
pub fn parse_stat(text: &str) -> Option<CpuSample> {
    let mut sample = CpuSample::default();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("cpu") => {
                sample.modes = CPU_MODES
                    .iter()
                    .zip(fields)
                    .filter_map(|(mode, ticks)| {
                        Some((*mode, ticks.parse::<u64>().ok()? as f64 / TICKS_PER_SECOND))
                    })
                    .collect();
            }
            Some(name) if name.starts_with("cpu") => sample.cpus += 1,
            Some("btime") => sample.boot_time = fields.next().and_then(|time| time.parse().ok()),
            _ => {}
        }
    }
    (!sample.modes.is_empty()).then_some(sample)
}

/// `/proc/meminfo`, in bytes.
#[must_use]
pub fn parse_meminfo(text: &str) -> Option<MemorySample> {
    let mut sample = MemorySample::default();
    let mut total = None;
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let Some(kibibytes) = rest
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        let bytes = kibibytes.saturating_mul(1024);
        match key {
            "MemTotal" => total = Some(bytes),
            "MemFree" => sample.free = bytes,
            "MemAvailable" => sample.available = bytes,
            "Buffers" => sample.buffers = bytes,
            "Cached" => sample.cached = bytes,
            "SwapTotal" => sample.swap_total = bytes,
            "SwapFree" => sample.swap_free = bytes,
            _ => {}
        }
    }
    sample.total = total?;
    Some(sample)
}

/// `/proc/loadavg`: the load over 1, 5 and 15 minutes.
#[must_use]
pub fn parse_loadavg(text: &str) -> Option<[f64; 3]> {
    let mut fields = text.split_whitespace().map(str::parse::<f64>);
    Some([
        fields.next()?.ok()?,
        fields.next()?.ok()?,
        fields.next()?.ok()?,
    ])
}

/// Filesystems that hold the host's data. A network filesystem is never
/// among them: asking a mount whose server is gone blocks.
const MEASURED: [&str; 12] = [
    "ext2", "ext3", "ext4", "xfs", "btrfs", "zfs", "f2fs", "vfat", "exfat", "ntfs3", "jfs",
    "bcachefs",
];
/// Memory filesystems of the host itself, as opposed to the many an
/// engine mounts for its containers.
const MEASURED_TMPFS: [&str; 3] = ["/tmp", "/run", "/dev/shm"];
/// Where engines keep their containers' layers and mounts.
const ENGINE_PATHS: [&str; 4] = [
    "/var/lib/docker/",
    "/var/lib/containers/",
    "/run/containers/",
    "/run/docker/",
];

/// The host's filesystems from its mount table (`/proc/1/mounts`), one
/// entry per mountpoint, at most [`MAX_DEVICES`].
#[must_use]
pub fn parse_mounts(text: &str) -> Vec<Mount> {
    let mut mounts: Vec<Mount> = Vec::new();
    for line in text.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let [device, mountpoint, fstype, options, ..] = fields.as_slice() else {
            continue;
        };
        let mountpoint = unescape(mountpoint);
        let measured = MEASURED.contains(fstype)
            || (*fstype == "tmpfs" && MEASURED_TMPFS.contains(&mountpoint.as_str()));
        if !measured || ENGINE_PATHS.iter().any(|path| mountpoint.starts_with(path)) {
            continue;
        }
        let mount = Mount {
            device: unescape(device),
            mountpoint,
            fstype: (*fstype).to_owned(),
            readonly: options.split(',').any(|option| option == "ro"),
        };
        let full = mounts.len() >= MAX_DEVICES;
        match mounts
            .iter_mut()
            .find(|known| known.mountpoint == mount.mountpoint)
        {
            Some(known) => *known = mount,
            None if !full => mounts.push(mount),
            None => {}
        }
    }
    mounts
}

/// A mount table field with its spaces, tabs, newlines and backslashes
/// written as octal escapes.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let octal = bytes
            .get(index + 1..index + 4)
            .filter(|digits| {
                byte == b'\\' && digits.iter().all(|digit| (b'0'..=b'7').contains(digit))
            })
            .and_then(|digits| u8::from_str_radix(std::str::from_utf8(digits).ok()?, 8).ok());
        match octal {
            Some(value) => {
                out.push(value);
                index += 4;
            }
            None => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Whether a block device is a whole disk worth reporting: not a loop,
/// RAM, floppy or optical device, and not a partition.
fn is_disk(name: &str) -> bool {
    let skipped = ["loop", "ram", "zram", "fd", "sr"];
    if skipped.iter().any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|rest| rest.bytes().all(|byte| byte.is_ascii_digit()))
    }) {
        return false;
    }
    let partition_of = |prefix: &str| {
        name.strip_prefix(prefix).is_some_and(|rest| {
            let letters = rest.trim_end_matches(|character: char| character.is_ascii_digit());
            letters.len() < rest.len()
                && !letters.is_empty()
                && letters.bytes().all(|byte| byte.is_ascii_lowercase())
        })
    };
    let numbered_partition = ["nvme", "mmcblk"].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .and_then(|rest| rest.rsplit_once('p'))
            .is_some_and(|(_, number)| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
    });
    !(["sd", "vd", "hd", "xvd"]
        .iter()
        .any(|prefix| partition_of(prefix))
        || numbered_partition)
}

/// `/proc/diskstats`, whole disks only, at most [`MAX_DEVICES`].
#[must_use]
pub fn parse_diskstats(text: &str) -> Vec<DiskSample> {
    text.lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let device = *fields.get(2)?;
            if !is_disk(device) {
                return None;
            }
            let number = |index: usize| -> Option<u64> { fields.get(index)?.parse().ok() };
            Some(DiskSample {
                device: device.to_owned(),
                reads: number(3)?,
                read_bytes: number(5)?.saturating_mul(SECTOR_BYTES),
                writes: number(7)?,
                written_bytes: number(9)?.saturating_mul(SECTOR_BYTES),
                io_milliseconds: number(12)?,
            })
        })
        .take(MAX_DEVICES)
        .collect()
}

/// `/proc/net/dev`, without the host ends of containers' links (`veth…`),
/// at most [`MAX_DEVICES`].
#[must_use]
pub fn parse_net_dev(text: &str) -> Vec<InterfaceSample> {
    text.lines()
        .filter_map(|line| {
            let (name, counters) = line.split_once(':')?;
            let interface = name.trim();
            if interface.is_empty() || interface.starts_with("veth") {
                return None;
            }
            let numbers = counters
                .split_whitespace()
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            let number = |index: usize| numbers.get(index).copied();
            Some(InterfaceSample {
                interface: interface.to_owned(),
                receive_bytes: number(0)?,
                receive_packets: number(1)?,
                receive_errors: number(2)?,
                receive_drops: number(3)?,
                transmit_bytes: number(8)?,
                transmit_packets: number(9)?,
                transmit_errors: number(10)?,
                transmit_drops: number(11)?,
            })
        })
        .take(MAX_DEVICES)
        .collect()
}
