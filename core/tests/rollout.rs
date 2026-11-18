//! How a deployment rolls out: the order of stacks and nodes, each
//! service's `update:` settings, and what a rollback returns to.

use std::collections::BTreeMap;

use mm::domain::config::{MmConfig, UpdateFailure, UpdateOrder};
use mm::domain::rollout::{
    ClusterRollout, DeploymentRecord, DeploymentStatus, NodeDeploymentStatus, NodeRollout,
    NodeRolloutInput, NodeStatusRow, RolloutPolicy, RolloutState, StepFailure, rollback_target,
    stack_order,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn config(
    stacks: &str,
    environment: &str,
    services: &str,
) -> Result<MmConfig, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(&format!(
        "schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: shop
target: local
stacks:
{stacks}
environments:
  production: {environment}
modules:
  shop:
    path: .
    services:
{services}
"
    ))
}

const THREE: &str =
    "  app: {files: [app.yaml]}\n  db: {files: [db.yaml]}\n  cache: {files: [cache.yaml]}";

#[test]
fn with_no_configuration_stacks_roll_by_name_one_node_at_a_time_with_ten_minutes() -> TestResult {
    let config = config(
        THREE,
        "{}",
        "      web: {stack: app}\n      pg: {stack: db}",
    )?;
    let policy = RolloutPolicy::from_config(&config, "production")?;
    assert_eq!(policy.stack_order, ["app", "cache", "db"]);
    assert_eq!(policy.convergence_timeout_seconds, 600);
    assert!(policy.services.is_empty(), "defaults are not listed");
    assert!(policy.start_first("app").is_empty());
    Ok(())
}

#[test]
fn after_puts_a_stack_behind_the_stacks_it_names() -> TestResult {
    let stacks = "  app: {files: [app.yaml], after: [db, cache]}\n  db: {files: [db.yaml]}\n  cache: {files: [cache.yaml], after: [db]}";
    let config = config(stacks, "{}", "      web: {stack: app}")?;
    assert_eq!(stack_order(&config.stacks)?, ["db", "cache", "app"]);

    let unknown = self::config(
        "  app: {files: [a.yaml], after: [queue]}",
        "{}",
        "      web: {stack: app}",
    )?;
    let error = stack_order(&unknown.stacks).err().unwrap_or_default();
    assert!(
        error.contains("stacks.app.after names 'queue', which is not a stack"),
        "{error}"
    );
    assert!(error.contains("the stacks are app"), "{error}");

    let looped = self::config(
        "  app: {files: [a.yaml], after: [db]}\n  db: {files: [d.yaml], after: [app]}",
        "{}",
        "      web: {stack: app}",
    )?;
    let error = stack_order(&looped.stacks).err().unwrap_or_default();
    assert!(
        error.contains("stacks app, db wait for each other"),
        "{error}"
    );
    assert!(error.contains("remove one of those lines"), "{error}");

    let own = self::config(
        "  app: {files: [a.yaml], after: [app]}",
        "{}",
        "      web: {stack: app}",
    )?;
    assert!(stack_order(&own.stacks).is_err());
    assert_eq!(stack_order(&BTreeMap::new())?, Vec::<String>::new());
    Ok(())
}

#[test]
fn a_policy_carries_the_window_and_only_the_update_settings_that_differ() -> TestResult {
    let config = config(
        THREE,
        "{deploy: {convergence-timeout: 90s}}",
        "      web: {stack: app, update: {order: start-first}}
      api: {stack: app, update: {parallelism: 2, failure: pause}}
      worker: {stack: app, update: {}}
      pg: {stack: db, update: {order: stop-first, parallelism: 1, failure: rollback}}
      preview: {stack: app, environments: [staging], update: {order: start-first}}",
    )?;
    let policy = RolloutPolicy::from_config(&config, "production")?;
    assert_eq!(policy.convergence_timeout().as_secs(), 90);
    assert_eq!(
        policy
            .services
            .iter()
            .map(|service| (
                service.service.as_str(),
                service.order,
                service.parallelism,
                service.failure
            ))
            .collect::<Vec<_>>(),
        [
            ("api", UpdateOrder::StopFirst, 2, UpdateFailure::Pause),
            ("web", UpdateOrder::StartFirst, 1, UpdateFailure::Rollback),
        ]
    );
    assert_eq!(policy.start_first("app"), ["web"]);
    assert!(policy.start_first("db").is_empty());

    for (window, problem) in [
        ("soon", "duration"),
        ("0s", "at least 1s"),
        ("500ms", "at least 1s"),
    ] {
        let config = self::config(
            THREE,
            &format!("{{deploy: {{convergence-timeout: {window}}}}}"),
            "      web: {stack: app}",
        )?;
        let error = RolloutPolicy::from_config(&config, "production")
            .err()
            .unwrap_or_default();
        assert!(
            error.contains("environments.production.deploy.convergence-timeout"),
            "{error}"
        );
        assert!(error.contains(problem), "{window}: {error}");
    }
    let zero = self::config(
        THREE,
        "{}",
        "      web: {stack: app, update: {parallelism: 0}}",
    )?;
    let error = RolloutPolicy::from_config(&zero, "production")
        .err()
        .unwrap_or_default();
    assert!(
        error.contains("modules.shop.services.web.update.parallelism must be at least 1"),
        "{error}"
    );
    // A plan stored before rollouts existed has no window: the default.
    assert_eq!(
        RolloutPolicy::default().convergence_timeout().as_secs(),
        600
    );
    Ok(())
}

#[test]
fn a_policy_orders_the_stacks_a_host_renders() -> TestResult {
    let config = config(
        "  app: {files: [a.yaml], after: [db]}\n  db: {files: [d.yaml]}",
        "{}",
        "      web: {stack: app}",
    )?;
    let policy = RolloutPolicy::from_config(&config, "production")?;
    let rendered = ["app", "zeta", "db", "alpha"];
    assert_eq!(
        policy.ordered(&rendered, |stack| *stack),
        [&"db", &"app", &"alpha", &"zeta"],
        "listed stacks in order, then the others by name"
    );
    Ok(())
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_cluster_rolls_one_stack_at_a_time_over_the_nodes_that_run_it() -> TestResult {
    let config = config(
        "  app: {files: [a.yaml], after: [db]}\n  db: {files: [d.yaml]}",
        "{deploy: {convergence-timeout: 2m}}",
        "      web: {stack: app, update: {order: start-first}}\n      pg: {stack: db}",
    )?;
    let policy = RolloutPolicy::from_config(&config, "production")?;
    let (both, app, none) = (names(&["app", "db"]), names(&["app"]), Vec::new());
    let rollout = ClusterRollout::of(&[
        NodeRolloutInput {
            node: "app-1",
            stacks: &app,
            policy: &policy,
        },
        NodeRolloutInput {
            node: "app-2",
            stacks: &app,
            policy: &policy,
        },
        NodeRolloutInput {
            node: "db-1",
            stacks: &both,
            policy: &policy,
        },
        NodeRolloutInput {
            node: "idle",
            stacks: &none,
            policy: &policy,
        },
    ]);
    assert_eq!(rollout.convergence_timeout_seconds, 120);
    assert_eq!(
        rollout
            .stages
            .iter()
            .map(|stage| stage.describe())
            .collect::<Vec<_>>(),
        ["db on db-1", "app on app-1, then app-2, then db-1"]
    );
    assert_eq!(rollout.stages[1].start_first, ["web"]);
    assert!(rollout.stages[0].start_first.is_empty());

    // Each node checks the order against what its own plan renders.
    rollout.check_node("db-1", &both, &[])?;
    rollout.check_node("idle", &none, &[])?;
    let missing = rollout
        .check_node("idle", &app, &[])
        .err()
        .unwrap_or_default();
    assert!(
        missing.contains("leaves out stack 'app' on node 'idle'"),
        "{missing}"
    );
    let extra = rollout
        .check_node("app-1", &none, &[])
        .err()
        .unwrap_or_default();
    assert!(
        extra.contains("has stack 'app' on node 'app-1', which does not run it"),
        "{extra}"
    );
    Ok(())
}

#[test]
fn parallelism_batches_the_nodes_and_pause_is_per_service() -> TestResult {
    let config = config(
        "  app: {files: [a.yaml]}",
        "{}",
        "      web: {stack: app, update: {parallelism: 2}}\n      worker: {stack: app, update: {parallelism: 3, failure: pause}}",
    )?;
    let policy = RolloutPolicy::from_config(&config, "production")?;
    let app = names(&["app"]);
    let nodes = ["n1", "n2", "n3", "n4", "n5"];
    let rollout = ClusterRollout::of(
        &nodes
            .iter()
            .map(|node| NodeRolloutInput {
                node,
                stacks: &app,
                policy: &policy,
            })
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        rollout.stages[0].describe(),
        "app on n1 and n2, then n3 and n4, then n5",
        "the smallest parallelism of the stack's services"
    );
    assert!(rollout.pauses("app", Some("worker")));
    assert!(!rollout.pauses("app", Some("web")));
    assert!(
        !rollout.pauses("app", None),
        "a failure no service owns rolls back"
    );
    assert!(!rollout.pauses("db", Some("worker")));
    Ok(())
}

fn record(commit: char, at: i64, plan: char) -> DeploymentRecord {
    DeploymentRecord {
        commit: commit.to_string().repeat(40),
        deployed_at_unix_ms: at,
        deployed_by: "alice".to_owned(),
        runs_services: true,
        plan_hash: Some(plan.to_string().repeat(64)),
        receipt: None,
        release: None,
    }
}

fn row(
    node: &str,
    current: Option<DeploymentRecord>,
    previous: Option<DeploymentRecord>,
) -> NodeStatusRow {
    NodeStatusRow {
        node: node.to_owned(),
        problem: None,
        status: Some(NodeDeploymentStatus {
            current,
            previous,
            rollout: None,
            operations: Vec::new(),
        }),
    }
}

fn status(nodes: Vec<NodeStatusRow>) -> DeploymentStatus {
    DeploymentStatus {
        target: "production".to_owned(),
        environment: "production".to_owned(),
        nodes,
        answered_unhealthy: Vec::new(),
    }
}

fn unfinished(plan: char, active: bool) -> NodeRollout {
    NodeRollout {
        plan_hash: plan.to_string().repeat(64),
        commit: "c".repeat(40),
        started_by: "bob".to_owned(),
        started_at_unix_ms: 1_000,
        heard_at_unix_ms: 2_000,
        active,
        state: RolloutState::Running,
        stacks: vec!["app".to_owned()],
        failure: None,
    }
}

#[test]
fn a_rollback_returns_the_nodes_of_the_last_deployment_to_what_they_ran_before() -> TestResult {
    // Everything ran a, then b; the last deployment put c on app-1 only.
    let last = status(vec![
        row(
            "app-1",
            Some(record('c', 30, '3')),
            Some(record('b', 20, '2')),
        ),
        row(
            "app-2",
            Some(record('b', 20, '2')),
            Some(record('a', 10, '1')),
        ),
        row(
            "db-1",
            Some(record('b', 21, '2')),
            Some(record('a', 10, '1')),
        ),
    ]);
    let target = rollback_target(&last, None)?;
    assert_eq!(target.commit, "b".repeat(40));
    assert_eq!(
        target.nodes,
        ["app-1"],
        "only the node the last deployment changed"
    );
    assert!(!target.every_node);
    assert!(
        target
            .undoes
            .contains("the deployment of commit cccccccccccc by alice"),
        "{}",
        target.undoes
    );

    // The whole cluster took part: every node returns.
    let whole = status(vec![
        row(
            "app-1",
            Some(record('b', 20, '2')),
            Some(record('a', 10, '1')),
        ),
        row(
            "app-2",
            Some(record('b', 22, '2')),
            Some(record('a', 10, '1')),
        ),
    ]);
    let target = rollback_target(&whole, None)?;
    assert_eq!(
        (target.commit.as_str(), target.every_node),
        ("a".repeat(40).as_str(), true)
    );
    assert_eq!(target.nodes, ["app-1", "app-2"]);

    // Limited to some nodes.
    let only = names(&["app-2"]);
    let target = rollback_target(&whole, Some(&only))?;
    assert_eq!(target.nodes, ["app-2"]);
    assert!(!target.every_node);
    Ok(())
}

#[test]
fn a_rollback_undoes_an_unfinished_rollout_before_anything_else() -> TestResult {
    let mut interrupted = status(vec![
        row(
            "app-1",
            Some(record('b', 20, '2')),
            Some(record('a', 10, '1')),
        ),
        row(
            "app-2",
            Some(record('b', 20, '2')),
            Some(record('a', 10, '1')),
        ),
    ]);
    if let Some(node) = interrupted.nodes[0].status.as_mut() {
        node.rollout = Some(unfinished('9', false));
    }
    assert_eq!(interrupted.unfinished().len(), 1);
    assert!(interrupted.running().is_none());
    let target = rollback_target(&interrupted, None)?;
    assert_eq!(
        target.commit,
        "b".repeat(40),
        "the deployment the nodes last finished"
    );
    assert_eq!(target.nodes, ["app-1", "app-2"]);
    assert!(
        target
            .undoes
            .contains("the unfinished rollout of plan 999999999999"),
        "{}",
        target.undoes
    );
    assert!(
        target.undoes.contains("started by bob") && target.undoes.contains("on app-1"),
        "{}",
        target.undoes
    );

    // While a client is still deploying, a rollback waits.
    if let Some(node) = interrupted.nodes[0].status.as_mut() {
        node.rollout = Some(unfinished('9', true));
    }
    assert!(interrupted.running().is_some());
    let error = rollback_target(&interrupted, None)
        .err()
        .unwrap_or_default();
    assert!(
        error.contains("bob is deploying plan 999999999999"),
        "{error}"
    );
    assert!(error.contains("`mm deploy status`"), "{error}");
    Ok(())
}

#[test]
fn a_rollback_that_cannot_be_one_plan_names_the_next_command() -> TestResult {
    let nothing = status(vec![row("app-1", None, None)]);
    let error = rollback_target(&nothing, None).err().unwrap_or_default();
    assert!(
        error.contains("nothing is deployed to 'production' yet"),
        "{error}"
    );

    let first = status(vec![row("app-1", Some(record('a', 10, '1')), None)]);
    let error = rollback_target(&first, None).err().unwrap_or_default();
    assert!(
        error.contains("is the first deployment to 'production'"),
        "{error}"
    );

    // The nodes of the last deployment ran different commits before it.
    let mixed = status(vec![
        row(
            "app-1",
            Some(record('c', 30, '3')),
            Some(record('a', 10, '1')),
        ),
        row(
            "app-2",
            Some(record('c', 30, '3')),
            Some(record('b', 20, '2')),
        ),
    ]);
    let error = rollback_target(&mixed, None).err().unwrap_or_default();
    assert!(error.contains("one plan deploys one commit"), "{error}");
    assert!(
        error.contains(&format!(
            "`mm deploy plan --commit {} --nodes app-1`",
            "a".repeat(40)
        )),
        "{error}"
    );
    assert!(
        error.contains(&format!(
            "`mm deploy plan --commit {} --nodes app-2`",
            "b".repeat(40)
        )),
        "{error}"
    );

    // A node that does not answer cannot say what it ran.
    let mut silent = status(vec![
        row(
            "app-1",
            Some(record('b', 20, '2')),
            Some(record('a', 10, '1')),
        ),
        row("app-2", None, None),
    ]);
    silent.nodes[1].status = None;
    silent.nodes[1].problem = Some("did not answer (Connection timed out)".to_owned());
    let error = rollback_target(&silent, None).err().unwrap_or_default();
    assert!(error.contains("node 'app-2' did not answer"), "{error}");
    assert!(
        error.contains("`mm deploy rollback --nodes app-1`"),
        "{error}"
    );
    let only = names(&["app-1"]);
    assert_eq!(rollback_target(&silent, Some(&only))?.nodes, ["app-1"]);
    Ok(())
}

#[test]
fn a_failure_reads_as_one_line_and_keeps_its_log_apart() {
    let failure = StepFailure {
        stack: "app".to_owned(),
        service: Some("web".to_owned()),
        reason: "its health check fails".to_owned(),
        log: vec!["listening on :80".to_owned(), "upstream refused".to_owned()],
        code: None,
    };
    assert_eq!(
        failure.to_string(),
        "service 'web' of stack 'app': its health check fails"
    );
    assert_eq!(
        failure.with_log(),
        "service 'web' of stack 'app': its health check fails. Its last log lines: listening on :80 | upstream refused"
    );
    let whole = StepFailure {
        service: None,
        log: Vec::new(),
        ..failure.clone()
    };
    assert_eq!(whole.to_string(), "stack 'app': its health check fails");
    assert_eq!(whole.with_log(), whole.to_string());
}

#[test]
fn the_same_commit_with_the_same_images_is_the_same_release() {
    let first = record('a', 10, '1');
    let again = record('a', 99, '7');
    assert!(
        first.same_release(&again),
        "when and under which plan does not matter"
    );
    assert!(!first.same_release(&record('b', 10, '1')));
}

#[test]
fn deploy_status_names_the_nodes_that_did_not_answer() {
    let silent = |node: &str| NodeStatusRow {
        node: node.to_owned(),
        problem: Some("speaks another remote protocol version than this mm".to_owned()),
        status: None,
    };
    let answered = status(vec![row("app-1", Some(record('a', 10, '1')), None)]);
    assert_eq!(answered.summary(), "No rollout is in progress.");
    assert!(!answered.none_answered());

    let some = status(vec![
        row("app-1", Some(record('a', 10, '1')), None),
        silent("app-2"),
        silent("app-3"),
    ]);
    assert_eq!(some.unanswered(), ["app-2", "app-3"]);
    assert!(!some.none_answered());
    assert_eq!(
        some.summary(),
        "No rollout is in progress on the nodes that answered. app-2, app-3 did not answer: what runs there is unknown."
    );

    let none = status(vec![silent("app-1"), silent("app-2")]);
    assert!(none.none_answered());
    let said = none.summary();
    assert!(said.starts_with("No node answered"), "{said}");
    assert!(!said.contains("No rollout is in progress"), "{said}");

    let mut running = row("app-1", Some(record('a', 10, '1')), None);
    if let Some(status) = running.status.as_mut() {
        status.rollout = Some(unfinished('9', true));
    }
    let deploying = status(vec![running, silent("app-2")]);
    let said = deploying.summary();
    assert!(
        said.starts_with("bob is deploying plan 999999999999"),
        "{said}"
    );
    assert!(
        said.ends_with("app-2 did not answer: what runs there is unknown."),
        "{said}"
    );
}

#[test]
fn small_findings_name_the_place_and_the_next_step() -> TestResult {
    use mm::domain::diagnostic::Diagnostic;
    use mm::domain::remote::{NodeReport, NodeState, node_diagnostics, refused_handshake};

    let text = "# Host policy for mmd.\nschema-version: 2\nbreak-glass: []\nprojects:\n  a: {}\nprojects:\n  b: {}\n";
    let parsed = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(text)
        .err()
        .ok_or("a duplicate key was accepted")?;
    let said = mm::domain::host_policy::duplicate_key_message(text, &parsed.to_string());
    assert_eq!(
        said,
        "key `projects` is given twice, at line 4 and again at line 6; keep one of them"
    );
    assert_eq!(
        mm::domain::host_policy::duplicate_key_message(text, "another problem"),
        "another problem"
    );

    let report = |state: NodeState, problem: &str| NodeReport {
        name: "app-1".to_owned(),
        host: "203.0.113.11".to_owned(),
        port: 22,
        mesh_address: None,
        state,
        reachable: false,
        problem: Some(problem.to_owned()),
        mmd_version: None,
        deployed: None,
        rollout: None,
        engine: None,
        daemon: None,
        secret_store: None,
    };
    assert!(refused_handshake(
        "speaks another remote protocol version than this mm. Upgrade mm on this machine"
    ));
    assert!(!refused_handshake("did not answer on 203.0.113.11:22"));
    let found: Vec<Diagnostic> = node_diagnostics(
        "production",
        &[
            report(NodeState::Refused, "MM_CONFIG_PARSE: /etc/mm/host.yaml: …"),
            report(NodeState::Unreachable, "did not answer"),
        ],
    );
    assert_eq!(found[0].code, "MM_NODE_REFUSED");
    let help = found[0].help.clone().unwrap_or_default();
    assert!(help.contains("what it said is the next step"), "{help}");
    assert!(!help.contains("systemctl status mmd"), "{help}");
    assert_eq!(found[1].code, "MM_NODE_UNREACHABLE");

    let late = mm::domain::runtime::RuntimeProbe::unavailable(
        mm::domain::runtime::RuntimeKind::DockerCompose,
        Diagnostic::error(
            mm::domain::runtime::PROBE_TIMEOUT_CODE,
            "docker timed out after 30 seconds",
        ),
    )
    .late_engine(Some("node 'app-1'"))
    .map(|error| error.to_string())
    .unwrap_or_default();
    assert!(
        late.starts_with("the Docker engine of node 'app-1' did not answer in time"),
        "{late}"
    );
    Ok(())
}
