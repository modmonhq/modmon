use super::{ClusterSetupRequest, ClusterSetupService};
use crate::app::ports::{ProgressReporter, progress_step};
use crate::domain::cluster_setup::{ClusterSetupPlan, ClusterSetupResult, ClusterSetupStep};
use crate::domain::error::MmError;
use crate::domain::progress::{NodeState, ProgressEvent};

/// What a person reads while a step runs, and once it is done.
#[must_use]
pub fn step_text(step: ClusterSetupStep, plan: &ClusterSetupPlan) -> (String, String) {
    let nodes = match plan.nodes.len() {
        1 => "1 node".to_owned(),
        count => format!("{count} nodes"),
    };
    let secrets = match plan.secrets.len() {
        1 => "1 secret".to_owned(),
        count => format!("{count} secrets"),
    };
    match step {
        ClusterSetupStep::PrepareNodes => (
            format!("Preparing {nodes}"),
            format!("Prepared {nodes}: names, node facts, mesh tools"),
        ),
        ClusterSetupStep::ProvisionNodes => (
            format!("Provisioning {nodes}"),
            format!("Provisioned {nodes}: {} and mmd", plan.runtime.name()),
        ),
        ClusterSetupStep::ConnectNodes => (
            "Connecting the mesh".to_owned(),
            "Connected the mesh".to_owned(),
        ),
        ClusterSetupStep::ServeStorage => (
            format!("Serving shared volumes from {}", plan.storage),
            format!("Serving shared volumes from {}", plan.storage),
        ),
        ClusterSetupStep::SecretStore => {
            let version = plan
                .secret_store
                .as_ref()
                .map_or("", |store| store.version.as_str());
            (
                format!("Setting up the secret store, OpenBao {version}"),
                "Secret store ready".to_owned(),
            )
        }
        ClusterSetupStep::PushSecrets => (
            format!("Pushing {secrets}"),
            format!("Pushed {secrets} to {nodes}"),
        ),
        ClusterSetupStep::SyncAccess => (
            "Syncing users".to_owned(),
            format!("Synced users from {}", short(&plan.access_commit)),
        ),
        ClusterSetupStep::Verify => (format!("Verifying {nodes}"), format!("Verified {nodes}")),
    }
}

fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

impl ClusterSetupService {
    /// Re-plans, refuses a plan that differs from the reviewed hash, and runs
    /// every step. Re-running after a failure resumes safely.
    pub async fn apply(
        &self,
        request: &ClusterSetupRequest,
        reviewed_hash: &str,
        progress: &dyn ProgressReporter,
    ) -> Result<ClusterSetupResult, MmError> {
        let plan = self.plan_reporting(request, progress).await?;
        if plan.plan_hash != reviewed_hash {
            return Err(MmError::State {
                message: format!(
                    "cluster setup plan changed (expected {reviewed_hash}, current {}); review the new plan",
                    plan.plan_hash
                ),
            });
        }
        plan.validate()
            .map_err(|message| MmError::Security { message })?;
        let document = self.load(&request.start)?;
        let step = |step: ClusterSetupStep| {
            let (running, done) = step_text(step, &plan);
            (step.label(), running, done)
        };

        let (key, running, done) = step(ClusterSetupStep::PrepareNodes);
        progress_step(
            progress,
            (key, &running),
            self.prepare_nodes(&plan, progress),
            |()| Some(done),
        )
        .await?;
        let (key, running, done) = step(ClusterSetupStep::ProvisionNodes);
        let records = progress_step(
            progress,
            (key, &running),
            self.provision_nodes(&plan, &document.root, progress),
            |_| Some(done),
        )
        .await?;
        let (key, running, done) = step(ClusterSetupStep::ConnectNodes);
        progress_step(progress, (key, &running), self.connect_nodes(&plan), |()| {
            Some(done)
        })
        .await?;
        // Left out of the steps when a subset setup skips the storage node.
        if plan.steps.contains(&ClusterSetupStep::ServeStorage) {
            let (key, running, done) = step(ClusterSetupStep::ServeStorage);
            progress_step(progress, (key, &running), self.serve_storage(&plan), |()| {
                Some(done)
            })
            .await?;
        }
        if plan.steps.contains(&ClusterSetupStep::SecretStore) {
            let (key, running, done) = step(ClusterSetupStep::SecretStore);
            progress_step(
                progress,
                (key, &running),
                self.install_store(&plan, progress),
                |summary| Some(format!("{done}: {summary}")),
            )
            .await?;
        }
        let (key, running, done) = step(ClusterSetupStep::PushSecrets);
        let secrets_changed = progress_step(
            progress,
            (key, &running),
            self.push_secrets(&plan, &document, progress),
            |changed| Some(format!("{done}; {changed} changed")),
        )
        .await?;
        let (key, running, done) = step(ClusterSetupStep::SyncAccess);
        progress_step(progress, (key, &running), self.sync_access(&plan), |()| {
            Some(done)
        })
        .await?;
        let (key, running, done) = step(ClusterSetupStep::Verify);
        progress_step(
            progress,
            (key, &running),
            self.verify(&plan, &records),
            |()| Some(done),
        )
        .await?;

        Ok(ClusterSetupResult {
            project_id: plan.project_id,
            environment: plan.environment.clone(),
            target: plan.target.clone(),
            plan_hash: plan.plan_hash.clone(),
            nodes: records,
            secrets_changed,
            access_commit: plan.access_commit.clone(),
            record_command: format!("mm --environment {} target setup record", plan.environment),
        })
    }
}

/// Every node of `plan` waits for its turn in `step`.
pub(super) fn nodes_wait(
    progress: &dyn ProgressReporter,
    step: ClusterSetupStep,
    plan: &ClusterSetupPlan,
) {
    for node in &plan.nodes {
        progress.report(ProgressEvent::node(
            step.label(),
            &node.name,
            NodeState::Waiting,
            None,
        ));
    }
}
