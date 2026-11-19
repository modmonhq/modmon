//! `mm target setup` for a cluster target: one reviewed plan that prepares
//! every node, connects them over a private network, installs the container
//! engine and `mmd`, pushes the environment's secrets and deploy key, and
//! applies the users file.

use std::net::Ipv4Addr;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::repo_target::{RepoClusterConfig, RepoTargetSetup};
use crate::domain::runtime::RuntimeKind;
use crate::domain::target::BootstrapInputFile;
use crate::domain::workspace::ProjectId;

/// The release of mm a node's `mmd` reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct HostRelease {
    pub version: String,
    /// The newest remote protocol it speaks.
    pub protocol: u16,
    /// The digest of the `mmd` executable ([`build_digest`]), which tells
    /// two builds of one release apart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

impl HostRelease {
    /// A later version, or the same version and a later remote protocol.
    /// The version comes first: builds before 0.1.1 report protocols up to
    /// 19, and 0.1.1 starts again at 1.
    #[must_use]
    pub fn is_newer_than(&self, other: &Self) -> bool {
        let version = match (
            semver::Version::parse(&self.version),
            semver::Version::parse(&other.version),
        ) {
            (Ok(mine), Ok(theirs)) => mine.cmp(&theirs),
            _ => std::cmp::Ordering::Equal,
        };
        version.then(self.protocol.cmp(&other.protocol)) == std::cmp::Ordering::Greater
    }

    /// The same release, and not known to be another build of it.
    #[must_use]
    pub fn same_build(&self, other: &Self) -> bool {
        self.version == other.version
            && self.protocol == other.protocol
            && match (&self.build, &other.build) {
                (Some(mine), Some(theirs)) => mine == theirs,
                _ => true,
            }
    }
}

impl std::fmt::Display for HostRelease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.build {
            Some(build) => write!(
                formatter,
                "{} (remote protocol {}, build {})",
                self.version,
                self.protocol,
                build.get(..12).unwrap_or(build)
            ),
            None => write!(
                formatter,
                "{} (remote protocol {})",
                self.version, self.protocol
            ),
        }
    }
}

/// The digest of an `mmd` executable's bytes.
#[must_use]
pub fn build_digest(executable: &[u8]) -> String {
    blake3::hash(executable).to_hex().to_string()
}

/// What setup was told it may install over what the nodes run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReleaseConsent {
    /// `--downgrade`: an older release over a newer one.
    pub downgrade: bool,
    /// `--upgrade`, or `deploy.mm-version` names this release: another
    /// build, or a later release, over what the nodes run.
    pub upgrade: bool,
}

/// Per node, what setup replaces with `this`. A newer release is refused
/// unless `consent.downgrade`; another build or an older release unless
/// `consent.upgrade`.
pub fn release_notices(
    nodes: &[(&str, Option<&HostRelease>)],
    this: &HostRelease,
    consent: ReleaseConsent,
) -> Result<Vec<String>, String> {
    let newer = nodes
        .iter()
        .filter_map(|(node, release)| {
            release
                .filter(|release| release.is_newer_than(this))
                .map(|release| (*node, release))
        })
        .collect::<Vec<_>>();
    if let Some(newest) = newer
        .iter()
        .map(|(_, release)| *release)
        .reduce(|newest, release| {
            if release.is_newer_than(newest) {
                release
            } else {
                newest
            }
        })
        && !consent.downgrade
    {
        return Err(format!(
            "{} {} mm {newest}, newer than this mm {this}: setup would install this older mm and mmd there and cut off every client that is up to date. Install mm {} or later on this machine and run setup again, or pass --downgrade to install {this} anyway",
            names(&newer),
            if newer.len() == 1 { "runs" } else { "run" },
            newest.version
        ));
    }
    let replaced = nodes
        .iter()
        .filter_map(|(node, release)| {
            release
                .filter(|release| !release.is_newer_than(this) && !release.same_build(this))
                .map(|release| (*node, release))
        })
        .collect::<Vec<_>>();
    if let Some((_, first)) = replaced.first()
        && !consent.upgrade
    {
        return Err(format!(
            "{} {} mm {first}, and this mm is {this}: setup would replace the mm and mmd the hosts run with this build. Run setup with the mm the hosts run to keep them; pass --upgrade, or pin this release with deploy.mm-version, to install {this}",
            names(&replaced),
            if replaced.len() == 1 { "runs" } else { "run" },
        ));
    }
    Ok(nodes
        .iter()
        .map(|(node, release)| match release {
            Some(release) if release.same_build(this) => {
                format!("{node} runs this release of mm, {this}; setup installs its host binaries again")
            }
            Some(release) if release.is_newer_than(this) => {
                format!("{node}: downgrades mm and mmd from {release} to {this} (--downgrade)")
            }
            Some(release) => format!("{node}: replaces mm and mmd {release} with {this}"),
            None => format!(
                "{node}: installs mm and mmd {this}, in place of the earlier release the node may run"
            ),
        })
        .collect())
}

fn names(nodes: &[(&str, &HostRelease)]) -> String {
    nodes
        .iter()
        .map(|(node, _)| *node)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Where a node's SSH host key in the plan came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HostKeyOrigin {
    /// The repo records no key yet; the scanned key is trusted on first use.
    FirstUse,
    /// The scanned key equals the one the repo records.
    Recorded,
    /// The scanned key differs from the recorded one (a rebuilt host) and the
    /// operator passed `--trust-new-host-keys`.
    Replaced,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ClusterNodePlan {
    pub name: String,
    pub host: String,
    pub port: u16,
    /// The node's address on the private network: declared, or assigned
    /// from the mesh subnet.
    pub private_address: Ipv4Addr,
    /// The full `<type> <base64>` key setup connects with, strictly.
    pub host_key: String,
    pub host_key_fingerprint: String,
    pub host_key_origin: HostKeyOrigin,
    /// The node's CPU, when the plan could read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu: Option<String>,
}

/// A node of the target that a subset setup leaves out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ClusterSkippedNode {
    pub name: String,
    /// The node's place in the target's declared order, counted from zero.
    pub position: usize,
    pub private_address: Ipv4Addr,
}

/// A secret pushed to every node: its reference and revision, never its value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ClusterSecretPlan {
    pub name: String,
    pub reference: String,
    pub revision: String,
}

/// The deploy key setup installs on every node. All empty on a target mm
/// pushes the project's files to: its nodes fetch nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ClusterDeployKeyPlan {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secret: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub revision: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub public_key: String,
}

impl ClusterDeployKeyPlan {
    /// Whether the plan installs no deploy key.
    #[must_use]
    pub const fn is_none(&self) -> bool {
        self.secret.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ClusterSetupStep {
    /// Hostname, `/etc/mm/node.env`, and the tools the mesh needs.
    PrepareNodes,
    /// The project's bootstrap playbook and the remote access layer, per node.
    ProvisionNodes,
    /// Each node's `mmd` makes its key and brings up the mesh interface
    /// with the planned peers, unless the target names an existing private
    /// interface; then every node reaches every other.
    ConnectNodes,
    /// The storage node's own NFS server for shared volumes.
    ServeStorage,
    /// The environment's managed secret store: an OpenBao member on each
    /// of its nodes, initialised on the first run. Only in a plan whose
    /// environment declares one.
    SecretStore,
    /// The environment's secrets and deploy key, sealed to each node.
    PushSecrets,
    /// The first users-file sync, run as root on every node.
    SyncAccess,
    /// Host keys, daemon identities, and versions match on every node.
    Verify,
}

impl ClusterSetupStep {
    pub const ALL: [Self; 7] = [
        Self::PrepareNodes,
        Self::ProvisionNodes,
        Self::ConnectNodes,
        Self::ServeStorage,
        Self::PushSecrets,
        Self::SyncAccess,
        Self::Verify,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::PrepareNodes => "prepare-nodes",
            Self::ProvisionNodes => "provision-nodes",
            Self::ConnectNodes => "connect-nodes",
            Self::ServeStorage => "serve-storage",
            Self::SecretStore => "secret-store",
            Self::PushSecrets => "push-secrets",
            Self::SyncAccess => "sync-access",
            Self::Verify => "verify",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ClusterSetupPlan {
    pub project_id: ProjectId,
    pub project_name: String,
    pub config_hash: String,
    pub environment: String,
    pub target: String,
    pub nodes: Vec<ClusterNodePlan>,
    /// Nodes left out with `--nodes`. Setup does not contact them: they
    /// keep the mm version and the configuration they have.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<ClusterSkippedNode>,
    pub cluster: RepoClusterConfig,
    /// The WireGuard mesh: every node's interface and the peer changes on
    /// it. `None` when the target names an existing private interface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<crate::domain::mesh::MeshPlan>,
    /// The node that serves shared volumes.
    pub storage: String,
    /// The container runtime every node gets.
    pub runtime: RuntimeKind,
    pub setup: RepoTargetSetup,
    /// The project's bootstrap inputs (`.config/ansible`), by digest.
    pub input_files: Vec<BootstrapInputFile>,
    /// `mm` and `mmd` for each CPU the nodes need, in CPU order.
    pub binaries: Vec<crate::domain::target::HostBinaries>,
    /// `--host-binaries` as given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_binaries: Option<PathBuf>,
    pub secrets: Vec<ClusterSecretPlan>,
    /// The environment's managed secret store, when it declares one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_store: Option<crate::domain::secret_store::ClusterStorePlan>,
    /// Declared secrets with no value in this environment; not pushed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unset_secrets: Vec<String>,
    /// What the operator still has to provide, such as a registry token.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<String>,
    pub deploy_key: ClusterDeployKeyPlan,
    /// The commit every node syncs the users file from.
    pub access_commit: String,
    pub steps: Vec<ClusterSetupStep>,
    pub plan_hash: String,
}

impl ClusterSetupPlan {
    /// Whether mm sends the nodes the project's files (`delivery: push`),
    /// so they are set up without an origin and a deploy key.
    #[must_use]
    pub const fn pushes(&self) -> bool {
        self.setup.origin.is_empty()
    }

    /// BLAKE3 of the plan's canonical JSON without its own hash.
    pub fn canonical_hash(&self) -> Result<String, String> {
        let mut unhashed = self.clone();
        unhashed.plan_hash.clear();
        let bytes = serde_json::to_vec(&unhashed)
            .map_err(|error| format!("serializing cluster setup plan: {error}"))?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"mm-cluster-setup-plan\0");
        hasher.update(&bytes);
        Ok(hasher.finalize().to_hex().to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.nodes.is_empty() {
            return Err("cluster setup plan lists no nodes".to_owned());
        }
        crate::domain::target::validate_host_binaries(&self.binaries)?;
        if self.canonical_hash()? != self.plan_hash {
            return Err("cluster setup plan hash does not match its contents".to_owned());
        }
        Ok(())
    }

    /// The `mm` and `mmd` setup hands `node`: those for its CPU, or every
    /// set when the plan could not read its CPU.
    #[must_use]
    pub fn binaries_for(&self, node: &ClusterNodePlan) -> Vec<crate::domain::target::HostBinaries> {
        crate::domain::target::binaries_for(&self.binaries, node.cpu.as_deref())
            .into_iter()
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn apply_command(&self) -> String {
        let mut command = format!(
            "mm --environment {} target setup apply --plan {}",
            self.environment, self.plan_hash
        );
        if !self.skipped.is_empty() {
            command.push_str(" --nodes ");
            command.push_str(
                &self
                    .nodes
                    .iter()
                    .map(|node| node.name.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        if let Some(given) = &self.host_binaries {
            command.push_str(" --host-binaries ");
            command.push_str(&given.display().to_string());
        }
        command
    }

    /// Every node's private address in the target's declared order, the
    /// skipped nodes included: who may reach the cluster's shared storage
    /// does not depend on which nodes a setup run touches.
    #[must_use]
    pub fn private_addresses(&self) -> Vec<Ipv4Addr> {
        let mut selected = self.nodes.iter().map(|node| node.private_address);
        (0..self.nodes.len() + self.skipped.len())
            .filter_map(|position| {
                self.skipped
                    .iter()
                    .find(|skipped| skipped.position == position)
                    .map(|skipped| skipped.private_address)
                    .or_else(|| selected.next())
            })
            .collect()
    }
}

/// What setup learned from one node, for `mm target setup record`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ClusterNodeRecord {
    pub name: String,
    pub private_address: Ipv4Addr,
    pub host_key: String,
    pub daemon_fingerprint: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ClusterSetupResult {
    pub project_id: ProjectId,
    pub environment: String,
    pub target: String,
    pub plan_hash: String,
    pub nodes: Vec<ClusterNodeRecord>,
    pub secrets_changed: usize,
    pub access_commit: String,
    pub record_command: String,
}

#[cfg(test)]
#[path = "cluster_setup/tests.rs"]
mod tests;
