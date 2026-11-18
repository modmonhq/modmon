//! Requests that change a host take turns without stopping a thread, and
//! access state is locked only for short sections. What that must not
//! give away: a principal whose access is taken away while its operation
//! runs does not finish a step it could not start now.

mod daemon_host;
mod temp_dirs;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use daemon_host::{ALICE, Host, OPS, PROMPT, TestResult, exec_request};
use mm::domain::remote::{RemoteMethod, RemoteResult, RemoteStreamItem, ServerFrame};
use mm::domain::rollout::RolloutStep;

/// A key that has no access anywhere: blocking it changes access state
/// and nobody's rights.
const STRANGER: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINlgyKENv7afsOxxzNFVX/GU+789xK97zuDNx8Thx+g+";

/// Starts alice's deployment and holds it open inside the engine.
async fn held_deployment(host: &Host) -> TestResult<daemon_host::Client> {
    let plan = host.plan(ALICE).await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let apply = RemoteMethod::ApplyDeployment {
        remote_plan_hash: plan,
    };
    let client = host.ask(ALICE, apply).await?;
    host.handlers.entered(1).await?;
    Ok(client)
}

#[tokio::test]
async fn a_second_change_to_the_host_waits_its_turn_and_then_runs() -> TestResult {
    let host = Host::with_alice().await?;
    let mut deployment = held_deployment(&host).await?;
    // Planned while the deployment runs, so it is a plan of access as the
    // deployment left it.
    let sync = host.sync_plan(OPS).await?;
    let apply = RemoteMethod::ApplyTeamSync {
        remote_plan_hash: sync,
    };
    let mut second = host.ask(OPS, apply).await?;
    assert!(
        second.is_silent(Duration::from_millis(500)).await,
        "the access sync ran next to the deployment"
    );
    // Reads are answered meanwhile.
    assert!(matches!(
        host.ask(OPS, RemoteMethod::Status)
            .await?
            .result(PROMPT)
            .await?,
        RemoteResult::Status(_)
    ));
    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    assert!(matches!(
        deployment.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    assert!(matches!(
        second.result(PROMPT).await?,
        RemoteResult::AccessApplied(_)
    ));
    Ok(())
}

#[tokio::test]
async fn a_change_that_does_not_get_its_turn_in_time_says_who_has_it_and_changes_nothing()
-> TestResult {
    let host = Host::with_alice().await?;
    let mut deployment = held_deployment(&host).await?;
    let sync = host.sync_plan(OPS).await?;
    let apply = RemoteMethod::ApplyTeamSync {
        remote_plan_hash: sync.clone(),
    };
    let began = Instant::now();
    // Its own deadline is five and a half seconds away; it gives up two
    // seconds before that, so its client still hears why.
    let mut waiting = host
        .ask_within(OPS, apply, Duration::from_millis(5_500))
        .await?;
    let frame = waiting.next(PROMPT).await?;
    let ServerFrame::Error { error, .. } = frame else {
        return Err(format!("expected a refusal, got {frame:?}").into());
    };
    assert!(
        (Duration::from_millis(3_400)..Duration::from_millis(5_000)).contains(&began.elapsed()),
        "it waits until shortly before its own deadline, not {:?}",
        began.elapsed()
    );
    assert_eq!(error.code, "MM_HOST_BUSY");
    assert_eq!(
        error.mutation_state,
        mm::domain::remote::MutationState::NotStarted,
        "a request that never had its turn changed nothing"
    );
    for part in [
        "alice's deployment has been changing this host since 20",
        "waited 3 s",
        "changed nothing",
        "`mm deploy status`",
    ] {
        assert!(error.message.contains(part), "{}", error.message);
    }
    // The plan it did not apply is still good once the deployment is done.
    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    assert!(matches!(
        deployment.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    let apply = RemoteMethod::ApplyTeamSync {
        remote_plan_hash: sync,
    };
    assert!(matches!(
        host.ask(OPS, apply).await?.result(PROMPT).await?,
        RemoteResult::AccessApplied(_)
    ));
    Ok(())
}

#[tokio::test]
async fn a_change_whose_client_went_away_while_it_waited_never_runs() -> TestResult {
    let host = Host::with_alice().await?;
    let mut deployment = held_deployment(&host).await?;
    let sync = host.sync_plan(OPS).await?;
    let apply = |hash: &str| RemoteMethod::ApplyTeamSync {
        remote_plan_hash: hash.to_owned(),
    };
    let mut waiting = host.ask(OPS, apply(&sync)).await?;
    assert!(waiting.is_silent(Duration::from_millis(300)).await);
    waiting.hang_up();
    // The daemon notices at once, not when the request's turn comes.
    let mut given_up = None;
    for _ in 0..300 {
        given_up = host
            .audit()?
            .into_iter()
            .find(|record| record["method"] == "apply_team_sync" && record["result"] == "failed");
        if given_up.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let given_up = given_up.ok_or("the abandoned request was not given up")?;
    // A request whose client went away is cancelled, not a state error.
    assert_eq!(given_up["error_code"], "MM_CANCELLED");
    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    assert!(matches!(
        deployment.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    // It never ran: its plan is still unclaimed and applies now.
    assert!(matches!(
        host.ask(OPS, apply(&sync)).await?.result(PROMPT).await?,
        RemoteResult::AccessApplied(_)
    ));
    Ok(())
}

/// The security property. Before, an access change waited for the lock a
/// running operation held; now it lands at once, and the operation of a
/// principal it revokes is given up instead of finished.
#[tokio::test]
async fn a_key_blocked_while_its_deployment_runs_does_not_finish_the_deployment() -> TestResult {
    let host = Host::with_alice().await?;
    let before = host.deployed_commit()?;
    let mut deployment = held_deployment(&host).await?;

    let began = Instant::now();
    host.block(ALICE, true).await?;
    assert!(
        began.elapsed() < Duration::from_secs(1),
        "the block waited for the running deployment"
    );

    let (code, message) = deployment.refusal(PROMPT).await?;
    assert_eq!(code, "MM_SECURITY");
    assert!(
        message.contains("access to 'production' changed while this operation ran")
            && message.contains("alice may no longer run it"),
        "{message}"
    );
    assert_eq!(
        host.handlers.abandoned.load(Ordering::SeqCst),
        1,
        "the engine's work was given up"
    );
    assert_eq!(host.handlers.finished.load(Ordering::SeqCst), 0);
    assert_eq!(
        host.deployed_commit()?,
        before,
        "the deployment was not recorded as done"
    );
    let failed = host
        .audit()?
        .into_iter()
        .rev()
        .find(|record| record["method"] == "apply_deployment" && record["result"] == "failed")
        .ok_or("the given-up deployment is not in the audit log")?;
    assert_eq!(failed["error_code"], "MM_SECURITY");

    // She cannot start anything now, and the host is free for others.
    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    let (code, _) = host
        .ask(ALICE, RemoteMethod::Status)
        .await?
        .refusal(PROMPT)
        .await?;
    assert_eq!(code, "MM_SECURITY");
    let plan = host.plan(OPS).await?;
    let apply = RemoteMethod::ApplyDeployment {
        remote_plan_hash: plan,
    };
    assert!(matches!(
        host.ask(OPS, apply).await?.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));

    // With the block lifted she deploys again.
    host.block(ALICE, false).await?;
    let plan = host.plan(ALICE).await?;
    let apply = RemoteMethod::ApplyDeployment {
        remote_plan_hash: plan,
    };
    assert!(matches!(
        host.ask(ALICE, apply).await?.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    Ok(())
}

#[tokio::test]
async fn a_key_blocked_while_its_command_runs_loses_the_command() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_exec.store(true, Ordering::SeqCst);
    let mut command = host.ask(ALICE, RemoteMethod::Exec(exec_request())).await?;
    assert!(matches!(
        command.result(PROMPT).await?,
        RemoteResult::StreamStarted
    ));
    assert!(matches!(
        command.next(PROMPT).await?,
        ServerFrame::Stream { item, .. } if matches!(*item, RemoteStreamItem::Exec(_))
    ));
    host.handlers.entered(1).await?;
    assert!(command.is_silent(Duration::from_millis(300)).await);

    host.block(ALICE, true).await?;
    let (code, message) = command.refusal(PROMPT).await?;
    assert_eq!(code, "MM_SECURITY");
    assert!(message.contains("alice may no longer run it"), "{message}");
    for _ in 0..200 {
        if host.handlers.abandoned.load(Ordering::SeqCst) == 1 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("the command was not cancelled in its container".into())
}

#[tokio::test]
async fn an_access_change_that_leaves_the_principal_its_rights_does_not_stop_its_deployment()
-> TestResult {
    let host = Host::with_alice().await?;
    let mut deployment = held_deployment(&host).await?;
    host.block(STRANGER, true).await?;
    // Longer than the daemon takes to notice a change.
    assert!(
        deployment.is_silent(Duration::from_millis(3_000)).await,
        "the deployment was disturbed"
    );
    assert_eq!(host.handlers.abandoned.load(Ordering::SeqCst), 0);
    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    assert!(matches!(
        deployment.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    assert_eq!(host.deployed_commit()?, host.commit);
    Ok(())
}

/// Waiting for the lock on access state never stops a thread, ends after
/// ten seconds, and then names the process that holds it.
#[tokio::test]
async fn waiting_for_the_access_lock_is_bounded_and_names_the_holder() -> TestResult {
    let host = Host::new()?;
    let (stuck, waiting) = (host.root(), host.root());
    let (held, hold) = std::sync::mpsc::channel::<()>();
    let (release, released) = std::sync::mpsc::channel::<()>();
    // Another process that took the lock and got stuck with it.
    let holder = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(stuck.locked("mmd access block", |_| {
                held.send(()).map_err(|_| mm::MmError::State {
                    message: "nobody waits for the lock".to_owned(),
                })?;
                // Until the test is done, at most.
                released
                    .recv_timeout(Duration::from_secs(30))
                    .map_or(Ok(()), Ok)
            }))
            .map_err(|error| std::io::Error::other(error.to_string()))
    });
    hold.recv_timeout(PROMPT)?;

    // This runtime has one thread. While it waits for the lock, its other
    // tasks keep running.
    let ticks = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let ticking = tokio::spawn({
        let ticks = std::sync::Arc::clone(&ticks);
        async move {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                ticks.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    let began = Instant::now();
    let refused = waiting.render_current_keys().await;
    let waited = began.elapsed();
    ticking.abort();
    release.send(())?;
    holder.join().map_err(|_| "the holding thread panicked")??;

    let message = refused.err().ok_or("the lock was taken twice")?.to_string();
    assert!(
        (Duration::from_secs(9)..Duration::from_secs(13)).contains(&waited),
        "waited {waited:?}"
    );
    assert!(
        ticks.load(Ordering::SeqCst) > 100,
        "the thread was stopped while it waited"
    );
    let holder = format!("process {} (mmd access block)", std::process::id());
    for part in [
        "access.lock is locked",
        holder.as_str(),
        "more than 10 s",
        "nothing was changed",
    ] {
        assert!(message.contains(part), "{message}");
    }
    // With the holder gone the lock is free and its note is cleared.
    waiting.render_current_keys().await?;
    assert_eq!(
        std::fs::read_to_string(host.config.state_root.join("access.lock"))?,
        ""
    );
    Ok(())
}

/// Alice may deploy, and the commit she deploys gives her only `view`.
const LOWERED: (&str, &str) = (
    "production: {level: deploy, exec: [web], secrets: true}",
    "production: {level: view}",
);

fn rollout(plan: &str, step: RolloutStep) -> RemoteMethod {
    RemoteMethod::Rollout {
        plan_hash: plan.to_owned(),
        step,
    }
}

#[tokio::test]
async fn a_rollout_that_lowers_its_deployer_is_still_theirs_to_finish_and_nothing_more()
-> TestResult {
    let host = Host::with_alice().await?;
    let lowered = host.commit_with(LOWERED.0, LOWERED.1)?;
    let plan = host.plan_at(ALICE, &lowered).await?;
    // Begin applies the commit's access first: alice may only view from
    // then on, and she still carries her rollout to its end.
    for step in [RolloutStep::Hold, RolloutStep::Begin, RolloutStep::Finish] {
        let name = step.name();
        let answer = host
            .ask(ALICE, rollout(&plan, step))
            .await?
            .result(PROMPT)
            .await;
        assert!(
            matches!(answer, Ok(RemoteResult::Rollout(_))),
            "{name}: {answer:?}"
        );
    }
    assert_eq!(host.deployed_commit()?, lowered);
    // Everything else is decided by her access as it is now: she reads,
    // and no longer runs commands in web.
    host.ask(ALICE, RemoteMethod::Status)
        .await?
        .result(PROMPT)
        .await?;
    let (code, _) = host
        .ask(ALICE, RemoteMethod::Exec(exec_request()))
        .await?
        .refusal(PROMPT)
        .await?;
    assert_eq!(code, "MM_SECURITY");
    Ok(())
}

#[tokio::test]
async fn a_lowered_deployer_may_abort_and_another_access_change_ends_even_that() -> TestResult {
    // Her abort is hers too.
    let host = Host::with_alice().await?;
    let lowered = host.commit_with(LOWERED.0, LOWERED.1)?;
    let plan = host.plan_at(ALICE, &lowered).await?;
    for step in [RolloutStep::Hold, RolloutStep::Begin, RolloutStep::Rollback] {
        let name = step.name();
        let answer = host
            .ask(ALICE, rollout(&plan, step))
            .await?
            .result(PROMPT)
            .await;
        assert!(
            matches!(answer, Ok(RemoteResult::Rollout(_))),
            "{name}: {answer:?}"
        );
    }

    // Once access to the environment changed again, here a block of
    // another key, her steps are decided by her access as it is.
    let host = Host::with_alice().await?;
    let lowered = host.commit_with(LOWERED.0, LOWERED.1)?;
    let plan = host.plan_at(ALICE, &lowered).await?;
    for step in [RolloutStep::Hold, RolloutStep::Begin] {
        host.ask(ALICE, rollout(&plan, step))
            .await?
            .result(PROMPT)
            .await?;
    }
    host.block(STRANGER, true).await?;
    let (code, _) = host
        .ask(ALICE, rollout(&plan, RolloutStep::Finish))
        .await?
        .refusal(PROMPT)
        .await?;
    assert_eq!(code, "MM_SECURITY");
    Ok(())
}

/// Applies the users file of `commit` as `mmd access sync` does.
async fn root_sync(host: &Host, commit: &str) -> TestResult {
    let request = mm::adapters::remote::RootAccessSyncRequest {
        project_id: daemon_host::project(),
        environment: "production".to_owned(),
        commit: commit.to_owned(),
        dry_run: false,
    };
    let daemon = std::sync::Arc::clone(&host.daemon);
    host.on_daemon(async move { daemon.sync_access_as_root(&request).await })
        .await??;
    Ok(())
}

#[tokio::test]
async fn a_sync_that_changes_no_grant_keeps_a_reviewed_plan_and_one_that_does_not_does_not()
-> TestResult {
    let host = Host::with_alice().await?;
    let plan = host.plan(ALICE).await?;
    let same = host.commit_with(
        "production: {level: operate}",
        "production: {level: operate}  # unchanged",
    )?;
    root_sync(&host, &same).await?;
    for step in [RolloutStep::Hold, RolloutStep::Begin, RolloutStep::Finish] {
        let name = step.name();
        let answer = host
            .ask(ALICE, rollout(&plan, step))
            .await?
            .result(PROMPT)
            .await;
        assert!(
            matches!(answer, Ok(RemoteResult::Rollout(_))),
            "{name}: {answer:?}"
        );
    }

    let host = Host::with_alice().await?;
    let plan = host.plan(ALICE).await?;
    let raised = host.commit_with(
        "production: {level: deploy, exec: [web], secrets: true}",
        "production: {level: admin}",
    )?;
    root_sync(&host, &raised).await?;
    let mut refused = None;
    for step in [RolloutStep::Hold, RolloutStep::Begin] {
        if let Err(error) = host
            .ask(ALICE, rollout(&plan, step))
            .await?
            .result(PROMPT)
            .await
        {
            refused = Some(error.to_string());
            break;
        }
    }
    let message = refused.ok_or("the stale plan was let in")?;
    assert!(message.starts_with("MM_SECURITY: "), "{message}");
    assert!(message.contains("stale"), "{message}");
    Ok(())
}
