//! Secrets and variables staged on a host over the relay: the
//! users file's `secrets` grant stages, view reads what the host holds,
//! every step is audited without a value, a plan lists what is staged and
//! a deployment that succeeds makes it live.

mod daemon_host;
mod temp_dirs;

use std::time::Duration;

use daemon_host::{ALICE, BOB, CAROL, Host, PROMPT, TestResult};
use mm::domain::remote::{RemoteMethod, RemoteResult};
use mm::domain::staged::{StageState, StageStep, StagedAction, ValueChange, ValueKind, ValueStage};

const SEALED: &str = "-----BEGIN AGE ENCRYPTED FILE-----\nsealed\n-----END AGE ENCRYPTED FILE-----";

fn prepare(stage: uuid::Uuid) -> RemoteMethod {
    RemoteMethod::StageValues(Box::new(ValueStage {
        stage,
        step: StageStep::Prepare {
            changes: vec![ValueChange {
                key: "web.app-key".to_owned(),
                kind: ValueKind::Secret,
                action: StagedAction::Set {
                    revision: "r2".to_owned(),
                },
            }],
            sealed: Some(SEALED.to_owned()),
        },
    }))
}

fn commit(stage: uuid::Uuid) -> RemoteMethod {
    RemoteMethod::StageValues(Box::new(ValueStage {
        stage,
        step: StageStep::Commit,
    }))
}

async fn audited(host: &Host, field: &str, value: &str) -> TestResult<Vec<serde_json::Value>> {
    for _ in 0..100 {
        let found = host
            .audit()?
            .into_iter()
            .filter(|record| record[field] == value)
            .collect::<Vec<_>>();
        if !found.is_empty() {
            return Ok(found);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(Vec::new())
}

#[tokio::test]
async fn the_secrets_grant_stages_and_every_step_is_audited_without_the_value() -> TestResult {
    let host = Host::with_alice().await?;
    let stage = uuid::Uuid::new_v4();
    let RemoteResult::Staged(prepared) = host
        .ask(ALICE, prepare(stage))
        .await?
        .result(PROMPT)
        .await?
    else {
        return Err("the prepare step did not answer".into());
    };
    assert_eq!(prepared.state, StageState::Prepared);
    let RemoteResult::Staged(committed) =
        host.ask(ALICE, commit(stage)).await?.result(PROMPT).await?
    else {
        return Err("the commit step did not answer".into());
    };
    assert_eq!(committed.state, StageState::Committed);
    let staged = audited(&host, "result", "values-staged").await?;
    assert_eq!(staged.len(), 2, "both steps are audited");
    assert_eq!(staged[0]["principal"], "alice");
    assert_eq!(staged[0]["detail"]["changes"][0]["key"], "web.app-key");
    assert_eq!(
        staged[0]["detail"]["changes"][0]["action"]["revision"],
        "r2"
    );
    let text = std::fs::read_to_string(host.config.state_root.join("audit.jsonl"))?;
    assert!(
        !text.contains("sealed"),
        "the audit log holds no sealed value"
    );
    assert!(!text.contains("AGE ENCRYPTED"));
    Ok(())
}

#[tokio::test]
async fn deploy_and_view_without_the_grant_are_refused_and_audited() -> TestResult {
    let host = Host::with_alice().await?;
    for key in [BOB, CAROL] {
        let (code, message) = host
            .ask(key, prepare(uuid::Uuid::new_v4()))
            .await?
            .refusal(PROMPT)
            .await?;
        assert_eq!(code, "MM_SECURITY", "{message}");
        let (code, _) = host
            .ask(key, RemoteMethod::SecretRecipient)
            .await?
            .refusal(PROMPT)
            .await?;
        assert_eq!(code, "MM_SECURITY");
    }
    let denied = audited(&host, "method", "stage_values")
        .await?
        .into_iter()
        .filter(|record| record["result"] == "denied")
        .count();
    assert_eq!(denied, 2);
    assert!(
        host.handlers
            .stages
            .lock()
            .map_err(|_| "poisoned")?
            .is_empty(),
        "nothing reached the store"
    );
    Ok(())
}

#[tokio::test]
async fn a_viewer_reads_what_the_host_holds_with_the_deployed_commit() -> TestResult {
    let host = Host::with_alice().await?;
    let RemoteResult::HostValues(values) = host
        .ask(CAROL, RemoteMethod::HostValues)
        .await?
        .result(PROMPT)
        .await?
    else {
        return Err("the host did not say what it holds".into());
    };
    assert_eq!(values.commit, host.deployed_commit()?);
    assert_eq!(values.values[0].key, "web.app-key");
    assert_eq!(
        values.values[0]
            .staged
            .as_ref()
            .map(|staged| staged.revision.as_str()),
        Some("r2")
    );
    Ok(())
}

#[tokio::test]
async fn a_plan_lists_what_is_staged_and_a_successful_apply_makes_it_live() -> TestResult {
    let host = Host::with_alice().await?;
    let method = RemoteMethod::PlanDeployment {
        commit: host.commit.clone(),
        receipt: None,
        release: Default::default(),
    };
    let RemoteResult::DeploymentPlan(plan) = host.ask(ALICE, method).await?.result(PROMPT).await?
    else {
        return Err("no plan".into());
    };
    assert_eq!(plan.staged.len(), 1);
    assert_eq!(
        plan.staged[0].line(),
        "~ web.app-key (revision changed), staged by alice"
    );
    let apply = RemoteMethod::ApplyDeployment {
        remote_plan_hash: plan.binding.plan_hash.clone(),
    };
    host.ask(ALICE, apply).await?.result(PROMPT).await?;
    assert_eq!(
        *host.handlers.live.lock().map_err(|_| "poisoned")?,
        vec!["web.app-key".to_owned()]
    );
    let live = audited(&host, "result", "values-live").await?;
    assert_eq!(live.len(), 1);
    assert_eq!(live[0]["detail"]["changes"][0]["revision"], "r2");
    Ok(())
}
