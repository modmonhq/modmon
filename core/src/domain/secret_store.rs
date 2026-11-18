//! The OpenBao store mm runs on a cluster environment's nodes.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::domain::config::MmConfig;
use crate::domain::secret_backend::{
    ProviderConfiguration, ProviderInstanceConfig, ProviderInstanceName, ProviderScope,
};
use crate::domain::workspace::ProjectId;

pub const STORE_DRIVER: &str = "openbao";
pub const STORE_API_PORT: u16 = 8200;
pub const STORE_CLUSTER_PORT: u16 = 8201;
pub const STORE_MOUNT: &str = "mm-kv";
pub const STORE_AUTH_MOUNT: &str = "mm-approle";
pub const DEFAULT_MANAGEMENT_PORT: u16 = 18200;

/// `nodes: all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EveryNode {
    All,
}

/// The Raft members of a managed store: every node of the target, or the
/// nodes named. Three or five.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum StoreNodes {
    Every(EveryNode),
    Named(Vec<String>),
}

/// One release archive of a pinned OpenBao release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedArchive {
    /// As `uname -m` prints it.
    pub machine: &'static str,
    pub name: &'static str,
    pub sha256: &'static str,
}

/// An OpenBao release line mm installs on cluster nodes, and the exact
/// release and archive digests it installs for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedRelease {
    pub line: &'static str,
    pub version: &'static str,
    pub archives: [PinnedArchive; 2],
}

impl PinnedRelease {
    #[must_use]
    pub fn url(&self, archive: &PinnedArchive) -> String {
        format!(
            "https://github.com/openbao/openbao/releases/download/v{}/{}",
            self.version, archive.name
        )
    }
}

pub const PINNED_RELEASES: [PinnedRelease; 2] = [
    PinnedRelease {
        line: "2.4",
        version: "2.4.4",
        archives: [
            PinnedArchive {
                machine: "x86_64",
                name: "bao_2.4.4_Linux_x86_64.tar.gz",
                sha256: "a02f6e3e710616ae2687592a65bcbd72989500686d505bbd35ec5402b2847923",
            },
            PinnedArchive {
                machine: "aarch64",
                name: "bao_2.4.4_Linux_arm64.tar.gz",
                sha256: "30a0e40ee4f267f3acf4f4c419ceb4b3f63fa9f460d7d51f747ba26e6abeaba3",
            },
        ],
    },
    PinnedRelease {
        line: "2.5",
        version: "2.5.5",
        archives: [
            PinnedArchive {
                machine: "x86_64",
                name: "bao_2.5.5_Linux_x86_64.tar.gz",
                sha256: "2c5577707e97fc95c2086950f39880ead5e45b356c94388e5cb606f5a5c2b697",
            },
            PinnedArchive {
                machine: "aarch64",
                name: "bao_2.5.5_Linux_arm64.tar.gz",
                sha256: "9b133729e503ecf4d52f1d4e062954b9fd3798f14335ad8af5f7435f5b8ebd16",
            },
        ],
    },
];

#[must_use]
pub fn pinned_release(line: &str) -> Option<&'static PinnedRelease> {
    PINNED_RELEASES.iter().find(|release| release.line == line)
}

#[must_use]
pub fn pinned_lines() -> Vec<&'static str> {
    PINNED_RELEASES.iter().map(|release| release.line).collect()
}

/// Whether a provider is the managed store of cluster environments.
#[must_use]
pub fn is_cluster_store(instance: &ProviderInstanceConfig) -> bool {
    instance.scope == ProviderScope::Environment
        && matches!(
            instance.configuration,
            ProviderConfiguration::Managed { .. }
        )
}

/// The managed store a project declares for its cluster environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclaredStore<'a> {
    pub provider: &'a ProviderInstanceName,
    pub release: &'a str,
    pub nodes: Option<&'a StoreNodes>,
}

#[must_use]
pub fn declared_store(config: &MmConfig) -> Option<DeclaredStore<'_>> {
    config.providers.iter().find_map(|(name, instance)| {
        let ProviderConfiguration::Managed { release, nodes, .. } = &instance.configuration else {
            return None;
        };
        is_cluster_store(instance).then_some(DeclaredStore {
            provider: name,
            release,
            nodes: nodes.as_ref(),
        })
    })
}

/// The members of the store among `target_nodes`, in the target's order.
pub fn store_members(
    nodes: Option<&StoreNodes>,
    target_nodes: &[&str],
    provider: &str,
) -> Result<Vec<String>, String> {
    let members = match nodes {
        None | Some(StoreNodes::Every(EveryNode::All)) => {
            target_nodes.iter().map(|name| (*name).to_owned()).collect()
        }
        Some(StoreNodes::Named(named)) => {
            if let Some(unknown) = named
                .iter()
                .find(|name| !target_nodes.contains(&name.as_str()))
            {
                return Err(format!(
                    "providers.{provider}.nodes names '{unknown}', which is not a node of the target; its nodes are {}",
                    target_nodes.join(", ")
                ));
            }
            target_nodes
                .iter()
                .filter(|name| named.iter().any(|named| named == *name))
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>()
        }
    };
    if matches!(members.len(), 3 | 5) {
        return Ok(members);
    }
    let count = members.len();
    Err(match nodes {
        Some(StoreNodes::Named(_)) => format!(
            "providers.{provider}.nodes names {count} node(s); the store's Raft members are 3 or 5 nodes. List 3 or 5 of {}",
            target_nodes.join(", ")
        ),
        _ => format!(
            "the target has {count} node(s), and the managed store runs on 3 or 5. Set providers.{provider}.nodes to 3 of them, for example [{}]",
            target_nodes
                .iter()
                .take(3)
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}

/// What names one environment's store on a node: its unit, its
/// configuration and its data.
#[must_use]
pub fn store_id(project: ProjectId, environment: &str) -> String {
    let project = project.0.simple().to_string();
    format!("{environment}-{}", &project[project.len() - 12..])
}

#[must_use]
pub fn store_unit(id: &str) -> String {
    format!("mm-openbao-{id}.service")
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StoreMember {
    pub name: String,
    pub address: Ipv4Addr,
}

/// What setup does to the environment's store, bound into its plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ClusterStorePlan {
    pub provider: String,
    pub release: String,
    pub version: String,
    pub members: Vec<StoreMember>,
    /// The release each node of the plan runs now, as the node reports it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub installed: BTreeMap<String, String>,
    /// A member reports the store initialised.
    pub initialised: bool,
    /// Members that hold none of the store's data while its Raft
    /// configuration still lists them: setup removes that entry through a
    /// serving member, then joins them again, one at a time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejoins: Vec<String>,
}

impl ClusterStorePlan {
    /// One line per node that changes, for the plan.
    #[must_use]
    pub fn changes(&self) -> Vec<String> {
        let mut lines = self
            .members
            .iter()
            .map(|member| match self.installed.get(&member.name) {
                None => format!(
                    "{}: installs OpenBao {} on {}",
                    member.name, self.version, member.address
                ),
                Some(version) if version == &self.version => {
                    format!("{}: runs OpenBao {}", member.name, self.version)
                }
                Some(version) => format!(
                    "{}: upgrades OpenBao {version} to {}",
                    member.name, self.version
                ),
            })
            .collect::<Vec<_>>();
        lines.extend(self.rejoins.iter().map(|name| {
            format!(
                "{name}: rejoins: it holds none of the store's data while the Raft configuration still lists it; setup removes that entry through a serving member, then joins it again"
            )
        }));
        if !self.initialised {
            lines.push(format!(
                "initialises the store on {}: unseal keys sealed to every member's mmd, a recovery copy sealed to you, the root token revoked",
                self.members
                    .first()
                    .map_or("the first member", |member| member.name.as_str())
            ));
        }
        lines
    }
}

/// What a node holds of its member: the record `member.json` keeps, and
/// whether the member's Raft data is on disk.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StoreFacts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<StoreRecordFacts>,
    #[serde(default)]
    pub data: bool,
}

/// The fields of a member record a plan reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StoreRecordFacts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub initialized: bool,
}

impl StoreFacts {
    /// The release the member was set up with.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.record.as_ref()?.version.as_deref()
    }

    /// The member joined the store and its Raft data is still there.
    #[must_use]
    pub fn holds_data(&self) -> bool {
        self.data
            && self
                .record
                .as_ref()
                .is_some_and(|record| record.initialized)
    }
}

/// What setup tells a node's mmd about its member. Nothing in it is secret:
/// the node makes its own TLS key, and the certificate is public.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StoreMemberSpec {
    pub provider: String,
    pub node: String,
    pub version: String,
    pub members: Vec<StoreMember>,
    pub ca_pem: String,
    /// Signed by the environment's authority for the node's request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_pem: Option<String>,
}

/// What `mmd openbao member` answers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StoreMemberAnswer {
    pub changed: bool,
    /// A certificate request for the node's own key, when the node holds
    /// no valid certificate of the environment's authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csr: Option<String>,
    pub has_keys: bool,
    /// The member joined the store and holds its data; a member that waits
    /// to join under its node id does not.
    pub initialized: bool,
    pub sealed: bool,
}

/// What `mmd openbao init` is given: the age recipients every member's
/// store and the operator are sealed to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StoreInitRequest {
    pub recipients: BTreeMap<String, String>,
    pub operator: String,
}

/// What `mmd openbao init` answers: the recovery document sealed to each
/// other member and to the operator. Never a value in the clear.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StoreInitAnswer {
    pub sealed: BTreeMap<String, String>,
    pub recovery: String,
}

/// An AppRole login: what mmd reads and writes the environment's values
/// with, or reads the store's own state with.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StoreCredential {
    pub role_id: Zeroizing<String>,
    pub secret_id: Zeroizing<String>,
}

impl std::fmt::Debug for StoreCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("StoreCredential([REDACTED])")
    }
}

/// Everything a member needs to unseal itself and serve mmd, and the
/// operator needs to recover the store. Only ever sealed with age.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StoreRecovery {
    pub project_id: ProjectId,
    pub environment: String,
    pub provider: String,
    pub unseal_keys: Vec<Zeroizing<String>>,
    pub deploy: StoreCredential,
    pub operations: StoreCredential,
    pub ca_pem: String,
    pub members: Vec<StoreMember>,
}

impl std::fmt::Debug for StoreRecovery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StoreRecovery")
            .field("project_id", &self.project_id)
            .field("environment", &self.environment)
            .field("provider", &self.provider)
            .field("unseal_keys", &self.unseal_keys.len())
            .finish_non_exhaustive()
    }
}

/// One Raft member as the store reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StorePeer {
    pub name: String,
    pub address: String,
    pub leader: bool,
    pub voter: bool,
}

/// A member's view of the store, as `mmd openbao status` answers it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StoreStatus {
    pub provider: String,
    pub node: String,
    pub unit: String,
    pub version: Option<String>,
    pub running: bool,
    pub initialized: bool,
    pub sealed: bool,
    pub has_keys: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub peers: Vec<StorePeer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

/// The doctor findings about the store, from what its members report.
#[must_use]
pub fn store_diagnostics(
    target: &str,
    members: &[(&str, &StoreStatus)],
) -> Vec<crate::domain::diagnostic::Diagnostic> {
    store_findings(target, members, &[])
}

/// The findings about the store, from what its members report and the
/// members that reported nothing, with why.
fn store_findings(
    target: &str,
    members: &[(&str, &StoreStatus)],
    absent: &[(&str, &str)],
) -> Vec<crate::domain::diagnostic::Diagnostic> {
    use crate::domain::diagnostic::Diagnostic;
    let mut diagnostics = Vec::new();
    let names = |filter: &dyn Fn(&StoreStatus) -> bool| {
        members
            .iter()
            .filter(|(_, status)| filter(status))
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
    };
    let sealed = names(&|status| status.running && status.initialized && status.sealed);
    if !sealed.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "MM_SECRET_STORE_SEALED",
                format!(
                    "the secret store's member on {} of '{target}' is sealed",
                    sealed.join(", ")
                ),
            )
            .with_help(
                "a node's mmd unseals its member once it holds the keys: run `mm target setup plan` and apply it, which gives a node the keys it lacks",
            ),
        );
    }
    let mut degraded = absent
        .iter()
        .map(|(name, why)| format!("{name}: {why}"))
        .collect::<Vec<_>>();
    degraded.extend(self::degraded(members));
    if !degraded.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "MM_SECRET_STORE_DEGRADED",
                format!(
                    "the secret store of '{target}' is degraded: {}",
                    degraded.join("; ")
                ),
            )
            .with_help(
                "on the node, `systemctl status mm-openbao-*` and `journalctl -u mm-openbao-*` say why; `mm target setup plan` and its apply bring every member back",
            ),
        );
    }
    let versions = members
        .iter()
        .filter_map(|(name, status)| status.version.as_ref().map(|version| (*name, version)))
        .collect::<Vec<_>>();
    if versions
        .iter()
        .map(|(_, version)| *version)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        > 1
    {
        diagnostics.push(
            Diagnostic::warning(
                "MM_SECRET_STORE_VERSION_SKEW",
                format!(
                    "the secret store's members of '{target}' run different OpenBao releases: {}",
                    versions
                        .iter()
                        .map(|(name, version)| format!("{name} {version}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
            .with_help(
                "run `mm target setup plan` and apply it: setup brings every member to the release the configuration names, one at a time",
            ),
        );
    }
    diagnostics
}

#[path = "secret_store/rejoin.rs"]
mod rejoin;
#[path = "secret_store/report.rs"]
mod report;
pub use rejoin::{
    StorePeerRemoval, StoreSecretVersion, StoreSecretVersions, rejoining, removal_through,
};
pub use report::{ClusterStoreMember, ClusterStoreReport};

/// A snapshot file's contents: the Raft data and the keys it opens with.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StoreSnapshot {
    pub project_id: ProjectId,
    pub environment: String,
    pub provider: String,
    pub taken_at_unix_ms: i64,
    /// The node whose member answered.
    pub node: String,
    /// The Raft snapshot, base64.
    pub raft: Zeroizing<String>,
    pub recovery: StoreRecovery,
}

impl std::fmt::Debug for StoreSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StoreSnapshot")
            .field("environment", &self.environment)
            .field("provider", &self.provider)
            .field("node", &self.node)
            .finish_non_exhaustive()
    }
}

/// One step of putting a snapshot back, as a node takes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "step", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoreRestoreStep {
    /// The snapshot, sealed to the node's store, for its member to take
    /// in place of everything the store holds.
    Snapshot { sealed: String },
    /// The recovery document the snapshot opens with, sealed to the node's
    /// store: the node keeps it and unseals its member with it.
    Keys { sealed: String },
}

/// What a node did with a step of a restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StoreRestored {
    pub node: String,
    pub sealed: bool,
}

/// The largest Raft snapshot mm moves over the relay.
pub const MAX_STORE_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;

/// The members of the store on `target`, with their mesh addresses.
pub fn members_on(
    target: &crate::domain::repo_target::RepoTargetConfig,
    nodes: Option<&StoreNodes>,
    provider: &str,
) -> Result<Vec<StoreMember>, String> {
    let names = target
        .nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>();
    let addresses = target.mesh_addresses()?;
    Ok(store_members(nodes, &names, provider)?
        .into_iter()
        .filter_map(|name| {
            let position = names.iter().position(|known| *known == name)?;
            Some(StoreMember {
                name,
                address: *addresses.get(position)?,
            })
        })
        .collect())
}

/// What `mm secret provider restore` puts back, for review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StoreRestorePlan {
    pub environment: String,
    pub provider: String,
    pub taken_at_unix_ms: i64,
    /// The node whose member the snapshot was taken from.
    pub taken_on: String,
    pub members: Vec<StoreMember>,
    pub plan_hash: String,
}

impl StoreRestorePlan {
    /// The plan for putting the snapshot sealed in `file` back on `members`.
    #[must_use]
    pub fn of(snapshot: &StoreSnapshot, members: Vec<StoreMember>, file: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"mm-store-restore\0");
        hasher.update(file);
        for member in &members {
            hasher.update(member.name.as_bytes());
            hasher.update(&member.address.octets());
        }
        hasher.update(snapshot.environment.as_bytes());
        Self {
            environment: snapshot.environment.clone(),
            provider: snapshot.provider.clone(),
            taken_at_unix_ms: snapshot.taken_at_unix_ms,
            taken_on: snapshot.node.clone(),
            members,
            plan_hash: hasher.finalize().to_hex().to_string(),
        }
    }
}

/// One secret `mm secret migrate` moved from the age store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct MigratedSecret {
    pub key: String,
    pub location: String,
    /// The revision it has in both stores; `None` for a secret with no
    /// value to move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// A new version was written; false when the store held it already.
    pub written: bool,
}

/// What `mm secret migrate` moved into the managed store, by one node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StoreMigrated {
    pub node: String,
    pub provider: String,
    pub secrets: Vec<MigratedSecret>,
}

/// Why the store is degraded, member by member.
fn degraded(members: &[(&str, &StoreStatus)]) -> Vec<String> {
    let mut degraded = members
        .iter()
        .filter_map(|(name, status)| {
            if !status.running {
                Some(format!("{name}: {} is not running", status.unit))
            } else if !status.initialized {
                Some(format!("{name}: the member is not initialised"))
            } else {
                status
                    .problems
                    .iter()
                    .find(|problem| !problem.contains("is sealed"))
                    .map(|problem| format!("{name}: {problem}"))
            }
        })
        .collect::<Vec<_>>();
    let seen = members
        .iter()
        .flat_map(|(_, status)| &status.peers)
        .collect::<Vec<_>>();
    if !seen.is_empty() {
        for (name, _) in members {
            if !seen.iter().any(|peer| peer.name == *name && peer.voter) {
                degraded.push(format!("{name}: not a voter of the Raft cluster"));
            }
        }
        if !seen.iter().any(|peer| peer.leader) {
            degraded.push("no member leads".to_owned());
        }
    }
    degraded
}
