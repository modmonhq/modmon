//! The user ids a service gets on a host that runs rootful Podman.
//!
//! Every service there runs in a user namespace of its own whose ids mm
//! chooses and knows: container ids 0 to 65535 are 65536 host ids no other
//! service of the host has, the same for every container of the service,
//! from one deployment to the next. A file mm hands to the container is
//! then given to the host id the container's user maps to.
//!
//! The ids come from one range of host ids that setup reserves for mm in
//! `/etc/subuid` and `/etc/subgid` (the entry [`RANGE_OWNER`]), split into
//! blocks of 65536. A service's first choice of block is a digest of its
//! project, environment, stack and name, so every node of a cluster gives
//! it the same ids. The host keeps the block each service got; a service
//! whose first choice another service already has takes the next free
//! block of its own sequence, on that host only.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::domain::config::MmConfig;
use crate::domain::workspace::ProjectId;

/// The ids one service has: its container ids 0 to 65535.
pub const SERVICE_IDS: u32 = 65_536;
/// The name of mm's entry in `/etc/subuid` and `/etc/subgid`. No account
/// has this name, as none has Podman's own `containers`.
pub const RANGE_OWNER: &str = "mm-services";
/// The first host id of the range setup reserves: 3221225472 (0xC0000000).
pub const DEFAULT_FIRST: u32 = 0xC000_0000;
/// The range's size: 16383 blocks, ending before the last 65536 ids of the
/// id space, which hold the ids that mean "no id".
pub const DEFAULT_COUNT: u32 = 0xFFFF_0000 - DEFAULT_FIRST;
/// How many choices of its own sequence a service tries before it takes
/// the next free block after its first choice.
const CHOICES: u32 = 32;

/// mm's range of host ids on one host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostIdRange {
    pub first: u32,
    pub count: u32,
}

/// Which service a block of ids belongs to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ServiceIdsOwner {
    pub project: ProjectId,
    pub environment: String,
    pub stack: String,
    pub service: String,
}

/// The host ids of one service: `first` is what the container's root is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ServiceIds {
    pub first: u32,
}

/// The ids a service has on this host, and whether they are its first
/// choice, the ids every other node gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceIdsChoice {
    pub ids: ServiceIds,
    /// The ids every node gives the service unless another service of the
    /// node has them.
    pub first_choice: ServiceIds,
    /// Who has the service's first choice on this host instead.
    pub first_choice_held_by: Option<ServiceIdsOwner>,
}

impl ServiceIdsChoice {
    /// Whether the service has the ids every other node gives it.
    #[must_use]
    pub fn is_first_choice(&self) -> bool {
        self.ids == self.first_choice
    }
}

/// Every block of the range is taken.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "every block of {SERVICE_IDS} ids in {RANGE_OWNER}:{}:{} belongs to a service already",
    range.first,
    range.count
)]
pub struct RangeExhausted {
    pub range: HostIdRange,
    pub wanted: ServiceIdsOwner,
}

impl HostIdRange {
    /// The range setup writes.
    #[must_use]
    pub const fn default_range() -> Self {
        Self {
            first: DEFAULT_FIRST,
            count: DEFAULT_COUNT,
        }
    }

    /// A range that holds at least one service and ends before the ids
    /// that mean "no id".
    pub fn new(first: u32, count: u32) -> Result<Self, String> {
        let last = u64::from(first) + u64::from(count);
        if first == 0 || count < SERVICE_IDS || last > u64::from(u32::MAX - 1) {
            return Err(format!(
                "{first}:{count} is not a range of host ids mm can use: it has to start above 0, hold at least {SERVICE_IDS} ids, and end below {}",
                u32::MAX - 1
            ));
        }
        Ok(Self { first, count })
    }

    /// How many services the range holds.
    #[must_use]
    pub const fn blocks(self) -> u32 {
        self.count / SERVICE_IDS
    }

    fn block_first(self, block: u32) -> u32 {
        self.first + block * SERVICE_IDS
    }

    fn block_of(self, ids: ServiceIds) -> Option<u32> {
        let offset = ids.first.checked_sub(self.first)?;
        (offset % SERVICE_IDS == 0 && offset / SERVICE_IDS < self.blocks())
            .then_some(offset / SERVICE_IDS)
    }

    /// mm's range as `/etc/subuid` and `/etc/subgid` say: `None` when
    /// neither has an entry for [`RANGE_OWNER`]. Both must have the same
    /// one, and no other entry of either may overlap it.
    pub fn from_subordinate_ids(subuid: &str, subgid: &str) -> Result<Option<Self>, String> {
        let uids = entry(subuid, "/etc/subuid")?;
        let gids = entry(subgid, "/etc/subgid")?;
        match (uids, gids) {
            (None, None) => Ok(None),
            (Some(uids), Some(gids)) if uids == gids => Ok(Some(uids)),
            (uids, gids) => Err(format!(
                "/etc/subuid says {} and /etc/subgid says {} for {RANGE_OWNER}; a service's user ids and group ids come from the same numbers, so both files need the same line `{RANGE_OWNER}:<first>:<count>`",
                describe(uids),
                describe(gids)
            )),
        }
    }
}

fn describe(range: Option<HostIdRange>) -> String {
    range.map_or_else(
        || "nothing".to_owned(),
        |range| format!("{}:{}", range.first, range.count),
    )
}

/// mm's entry in one subordinate id file, checked against the others.
fn entry(text: &str, file: &str) -> Result<Option<HostIdRange>, String> {
    let mut ours = None;
    let mut others = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split(':');
        let (Some(name), Some(first), Some(count), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let (Ok(first), Ok(count)) = (first.parse::<u32>(), count.parse::<u32>()) else {
            continue;
        };
        if name == RANGE_OWNER {
            if ours.is_some() {
                return Err(format!(
                    "{file} has more than one line for {RANGE_OWNER}; keep one"
                ));
            }
            ours = Some(
                HostIdRange::new(first, count)
                    .map_err(|problem| format!("{file}, {RANGE_OWNER}: {problem}"))?,
            );
        } else {
            others.push((name.to_owned(), u64::from(first), u64::from(count)));
        }
    }
    let Some(range) = ours else {
        return Ok(None);
    };
    let (start, end) = (
        u64::from(range.first),
        u64::from(range.first) + u64::from(range.count),
    );
    if let Some((name, first, count)) = others
        .iter()
        .find(|(_, first, count)| *first < end && start < first + count)
    {
        return Err(format!(
            "{file}: {RANGE_OWNER}:{}:{} overlaps {name}:{first}:{count}; the ids mm gives services must be nobody else's",
            range.first, range.count
        ));
    }
    Ok(Some(range))
}

impl ServiceIds {
    /// What the service's containers are given as `userns_mode`: Podman's
    /// automatic user namespace with every container id mapped to the
    /// service's own host ids.
    #[must_use]
    pub fn userns_mode(self) -> String {
        format!(
            "auto:uidmapping=0:{first}:{SERVICE_IDS},gidmapping=0:{first}:{SERVICE_IDS},size={SERVICE_IDS}",
            first = self.first
        )
    }

    /// The host id that container id `id` is.
    #[must_use]
    pub fn host_id(self, id: u32) -> Option<u32> {
        (id < SERVICE_IDS).then(|| self.first + id)
    }

    /// The last host id of the service.
    #[must_use]
    pub const fn last(self) -> u32 {
        self.first + (SERVICE_IDS - 1)
    }
}

impl ServiceIdsOwner {
    /// The block the service tries on its `attempt`-th choice.
    fn choice(&self, attempt: u32, blocks: u32) -> u32 {
        let digest = blake3::hash(
            format!(
                "mm-service-ids\0{}\0{}\0{}\0{}\0{attempt}",
                self.project.0, self.environment, self.stack, self.service
            )
            .as_bytes(),
        );
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(&digest.as_bytes()[..8]);
        let value = u64::from_be_bytes(bytes) % u64::from(blocks.max(1));
        u32::try_from(value).unwrap_or_default()
    }

    /// The ids every node gives the service unless another service of the
    /// node already has them.
    #[must_use]
    pub fn first_choice(&self, range: HostIdRange) -> ServiceIds {
        ServiceIds {
            first: range.block_first(self.choice(0, range.blocks())),
        }
    }
}

/// The ids of each `wanted` service. A service keeps what `kept` says it
/// has, when that is in the range and nobody else's; any other gets the
/// first free block of its own sequence. `kept` plus the result is what
/// the host keeps afterwards.
pub fn allocate(
    range: HostIdRange,
    kept: &BTreeMap<ServiceIdsOwner, ServiceIds>,
    wanted: &BTreeSet<ServiceIdsOwner>,
) -> Result<BTreeMap<ServiceIdsOwner, ServiceIdsChoice>, RangeExhausted> {
    let blocks = range.blocks();
    let mut taken = BTreeMap::<u32, &ServiceIdsOwner>::new();
    for (owner, ids) in kept {
        if let Some(block) = range.block_of(*ids) {
            taken.entry(block).or_insert(owner);
        }
    }
    let mut chosen = BTreeMap::new();
    for owner in wanted {
        let kept_block = kept
            .get(owner)
            .and_then(|ids| range.block_of(*ids))
            .filter(|block| taken.get(block) == Some(&owner));
        let block = match kept_block {
            Some(block) => block,
            None => {
                let block = free_block(owner, blocks, &taken).ok_or_else(|| RangeExhausted {
                    range,
                    wanted: owner.clone(),
                })?;
                taken.insert(block, owner);
                block
            }
        };
        let first = owner.choice(0, blocks);
        chosen.insert(
            owner.clone(),
            ServiceIdsChoice {
                ids: ServiceIds {
                    first: range.block_first(block),
                },
                first_choice: ServiceIds {
                    first: range.block_first(first),
                },
                first_choice_held_by: (block != first)
                    .then(|| taken.get(&first).map(|holder| (*holder).clone()))
                    .flatten(),
            },
        );
    }
    Ok(chosen)
}

fn free_block(
    owner: &ServiceIdsOwner,
    blocks: u32,
    taken: &BTreeMap<u32, &ServiceIdsOwner>,
) -> Option<u32> {
    if let Some(block) = (0..CHOICES)
        .map(|attempt| owner.choice(attempt, blocks))
        .find(|block| !taken.contains_key(block))
    {
        return Some(block);
    }
    let start = owner.choice(0, blocks);
    (1..blocks)
        .map(|step| (start + step) % blocks)
        .find(|block| !taken.contains_key(block))
}

/// Whether `environment` runs on hosts `mm target setup` prepared: its
/// target (`target` when a request names one) is a target of the
/// repository. There `mmd` deploys it, and on rootful Podman gives each
/// service user ids of its own.
#[must_use]
pub fn on_prepared_hosts(config: &MmConfig, environment: &str, target: Option<&str>) -> bool {
    let target = target
        .or_else(|| {
            config
                .environments
                .get(environment)
                .and_then(|overlay| overlay.target.as_deref())
        })
        .unwrap_or(config.target.as_str());
    config.targets.contains_key(target)
}
