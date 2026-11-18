//! A daemon as one node of a cluster target: it answers only for its own
//! name, plans only what is placed on it, and applies only a plan that a
//! sealed cluster plan covers.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::fixture::{
    FakeUseCases, StoppedSteps, ask_and_stop_reading, ask_in, call_in, config, frames_in, origin,
    remote_scope,
};
use mm::adapters::remote::{DaemonConfig, RemoteDaemon};
use mm::app::ports::RemoteUseCases;
use mm::domain::observability::{ApprovalContext, LogRequest, ServiceSelector};
use mm::domain::remote::{
    ClusterPlanSeal, ClusterPlanSection, RemoteMethod, RemotePlanKind, RemoteResult, RemoteScope,
    ServerFrame, nothing_placed_hash,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

pub(super) struct Node {
    pub(super) daemon: Arc<RemoteDaemon>,
    pub(super) config: DaemonConfig,
    pub(super) commit: String,
    /// What the host did after a step was stopped.
    pub(super) stopped: Arc<StoppedSteps>,
    _temporary: tempfile::TempDir,
}

impl Node {
    /// A daemon whose host is named `name` (or has no node name).
    pub(super) fn named(name: Option<&str>) -> Result<Self, Box<dyn std::error::Error>> {
        let temporary = crate::temp_dirs::tempdir()?;
        let root = temporary.path().join("project");
        std::fs::create_dir(&root)?;
        let uid = rustix::process::getuid().as_raw();
        let gid = rustix::process::getgid().as_raw();
        let config = config(temporary.path(), &root, uid, gid)?;
        let commit = origin(temporary.path())?.1;
        let stopped = Arc::new(StoppedSteps::default());
        let handler: Arc<dyn RemoteUseCases> = Arc::new(FakeUseCases {
            cancelled: Arc::new(AtomicBool::new(false)),
            stopped: Arc::clone(&stopped),
        });
        let node = Self {
            daemon: Arc::new(RemoteDaemon::new(config.clone(), handler)?),
            config,
            commit,
            stopped,
            _temporary: temporary,
        };
        node.rename(name)?;
        Ok(node)
    }

    /// Rewrites `/etc/mm/node.env`; the daemon reads it on every request.
    fn rename(&self, name: Option<&str>) -> std::io::Result<()> {
        match name {
            Some(name) => std::fs::write(
                &self.config.node_env_path,
                format!(
                    "MM_NODE_NAME={name}\nMM_PRIVATE_INTERFACE=mm0\nMM_PRIVATE_ADDRESS=10.90.0.7\n"
                ),
            ),
            None => match std::fs::remove_file(&self.config.node_env_path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            },
        }
    }

    /// Forgets the deployment the shared test host starts with.
    fn forget_deployment(&self) -> std::io::Result<()> {
        std::fs::remove_file(
            self.config
                .state_root
                .join("projects")
                .join(uuid::Uuid::from_u128(4).to_string())
                .join("deployed/production.json"),
        )
    }

    pub(super) async fn call(
        &self,
        id: u128,
        node: Option<&str>,
        method: RemoteMethod,
    ) -> Result<ServerFrame, Box<dyn std::error::Error>> {
        call_in(&self.daemon, id, scope(node), method).await
    }

    /// Asks, and returns the session open: see `fixture::ask_in`.
    pub(super) async fn ask(
        &self,
        id: u128,
        node: Option<&str>,
        method: RemoteMethod,
    ) -> Result<
        (
            tokio::task::JoinHandle<Result<(), mm::MmError>>,
            tokio::net::UnixStream,
        ),
        Box<dyn std::error::Error>,
    > {
        ask_in(&self.daemon, id, scope(node), method).await
    }

    /// Asks as a client whose way back broke at once.
    pub(super) async fn ask_and_stop_reading(
        &self,
        id: u128,
        node: Option<&str>,
        method: RemoteMethod,
    ) -> Result<(), Box<dyn std::error::Error>> {
        ask_and_stop_reading(&self.daemon, id, scope(node), method).await
    }

    pub(super) async fn plan(
        &self,
        id: u128,
        node: &str,
    ) -> Result<mm::domain::remote::RemoteDeploymentPlan, Box<dyn std::error::Error>> {
        let method = RemoteMethod::PlanDeployment {
            commit: self.commit.clone(),
            receipt: None,
            release: Default::default(),
        };
        match self.call(id, Some(node), method).await? {
            ServerFrame::Result { result, .. } => match *result {
                RemoteResult::DeploymentPlan(plan) => Ok(*plan),
                other => Err(format!("plan returned {other:?}").into()),
            },
            other => Err(format!("plan failed: {other:?}").into()),
        }
    }

    pub(super) fn seal(&self, sections: &[(&str, &str)]) -> Result<ClusterPlanSeal, String> {
        ClusterPlanSeal::new(
            RemotePlanKind::Deployment,
            &scope(None),
            &self.commit,
            sections
                .iter()
                .map(|(node, plan_hash)| ClusterPlanSection {
                    node: (*node).to_owned(),
                    plan_hash: (*plan_hash).to_owned(),
                })
                .collect(),
            Vec::new(),
            (None, Some(mm::domain::rollout::ClusterRollout::default())),
        )
    }
}

/// The cluster target the shared test origin declares: app-1, app-2.
fn scope(node: Option<&str>) -> RemoteScope {
    RemoteScope {
        target: "cluster".to_owned(),
        node: node.map(str::to_owned),
        ..remote_scope(std::path::PathBuf::new())
    }
}

pub(super) fn refusal(frame: &ServerFrame) -> (String, String) {
    match frame {
        ServerFrame::Error { error, .. } => (error.code.clone(), error.message.clone()),
        other => ("not an error".to_owned(), format!("{other:?}")),
    }
}

#[tokio::test]
async fn a_request_for_another_node_is_refused_before_anything_runs() -> TestResult {
    let node = Node::named(Some("app-2"))?;
    let plan = RemoteMethod::PlanDeployment {
        commit: node.commit.clone(),
        receipt: None,
        release: Default::default(),
    };
    let (code, message) = refusal(&node.call(100, Some("app-1"), plan.clone()).await?);
    assert_eq!(code, "MM_NODE_MISMATCH");
    assert!(
        message.contains("for node 'app-1'") && message.contains("is node 'app-2'"),
        "{message}"
    );
    assert!(message.contains("mm target setup record"), "{message}");
    for (id, method) in [
        (110, RemoteMethod::Status),
        (
            120,
            RemoteMethod::ApplyDeployment {
                remote_plan_hash: "a".repeat(64),
            },
        ),
        (
            130,
            RemoteMethod::SealedPlan {
                cluster_plan_hash: "a".repeat(64),
            },
        ),
    ] {
        assert_eq!(
            refusal(&node.call(id, Some("app-1"), method).await?).0,
            "MM_NODE_MISMATCH"
        );
    }

    // A host that was never named is no node at all.
    node.rename(None)?;
    let (code, message) = refusal(&node.call(140, Some("app-1"), plan).await?);
    assert_eq!(code, "MM_NODE_MISMATCH");
    assert!(message.contains("/etc/mm/node.env is missing"), "{message}");

    // It still says which node it is, so `mm target nodes` can show the mix-up.
    node.rename(Some("app-2"))?;
    let ServerFrame::Result { result, .. } =
        node.call(150, Some("app-1"), RemoteMethod::Node).await?
    else {
        return Err("the node report was refused".into());
    };
    let RemoteResult::Node(facts) = *result else {
        return Err("node returned another result".into());
    };
    assert_eq!(facts.node.as_deref(), Some("app-2"));
    assert_eq!(facts.private_address.as_deref(), Some("10.90.0.7"));
    assert_eq!(facts.daemon_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(
        facts.deployed.map(|deployed| deployed.commit),
        Some("a".repeat(40))
    );

    // A request that names no node is for a single host and is not checked.
    assert!(matches!(
        node.call(160, None, RemoteMethod::Node).await?,
        ServerFrame::Result { .. }
    ));
    let audit = std::fs::read_to_string(node.config.state_root.join("audit.jsonl"))?;
    assert!(
        audit.contains("\"error_code\":\"MM_NODE_MISMATCH\""),
        "refusals are audited"
    );
    Ok(())
}

#[tokio::test]
async fn the_first_node_plans_the_services_and_every_other_node_runs_nothing() -> TestResult {
    let first = Node::named(Some("app-1"))?;
    let planned = first.plan(200, "app-1").await?;
    planned.validate()?;
    assert_eq!(planned.binding.node.as_deref(), Some("app-1"));
    assert!(planned.plan.is_some(), "the first node runs every service");

    // The second node ran this environment before and nothing is placed on
    // it now: the engine plans there all the same, to remove what is left.
    let second = Node::named(Some("app-2"))?;
    let leftover = second.plan(210, "app-2").await?;
    leftover.validate()?;
    assert!(
        leftover.plan.is_some(),
        "a node that still runs containers gets an engine plan"
    );

    second.forget_deployment()?;
    let idle = second.plan(220, "app-2").await?;
    idle.validate()?;
    assert_eq!(idle.plan, None, "nothing is placed on the second node");
    assert_eq!(idle.binding.deployment_plan_hash, nothing_placed_hash());
    assert_eq!(idle.binding.node.as_deref(), Some("app-2"));

    // A node the commit does not list cannot be planned.
    second.rename(Some("app-9"))?;
    let method = RemoteMethod::PlanDeployment {
        commit: second.commit.clone(),
        receipt: None,
        release: Default::default(),
    };
    let (code, message) = refusal(&second.call(230, Some("app-9"), method).await?);
    assert_eq!(code, "MM_PLACEMENT");
    assert!(
        message.contains("'app-9' is not one of the target's nodes (app-1, app-2)"),
        "{message}"
    );

    // Nor can a target the commit declares without nodes.
    second.rename(Some("app-2"))?;
    let mut flat = scope(Some("app-2"));
    flat.target = "production".to_owned();
    let method = RemoteMethod::PlanDeployment {
        commit: second.commit.clone(),
        receipt: None,
        release: Default::default(),
    };
    let (code, message) = refusal(&call_in(&second.daemon, 240, flat, method).await?);
    assert_eq!(code, "MM_TARGET");
    assert!(
        message.contains("does not list nodes under targets.production"),
        "{message}"
    );
    Ok(())
}

#[tokio::test]
async fn a_node_applies_only_the_plan_a_sealed_cluster_plan_covers() -> TestResult {
    let node = Node::named(Some("app-1"))?;
    let planned = node.plan(300, "app-1").await?;
    let own = planned.binding.plan_hash.clone();
    let apply = |hash: &str| RemoteMethod::ApplyDeployment {
        remote_plan_hash: hash.to_owned(),
    };

    // The node's own plan hash is not a cluster plan.
    let (_, message) = refusal(&node.call(310, Some("app-1"), apply(&own)).await?);
    assert!(
        message.contains("holds no reviewed cluster plan"),
        "{message}"
    );
    assert!(message.contains("mm deploy plan"), "{message}");

    // A seal must cover exactly the plan this node made.
    let other = node.seal(&[("app-1", &"9".repeat(64)), ("app-2", &"8".repeat(64))])?;
    let frame = node
        .call(320, Some("app-1"), RemoteMethod::SealPlan(Box::new(other)))
        .await?;
    assert!(matches!(frame, ServerFrame::Error { .. }), "{frame:?}");
    let elsewhere = node.seal(&[("app-2", &own)])?;
    let (_, message) = refusal(
        &node
            .call(
                330,
                Some("app-1"),
                RemoteMethod::SealPlan(Box::new(elsewhere)),
            )
            .await?,
    );
    assert!(message.contains("no section for node 'app-1'"), "{message}");
    let mut tampered = node.seal(&[("app-1", &own), ("app-2", &"8".repeat(64))])?;
    tampered.sections[1].plan_hash = "7".repeat(64);
    let (_, message) = refusal(
        &node
            .call(
                340,
                Some("app-1"),
                RemoteMethod::SealPlan(Box::new(tampered)),
            )
            .await?,
    );
    assert!(
        message.contains("does not cover its node sections"),
        "{message}"
    );

    let seal = node.seal(&[("app-1", &own), ("app-2", &"8".repeat(64))])?;
    let hash = seal.cluster_plan_hash.clone();
    let frame = node
        .call(
            350,
            Some("app-1"),
            RemoteMethod::SealPlan(Box::new(seal.clone())),
        )
        .await?;
    assert!(
        matches!(&frame, ServerFrame::Result { result, .. } if **result == RemoteResult::Sealed(Box::new(seal.clone()))),
        "{frame:?}"
    );
    // Sealing the same plan twice is harmless.
    let frame = node
        .call(
            352,
            Some("app-1"),
            RemoteMethod::SealPlan(Box::new(seal.clone())),
        )
        .await?;
    assert!(matches!(frame, ServerFrame::Result { .. }), "{frame:?}");

    let sealed = RemoteMethod::SealedPlan {
        cluster_plan_hash: hash.clone(),
    };
    let frame = node.call(360, Some("app-1"), sealed.clone()).await?;
    assert!(
        matches!(&frame, ServerFrame::Result { result, .. } if **result == RemoteResult::Sealed(Box::new(seal.clone()))),
        "{frame:?}"
    );

    let frame = node.call(370, Some("app-1"), apply(&hash)).await?;
    assert!(
        matches!(&frame, ServerFrame::Result { result, .. } if matches!(**result, RemoteResult::DeploymentApplied(_))),
        "{frame:?}"
    );

    // Applied once: the plan is spent, for the preflight and for a replay.
    let (_, message) = refusal(&node.call(380, Some("app-1"), sealed).await?);
    assert!(message.contains("already applied its part"), "{message}");
    let (code, message) = refusal(&node.call(390, Some("app-1"), apply(&hash)).await?);
    assert_eq!(
        code, "MM_SECURITY",
        "a spent plan is not applied again: {message}"
    );
    let audit = std::fs::read_to_string(node.config.state_root.join("audit.jsonl"))?;
    assert!(
        audit.contains("\"result\":\"sealed\""),
        "sealing is audited"
    );
    Ok(())
}

#[tokio::test]
async fn a_node_that_runs_nothing_records_the_commit_and_serves_no_service() -> TestResult {
    let node = Node::named(Some("app-2"))?;
    node.forget_deployment()?;
    let planned = node.plan(400, "app-2").await?;
    let seal = node.seal(&[
        ("app-1", &"8".repeat(64)),
        ("app-2", &planned.binding.plan_hash),
    ])?;
    let hash = seal.cluster_plan_hash.clone();
    node.call(410, Some("app-2"), RemoteMethod::SealPlan(Box::new(seal)))
        .await?;
    let frame = node
        .call(
            420,
            Some("app-2"),
            RemoteMethod::ApplyDeployment {
                remote_plan_hash: hash,
            },
        )
        .await?;
    assert!(
        matches!(&frame, ServerFrame::Result { result, .. } if **result == RemoteResult::NothingPlaced),
        "{frame:?}"
    );

    let ServerFrame::Result { result, .. } =
        node.call(430, Some("app-2"), RemoteMethod::Node).await?
    else {
        return Err("the node report failed".into());
    };
    let RemoteResult::Node(facts) = *result else {
        return Err("node returned another result".into());
    };
    let deployed = facts.deployed.ok_or("the applied commit is recorded")?;
    assert_eq!(deployed.commit, node.commit);
    assert!(!deployed.runs_services);

    // Its status lists no service and names the deployment, and a log read
    // of a service that runs on other nodes has no lines here: the client
    // asks every node and puts their answers together.
    let ServerFrame::Result { result, .. } =
        node.call(440, Some("app-2"), RemoteMethod::Status).await?
    else {
        return Err("status failed".into());
    };
    let RemoteResult::Status(snapshot) = *result else {
        return Err("status returned another result".into());
    };
    assert!(snapshot.stacks.is_empty());
    assert_eq!(
        snapshot
            .deployment
            .as_ref()
            .map(|deployment| deployment.commit.as_str()),
        Some(node.commit.as_str())
    );
    let logs = RemoteMethod::Logs(LogRequest::new(
        ServiceSelector {
            stack: None,
            module: None,
            service: "web".to_owned(),
            container: None,
        },
        ApprovalContext {
            principal: "ops".to_owned(),
            source: "test".to_owned(),
            approved_high_risk: false,
        },
    ));
    let frames = frames_in(&node.daemon, 450, scope(Some("app-2")), logs).await?;
    assert!(
        matches!(
            frames.as_slice(),
            [
                ServerFrame::Result { result, .. },
                ServerFrame::End { frames: 0, .. }
            ] if **result == RemoteResult::StreamStarted
        ),
        "{frames:?}"
    );
    // A command still needs a container on this node.
    let exec = RemoteMethod::Exec(mm::domain::observability::ExecRequest::new(
        ServiceSelector {
            stack: None,
            module: None,
            service: "web".to_owned(),
            container: None,
        },
        vec!["true".to_owned()],
        ApprovalContext {
            principal: "ops".to_owned(),
            source: "test".to_owned(),
            approved_high_risk: false,
        },
    ));
    let (code, message) = refusal(&node.call(460, Some("app-2"), exec).await?);
    assert_eq!(code, "MM_ENVIRONMENT");
    assert!(
        message.contains("service 'web' does not run on node 'app-2'"),
        "{message}"
    );
    assert!(message.contains("mm service status web"), "{message}");
    Ok(())
}

#[tokio::test]
async fn a_node_reports_its_mesh_under_its_own_name_and_only_when_addressed() -> TestResult {
    let node = Node::named(Some("app-2"))?;
    let ServerFrame::Result { result, .. } = node
        .call(600, Some("app-2"), RemoteMethod::MeshStatus)
        .await?
    else {
        return Err("the mesh status was refused".into());
    };
    let RemoteResult::MeshStatus(status) = *result else {
        return Err("mesh status returned another result".into());
    };
    assert_eq!(
        status.node.as_deref(),
        Some("app-2"),
        "the daemon names itself"
    );
    assert_eq!(
        (status.interface.as_deref(), status.up, status.port),
        (Some("mm0"), true, Some(51820))
    );

    // It is a read like the node list: asked of the wrong host it is refused.
    let (code, _) = refusal(
        &node
            .call(610, Some("app-1"), RemoteMethod::MeshStatus)
            .await?,
    );
    assert_eq!(code, "MM_NODE_MISMATCH");
    assert!(!RemoteMethod::MeshStatus.is_mutation());
    assert_eq!(RemoteMethod::MeshStatus.name(), "mesh_status");
    Ok(())
}

fn node_facts(
    frame: ServerFrame,
) -> Result<mm::domain::remote::NodeFacts, Box<dyn std::error::Error>> {
    match frame {
        ServerFrame::Result { result, .. } => match *result {
            RemoteResult::Node(facts) => Ok(*facts),
            other => Err(format!("node returned {other:?}").into()),
        },
        other => Err(format!("the node report failed: {other:?}").into()),
    }
}

#[tokio::test]
async fn a_moved_service_is_stopped_in_a_step_after_the_apply() -> TestResult {
    let node = Node::named(Some("app-2"))?;
    let planned = node.plan(500, "app-2").await?;
    let leaving = planned
        .plan
        .as_ref()
        .and_then(|plan| plan.node.as_ref())
        .map(|node| {
            node.moved()
                .map(|service| service.service.clone())
                .collect::<Vec<_>>()
        });
    assert_eq!(
        leaving,
        Some(vec!["worker".to_owned()]),
        "the plan lists what moves away"
    );
    let seal = node.seal(&[
        ("app-1", &"8".repeat(64)),
        ("app-2", &planned.binding.plan_hash),
    ])?;
    let hash = seal.cluster_plan_hash.clone();
    let finish = || RemoteMethod::FinishMoves {
        cluster_plan_hash: hash.clone(),
    };

    // Nothing is stopped for a plan this node does not hold or has not applied.
    let (_, message) = refusal(&node.call(510, Some("app-2"), finish()).await?);
    assert!(
        message.contains("holds no reviewed cluster plan"),
        "{message}"
    );
    node.call(520, Some("app-2"), RemoteMethod::SealPlan(Box::new(seal)))
        .await?;
    let (code, message) = refusal(&node.call(530, Some("app-2"), finish()).await?);
    assert_eq!(code, "MM_STATE");
    assert!(
        message.contains("has not applied its part of cluster plan"),
        "{message}"
    );
    assert!(message.contains("nothing was stopped"), "{message}");

    // The apply keeps the moved service: the node still runs containers.
    let frame = node
        .call(
            540,
            Some("app-2"),
            RemoteMethod::ApplyDeployment {
                remote_plan_hash: hash.clone(),
            },
        )
        .await?;
    assert!(
        matches!(&frame, ServerFrame::Result { result, .. } if matches!(**result, RemoteResult::DeploymentApplied(_))),
        "{frame:?}"
    );
    let deployed = node_facts(node.call(550, Some("app-2"), RemoteMethod::Node).await?)?.deployed;
    assert!(deployed.is_some_and(|deployed| deployed.runs_services));

    // The last step stops it, and the node then runs nothing.
    let frame = node.call(560, Some("app-2"), finish()).await?;
    let stopped = match frame {
        ServerFrame::Result { result, .. } => match *result {
            RemoteResult::MovesFinished(stopped) => stopped,
            other => return Err(format!("finish returned {other:?}").into()),
        },
        other => return Err(format!("finish failed: {other:?}").into()),
    };
    assert_eq!(
        stopped
            .iter()
            .map(|service| service.service.as_str())
            .collect::<Vec<_>>(),
        ["worker"]
    );
    let deployed = node_facts(node.call(570, Some("app-2"), RemoteMethod::Node).await?)?.deployed;
    assert!(deployed.is_some_and(|deployed| !deployed.runs_services));
    let audit = std::fs::read_to_string(node.config.state_root.join("audit.jsonl"))?;
    assert!(
        audit.contains("\"result\":\"moves-finished\""),
        "the step is audited"
    );
    assert!(audit.contains("app/worker"), "with what it stopped");

    // A request that names no node is not a cluster node's step.
    let (_, message) = refusal(&node.call(580, None, finish()).await?);
    assert!(
        message.contains("request names no node") || message.contains("this request"),
        "{message}"
    );
    Ok(())
}
