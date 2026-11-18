//! A deployment rolled out in steps on one node: the environment's
//! deployment lock, the steps in order, and what the node remembers.

use std::sync::atomic::Ordering;

use super::cluster::{Node, refusal};
use super::fixture::next_frame;
use mm::domain::remote::{RemoteMethod, RemoteResult, ServerFrame};
use mm::domain::rollout::{NodeDeploymentStatus, RolloutAnswer, RolloutState, RolloutStep};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NODE: &str = "app-1";

fn step(hash: &str, step: RolloutStep) -> RemoteMethod {
    RemoteMethod::Rollout {
        plan_hash: hash.to_owned(),
        step,
    }
}

fn answer(frame: ServerFrame) -> Result<RolloutAnswer, Box<dyn std::error::Error>> {
    match frame {
        ServerFrame::Result { result, .. } => match *result {
            RemoteResult::Rollout(answer) => Ok(*answer),
            other => Err(format!("the step returned {other:?}").into()),
        },
        other => Err(format!("the step failed: {other:?}").into()),
    }
}

impl Node {
    /// Plans on this node and seals the plan into a cluster plan.
    async fn sealed(&self, id: u128) -> Result<String, Box<dyn std::error::Error>> {
        let planned = self.plan(id, NODE).await?;
        let seal = self.seal(&[
            (NODE, &planned.binding.plan_hash),
            ("app-2", &"8".repeat(64)),
        ])?;
        let hash = seal.cluster_plan_hash.clone();
        let frame = self
            .call(id + 2, Some(NODE), RemoteMethod::SealPlan(Box::new(seal)))
            .await?;
        assert!(matches!(frame, ServerFrame::Result { .. }), "{frame:?}");
        Ok(hash)
    }

    async fn status(&self, id: u128) -> Result<NodeDeploymentStatus, Box<dyn std::error::Error>> {
        match self
            .call(id, Some(NODE), RemoteMethod::DeploymentStatus)
            .await?
        {
            ServerFrame::Result { result, .. } => match *result {
                RemoteResult::DeploymentStatus(status) => Ok(*status),
                other => Err(format!("status returned {other:?}").into()),
            },
            other => Err(format!("status failed: {other:?}").into()),
        }
    }
}

#[tokio::test]
async fn one_deployment_at_a_time_holds_the_environment() -> TestResult {
    let node = Node::named(Some(NODE))?;
    let first = node.sealed(1_000).await?;
    let second = node.sealed(1_010).await?;

    // Nothing begins without the lock.
    let (code, message) = refusal(
        &node
            .call(1_020, Some(NODE), step(&first, RolloutStep::Begin))
            .await?,
    );
    assert_eq!(code, "MM_STATE");
    assert!(
        message.contains("does not hold the deployment lock"),
        "{message}"
    );

    assert_eq!(
        answer(
            node.call(1_030, Some(NODE), step(&first, RolloutStep::Hold))
                .await?
        )?,
        RolloutAnswer::Held
    );
    // The client says it is still there: the same answer, no second lock.
    assert_eq!(
        answer(
            node.call(1_040, Some(NODE), step(&first, RolloutStep::Hold))
                .await?
        )?,
        RolloutAnswer::Held
    );

    // A second deployment is refused with who holds the environment.
    let (code, message) = refusal(
        &node
            .call(1_050, Some(NODE), step(&second, RolloutStep::Hold))
            .await?,
    );
    assert_eq!(code, "MM_DEPLOY_RUNNING");
    assert!(message.contains("started deploying plan"), "{message}");
    assert!(message.contains(&first[..12]), "{message}");
    assert!(message.contains("`mm deploy status`"), "{message}");
    // So is an apply in one call.
    let apply = RemoteMethod::ApplyDeployment {
        remote_plan_hash: second.clone(),
    };
    assert_eq!(
        refusal(&node.call(1_060, Some(NODE), apply).await?).0,
        "MM_DEPLOY_RUNNING"
    );

    // The node's own report says a deployment holds it.
    let rollout = node
        .status(1_070)
        .await?
        .rollout
        .ok_or("the lock is on record")?;
    assert_eq!((rollout.state, rollout.active), (RolloutState::Held, true));
    assert_eq!(rollout.plan_hash, first);

    // Given up, the environment is free for the next one.
    assert_eq!(
        answer(
            node.call(1_080, Some(NODE), step(&first, RolloutStep::Release))
                .await?
        )?,
        RolloutAnswer::Released
    );
    assert_eq!(node.status(1_090).await?.rollout, None);
    assert_eq!(
        answer(
            node.call(1_100, Some(NODE), step(&second, RolloutStep::Hold))
                .await?
        )?,
        RolloutAnswer::Held
    );
    assert!(
        !step(&first, RolloutStep::Hold).is_mutation(),
        "the lock never waits behind a running step"
    );
    assert!(step(&first, RolloutStep::Begin).is_mutation());
    Ok(())
}

#[tokio::test]
async fn a_rollout_is_recorded_step_by_step_and_a_failure_is_rolled_back() -> TestResult {
    let node = Node::named(Some(NODE))?;
    let hash = node.sealed(2_000).await?;
    answer(
        node.call(2_010, Some(NODE), step(&hash, RolloutStep::Hold))
            .await?,
    )?;
    assert_eq!(
        answer(
            node.call(2_020, Some(NODE), step(&hash, RolloutStep::Begin))
                .await?
        )?,
        RolloutAnswer::Begun
    );
    // The plan is spent once the node began.
    let (_, message) = refusal(
        &node
            .call(2_030, Some(NODE), step(&hash, RolloutStep::Begin))
            .await?,
    );
    assert!(message.contains("has begun already"), "{message}");

    let apply = |stack: &str| {
        step(
            &hash,
            RolloutStep::Apply {
                stack: stack.to_owned(),
            },
        )
    };
    let applied = answer(node.call(2_040, Some(NODE), apply("app")).await?)?;
    assert!(
        matches!(&applied, RolloutAnswer::Applied { addresses } if addresses.len() == 1),
        "{applied:?}"
    );
    // A stack that does not become healthy is an answer, not an error.
    let failed = answer(node.call(2_050, Some(NODE), apply("sick")).await?)?;
    let RolloutAnswer::Failed(failure) = failed else {
        return Err("the sick stack was reported healthy".into());
    };
    assert_eq!(failure.service.as_deref(), Some("web"));
    assert_eq!(failure.log, ["cannot reach the database"]);

    let rollout = node
        .status(2_060)
        .await?
        .rollout
        .ok_or("the rollout is on record")?;
    assert_eq!(rollout.state, RolloutState::Running);
    assert_eq!(rollout.stacks, ["app"]);
    assert_eq!(
        rollout.failure.map(|failure| failure.stack),
        Some("sick".to_owned())
    );
    // `mm target nodes` and the terminal UI learn of it from the node report.
    let ServerFrame::Result { result, .. } =
        node.call(2_070, Some(NODE), RemoteMethod::Node).await?
    else {
        return Err("the node report failed".into());
    };
    assert!(matches!(*result, RemoteResult::Node(facts) if facts.rollout.is_some()));

    assert_eq!(
        answer(
            node.call(2_080, Some(NODE), step(&hash, RolloutStep::Rollback))
                .await?
        )?,
        RolloutAnswer::RolledBack
    );
    let status = node.status(2_090).await?;
    let rollout = status
        .rollout
        .ok_or("the rolled-back rollout stays on record")?;
    assert_eq!(
        (rollout.state, rollout.active),
        (RolloutState::RolledBack, false)
    );
    // The node still runs what it ran before.
    assert_eq!(
        status.current.map(|current| current.commit),
        Some("a".repeat(40))
    );
    // Rolling back twice, or a plan that never began, is harmless.
    assert_eq!(
        answer(
            node.call(2_100, Some(NODE), step(&hash, RolloutStep::Rollback))
                .await?
        )?,
        RolloutAnswer::RolledBack
    );
    let audit = std::fs::read_to_string(node.config.state_root.join("audit.jsonl"))?;
    assert!(
        audit.contains("\"result\":\"rollout-apply\""),
        "steps are audited"
    );
    assert!(audit.contains("\"result\":\"rollout-rollback\""));
    Ok(())
}

#[tokio::test]
async fn a_finished_rollout_becomes_the_deployment_and_keeps_the_one_before() -> TestResult {
    let node = Node::named(Some(NODE))?;
    let hash = node.sealed(3_000).await?;
    // The plan's addresses are only stored for the client that holds the
    // environment.
    let (code, _) = refusal(
        &node
            .call(3_005, Some(NODE), step(&hash, RolloutStep::Networks))
            .await?,
    );
    assert_eq!(code, "MM_STATE");
    for (id, rollout_step) in [
        (3_010, RolloutStep::Hold),
        (3_015, RolloutStep::Networks),
        (3_020, RolloutStep::Begin),
        (
            3_030,
            RolloutStep::Apply {
                stack: "app".to_owned(),
            },
        ),
    ] {
        answer(node.call(id, Some(NODE), step(&hash, rollout_step)).await?)?;
    }
    let finished = answer(
        node.call(3_040, Some(NODE), step(&hash, RolloutStep::Finish))
            .await?,
    )?;
    assert!(
        matches!(finished, RolloutAnswer::Finished(Some(_))),
        "{finished:?}"
    );

    let status = node.status(3_050).await?;
    let current = status
        .current
        .ok_or("the finished deployment is recorded")?;
    assert_eq!(current.commit, node.commit);
    assert_eq!(
        current.plan_hash.as_deref(),
        Some(hash.as_str()),
        "under the cluster plan's hash"
    );
    // What the host ran before, although it kept no history then.
    assert_eq!(
        status.previous.map(|previous| previous.commit),
        Some("a".repeat(40))
    );
    let rollout = status
        .rollout
        .ok_or("the finished rollout stays on record")?;
    assert_eq!(
        (rollout.state, rollout.active),
        (RolloutState::Finished, false)
    );

    // Deploying the same commit again keeps the deployment to return to.
    let again = node.sealed(3_060).await?;
    for (id, rollout_step) in [
        (3_070, RolloutStep::Hold),
        (3_080, RolloutStep::Begin),
        (3_090, RolloutStep::Finish),
    ] {
        answer(
            node.call(id, Some(NODE), step(&again, rollout_step))
                .await?,
        )?;
    }
    let status = node.status(3_100).await?;
    assert_eq!(
        status.current.and_then(|current| current.plan_hash),
        Some(again)
    );
    assert_eq!(
        status.previous.map(|previous| previous.commit),
        Some("a".repeat(40))
    );
    Ok(())
}

#[tokio::test]
async fn a_client_that_went_away_loses_the_lock_and_its_rollout_stays_on_record() -> TestResult {
    let node = Node::named(Some(NODE))?;
    let abandoned = node.sealed(4_000).await?;
    answer(
        node.call(4_020, Some(NODE), step(&abandoned, RolloutStep::Hold))
            .await?,
    )?;
    answer(
        node.call(4_030, Some(NODE), step(&abandoned, RolloutStep::Begin))
            .await?,
    )?;
    // Planned once the first one began: its access change is in force.
    let planned = node.plan(4_034, NODE).await?;
    assert_eq!(
        planned.unfinished, None,
        "a rollout a client still holds is not unfinished"
    );
    let next = node.sealed(4_010).await?;
    // The client is gone: nobody renewed the lock for a minute.
    let project = mm::domain::workspace::ProjectId(uuid::Uuid::from_u128(4));
    mm::adapters::remote::HostAccessStore::new(&node.config).update_deployments(
        project,
        "production",
        |stored| {
            if let Some(rollout) = &mut stored.rollout {
                rollout.expires_at_unix_ms = 1;
            }
            Ok(())
        },
    )?;
    let rollout = node
        .status(4_040)
        .await?
        .rollout
        .ok_or("the rollout is on record")?;
    assert_eq!(
        (rollout.state, rollout.active),
        (RolloutState::Running, false)
    );
    assert!(rollout.unfinished());
    // The next plan says so.
    let planned = node.plan(4_044, NODE).await?;
    assert_eq!(
        planned.unfinished.map(|rollout| rollout.plan_hash),
        Some(abandoned.clone())
    );

    // Another deployment takes the lock; until it begins, the unfinished
    // one is what the node reports once that client lets go again.
    assert_eq!(
        answer(
            node.call(4_050, Some(NODE), step(&next, RolloutStep::Hold))
                .await?
        )?,
        RolloutAnswer::Held
    );
    assert_eq!(
        node.status(4_060)
            .await?
            .rollout
            .map(|rollout| rollout.plan_hash),
        Some(next.clone())
    );
    answer(
        node.call(4_070, Some(NODE), step(&next, RolloutStep::Release))
            .await?,
    )?;
    let rollout = node
        .status(4_080)
        .await?
        .rollout
        .ok_or("the unfinished rollout is back")?;
    assert_eq!(rollout.plan_hash, abandoned);
    assert!(rollout.unfinished() && !rollout.active);

    // Beginning the next one undoes it, and it is forgotten.
    answer(
        node.call(4_090, Some(NODE), step(&next, RolloutStep::Hold))
            .await?,
    )?;
    answer(
        node.call(4_100, Some(NODE), step(&next, RolloutStep::Begin))
            .await?,
    )?;
    let rollout = node
        .status(4_110)
        .await?
        .rollout
        .ok_or("the new rollout is on record")?;
    assert_eq!(
        (rollout.plan_hash, rollout.state),
        (next, RolloutState::Running)
    );
    Ok(())
}

/// A stack step whose answer cannot reach its client: the client is gone,
/// so nobody renews the lock, and the node frees the environment at once
/// rather than a lease later. Found with the live script's SIGSTOP stage:
/// the step's engine command timed out while `mmd` was stopped and the
/// step ended by itself just as its client left.
#[tokio::test]
async fn a_step_whose_answer_finds_its_client_gone_frees_the_environment() -> TestResult {
    let node = Node::named(Some(NODE))?;
    let hash = node.sealed(3_000).await?;
    answer(
        node.call(3_010, Some(NODE), step(&hash, RolloutStep::Hold))
            .await?,
    )?;
    answer(
        node.call(3_020, Some(NODE), step(&hash, RolloutStep::Begin))
            .await?,
    )?;
    let apply = step(
        &hash,
        RolloutStep::Apply {
            stack: "app".to_owned(),
        },
    );
    node.ask_and_stop_reading(3_030, Some(NODE), apply).await?;
    let rollout = node
        .status(3_040)
        .await?
        .rollout
        .ok_or("the rollout is on record")?;
    assert_eq!(rollout.state, RolloutState::Running);
    assert_eq!(rollout.stacks, ["app"], "the step did its work");
    assert!(
        !rollout.active,
        "the environment is free at once: {rollout:?}"
    );
    Ok(())
}

impl Node {
    /// The node's deployment status once `check` holds for it. Each read
    /// takes its own request ID from `id`.
    async fn status_until(
        &self,
        id: u128,
        check: impl Fn(&NodeDeploymentStatus) -> bool,
    ) -> Result<NodeDeploymentStatus, Box<dyn std::error::Error>> {
        for attempt in 0..300 {
            let status = self.status((id << 16) + 2 * attempt).await?;
            if check(&status) {
                return Ok(status);
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        Err(format!("the node never got there: {:?}", self.status(id).await?).into())
    }

    /// Holds and begins plan `hash` on this node.
    async fn begun(&self, id: u128, hash: &str) -> TestResult {
        answer(
            self.call(id, Some(NODE), step(hash, RolloutStep::Hold))
                .await?,
        )?;
        answer(
            self.call(id + 2, Some(NODE), step(hash, RolloutStep::Begin))
                .await?,
        )?;
        Ok(())
    }
}

/// Waits until `check` holds, at most five seconds.
async fn until(what: &str, check: impl Fn() -> bool) -> TestResult {
    for _ in 0..250 {
        if check() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    Err(format!("never: {what}").into())
}

fn draining(hash: &str) -> RemoteMethod {
    step(
        hash,
        RolloutStep::Apply {
            stack: "draining".to_owned(),
        },
    )
}

/// A step stopped in the middle of an old container's drain, because its
/// client went away: the leave it began would go on without it and remove
/// the container a few seconds later, after the environment reads free, so
/// a plan made meanwhile no longer matched the host when it was applied.
/// The node takes the leaves back first, and holds the environment until
/// it has. Found with the live script's SIGSTOP stage on Podman nodes:
/// "deployment plan changed" after `mmd` ran again.
#[tokio::test]
async fn a_step_whose_client_goes_away_takes_back_its_leaves_before_the_environment_is_free()
-> TestResult {
    let node = Node::named(Some(NODE))?;
    let hash = node.sealed(5_000).await?;
    node.begun(5_010, &hash).await?;
    // A step that ends by itself leaves nothing to take back.
    let apply = step(
        &hash,
        RolloutStep::Apply {
            stack: "app".to_owned(),
        },
    );
    answer(node.call(5_020, Some(NODE), apply).await?)?;
    assert!(node.stopped.taken_back().is_empty());

    node.stopped.slow_take_back.store(true, Ordering::SeqCst);
    let (_session, client) = node.ask(5_030, Some(NODE), draining(&hash)).await?;
    node.status_until(5_040, |status| !status.operations.is_empty())
        .await?;
    drop(client);
    until("the stopped step takes back its leaves", || {
        node.stopped.taken_back() == ["draining"]
    })
    .await?;
    let rollout = node
        .status(5_050)
        .await?
        .rollout
        .ok_or("the rollout is on record")?;
    assert!(
        rollout.active,
        "the environment is held while the leaves are taken back: {rollout:?}"
    );

    node.stopped.slow_take_back.store(false, Ordering::SeqCst);
    let status = node
        .status_until(5_060, |status| status.operations.is_empty())
        .await?;
    let rollout = status.rollout.ok_or("the rollout is on record")?;
    assert_eq!(rollout.state, RolloutState::Running, "left unfinished");
    assert!(
        !rollout.active,
        "free once nothing the step began goes on: {rollout:?}"
    );
    assert_eq!(node.stopped.taken_back(), ["draining"]);
    Ok(())
}

/// A stack step given up because access changed under it (on the VMs,
/// `mmd access block` of the deployer's key, 10a item 8): the lock the step
/// held for its whole budget falls back to one lease, as after any step, so
/// the environment is free a minute after its client can no longer renew
/// it. Before, node 1 stayed "in progress" until the step's deadline (about
/// 23 minutes there) or `mm deploy cancel`.
#[tokio::test]
async fn a_step_given_up_for_an_access_change_holds_the_environment_one_lease_at_most() -> TestResult
{
    let node = Node::named(Some(NODE))?;
    let hash = node.sealed(8_000).await?;
    node.begun(8_010, &hash).await?;
    let (_session, mut client) = node.ask(8_020, Some(NODE), draining(&hash)).await?;
    node.status_until(8_030, |status| !status.operations.is_empty())
        .await?;
    // The break-glass key the sessions use leaves the host policy.
    let policy = std::fs::read_to_string(&node.config.host_policy_path)?;
    std::fs::write(
        &node.config.host_policy_path,
        policy.replace(
            "AAAAC3NzaC1lZDI1NTE5AAAAIJZYcbK/sLQnEppB74bVUmUQnMUahuREnHUmhtFYGcf7",
            "AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w",
        ),
    )?;
    let (code, message) = refusal(&next_frame(&mut client).await?);
    assert_eq!(code, "MM_SECURITY", "{message}");
    let stored = mm::adapters::remote::HostAccessStore::new(&node.config).deployments(
        mm::domain::workspace::ProjectId(uuid::Uuid::from_u128(4)),
        "production",
    )?;
    let rollout = stored.rollout.ok_or("the rollout is on record")?;
    let now = chrono::Utc::now().timestamp_millis();
    assert!(rollout.holds_lock(now), "held for one lease: {rollout:?}");
    assert!(
        !rollout.holds_lock(now + mm::domain::rollout::LOCK_LEASE_MS + 5_000),
        "held longer than one lease: {rollout:?}"
    );
    Ok(())
}

/// `mm deploy cancel` of a step in progress: the step is stopped, takes
/// back its leaves, and leaves the environment to the cancellation, which
/// frees it once the node is rolled back. Until then nobody else may begin
/// a deployment that the rollback would change under it.
#[tokio::test]
async fn a_cancelled_step_holds_the_environment_until_it_is_rolled_back() -> TestResult {
    let node = Node::named(Some(NODE))?;
    let hash = node.sealed(6_000).await?;
    node.begun(6_010, &hash).await?;
    let (_step_session, _step_client) = node.ask(6_020, Some(NODE), draining(&hash)).await?;
    node.status_until(6_030, |status| !status.operations.is_empty())
        .await?;

    node.stopped.slow_rollback.store(true, Ordering::SeqCst);
    let cancel = RemoteMethod::Cancel { operation: None };
    let (_cancel_session, mut canceller) = node.ask(6_040, Some(NODE), cancel).await?;
    until("the cancellation rolls the node back", || {
        node.stopped.rollbacks.load(Ordering::SeqCst) == 1
    })
    .await?;
    assert_eq!(node.stopped.taken_back(), ["draining"]);
    let status = node.status(6_050).await?;
    assert!(status.operations.is_empty(), "the step was stopped");
    let rollout = status.rollout.ok_or("the rollout is on record")?;
    assert!(
        rollout.active,
        "the environment is held while the rollback runs: {rollout:?}"
    );
    let other = node.sealed(6_060).await?;
    let (code, message) = refusal(
        &node
            .call(6_070, Some(NODE), step(&other, RolloutStep::Hold))
            .await?,
    );
    assert_eq!(code, "MM_DEPLOY_RUNNING", "{message}");

    node.stopped.slow_rollback.store(false, Ordering::SeqCst);
    let ServerFrame::Result { result, .. } = next_frame(&mut canceller).await? else {
        return Err("the cancellation failed".into());
    };
    let RemoteResult::Cancelled(outcome) = *result else {
        return Err(format!("unexpected cancel answer: {result:?}").into());
    };
    assert_eq!(outcome.rolled_back.as_deref(), Some(hash.as_str()));
    assert!(outcome.problem.is_none(), "{:?}", outcome.problem);
    let rollout = node
        .status(6_080)
        .await?
        .rollout
        .ok_or("the rollout is on record")?;
    assert_eq!(rollout.state, RolloutState::RolledBack);
    assert!(!rollout.active, "free once rolled back: {rollout:?}");
    Ok(())
}

/// A node's turn can come long after the plan's ten-minute review window
/// closed: on the VMs a 10-minute `before-deploy` migration on node 1 made
/// node 2's Begin fail ("reviewed remote plan has expired") and rolled node
/// 1 back. The window is judged when the node took the hold, which the
/// client does on every node at the start; a hold that lapsed is judged
/// again when it begins.
#[test]
fn a_held_plan_is_judged_by_the_moment_its_node_took_the_hold() -> TestResult {
    let held = |state: &str| -> Result<mm::adapters::remote::StoredRollout, serde_json::Error> {
        serde_json::from_value(serde_json::json!({
            "plan-hash": "a".repeat(64),
            "commit": "b".repeat(40),
            "principal-id": "00000000-0000-0000-0000-000000000007",
            "started-by": "alice",
            "started-at-unix-ms": 1_000,
            "heard-at-unix-ms": 700_000,
            "expires-at-unix-ms": 760_000,
            "state": state,
        }))
    };
    // Eleven minutes after the hold, while the client kept renewing it.
    assert_eq!(held("held")?.reviewed_at(661_000), 1_000);
    // The client stopped renewing: the hold lapsed, the window is now.
    assert_eq!(held("held")?.reviewed_at(800_000), 800_000);
    Ok(())
}
