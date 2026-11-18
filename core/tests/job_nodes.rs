//! Which node of a cluster target runs a job: the first node its
//! `placement` selects that runs the job's service, or the first node that
//! runs the service. A node leaves out the jobs of other nodes, so their
//! runs and schedules happen once, on their node.

use mm::domain::config::MmConfig;
use mm::domain::placement::{NodeScope, job_node_on, view_from_node};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn config(jobs: &str) -> Result<MmConfig, serde_yaml_ng::Error> {
    config_with_web("{stack: app, placement: [app-2, app-3]}", jobs)
}

fn config_with_web(web: &str, jobs: &str) -> Result<MmConfig, serde_yaml_ng::Error> {
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
      web: {web}
      db: {{stack: app, placement: db-1}}
targets:
  cluster:
    nodes:
      - {{name: app-1, host: 203.0.113.11}}
      - {{name: app-2, host: 203.0.113.12}}
      - {{name: app-3, host: 203.0.113.13}}
      - {{name: db-1, host: 203.0.113.14}}
    cluster:
      groups: {{late: [app-3, db-1]}}
environments:
  production:
    target: cluster
jobs:
{jobs}"
    ))
}

#[test]
fn a_job_runs_on_the_first_node_of_its_service_unless_placed() -> TestResult {
    let config = config(
        "  migrate: {module: shop, service: web, command: [migrate]}\n  \
         pinned: {module: shop, service: web, command: [true], placement: app-3}\n  \
         grouped: {module: shop, service: web, command: [true], placement: late}\n",
    )?;
    assert_eq!(
        job_node_on(&config, "cluster", "migrate")?.as_deref(),
        Some("app-2")
    );
    assert_eq!(
        job_node_on(&config, "cluster", "pinned")?.as_deref(),
        Some("app-3")
    );
    assert_eq!(
        job_node_on(&config, "cluster", "grouped")?.as_deref(),
        Some("app-3"),
        "the group's first node that runs the service"
    );
    assert_eq!(job_node_on(&config, "local", "migrate")?, None);
    Ok(())
}

#[test]
fn a_job_placed_where_its_service_does_not_run_is_refused_with_where_it_runs() -> TestResult {
    let config =
        config("  migrate: {module: shop, service: web, command: [migrate], placement: db-1}\n")?;
    let problem = job_node_on(&config, "cluster", "migrate")
        .err()
        .unwrap_or_default();
    for expected in [
        "jobs.migrate.placement selects db-1",
        "it runs on app-2, app-3",
    ] {
        assert!(problem.contains(expected), "{expected}: {problem}");
    }
    Ok(())
}

#[test]
fn a_node_leaves_out_the_jobs_of_other_nodes() -> TestResult {
    let jobs = "  migrate: {module: shop, service: web, command: [migrate]}\n  \
                vacuum: {module: shop, service: db, command: [vacuum]}\n";
    for (node, here) in [("app-2", "migrate"), ("db-1", "vacuum")] {
        let mut config = config(jobs)?;
        view_from_node(
            &mut config,
            &NodeScope {
                target: "cluster".to_owned(),
                node: node.to_owned(),
                environment: "production".to_owned(),
                resolver_files: None,
                mesh_addresses: Default::default(),
            },
        )?;
        for (name, job) in &config.jobs {
            assert_eq!(
                job.placed_elsewhere.is_none(),
                name == here,
                "{node} keeps only {here}, not {name}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_scaled_services_before_deploy_job_runs_once_per_deployment() -> TestResult {
    use mm::domain::rollout::{ClusterRollout, NodeRolloutInput, RolloutPolicy};
    // web runs four containers over two nodes, two on each: its migration
    // is still one run, on the first of them.
    let scaled =
        |jobs: &str| config_with_web("{stack: app, placement: [app-2, app-3], replicas: 4}", jobs);
    let jobs = "  migrate: {module: shop, service: web, command: [migrate], run: before-deploy}\n";
    assert_eq!(
        scaled(jobs)?.modules["shop"].services["web"].replicas,
        Some(4)
    );
    let stacks = vec!["app".to_owned()];
    let mut policies = Vec::new();
    for node in ["app-1", "app-2", "app-3", "db-1"] {
        let mut config = scaled(jobs)?;
        view_from_node(
            &mut config,
            &NodeScope {
                target: "cluster".to_owned(),
                node: node.to_owned(),
                environment: "production".to_owned(),
                resolver_files: None,
                mesh_addresses: Default::default(),
            },
        )?;
        policies.push((node, RolloutPolicy::from_config(&config, "production")?));
    }
    let rollout = ClusterRollout::of(
        &policies
            .iter()
            .map(|(node, policy)| NodeRolloutInput {
                node,
                stacks: &stacks,
                policy,
            })
            .collect::<Vec<_>>(),
    );
    let runs = rollout
        .stages
        .iter()
        .flat_map(|stage| &stage.jobs)
        .map(|job| (job.job.as_str(), job.node.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(runs, [("migrate", "app-2")]);
    Ok(())
}
