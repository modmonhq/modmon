//! The user ids of the services on this host, kept in one file next to
//! the host's other state. mm's range comes from `/etc/subuid` and
//! `/etc/subgid`, where setup reserves it; the file says which block of it
//! each service has, so a service keeps its ids across deployments,
//! recreates and restarts of `mmd`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapters::node_files::{read_whole, write_whole};
use crate::app::ports::ServiceUserIds;
use crate::domain::error::MmError;
use crate::domain::stored_state::{StoredOn, check_schema, stored_version};
use crate::domain::user_namespace::{
    HostIdRange, RANGE_OWNER, RangeExhausted, SERVICE_IDS, ServiceIds, ServiceIdsChoice,
    ServiceIdsOwner, allocate,
};

const REGISTRY_FILE: &str = "service-user-ids.json";
const SCHEMA: u32 = 1;
const CODE: &str = "MM_USER_NAMESPACE";

/// The services' user ids of one host.
#[derive(Debug, Clone)]
pub struct FilesystemServiceUserIds {
    registry: PathBuf,
    subuid: PathBuf,
    subgid: PathBuf,
    apparmor: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Registry {
    schema: u32,
    services: Vec<Entry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    #[serde(flatten)]
    owner: ServiceIdsOwner,
    first: u32,
}

impl FilesystemServiceUserIds {
    /// The ids `mmd` gives services: its record in `mm_home`, mm's range
    /// in the system's subordinate id files.
    #[must_use]
    pub fn on_host(mm_home: &Path) -> Self {
        Self {
            apparmor: true,
            ..Self::at(
                mm_home.join(REGISTRY_FILE),
                PathBuf::from("/etc/subuid"),
                PathBuf::from("/etc/subgid"),
            )
        }
    }

    #[must_use]
    pub const fn at(registry: PathBuf, subuid: PathBuf, subgid: PathBuf) -> Self {
        Self {
            registry,
            subuid,
            subgid,
            apparmor: false,
        }
    }

    fn range(&self) -> Result<HostIdRange, MmError> {
        let read = |path: &Path| -> Result<String, MmError> {
            Ok(read_whole(path)?
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default())
        };
        let found = HostIdRange::from_subordinate_ids(&read(&self.subuid)?, &read(&self.subgid)?)
            .map_err(|message| MmError::Remote {
            code: CODE,
            message,
        })?;
        found.ok_or_else(|| {
            let range = HostIdRange::default_range();
            MmError::Remote {
                code: CODE,
                message: format!(
                    "this host has no user ids for the services mm runs on Podman: {} and {} have no line for {RANGE_OWNER}. Run `mm target setup plan` and apply it, which adds `{RANGE_OWNER}:{}:{}` to both (and keeps Podman's own `containers` range clear of it), or keep the host's ids for a service with `host-user-namespace: <why>`",
                    self.subuid.display(),
                    self.subgid.display(),
                    range.first,
                    range.count
                ),
            }
        })
    }

    fn read(&self) -> Result<BTreeMap<ServiceIdsOwner, ServiceIds>, MmError> {
        let Some(bytes) = read_whole(&self.registry)? else {
            return Ok(BTreeMap::new());
        };
        if let Some(schema) = stored_version(&bytes, "schema") {
            check_schema(&self.registry, schema, SCHEMA, StoredOn::Host)?;
        }
        let registry: Registry = serde_json::from_slice(&bytes)
            .ok()
            .filter(|registry: &Registry| registry.schema == SCHEMA)
            .ok_or_else(|| MmError::Remote {
                code: CODE,
                message: format!(
                    "{} is damaged, and it says which user ids each service on this host has. Put back a copy of it; without it, services would get other ids than the files they wrote have",
                    self.registry.display()
                ),
            })?;
        Ok(registry
            .services
            .into_iter()
            .map(|entry| (entry.owner, ServiceIds { first: entry.first }))
            .collect())
    }

    fn write(&self, services: &BTreeMap<ServiceIdsOwner, ServiceIds>) -> Result<(), MmError> {
        let registry = Registry {
            schema: SCHEMA,
            services: services
                .iter()
                .map(|(owner, ids)| Entry {
                    owner: owner.clone(),
                    first: ids.first,
                })
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&registry).map_err(|error| MmError::State {
            message: format!("could not record the services' user ids: {error}"),
        })?;
        write_whole(&self.registry, &bytes, 0o600)
    }
}

impl FilesystemServiceUserIds {
    /// The record's lock, held while the record is read and written.
    fn lock(&self) -> Result<std::fs::File, MmError> {
        use std::os::unix::fs::OpenOptionsExt;
        let path = self.registry.with_extension("lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(&path)
            .map_err(|source| MmError::Io {
                context: format!("opening {}", path.display()),
                source,
            })?;
        crate::adapters::file_lock::exclusive(&lock, &path)?;
        Ok(lock)
    }
}

impl ServiceUserIds for FilesystemServiceUserIds {
    fn reserve(
        &self,
        wanted: &BTreeSet<ServiceIdsOwner>,
    ) -> Result<BTreeMap<ServiceIdsOwner, ServiceIdsChoice>, MmError> {
        if wanted.is_empty() {
            return Ok(BTreeMap::new());
        }
        let range = self.range()?;
        let _lock = self.lock()?;
        let kept = self.read()?;
        let chosen = allocate(range, &kept, wanted).map_err(exhausted)?;
        let mut next = kept.clone();
        for (owner, choice) in &chosen {
            next.insert(owner.clone(), choice.ids);
        }
        if next != kept {
            self.write(&next)?;
        }
        Ok(chosen)
    }

    fn release(&self, owners: &BTreeSet<ServiceIdsOwner>) -> Result<(), MmError> {
        let _lock = self.lock()?;
        let mut kept = self.read()?;
        let before = kept.len();
        kept.retain(|owner, _| !owners.contains(owner));
        if kept.len() != before {
            self.write(&kept)?;
        }
        Ok(())
    }

    fn shared_pid_apparmor(&self) -> crate::domain::remote::SharedPidAppArmor {
        if self.apparmor {
            crate::adapters::runtime::podman_apparmor::shared_pid_apparmor()
        } else {
            crate::domain::remote::SharedPidAppArmor::NotConfined
        }
    }
}

fn exhausted(problem: RangeExhausted) -> MmError {
    let RangeExhausted { range, wanted } = problem;
    MmError::Remote {
        code: CODE,
        message: format!(
            "every block of {SERVICE_IDS} user ids in mm's range on this host ({RANGE_OWNER}:{}:{} in /etc/subuid and /etc/subgid, {} services) belongs to a service already, so service '{}' of stack '{}' in environment '{}' gets none. Widen the range in both files, clear of every other line there, or keep the host's ids for the service with `host-user-namespace: <why>`",
            range.first,
            range.count,
            range.blocks(),
            wanted.service,
            wanted.stack,
            wanted.environment
        ),
    }
}
