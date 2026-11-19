use async_trait::async_trait;

use crate::app::ports::{ExecStream, LogStream};
use crate::domain::build::BuildReceipt;
use crate::domain::deployment::{DeploymentPlan, DeploymentResult};
use crate::domain::error::MmError;
use crate::domain::observability::{
    ExecRequest, LogRequest, OperationCancellation, RestartOutcome, RestartRequest,
};
use crate::domain::remote::{
    ClusterPlanSeal, NodeFacts, RemoteAccessPlan, RemoteDeploymentPlan, RemoteIdentity, RemoteScope,
};
use crate::domain::scheduler::{JobRun, JobRunRequest, JobRunsRequest};
use crate::domain::users::AccessChange;
use crate::domain::workspace::WorkspaceSnapshot;

#[derive(Debug, Clone)]
pub struct RemoteRequestContext {
    pub request_id: uuid::Uuid,
    pub operation_nonce: uuid::Uuid,
    pub principal_id: crate::domain::access::PrincipalId,
    pub principal: String,
    pub key_fingerprint: String,
    pub session_id: uuid::Uuid,
    pub source: String,
    pub request_plan_hash: Option<String>,
    pub authorization: crate::domain::access::Authorization,
    pub deadline_unix_ms: i64,
    pub scope: RemoteScope,
    /// Derived by the daemon: the checkout of the deployed (or planned)
    /// commit. Clients never send a path.
    pub project_root: std::path::PathBuf,
    /// Set by the daemon for a plan and its apply: the release it deploys.
    pub release: Option<crate::domain::release_facts::ReleaseFacts>,
    /// The protocol the client and the daemon agreed on: every frame of
    /// the session has it, and an answer holds nothing newer.
    pub protocol: u16,
}

#[async_trait]
pub trait RemoteUseCases: Send + Sync {
    async fn status(&self, context: &RemoteRequestContext) -> Result<WorkspaceSnapshot, MmError>;

    async fn logs(
        &self,
        context: &RemoteRequestContext,
        request: LogRequest,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError>;

    async fn exec(
        &self,
        context: &RemoteRequestContext,
        request: ExecRequest,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError>;

    /// Plans the checkout in `context`. With a `receipt`, the plan deploys
    /// its images instead of this host's builds.
    async fn plan_deployment(
        &self,
        context: &RemoteRequestContext,
        receipt: Option<&BuildReceipt>,
    ) -> Result<DeploymentPlan, MmError>;

    /// Applies the reviewed plan. A `receipt` the plan used is recorded as
    /// the checkout's build first.
    async fn apply_deployment(
        &self,
        context: &RemoteRequestContext,
        deployment_plan_hash: &str,
        receipt: Option<&BuildReceipt>,
    ) -> Result<DeploymentResult, MmError>;

    /// On a cluster node: stops the services the applied plan
    /// `deployment_plan_hash` moved to other nodes, and returns them.
    async fn finish_moves(
        &self,
        _context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
    ) -> Result<Vec<crate::domain::placement::PlacedService>, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not move services between nodes".to_owned(),
        })
    }

    /// On a cluster node, for its autoscaler: has the engine count `count`
    /// containers of `service` of stack `stack`, starting the ones it misses
    /// as the last deployment here (`deployment_plan_hash`, from the
    /// checkout `root`) runs the service.
    async fn scale_service(
        &self,
        _scope: &crate::domain::remote::RemoteScope,
        _root: &std::path::Path,
        _deployment_plan_hash: &str,
        _service: (&str, &str),
        _count: u32,
    ) -> Result<(), MmError> {
        Err(MmError::Environment {
            message: "this daemon does not scale services".to_owned(),
        })
    }

    /// Opens the reviewed plan's deployment and prepares the host for its
    /// stacks, without changing a workload.
    async fn begin_rollout(
        &self,
        _context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
        _receipt: Option<&BuildReceipt>,
    ) -> Result<(), MmError> {
        Err(MmError::Environment {
            message: "this daemon does not roll deployments out in steps".to_owned(),
        })
    }

    /// One step of one stack of the deployment `begin_rollout` opened.
    async fn roll_stack(
        &self,
        _context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
        _stack: &str,
        _step: crate::app::StackStep,
    ) -> Result<crate::app::StackOutcome, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not roll deployments out in steps".to_owned(),
        })
    }

    /// Runs job `job` of `stack` for the deployment `begin_rollout` opened,
    /// to its end.
    async fn roll_job(
        &self,
        _context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
        _stack: &str,
        _job: &str,
    ) -> Result<JobRun, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not run jobs in a deployment".to_owned(),
        })
    }

    /// Finishes the deployment `begin_rollout` opened.
    async fn finish_rollout(
        &self,
        _context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
    ) -> Result<DeploymentResult, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not roll deployments out in steps".to_owned(),
        })
    }

    /// Returns the host to what it ran before `begin_rollout`: what
    /// `changes` lists, or everything when it is `None`. `false` when no
    /// such deployment is open here.
    async fn rollback_rollout(
        &self,
        _context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
        _changes: Option<&crate::domain::rollout::RolloutChanges>,
    ) -> Result<bool, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not roll deployments out in steps".to_owned(),
        })
    }

    /// After a step of `stack` was stopped: the containers it had begun to
    /// take out (`ContainerLeaving`) are routed to again, and a stop that
    /// had begun ends first, so nothing the step began goes on. Returns the
    /// containers taken back; none where no container leaves.
    async fn take_back_leaves(
        &self,
        _context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
        _stack: &str,
    ) -> Result<Vec<String>, MmError> {
        Ok(Vec::new())
    }

    /// What this host's resolver answers for the environment's service
    /// names; empty where no resolver runs.
    async fn names(
        &self,
        _context: &RemoteRequestContext,
    ) -> Result<crate::domain::discovery::NameReport, MmError> {
        Ok(crate::domain::discovery::NameReport::default())
    }

    /// Each OCI runtime the host's policy offers, tried on its engine.
    async fn runtimes(
        &self,
        _context: &RemoteRequestContext,
    ) -> Result<crate::domain::hardening::HostRuntimeStatus, MmError> {
        Ok(crate::domain::hardening::HostRuntimeStatus::default())
    }

    /// The host's container engine `runtime` as it answers now: its
    /// version and Compose provider. `None` where the daemon cannot say.
    async fn engine(
        &self,
        _runtime: crate::domain::runtime::RuntimeKind,
    ) -> Option<crate::domain::remote::EngineFacts> {
        None
    }

    /// The host's own WireGuard interface and its peers.
    async fn mesh_status(
        &self,
        _context: &RemoteRequestContext,
    ) -> Result<crate::domain::mesh::MeshNodeStatus, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not report a mesh".to_owned(),
        })
    }

    /// The cluster networks this host stored, and what its kernel and
    /// engine have that differs.
    async fn networks(
        &self,
        _context: &RemoteRequestContext,
    ) -> Result<crate::domain::cluster_network::NodeNetworkStatus, MmError> {
        Ok(crate::domain::cluster_network::NodeNetworkStatus::default())
    }

    /// Stores the addresses a reviewed cluster plan gives the cluster's
    /// networks, as node `node`. `converge` also brings the host to them,
    /// for a node the plan places nothing on.
    async fn store_networks(
        &self,
        _node: &str,
        _networks: &crate::domain::cluster_network::SealedNetworks,
        _converge: bool,
    ) -> Result<(), MmError> {
        Err(MmError::Environment {
            message: "this daemon does not hold cluster networks".to_owned(),
        })
    }

    /// Takes the networks a finished rollout retired off this node
    /// (`SealedNetworks::retired`); returns those it took away.
    async fn retire_networks(&self, _retired: &[String]) -> Result<Vec<String>, MmError> {
        Ok(Vec::new())
    }

    /// Makes the shared volumes of a reviewed cluster plan ready on node
    /// `node`: the storage node creates the missing directories and
    /// returns them; a node that mounts one checks that it reaches the
    /// storage node.
    async fn prepare_storage(
        &self,
        _node: &str,
        _storage: &crate::domain::shared_storage::SealedStorage,
    ) -> Result<Vec<String>, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not keep shared volumes".to_owned(),
        })
    }

    /// Serves the routes of a reviewed cluster plan on node `node` when
    /// the plan names it an ingress node, and takes away what it served of
    /// the environment otherwise. Called when the node's rollout finishes.
    async fn apply_ingress(
        &self,
        _context: &RemoteRequestContext,
        _node: &str,
        _ingress: &crate::domain::cluster_ingress::SealedIngress,
    ) -> Result<(), MmError> {
        Err(MmError::Environment {
            message: "this daemon does not run an ingress proxy".to_owned(),
        })
    }

    /// What this host says about the cluster's shared volumes.
    async fn storage_status(
        &self,
        _context: &RemoteRequestContext,
        _storage_node: &str,
        _sizes: bool,
    ) -> Result<crate::domain::shared_storage::StorageNodeStatus, MmError> {
        Ok(crate::domain::shared_storage::StorageNodeStatus::default())
    }

    /// What deleting a shared volume would remove on this host.
    async fn plan_volume_delete(
        &self,
        _context: &RemoteRequestContext,
        _volume: &crate::domain::shared_storage::VolumeName,
    ) -> Result<crate::domain::shared_storage::VolumeDeletePlan, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not keep shared volumes".to_owned(),
        })
    }

    /// Deletes a shared volume's directory on this host.
    async fn delete_volume(
        &self,
        _context: &RemoteRequestContext,
        _volume: &crate::domain::shared_storage::VolumeName,
        _plan_hash: &str,
    ) -> Result<crate::domain::shared_storage::VolumeDeleted, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not keep shared volumes".to_owned(),
        })
    }

    /// Where a forward to `service`'s `port` connects: its container.
    async fn forward_address(
        &self,
        _context: &RemoteRequestContext,
        _service: &str,
        _port: u16,
    ) -> Result<std::net::SocketAddr, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not support port forwarding".to_owned(),
        })
    }

    /// Restarts a service's containers on this host, or one of them, and
    /// waits until they are healthy again.
    async fn restart(
        &self,
        _context: &RemoteRequestContext,
        _request: RestartRequest,
    ) -> Result<RestartOutcome, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not restart services".to_owned(),
        })
    }

    /// `restart` for the node itself: the environment of `scope` deployed
    /// from the checkout `root`.
    async fn restart_at(
        &self,
        _scope: &RemoteScope,
        _root: &std::path::Path,
        _request: RestartRequest,
    ) -> Result<RestartOutcome, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not restart services".to_owned(),
        })
    }

    /// Runs a declared job of the environment on this host.
    async fn run_job(
        &self,
        _context: &RemoteRequestContext,
        _request: JobRunRequest,
    ) -> Result<JobRun, MmError> {
        Err(MmError::Environment {
            message: "this daemon does not run jobs".to_owned(),
        })
    }

    /// The job runs this host recorded, newest first.
    async fn job_runs(
        &self,
        _context: &RemoteRequestContext,
        _request: JobRunsRequest,
    ) -> Result<Vec<JobRun>, MmError> {
        Ok(Vec::new())
    }

    /// The age recipient of this host's store for the project.
    async fn secret_recipient(&self, _context: &RemoteRequestContext) -> Result<String, MmError> {
        Err(no_values())
    }

    /// One step of staging secrets and variables on this host.
    async fn stage_values(
        &self,
        _context: &RemoteRequestContext,
        _stage: crate::domain::staged::ValueStage,
    ) -> Result<crate::domain::staged::StageAnswer, MmError> {
        Err(no_values())
    }

    /// What this host holds and has staged for the deployed commit's keys.
    async fn host_values(
        &self,
        _context: &RemoteRequestContext,
    ) -> Result<crate::domain::staged::NodeValues, MmError> {
        Err(no_values())
    }

    /// This host's member of the environment's managed secret store, if any.
    async fn secret_store(
        &self,
        _scope: &crate::domain::remote::RemoteScope,
    ) -> Option<crate::domain::secret_store::StoreStatus> {
        None
    }

    /// A snapshot of this host's member, with its keys, sealed to `recipient`.
    async fn store_snapshot(
        &self,
        _context: &RemoteRequestContext,
        _recipient: &str,
    ) -> Result<String, MmError> {
        Err(no_store())
    }

    /// One step of putting a snapshot of the managed store back here.
    async fn store_restore(
        &self,
        _context: &RemoteRequestContext,
        _step: crate::domain::secret_store::StoreRestoreStep,
    ) -> Result<crate::domain::secret_store::StoreRestored, MmError> {
        Err(no_store())
    }

    /// The deployed commit's age secrets, written into the store `provider`.
    async fn store_migrate(
        &self,
        _context: &RemoteRequestContext,
        _provider: &str,
    ) -> Result<crate::domain::secret_store::StoreMigrated, MmError> {
        Err(no_store())
    }

    /// The staged changes a plan of the checkout in `context` makes live.
    async fn staged_plan(
        &self,
        _context: &RemoteRequestContext,
    ) -> Result<Vec<crate::domain::staged::StagedPlanItem>, MmError> {
        Ok(Vec::new())
    }

    /// Makes the staged changes a deployment planned live.
    async fn go_live(
        &self,
        _context: &RemoteRequestContext,
        _staged: &[crate::domain::staged::StagedPlanItem],
    ) -> Result<(), MmError> {
        Ok(())
    }

    /// The environment's job runs that run on this host now.
    async fn running_job_runs(
        &self,
        _context: &RemoteRequestContext,
    ) -> Result<Vec<crate::domain::rollout::OperationInFlight>, MmError> {
        Ok(Vec::new())
    }

    /// Stops the job runs a cancelled operation started here that have
    /// not ended: their containers stop and each run is recorded as
    /// cancelled. Returns those runs.
    async fn cancel_job_runs(
        &self,
        _context: &RemoteRequestContext,
        _started: &[crate::domain::scheduler::StartedRuns],
    ) -> Result<Vec<JobRun>, MmError> {
        Ok(Vec::new())
    }
}

/// One commit's bundle on this machine, ready to send to hosts that take
/// the project's files from the client.
#[derive(Clone)]
pub struct PreparedBundle {
    pub offer: crate::domain::delivery::BundleOffer,
    pub files: u64,
    /// The bundle's file; `None` when this checkout does not hold the
    /// commit, so there is nothing to send.
    pub path: Option<std::path::PathBuf>,
    /// Keeps the file until the last copy of this is dropped.
    pub keep: Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>,
}

/// Builds the bundle of a commit from the checkout mm runs in.
#[async_trait]
pub trait CommitBundles: Send + Sync {
    async fn bundle(&self, commit: &str) -> Result<PreparedBundle, MmError>;
}

#[async_trait]
pub trait RemoteTransport: Send + Sync {
    async fn status(&self, scope: RemoteScope) -> Result<WorkspaceSnapshot, MmError>;

    /// Offers a commit's bundle to a host the project is pushed to, and
    /// sends the file at `bundle` when the host does not hold the commit.
    async fn deliver(
        &self,
        _scope: RemoteScope,
        _offer: crate::domain::delivery::BundleOffer,
        _bundle: Option<std::path::PathBuf>,
    ) -> Result<crate::domain::delivery::Delivered, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot deliver a commit's files".to_owned(),
        })
    }

    async fn logs(
        &self,
        scope: RemoteScope,
        request: LogRequest,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError>;

    async fn exec(
        &self,
        scope: RemoteScope,
        request: ExecRequest,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError>;

    async fn plan_deployment(
        &self,
        scope: RemoteScope,
        commit: String,
        receipt: Option<BuildReceipt>,
        release: crate::domain::release_facts::ReleaseRequest,
    ) -> Result<RemoteDeploymentPlan, MmError>;

    /// Applies a reviewed plan. `None` when the host is a cluster node the
    /// plan places no service on.
    async fn apply_deployment(
        &self,
        scope: RemoteScope,
        remote_plan_hash: String,
    ) -> Result<Option<DeploymentResult>, MmError>;

    /// After every node of cluster plan `cluster_plan_hash` has applied:
    /// has this node stop the services its section moved to other nodes.
    /// Returns what it stopped.
    async fn finish_moves(
        &self,
        _scope: RemoteScope,
        _cluster_plan_hash: String,
    ) -> Result<Vec<crate::domain::placement::PlacedService>, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot finish the moves of a cluster plan".to_owned(),
        })
    }

    /// One step of rolling out the reviewed plan `plan_hash` on this host.
    /// `patience` is how long the step may take.
    async fn rollout(
        &self,
        _scope: RemoteScope,
        _plan_hash: String,
        _step: crate::domain::rollout::RolloutStep,
        _patience: std::time::Duration,
    ) -> Result<crate::domain::rollout::RolloutAnswer, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot roll a deployment out in steps".to_owned(),
        })
    }

    /// What the host's resolver answers for the environment's names.
    async fn names(
        &self,
        _scope: RemoteScope,
    ) -> Result<crate::domain::discovery::NameReport, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot read a node's names".to_owned(),
        })
    }

    /// The deployments the host remembers.
    async fn deployment_status(
        &self,
        _scope: RemoteScope,
    ) -> Result<crate::domain::rollout::NodeDeploymentStatus, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot read a host's deployments".to_owned(),
        })
    }

    /// Stops what runs for the environment on the host (`operation`, by
    /// its ID or the start of it, or whatever runs), and undoes a
    /// deployment that changed the host.
    async fn cancel(
        &self,
        _scope: RemoteScope,
        _operation: Option<String>,
    ) -> Result<crate::domain::rollout::CancelOutcome, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot cancel an operation on a host".to_owned(),
        })
    }

    /// Has a cluster node keep the seal that binds its plan into a cluster
    /// plan; the node refuses a seal that does not cover its plan.
    async fn seal_plan(
        &self,
        scope: RemoteScope,
        seal: ClusterPlanSeal,
    ) -> Result<ClusterPlanSeal, MmError>;

    /// The seal a cluster node keeps for a cluster plan it can still apply.
    async fn sealed_plan(
        &self,
        scope: RemoteScope,
        cluster_plan_hash: String,
    ) -> Result<ClusterPlanSeal, MmError>;

    /// The host's node name, `mmd` version, and last applied deployment.
    async fn node(&self, scope: RemoteScope) -> Result<NodeFacts, MmError>;

    /// The host's WireGuard interface: each peer's endpoint, last
    /// handshake, and traffic.
    async fn mesh_status(
        &self,
        _scope: RemoteScope,
    ) -> Result<crate::domain::mesh::MeshNodeStatus, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot read a node's mesh".to_owned(),
        })
    }

    /// The cluster networks the host stored, and what differs on it.
    async fn networks(
        &self,
        _scope: RemoteScope,
    ) -> Result<crate::domain::cluster_network::NodeNetworkStatus, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot read a node's networks".to_owned(),
        })
    }

    /// Each OCI runtime the host's policy offers, tried on its engine.
    async fn runtimes(
        &self,
        _scope: RemoteScope,
    ) -> Result<crate::domain::hardening::HostRuntimeStatus, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot ask a host about its runtimes".to_owned(),
        })
    }

    /// What the host says about the cluster's shared volumes, which
    /// `storage_node` keeps.
    async fn storage_status(
        &self,
        _scope: RemoteScope,
        _storage_node: String,
        _sizes: bool,
    ) -> Result<crate::domain::shared_storage::StorageNodeStatus, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot read a node's shared storage".to_owned(),
        })
    }

    /// What deleting a shared volume would remove on the storage node.
    async fn plan_volume_delete(
        &self,
        _scope: RemoteScope,
        _volume: crate::domain::shared_storage::VolumeName,
    ) -> Result<crate::domain::shared_storage::VolumeDeletePlan, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot delete a shared volume".to_owned(),
        })
    }

    /// Deletes a shared volume on the storage node, once its plan was
    /// reviewed.
    async fn delete_volume(
        &self,
        _scope: RemoteScope,
        _volume: crate::domain::shared_storage::VolumeName,
        _plan_hash: String,
    ) -> Result<crate::domain::shared_storage::VolumeDeleted, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot delete a shared volume".to_owned(),
        })
    }

    async fn plan_team_sync(
        &self,
        scope: RemoteScope,
        commit: String,
    ) -> Result<RemoteAccessPlan, MmError>;

    async fn apply_team_sync(
        &self,
        scope: RemoteScope,
        remote_plan_hash: String,
    ) -> Result<AccessChange, MmError>;

    async fn whoami(&self, scope: RemoteScope) -> Result<RemoteIdentity, MmError>;

    /// Restarts a service's containers on the host and waits until they
    /// are healthy again.
    async fn restart(
        &self,
        _scope: RemoteScope,
        _request: RestartRequest,
    ) -> Result<RestartOutcome, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot restart a service".to_owned(),
        })
    }

    /// Runs a declared job on the host.
    async fn run_job(
        &self,
        _scope: RemoteScope,
        _request: JobRunRequest,
        _patience: std::time::Duration,
    ) -> Result<JobRun, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot run a job".to_owned(),
        })
    }

    /// The job runs the host recorded, newest first.
    async fn job_runs(
        &self,
        _scope: RemoteScope,
        _request: JobRunsRequest,
    ) -> Result<Vec<JobRun>, MmError> {
        Err(MmError::Environment {
            message: "this connection cannot read a host's job runs".to_owned(),
        })
    }

    /// The age recipient of the host's store.
    async fn secret_recipient(&self, _scope: RemoteScope) -> Result<String, MmError> {
        Err(no_values())
    }

    /// One step of staging secrets and variables on the host.
    async fn stage_values(
        &self,
        _scope: RemoteScope,
        _stage: crate::domain::staged::ValueStage,
    ) -> Result<crate::domain::staged::StageAnswer, MmError> {
        Err(no_values())
    }

    /// What the host holds and has staged for each declared key.
    async fn host_values(
        &self,
        _scope: RemoteScope,
    ) -> Result<crate::domain::staged::NodeValues, MmError> {
        Err(no_values())
    }

    /// A snapshot of the host's member, with its keys, sealed to `recipient`.
    async fn store_snapshot(&self, _scope: RemoteScope, _to: String) -> Result<String, MmError> {
        Err(no_store())
    }

    /// One step of putting a snapshot of the managed store back on the host.
    async fn store_restore(
        &self,
        _scope: RemoteScope,
        _step: crate::domain::secret_store::StoreRestoreStep,
    ) -> Result<crate::domain::secret_store::StoreRestored, MmError> {
        Err(no_store())
    }

    /// The host's age secrets, moved into the managed store `provider`.
    async fn store_migrate(
        &self,
        _scope: RemoteScope,
        _provider: String,
    ) -> Result<crate::domain::secret_store::StoreMigrated, MmError> {
        Err(no_store())
    }

    /// Ends the connection this transport keeps open between requests, if
    /// it keeps one. Called after access on the host changed, so nothing
    /// authenticated before the change is used after it.
    fn close_kept_connection(&self) {}

    /// Carries one accepted local TCP connection to `service`'s `port` until
    /// either side closes it.
    async fn forward(
        &self,
        scope: RemoteScope,
        service: String,
        port: u16,
        local: tokio::net::TcpStream,
    ) -> Result<crate::domain::forward::ForwardTotals, MmError>;
}

fn no_store() -> MmError {
    MmError::Target {
        message: "this host runs no member of a managed secret store".to_owned(),
    }
}

fn no_values() -> MmError {
    MmError::Environment {
        message: "this connection cannot read or stage secrets and variables".to_owned(),
    }
}
