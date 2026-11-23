use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::access::PrincipalId;
use crate::domain::build::BuildReceipt;
use crate::domain::deployment::{DeploymentPlan, DeploymentResult};
use crate::domain::observability::{
    ExecEvent, ExecRequest, LogRecord, LogRequest, RestartOutcome, RestartRequest,
};
use crate::domain::release_facts::{ReleaseFacts, ReleaseRequest};
use crate::domain::runtime::RuntimeKind;
use crate::domain::scheduler::{JobRun, JobRunRequest, JobRunsRequest};
use crate::domain::users::AccessChange;
use crate::domain::workspace::{ProjectId, WorkspaceSnapshot};

/// The newest remote protocol this build speaks; protocol 1 is the 0.1.1 baseline.
pub const PROTOCOL_VERSION: u16 = 1;

/// The oldest remote protocol this build speaks. It rises only with a
/// breaking release.
pub const MINIMUM_PROTOCOL_VERSION: u16 = 1;

pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_STREAM_FRAMES: u64 = 1_000_000;
pub const MAX_STREAM_BYTES: u64 = 256 * 1024 * 1024;
pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;
pub const MAX_REQUEST_TIMEOUT_MS: u64 = 24 * 60 * 60 * 1_000;
pub const PLAN_TTL_MS: i64 = 10 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoteScope {
    pub project_id: ProjectId,
    pub environment: String,
    pub target: String,
    pub target_fingerprint: String,
    pub runtime: RuntimeKind,
    /// The node of a cluster target this request is for. The daemon refuses
    /// it unless that is its own name, so an address that reaches the wrong
    /// host never applies another node's plan. `None` for a target that
    /// names one host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
}

impl RemoteScope {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(node) = &self.node {
            crate::domain::target::validate_target_name(node)
                .map_err(|_| "remote node scope is invalid".to_owned())?;
        }
        if self.environment.is_empty()
            || self.environment.len() > 255
            || self.environment.chars().any(char::is_control)
        {
            return Err("remote environment scope is invalid".to_owned());
        }
        crate::domain::target::validate_target_name(&self.target)?;
        if !is_lower_hex_digest(&self.target_fingerprint) {
            return Err("remote target fingerprint is invalid".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClientHello {
    pub client_nonce: Uuid,
    pub expected_daemon_fingerprint: String,
    /// The client's release of mm, such as `0.1.1`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServerHello {
    pub client_nonce: Uuid,
    pub server_nonce: Uuid,
    pub daemon_version: String,
    pub daemon_fingerprint: String,
    pub instance_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientFrame {
    /// Added only by the installed forced-command relay after OpenSSH authentication.
    RelaySession {
        version: u16,
        key_fingerprint: String,
        source: String,
        session_id: Uuid,
    },
    /// `version` is the newest protocol the client speaks, `minimum` the
    /// oldest.
    Hello {
        version: u16,
        minimum: u16,
        hello: ClientHello,
    },
    Request {
        version: u16,
        request_id: Uuid,
        operation_nonce: Uuid,
        deadline_unix_ms: i64,
        scope: RemoteScope,
        method: Box<RemoteMethod>,
    },
    Cancel {
        version: u16,
        request_id: Uuid,
    },
    /// Bytes for a forwarded connection, base64-encoded.
    Data {
        version: u16,
        request_id: Uuid,
        bytes: String,
    },
}

impl ClientFrame {
    #[must_use]
    pub const fn version(&self) -> u16 {
        match self {
            Self::RelaySession { version, .. }
            | Self::Hello { version, .. }
            | Self::Request { version, .. }
            | Self::Cancel { version, .. }
            | Self::Data { version, .. } => *version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "method",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RemoteMethod {
    Status,
    Logs(LogRequest),
    Exec(ExecRequest),
    /// The daemon fetches `commit` from the project's pinned origin itself.
    /// `receipt` is a published build of that commit, such as CI's, whose
    /// images the deployment runs; remote hosts never build.
    PlanDeployment {
        commit: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        receipt: Option<Box<BuildReceipt>>,
        /// The release's name and inputs (`--release`, `--release-input`).
        #[serde(default, skip_serializing_if = "ReleaseRequest::is_empty")]
        release: ReleaseRequest,
    },
    ApplyDeployment {
        remote_plan_hash: String,
    },
    /// Reconciles access from `commit`'s users file without deploying.
    PlanTeamSync {
        commit: String,
    },
    ApplyTeamSync {
        remote_plan_hash: String,
    },
    /// Who the daemon thinks you are, and your effective access.
    WhoAmI,
    /// One forwarded TCP connection to a service's container port. The
    /// daemon connects only to that container, never to the host.
    Forward {
        service: String,
        port: u16,
    },
    /// Binds this node's stored plan into a cluster plan: the node checks
    /// that the cluster hash covers the plan it holds and keeps the seal.
    SealPlan(Box<ClusterPlanSeal>),
    /// The seal this node keeps for a cluster plan hash, while its own plan
    /// is still unexpired and unapplied.
    SealedPlan {
        cluster_plan_hash: String,
    },
    /// The node's name, version, and last applied deployment.
    Node,
    /// The node's WireGuard interface: each peer's endpoint, last
    /// handshake, and traffic.
    MeshStatus,
    /// The cluster networks the node stored, and what its kernel and engine
    /// have that differs.
    Networks,
    /// After every node of a cluster plan has applied: stops the services
    /// this node's section moved to other nodes. The node acts only on the
    /// plan it applied under this cluster hash.
    FinishMoves {
        cluster_plan_hash: String,
    },
    /// One step of rolling out the reviewed plan `plan_hash`: for a cluster
    /// node the hash of the cluster plan, as in `ApplyDeployment`.
    Rollout {
        plan_hash: String,
        step: crate::domain::rollout::RolloutStep,
    },
    /// The deployments the host remembers: the last it finished, the one
    /// before, and a rollout that runs or was left unfinished.
    DeploymentStatus,
    /// What the node's resolver answers for the environment's service
    /// names. Unlike `Status` it needs no finished deployment, so a
    /// rollout can ask while the first one is under way.
    Names,
    /// What the node says about the cluster's shared volumes:
    /// whether it reaches `storage_node`, which of the environment's
    /// volumes it has mounted, and, when it is that node, the volumes it
    /// keeps. `sizes` also counts each volume's files, which takes time.
    StorageStatus {
        storage_node: String,
        sizes: bool,
    },
    /// Tries each OCI runtime the host's policy offers: the engine has to
    /// know it and run a container under it.
    Runtimes,
    /// On the storage node: what deleting a shared volume would remove.
    PlanVolumeDelete {
        stack: String,
        volume: String,
    },
    /// On the storage node: deletes a shared volume's directory with
    /// everything in it. `plan_hash` is what `PlanVolumeDelete` returned.
    DeleteVolume {
        stack: String,
        volume: String,
        plan_hash: String,
    },
    /// For a host whose policy has the project delivered by push: the
    /// commit's tracked files. The host answers `Delivered` at once when it
    /// holds the commit, and `StreamStarted` when it wants the bundle; the
    /// client then sends it in `Data` frames and gets `Delivered`.
    Deliver(Box<crate::domain::delivery::BundleOffer>),
    /// Restarts the containers of a service this host runs, or one of
    /// them, and waits until they are healthy again. What is deployed
    /// does not change.
    Restart(RestartRequest),
    /// Runs one job the deployed configuration declares, in a fresh
    /// container of its service's image.
    RunJob(JobRunRequest),
    /// The job runs this host recorded, newest first.
    JobRuns(JobRunsRequest),
    /// Stops what runs for the environment on this host (`operation`, by
    /// its ID or the start of it, or whatever runs), ends its child
    /// processes, returns a deployment that changed the host to what it
    /// ran before, and frees the environment. Anyone who may deploy to the
    /// environment may cancel; it never waits behind what it stops.
    Cancel {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        operation: Option<String>,
    },
    /// The age recipient of the host's store, to seal staged values to.
    SecretRecipient,
    /// One step of staging secrets and variables for the next deployment.
    StageValues(Box<crate::domain::staged::ValueStage>),
    /// What the host holds and has staged for each secret and variable the
    /// deployed commit declares. Never a value.
    HostValues,
    /// A snapshot of the environment's managed secret store from this
    /// host's member, sealed to the age `recipient`.
    StoreSnapshot {
        recipient: String,
    },
    /// One step of putting a snapshot of the managed store back.
    StoreRestore(Box<crate::domain::secret_store::StoreRestoreStep>),
    /// Every age secret the deployed commit declares, written into the
    /// managed store `provider` with its revision.
    StoreMigrate {
        provider: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoteDeploymentPlan {
    pub binding: RemotePlanBinding,
    /// What the engine changes on this host. `None` on a cluster node that
    /// no service is placed on and that runs none: the node takes the
    /// access change, records the commit, and runs nothing.
    pub plan: Option<DeploymentPlan>,
    /// The commit's users file reconciled against what the environment
    /// enforces today; applied before the stacks.
    pub access: AccessChange,
    /// The build whose images the plan deploys, when the client sent one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<BuildReceipt>,
    /// A rollout the host began and nobody finished or rolled back, as of
    /// the moment it planned. Applying this plan undoes it first. Not part
    /// of the plan's hash: it says what the host is, not what the plan does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unfinished: Option<crate::domain::rollout::NodeRollout>,
    /// The `once` jobs this host knows ran or started in the environment.
    /// Not part of the plan's hash either.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub once: std::collections::BTreeMap<String, crate::domain::scheduler::OnceRecord>,
    /// The staged secrets and variables the plan makes live on this host.
    /// The plan's environment review binds their revisions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub staged: Vec<crate::domain::staged::StagedPlanItem>,
    /// The release the plan deploys; the engine's plan hashes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<ReleaseFacts>,
}

impl RemoteDeploymentPlan {
    pub fn validate(&self) -> Result<(), String> {
        self.binding.validate()?;
        if let Some(receipt) = &self.receipt {
            receipt.validate()?;
            let binding = &self.binding;
            check_receipt_scope(receipt, binding.project_id, &binding.commit)?;
        }
        let deployment_hash = match &self.plan {
            Some(plan) => {
                if plan.build_receipt != self.receipt.as_ref().map(BuildReceipt::origin) {
                    return Err(
                        "remote deployment plan does not deploy its build receipt".to_owned()
                    );
                }
                if plan.release != self.release {
                    return Err("remote deployment plan does not deploy its release".to_owned());
                }
                let hash = plan.canonical_hash()?;
                if hash != plan.plan_hash {
                    return Err(
                        "remote deployment binding does not match its canonical plan".to_owned(),
                    );
                }
                hash
            }
            None if self.binding.node.is_none() => {
                return Err("only a cluster node can be planned to run nothing".to_owned());
            }
            None => nothing_placed_hash(),
        };
        if self.binding.kind != RemotePlanKind::Deployment
            || deployment_hash != self.binding.deployment_plan_hash
            || self.access.canonical_hash()? != self.binding.access_change_hash
        {
            return Err("remote deployment binding does not match its canonical plan".to_owned());
        }
        Ok(())
    }
}

/// A receipt deploys only to its own project, and only the commit it was
/// built from. Another environment of the project may deploy it when its
/// configuration builds the same units (`BuildReceipt::promote`), which the
/// host checks after checking out the commit.
pub fn check_receipt_scope(
    receipt: &BuildReceipt,
    project_id: ProjectId,
    commit: &str,
) -> Result<(), String> {
    if receipt.plan.project_id != project_id {
        return Err("the build receipt is for another project".to_owned());
    }
    if receipt.source_commit() != commit {
        return Err(format!(
            "the build receipt is a build of commit {}, not {commit}",
            receipt.source_commit()
        ));
    }
    Ok(())
}

/// `mm team sync`: reconcile access from a commit without deploying.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoteAccessPlan {
    pub binding: RemotePlanBinding,
    pub access: AccessChange,
}

impl RemoteAccessPlan {
    pub fn validate(&self) -> Result<(), String> {
        self.binding.validate()?;
        let access_hash = self.access.canonical_hash()?;
        if self.binding.kind != RemotePlanKind::TeamSync
            || access_hash != self.binding.access_change_hash
            || access_hash != self.binding.deployment_plan_hash
        {
            return Err("remote access binding does not match its canonical change".to_owned());
        }
        Ok(())
    }
}

/// A reviewed plan as the daemon stores it until apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "plan", rename_all = "snake_case")]
pub enum StoredRemotePlan {
    Deployment(Box<RemoteDeploymentPlan>),
    TeamSync(Box<RemoteAccessPlan>),
}

impl StoredRemotePlan {
    #[must_use]
    pub fn binding(&self) -> &RemotePlanBinding {
        match self {
            Self::Deployment(plan) => &plan.binding,
            Self::TeamSync(plan) => &plan.binding,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Deployment(plan) => plan.validate(),
            Self::TeamSync(plan) => plan.validate(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemotePlanKind {
    Deployment,
    TeamSync,
}

/// The caller's identity and effective access, as the daemon enforces it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemoteIdentity {
    pub principal: String,
    pub principal_id: PrincipalId,
    pub key_fingerprint: String,
    pub break_glass: bool,
    pub environment: String,
    /// `None` when the key reaches the host but has no access here.
    pub access: Option<crate::domain::host_policy::EnvironmentAccess>,
    /// Commit whose users file this environment enforces.
    pub access_commit: Option<String>,
    /// Commit currently deployed to this environment.
    pub deployed_commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemotePlanBinding {
    /// The protocol of the session the plan was made in.
    pub protocol_version: u16,
    pub daemon_version: String,
    pub daemon_fingerprint: String,
    pub target_fingerprint: String,
    pub target: String,
    /// The cluster node the plan was made on and applies to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    pub kind: RemotePlanKind,
    pub project_id: ProjectId,
    pub environment: String,
    pub runtime: RuntimeKind,
    /// Full commit SHA the daemon fetched and checked out.
    pub commit: String,
    /// The deployment plan hash, or the access change hash for team sync.
    pub deployment_plan_hash: String,
    pub access_change_hash: String,
    pub principal_id: PrincipalId,
    pub authorization_revision: u64,
    pub authorization_hash: String,
    pub plan_nonce: Uuid,
    pub expires_at_unix_ms: i64,
    /// On a host the project is pushed to: the hash of the bundle the plan
    /// was made from. The host applies the plan only while the commit's
    /// files are still that bundle's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_hash: Option<String>,
    pub plan_hash: String,
}

impl RemotePlanBinding {
    pub fn canonical_hash(&self) -> Result<String, String> {
        if !ProtocolRange::THIS.contains(self.protocol_version)
            || self.daemon_version.is_empty()
            || !is_lower_hex_digest(&self.daemon_fingerprint)
            || !is_lower_hex_digest(&self.target_fingerprint)
            || !is_lower_hex_digest(&self.deployment_plan_hash)
            || !is_lower_hex_digest(&self.access_change_hash)
            || !is_commit_sha(&self.commit)
            || !is_lower_hex_digest(&self.authorization_hash)
            || self.authorization_revision == 0
            || self.environment.is_empty()
            || self
                .bundle_hash
                .as_deref()
                .is_some_and(|hash| !is_lower_hex_digest(hash))
        {
            return Err("remote plan binding contains invalid identity metadata".to_owned());
        }
        let mut hasher = blake3::Hasher::new();
        for value in [
            "mm-remote-plan-v3".to_owned(),
            self.protocol_version.to_string(),
            self.daemon_version.clone(),
            self.daemon_fingerprint.clone(),
            self.target_fingerprint.clone(),
            self.target.clone(),
            match self.kind {
                RemotePlanKind::Deployment => "deployment",
                RemotePlanKind::TeamSync => "team-sync",
            }
            .to_owned(),
            self.project_id.0.to_string(),
            self.environment.clone(),
            runtime_name(self.runtime).to_owned(),
            self.commit.clone(),
            self.deployment_plan_hash.clone(),
            self.access_change_hash.clone(),
            self.principal_id.0.to_string(),
            self.authorization_revision.to_string(),
            self.authorization_hash.clone(),
            self.plan_nonce.to_string(),
            self.expires_at_unix_ms.to_string(),
            self.node.clone().unwrap_or_default(),
            self.bundle_hash.clone().unwrap_or_default(),
        ] {
            hasher.update(&(value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
        Ok(hasher.finalize().to_hex().to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        let canonical = self.canonical_hash()?;
        if canonical != self.plan_hash {
            return Err("remote plan hash does not match its bound identity".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MutationState {
    NotStarted,
    Unknown,
    Applied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemoteError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub mutation_state: MutationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerFrame {
    Hello {
        version: u16,
        hello: ServerHello,
    },
    Result {
        version: u16,
        request_id: Uuid,
        result: Box<RemoteResult>,
    },
    Stream {
        version: u16,
        request_id: Uuid,
        sequence: u64,
        item: Box<RemoteStreamItem>,
    },
    End {
        version: u16,
        request_id: Uuid,
        frames: u64,
        bytes: u64,
    },
    Error {
        version: u16,
        request_id: Option<Uuid>,
        error: RemoteError,
    },
}

impl ServerFrame {
    #[must_use]
    pub const fn version(&self) -> u16 {
        match self {
            Self::Hello { version, .. }
            | Self::Result { version, .. }
            | Self::Stream { version, .. }
            | Self::End { version, .. }
            | Self::Error { version, .. } => *version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum RemoteResult {
    Status(Box<WorkspaceSnapshot>),
    DeploymentPlan(Box<RemoteDeploymentPlan>),
    DeploymentApplied(Box<DeploymentResult>),
    /// A cluster node applied a plan that places no service on it.
    NothingPlaced,
    AccessPlan(Box<RemoteAccessPlan>),
    AccessApplied(Box<AccessChange>),
    Identity(Box<RemoteIdentity>),
    Sealed(Box<ClusterPlanSeal>),
    Node(Box<NodeFacts>),
    MeshStatus(Box<crate::domain::mesh::MeshNodeStatus>),
    Networks(Box<crate::domain::cluster_network::NodeNetworkStatus>),
    /// The services a node stopped because they moved to other nodes.
    MovesFinished(Vec<crate::domain::placement::PlacedService>),
    /// What the host did for one step of a rollout.
    Rollout(Box<crate::domain::rollout::RolloutAnswer>),
    DeploymentStatus(Box<crate::domain::rollout::NodeDeploymentStatus>),
    Names(Box<crate::domain::discovery::NameReport>),
    StorageStatus(Box<crate::domain::shared_storage::StorageNodeStatus>),
    Runtimes(Box<crate::domain::hardening::HostRuntimeStatus>),
    VolumeDeletePlan(Box<crate::domain::shared_storage::VolumeDeletePlan>),
    VolumeDeleted(Box<crate::domain::shared_storage::VolumeDeleted>),
    /// What the host holds for a commit a client delivered.
    Delivered(Box<crate::domain::delivery::Delivered>),
    /// What a cancellation stopped and undid on the host.
    Cancelled(Box<crate::domain::rollout::CancelOutcome>),
    StreamStarted,
    Restarted(Box<RestartOutcome>),
    JobRun(Box<JobRun>),
    JobRuns(Vec<JobRun>),
    SecretRecipient(String),
    Staged(Box<crate::domain::staged::StageAnswer>),
    HostValues(Box<crate::domain::staged::NodeValues>),
    /// A snapshot of the managed store, sealed to the requester's recipient.
    StoreSnapshot(String),
    StoreRestored(Box<crate::domain::secret_store::StoreRestored>),
    StoreMigrated(Box<crate::domain::secret_store::StoreMigrated>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "item", content = "value", rename_all = "snake_case")]
pub enum RemoteStreamItem {
    Log(LogRecord),
    Exec(ExecEvent),
    /// Bytes from a forwarded connection, base64-encoded.
    Data(String),
}

/// A full 40-character lowercase SHA-1 commit ID.
#[must_use]
pub fn is_commit_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

/// The deployment hash a node's binding carries when the plan places no
/// service on it.
#[must_use]
pub fn nothing_placed_hash() -> String {
    blake3::hash(b"mm-node-runs-nothing-v1")
        .to_hex()
        .to_string()
}

pub fn is_lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

const fn runtime_name(runtime: RuntimeKind) -> &'static str {
    match runtime {
        RuntimeKind::PodmanCompose => "podman-compose",
        RuntimeKind::DockerCompose => "docker-compose",
    }
}

#[path = "remote/cluster.rs"]
mod cluster;
pub use cluster::{
    ClusterAccessPlan, ClusterApplyReport, ClusterDeploymentPlan, ClusterPlanSeal,
    ClusterPlanSection, NodeAccessPlan, NodeApplyOutcome, NodeApplyState, NodeDeployment,
    NodeDeploymentPlan, NodeFacts, NodeReport, NodeState, SealCarried, UnfinishedRollout,
    node_diagnostics, unreachable_help,
};
#[path = "remote/apparmor.rs"]
mod apparmor;
pub use apparmor::{
    CONTAINER_PROFILES, ContainerAppArmor, MM_CONTAINER_PROFILE, MM_SHARED_PID_PROFILE,
    SharedPidAppArmor, apparmor_diagnostic,
};
#[path = "remote/shared_pid.rs"]
mod shared_pid;
pub use shared_pid::{SharedPidService, refuse_unconfined_shared_pid, shared_pid_services};
#[path = "remote/engine.rs"]
mod engine;
pub use engine::{EngineFacts, PODMAN_MINIMUM, engine_diagnostics};
#[path = "remote/method.rs"]
mod method;
#[path = "remote/protocol.rs"]
mod protocol;
pub use protocol::{
    Behind, FrameHead, HANDSHAKE_MISMATCH, HelloOffer, IDENTITY_REFUSED, MethodHead,
    PROTOCOL_REFUSED, ProtocolRange, RequestHead, UPGRADE_REQUIRED_CODE, client_is_newer,
    negotiate, protocol_refusal, refused_handshake, request_not_understood, require_protocol,
};
#[path = "remote/moves.rs"]
mod moves;
pub use moves::{ServiceMove, check_moves_are_planned, service_moves};

#[cfg(test)]
mod tests;
