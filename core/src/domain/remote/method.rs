//! What kind of request a remote method is: its name, the protocol it
//! needs, whether it changes the host, waits for the host's turn, or is an
//! operation in flight, and the operation it belongs to.

use uuid::Uuid;

use super::{MINIMUM_PROTOCOL_VERSION, RemoteMethod};

impl RemoteMethod {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Logs(_) => "logs",
            Self::Exec(_) => "exec",
            Self::PlanDeployment { .. } => "plan_deployment",
            Self::ApplyDeployment { .. } => "apply_deployment",
            Self::PlanTeamSync { .. } => "plan_team_sync",
            Self::ApplyTeamSync { .. } => "apply_team_sync",
            Self::WhoAmI => "whoami",
            Self::Forward { .. } => "forward",
            Self::SealPlan(_) => "seal_plan",
            Self::SealedPlan { .. } => "sealed_plan",
            Self::Node => "node",
            Self::MeshStatus => "mesh_status",
            Self::Networks => "networks",
            Self::FinishMoves { .. } => "finish_moves",
            Self::Rollout { .. } => "rollout",
            Self::DeploymentStatus => "deployment_status",
            Self::Names => "names",
            Self::StorageStatus { .. } => "storage_status",
            Self::Runtimes => "runtimes",
            Self::PlanVolumeDelete { .. } => "plan_volume_delete",
            Self::DeleteVolume { .. } => "delete_volume",
            Self::Deliver(_) => "deliver",
            Self::Restart(_) => "restart",
            Self::RunJob(_) => "run_job",
            Self::JobRuns(_) => "job_runs",
            Self::Cancel { .. } => "cancel",
            Self::SecretRecipient => "secret_recipient",
            Self::StageValues(_) => "stage_values",
            Self::HostValues => "host_values",
            Self::StoreSnapshot { .. } => "store_snapshot",
            Self::StoreRestore(_) => "store_restore",
            Self::StoreMigrate { .. } => "store_migrate",
        }
    }

    /// The oldest protocol whose daemons take this request as meant. A new
    /// method or field does not raise it: a daemon that cannot read a
    /// request refuses it whole. Only a request whose meaning a breaking
    /// protocol changed names that protocol here, so that it is never sent
    /// to a host that agreed on an older one.
    #[must_use]
    pub const fn protocol(&self) -> u16 {
        MINIMUM_PROTOCOL_VERSION
    }

    #[must_use]
    pub const fn is_mutation(&self) -> bool {
        match self {
            Self::Exec(_)
            | Self::ApplyDeployment { .. }
            | Self::ApplyTeamSync { .. }
            | Self::FinishMoves { .. }
            | Self::DeleteVolume { .. }
            | Self::Deliver(_)
            | Self::Restart(_)
            | Self::RunJob(_)
            | Self::StageValues(_)
            | Self::StoreRestore(_)
            | Self::StoreMigrate { .. }
            | Self::Cancel { .. } => true,
            // Taking, renewing, and giving up the deployment lock must not
            // wait behind the step that is running under it.
            Self::Rollout { step, .. } => step.changes_host(),
            _ => false,
        }
    }

    /// Whether the request waits for the host's one turn to change it. A
    /// command in a container and a job do not: they change nothing the
    /// host's deployment records, and a job may run for hours. A
    /// cancellation must not wait behind what it stops.
    #[must_use]
    pub const fn takes_turn(&self) -> bool {
        self.is_mutation() && !matches!(self, Self::Exec(_) | Self::RunJob(_) | Self::Cancel { .. })
    }

    /// Whether the request is an operation in flight while it runs: one
    /// that `mm deploy status` lists, `mm deploy cancel` stops, and whose
    /// deadline the host enforces from outside it. A job run is one even
    /// though it takes no turn.
    #[must_use]
    pub const fn is_operation(&self) -> bool {
        self.is_mutation() && !matches!(self, Self::Exec(_) | Self::Cancel { .. })
    }

    /// The reviewed plan an apply method consumes.
    #[must_use]
    pub fn reviewed_plan(&self) -> Option<&str> {
        match self {
            Self::ApplyDeployment { remote_plan_hash }
            | Self::ApplyTeamSync { remote_plan_hash } => Some(remote_plan_hash),
            Self::Rollout { plan_hash, .. } => Some(plan_hash),
            _ => None,
        }
    }

    /// The ID of the operation a request is part of, when it changes the
    /// host: the reviewed plan every step and node of a deployment shares,
    /// or the request itself. `mm deploy status` shows it and `mm deploy
    /// cancel` takes it.
    #[must_use]
    pub fn operation_id(&self, request_id: Uuid) -> String {
        match self {
            Self::FinishMoves { cluster_plan_hash } => cluster_plan_hash.clone(),
            Self::DeleteVolume { plan_hash, .. } => plan_hash.clone(),
            Self::Deliver(offer) => offer
                .bundle
                .as_ref()
                .map_or_else(|| request_id.to_string(), |bundle| bundle.hash.clone()),
            other => other
                .reviewed_plan()
                .map_or_else(|| request_id.to_string(), str::to_owned),
        }
    }
}
