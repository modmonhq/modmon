//! Jobs that run with a deployment: which a node's deployment runs,
//! in which order, which a cluster rollout puts before a stage, and when a
//! `once` job is due.

use mm::domain::config::{JobWhen, MmConfig};
use mm::domain::rollout::{ClusterRollout, NodeRolloutInput, RolloutPolicy};
use mm::domain::scheduler::OnceRecord;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn config(jobs: &str) -> Result<MmConfig, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(&format!(
        "\
schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de6
  name: shop
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  shop:
    path: .
    services:
      web: {{stack: app}}
environments:
  production: {{}}
  uat: {{}}
jobs:
{jobs}"
    ))
}

const JOBS: &str = "  \
    restore: {module: shop, service: web, command: [restore], run: once, environments: [uat]}\n  \
    clear-cache: {module: shop, service: web, command: [clear], run: after-deploy}\n  \
    migrate: {module: shop, service: web, command: [migrate], run: before-deploy, timeout: 2m}\n  \
    report: {module: shop, service: web, command: [report]}\n  \
    seed: {module: shop, service: web, command: [seed], run: once, environment: production}\n";

#[test]
fn a_deployment_runs_its_jobs_before_after_and_once_in_that_order() -> TestResult {
    let config = config(JOBS)?;
    let policy = RolloutPolicy::from_config(&config, "production")?;
    let jobs = policy
        .jobs
        .iter()
        .map(|job| (job.job.as_str(), job.run))
        .collect::<Vec<_>>();
    assert_eq!(
        jobs,
        [
            ("migrate", JobWhen::BeforeDeploy),
            ("clear-cache", JobWhen::AfterDeploy),
            ("seed", JobWhen::Once),
        ],
        "a job without run: is not a deployment's; restore belongs to uat; `environment:` still reads"
    );
    assert_eq!(policy.jobs[0].timeout_seconds, 120);
    let uat = RolloutPolicy::from_config(&config, "uat")?;
    assert!(uat.jobs.iter().any(|job| job.job == "restore"));
    assert!(!uat.jobs.iter().any(|job| job.job == "seed"));
    Ok(())
}

#[test]
fn a_cluster_rollout_runs_before_jobs_in_their_stage_and_once_jobs_only_when_due() -> TestResult {
    let config = config(JOBS)?;
    let policy = RolloutPolicy::from_config(&config, "production")?;
    let stacks = vec!["app".to_owned()];
    let rollout = ClusterRollout::of(&[NodeRolloutInput {
        node: "app-1",
        stacks: &stacks,
        policy: &policy,
    }]);
    let stage_jobs = rollout.stages[0]
        .jobs
        .iter()
        .map(|job| (job.job.as_str(), job.node.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(stage_jobs, [("migrate", "app-1")]);
    assert_eq!(rollout.once.len(), 1, "seed is due");
    rollout.check_node("app-1", &stacks, &policy.jobs)?;
    let ran = rollout.knowing_ran_once([(
        "seed".to_owned(),
        OnceRecord {
            node: Some("app-1".to_owned()),
            run: None,
            plan_hash: "a".repeat(64),
            at_unix_ms: 1,
        },
    )]);
    assert!(ran.once.is_empty(), "a once job some node ran is not due");
    assert!(ran.ran_once.contains_key("seed"));
    Ok(())
}

#[test]
fn a_node_refuses_a_rollout_that_runs_other_jobs_on_it() -> TestResult {
    let config = config(JOBS)?;
    let policy = RolloutPolicy::from_config(&config, "production")?;
    let stacks = vec!["app".to_owned()];
    let mut rollout = ClusterRollout::of(&[NodeRolloutInput {
        node: "app-1",
        stacks: &stacks,
        policy: &policy,
    }]);
    rollout.stages[0].jobs.clear();
    let refused = rollout
        .check_node("app-1", &stacks, &policy.jobs)
        .err()
        .unwrap_or_default();
    assert!(
        refused.contains("runs other jobs on node 'app-1'"),
        "{refused}"
    );
    Ok(())
}

#[test]
fn a_rollback_puts_back_only_what_the_rollout_changed() {
    let mut changes = mm::domain::rollout::RolloutChanges::default();
    changes.note("app", true);
    assert!(
        changes.ahead_only("app"),
        "only app's ahead services started"
    );
    assert!(!changes.whole("app"));
    assert!(
        !changes.ahead_only("db") && !changes.whole("db"),
        "db was not reached"
    );
    changes.note("app", false);
    changes.note("app", false);
    assert!(changes.whole("app") && !changes.ahead_only("app"));
    assert_eq!(changes.stacks, ["app"], "noted once");
}

// Found on the test VMs: the first deployment of an environment ran its
// migration on mm-cluster-1 and a `once` job on mm-cluster-3, and `mm
// deploy logs` showed only the run on mm-cluster-3. Each node filed its runs
// under its own section's hash, and `deploy logs` shows the runs filed under
// the newest deployment.
#[test]
fn every_node_files_its_runs_of_one_cluster_deployment_under_the_cluster_plan() {
    use mm::domain::scheduler::deployment_label;
    let cluster = "36a34a2d3131c0ffee";
    assert_eq!(deployment_label(Some(cluster), "3bd0d1d36850aa"), cluster);
    assert_eq!(deployment_label(Some(cluster), "4e1f0b9d2c77bb"), cluster);
    // One host has one plan.
    assert_eq!(deployment_label(None, "3bd0d1d36850aa"), "3bd0d1d36850aa");
    // The record of a `once` job names the deployment the same way, so the
    // plan says "ran on mm-cluster-3 ... (plan <cluster plan>)" whichever
    // node started it.
    let record = OnceRecord::started(
        Some("mm-cluster-3".to_owned()),
        None,
        (Some(cluster), "4e1f0b9d2c77bb"),
        1,
    );
    assert_eq!(record.plan_hash, cluster);
    let one_host = OnceRecord::started(None, None, (None, "3bd0d1d36850aa"), 1);
    assert_eq!(one_host.plan_hash, "3bd0d1d36850aa");
}
