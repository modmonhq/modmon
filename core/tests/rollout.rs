//! How a deployment rolls out: the order of stacks and nodes, each
//! service's `update:` settings, and what a rollback returns to.

use std::collections::BTreeMap;

use mm::domain::config::{MmConfig, UpdateFailure, UpdateOrder};
use mm::domain::rollout::{
    ClusterRollout, DeploymentRecord, DeploymentStatus, FAILURE_LINE_CHARS, FAILURE_LOG_BYTES,
    NodeDeploymentStatus, NodeRollout, NodeRolloutInput, NodeStatusRow, RolloutPolicy,
    RolloutState, StepFailure, kept_failure_log, rollback_target, stack_order,
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
            ("web", UpdateOrder::StartFirst, 0, UpdateFailure::Rollback),
            ("pg", UpdateOrder::StopFirst, 1, UpdateFailure::Rollback),
        ],
        "a parallelism the service sets is kept, even 1; 0 is none set"
    );
    assert_eq!(policy.services.len(), 3);
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
            services: &[],
        },
        NodeRolloutInput {
            node: "app-2",
            stacks: &app,
            policy: &policy,
            services: &[],
        },
        NodeRolloutInput {
            node: "db-1",
            stacks: &both,
            policy: &policy,
            services: &[],
        },
        NodeRolloutInput {
            node: "idle",
            stacks: &none,
            policy: &policy,
            services: &[],
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
                services: &[],
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
fn a_failed_container_keeps_200_lines_by_default_and_never_more_than_its_byte_bound() -> TestResult
{
    let plain = config(THREE, "{}", "      web: {stack: app}")?;
    let policy = RolloutPolicy::from_config(&plain, "production")?;
    assert_eq!(policy.failure_log_lines(), 200);
    assert_eq!(
        policy.failure_log_lines, None,
        "the default is not in the plan"
    );

    let set = config(
        THREE,
        "{deploy: {failure-log-lines: 500}}",
        "      web: {stack: app}",
    )?;
    assert_eq!(
        RolloutPolicy::from_config(&set, "production")?.failure_log_lines(),
        500
    );
    for lines in [0, 1001] {
        let wrong = config(
            THREE,
            &format!("{{deploy: {{failure-log-lines: {lines}}}}}"),
            "      web: {stack: app}",
        )?;
        let error = RolloutPolicy::from_config(&wrong, "production")
            .err()
            .unwrap_or_default();
        assert!(
            error.contains(
                "environments.production.deploy.failure-log-lines must be between 1 and 1000"
            ),
            "{error}"
        );
    }

    let log = (1..=300)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>();
    let kept = kept_failure_log(&log, 200);
    assert_eq!(kept.len(), 200);
    assert_eq!(kept.first().map(String::as_str), Some("line 101"));
    assert_eq!(kept.last().map(String::as_str), Some("line 300"));

    let wide = vec!["é".repeat(399); 1000];
    let kept = kept_failure_log(&wide, 1000);
    assert!(kept.iter().map(|line| line.len() + 1).sum::<usize>() <= FAILURE_LOG_BYTES);
    assert!(kept.len() < 1000);

    let huge = [
        "start".to_owned(),
        format!("{}\u{7}end", "y".repeat(100_000)),
    ];
    let kept = kept_failure_log(&huge, 200);
    assert_eq!(kept.len(), 2, "one huge line does not empty the log");
    assert_eq!(kept[1].chars().count(), FAILURE_LINE_CHARS);
    assert!(kept[1].chars().all(|character| character == 'y'));

    let failure = StepFailure {
        stack: "app".to_owned(),
        service: Some("db".to_owned()),
        reason: "it exited".to_owned(),
        log: log.clone(),
        code: None,
    };
    assert!(
        failure
            .with_log()
            .ends_with("Its last log lines: line 281 | line 282 | line 283 | line 284 | line 285 | line 286 | line 287 | line 288 | line 289 | line 290 | line 291 | line 292 | line 293 | line 294 | line 295 | line 296 | line 297 | line 298 | line 299 | line 300"),
        "{}",
        failure.with_log()
    );
    Ok(())
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
        problem: Some("has no remote protocol in common with this mm".to_owned()),
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

    let text = "# Host policy for mmd.\nschema-version: 1\nbreak-glass: []\nprojects:\n  a: {}\nprojects:\n  b: {}\n";
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
        repo_address: None,
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
        "has no remote protocol in common with this mm. Update mm on this machine"
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

#[test]
fn the_pull_limit_is_twenty_minutes_unless_an_environment_sets_it() -> TestResult {
    let unset = config(THREE, "{}", "      web: {stack: app}")?;
    let default = RolloutPolicy::from_config(&unset, "production")?;
    assert_eq!(
        default.pull_timeout_seconds, None,
        "the plan carries no default"
    );
    assert_eq!(default.pull_timeout().as_secs(), 20 * 60);

    let set = config(
        THREE,
        "{deploy: {pull-timeout: 45m}}",
        "      web: {stack: app}",
    )?;
    let longer = RolloutPolicy::from_config(&set, "production")?;
    assert_eq!(longer.pull_timeout().as_secs(), 45 * 60);

    let app = names(&["app"]);
    let alone = ClusterRollout::of(&[NodeRolloutInput {
        node: "app-1",
        stacks: &app,
        policy: &default,
        services: &[],
    }]);
    assert_eq!(alone.pull_timeout_seconds, None);
    assert_eq!(alone.pull_timeout().as_secs(), 20 * 60);
    let mixed = ClusterRollout::of(&[
        NodeRolloutInput {
            node: "app-1",
            stacks: &app,
            policy: &default,
            services: &[],
        },
        NodeRolloutInput {
            node: "app-2",
            stacks: &app,
            policy: &longer,
            services: &[],
        },
    ]);
    assert_eq!(mixed.pull_timeout().as_secs(), 45 * 60);

    for (window, problem) in [("slow", "duration"), ("0s", "at least 1s")] {
        let config = self::config(
            THREE,
            &format!("{{deploy: {{pull-timeout: {window}}}}}"),
            "      web: {stack: app}",
        )?;
        let error = RolloutPolicy::from_config(&config, "production")
            .err()
            .unwrap_or_default();
        assert!(
            error.contains("environments.production.deploy.pull-timeout"),
            "{error}"
        );
        assert!(error.contains(problem), "{window}: {error}");
    }
    Ok(())
}

#[test]
fn a_service_on_several_nodes_starts_first_unless_the_host_holds_it() -> TestResult {
    use mm::domain::rollout::{HostTies, starts_first_by_default};

    let ties = |yaml: &str| -> Result<HostTies, serde_yaml_ng::Error> {
        Ok(HostTies::of(&serde_yaml_ng::from_str(yaml)?))
    };
    let free = [
        "image: web:2",
        "{image: web:2, volumes: ['config:/etc/web:ro', /cache]}",
        "{image: web:2, volumes: [{type: bind, source: ./conf, target: /conf, read_only: true}, {type: tmpfs, target: /tmp}, {type: volume, target: /scratch}]}",
        "{image: web:2, ports: []}",
        "{image: web:2, scale: 1}",
    ];
    for yaml in free {
        let ties = ties(yaml)?;
        assert!(!ties.any(), "{yaml}: {ties:?}");
        assert!(starts_first_by_default(2, &ties), "{yaml}");
        assert!(!starts_first_by_default(1, &ties), "one node: {yaml}");
        assert!(!starts_first_by_default(0, &ties), "no cluster: {yaml}");
    }
    let held = [
        "{image: web:2, ports: ['8080:80']}",
        "{image: web:2, ports: ['80']}",
        "{image: web:2, container_name: web}",
        "{image: web:2, network_mode: host}",
        "{image: web:2, pid: host}",
        "{image: db:17, volumes: ['data:/var/lib/postgresql/data']}",
        "{image: db:17, volumes: [{type: volume, source: data, target: /data}]}",
        "{image: web:2, devices: ['/dev/fuse']}",
        "{image: web:2, deploy: {replicas: 2}}",
    ];
    for yaml in held {
        assert!(!starts_first_by_default(3, &ties(yaml)?), "{yaml}");
    }
    Ok(())
}

#[test]
fn a_stack_is_left_alone_only_when_every_container_is_what_the_files_start() {
    use mm::domain::rollout::{RunningContainer, WantedService, stack_unchanged};

    let wanted = |service: &str, count: u32| WantedService {
        service: service.to_owned(),
        config_hash: format!("hash-{service}"),
        count,
        image_id: Some(format!("sha256:{service}")),
        always_pulled: false,
    };
    let running = |service: &str| RunningContainer {
        service: service.to_owned(),
        config_hash: Some(format!("hash-{service}")),
        image_id: format!("sha256:{service}"),
        running: true,
        health: Some("healthy".to_owned()),
        leaving: false,
    };
    let services = [wanted("web", 2), wanted("worker", 1), wanted("idle", 0)];
    let same = vec![running("web"), running("web"), running("worker")];
    assert!(stack_unchanged(&services, &same));

    let without_check = vec![
        running("web"),
        running("web"),
        RunningContainer {
            health: None,
            ..running("worker")
        },
    ];
    assert!(
        stack_unchanged(&services, &without_check),
        "no health check"
    );

    let changed: Vec<(&str, Vec<RunningContainer>)> = vec![
        (
            "one container short",
            vec![running("web"), running("worker")],
        ),
        (
            "an extra container",
            vec![
                running("web"),
                running("web"),
                running("worker"),
                running("worker"),
            ],
        ),
        (
            "a service the files no longer have",
            [same.clone(), vec![running("gone")]].concat(),
        ),
        (
            "another configuration",
            vec![
                running("web"),
                RunningContainer {
                    config_hash: Some("old".to_owned()),
                    ..running("web")
                },
                running("worker"),
            ],
        ),
        (
            "a tag that names another image now",
            vec![
                running("web"),
                running("web"),
                RunningContainer {
                    image_id: "sha256:old".to_owned(),
                    ..running("worker")
                },
            ],
        ),
        (
            "not healthy",
            vec![
                running("web"),
                running("web"),
                RunningContainer {
                    health: Some("starting".to_owned()),
                    ..running("worker")
                },
            ],
        ),
        (
            "stopped",
            vec![
                running("web"),
                running("web"),
                RunningContainer {
                    running: false,
                    ..running("worker")
                },
            ],
        ),
    ];
    for (why, containers) in changed {
        assert!(!stack_unchanged(&services, &containers), "{why}");
    }

    let always = [
        WantedService {
            always_pulled: true,
            ..wanted("web", 2)
        },
        wanted("worker", 1),
    ];
    assert!(!stack_unchanged(&always, &same), "pull_policy: always");
    let missing_image = [
        WantedService {
            image_id: None,
            ..wanted("web", 2)
        },
        wanted("worker", 1),
    ];
    assert!(
        !stack_unchanged(&missing_image, &same),
        "the tag is not on the node"
    );
    assert!(!stack_unchanged(&[], &[]), "nothing to compare");
}

#[test]
fn a_container_that_drains_from_an_earlier_deploy_is_neither_old_nor_new() {
    use mm::domain::rollout::{Beside, RunningContainer, WantedService, beside, stack_unchanged};

    let container = |hash: &str, leaving: bool| RunningContainer {
        service: "whoami".to_owned(),
        config_hash: Some(hash.to_owned()),
        image_id: format!("sha256:{hash}"),
        running: true,
        health: None,
        leaving,
    };
    let wanted = |hash: &str| WantedService {
        service: "whoami".to_owned(),
        config_hash: hash.to_owned(),
        count: 1,
        image_id: Some(format!("sha256:{hash}")),
        always_pulled: false,
    };

    let back_to_older = [container("v1.11", false), container("v1.10", true)];
    assert_eq!(
        beside("v1.10", &back_to_older),
        Beside::Start { scale: 3 },
        "the draining v1.10 is not the new container: a new one starts, and Compose counts the draining one"
    );
    assert!(!stack_unchanged(&[wanted("v1.10")], &back_to_older));

    let same_again = [container("v1.11", false), container("v1.10", true)];
    assert_eq!(beside("v1.11", &same_again), Beside::Nothing);
    assert!(
        stack_unchanged(&[wanted("v1.11")], &same_again),
        "the stack runs what the files start once the old container is gone"
    );

    assert_eq!(
        beside("v1.11", &[container("v1.10", false)]),
        Beside::Start { scale: 2 }
    );
    assert_eq!(
        beside(
            "v1.11",
            &[container("v1.11", false), container("v1.10", false)]
        ),
        Beside::Started,
        "an interrupted start left the new container next to the old"
    );
    assert_eq!(
        beside("v1.11", &[container("v1.10", true)]),
        Beside::Nothing
    );
    assert_eq!(
        beside(
            "v1.11",
            &[RunningContainer {
                running: false,
                ..container("v1.10", false)
            }]
        ),
        Beside::Nothing
    );
}

#[test]
fn a_stack_a_step_left_alone_is_not_rolled_back() {
    use mm::domain::rollout::RolloutChanges;

    let mut changes = RolloutChanges::default();
    changes.note("db", false);
    changes.note("app", false);
    changes.forget("app");
    assert!(changes.whole("db"));
    assert!(!changes.whole("app"));
    assert_eq!(changes.begun(), ["db"]);
}

#[test]
fn a_rendered_configuration_says_which_images_are_pulled_always() -> TestResult {
    let images = mm::domain::rollout::service_images(
        "services:\n  web:\n    image: web:2\n  cache:\n    image: redis:7\n    pull_policy: always\n  built:\n    build: .\n",
    )?;
    assert_eq!(images.get("web"), Some(&(Some("web:2".to_owned()), false)));
    assert_eq!(
        images.get("cache"),
        Some(&(Some("redis:7".to_owned()), true))
    );
    assert_eq!(images.get("built"), Some(&(None, false)));
    Ok(())
}

#[test]
fn every_node_of_a_stack_rolls_together_when_its_shared_services_start_first() -> TestResult {
    use mm::domain::placement::PlacedService;

    let placed = |services: &[&str]| {
        services
            .iter()
            .map(|service| PlacedService {
                stack: "app".to_owned(),
                service: (*service).to_owned(),
            })
            .collect::<Vec<_>>()
    };
    let (web_and_cache, web, worker) = (
        placed(&["web", "cache"]),
        placed(&["web"]),
        placed(&["worker"]),
    );
    let app = names(&["app"]);
    let stage = |services: &str,
                 lists: [&[PlacedService]; 3]|
     -> Result<String, Box<dyn std::error::Error>> {
        let config = config("  app: {files: [a.yaml]}", "{}", services)?;
        let policy = RolloutPolicy::from_config(&config, "production")?;
        let rollout = ClusterRollout::of(
            &["n1", "n2", "n3"]
                .iter()
                .zip(lists)
                .map(|(node, services)| NodeRolloutInput {
                    node,
                    stacks: &app,
                    policy: &policy,
                    services,
                })
                .collect::<Vec<_>>(),
        );
        Ok(rollout.stages[0].describe())
    };
    let lists = [&web_and_cache[..], &web[..], &worker[..]];
    assert_eq!(
        stage(
            "      web: {stack: app, update: {order: start-first}}",
            lists
        )?,
        "app on n1 and n2 and n3",
        "web runs on two nodes and starts first; cache and worker run on one each"
    );
    assert_eq!(
        stage("      web: {stack: app}", lists)?,
        "app on n1, then n2, then n3",
        "a stop-first service on two nodes keeps them one at a time"
    );
    assert_eq!(
        stage(
            "      web: {stack: app, update: {order: start-first, parallelism: 1}}",
            lists
        )?,
        "app on n1, then n2, then n3",
        "a parallelism the stack sets wins"
    );
    assert_eq!(
        stage(
            "      web: {stack: app, update: {order: start-first}}",
            [&web[..], &web[..], &[][..]]
        )?,
        "app on n1, then n2, then n3",
        "a node that does not say what it runs keeps them one at a time"
    );
    Ok(())
}

#[test]
fn stages_go_in_waves_by_the_layers_of_after_and_alone_when_they_cannot_share() {
    use mm::domain::rollout::{RolloutStage, StageJob, check_waves, waves_of};
    use std::collections::{BTreeMap, BTreeSet};

    let stage = |stack: &str, batches: &[&[&str]]| RolloutStage {
        stack: stack.to_owned(),
        batches: batches
            .iter()
            .map(|batch| batch.iter().map(|node| (*node).to_owned()).collect())
            .collect(),
        start_first: Vec::new(),
        pause_on_failure: Vec::new(),
        jobs: Vec::new(),
        ahead: Vec::new(),
    };
    let after = |pairs: &[(&str, &[&str])]| {
        pairs
            .iter()
            .map(|(stack, earlier)| {
                (
                    (*stack).to_owned(),
                    earlier
                        .iter()
                        .map(|name| (*name).to_owned())
                        .collect::<BTreeSet<_>>(),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    let one = &[&["a", "b"][..]][..];
    // The stages in the order the plan rolls them out: db first, then the
    // stacks that follow it, by name.
    let stages = vec![
        stage("cache", one),
        stage("db", one),
        stage("api", one),
        stage("web", one),
        stage("worker", &[&["a"], &["b"]]),
    ];
    let order = after(&[("api", &["db"]), ("web", &["api"]), ("worker", &["db"])]);
    let waves = waves_of(&stages, &order);
    assert_eq!(
        waves,
        [vec![0, 1], vec![2], vec![4], vec![3]],
        "cache and db side by side; api once db is done; worker has two batches, so alone"
    );
    assert!(check_waves(&waves, &stages, &order).is_ok());
    assert_eq!(
        waves_of(&stages, &order),
        waves,
        "the same plan, the same waves"
    );

    let mut with_job = stages.clone();
    with_job[0].jobs.push(StageJob {
        job: "migrate".to_owned(),
        node: "a".to_owned(),
        timeout_seconds: 60,
    });
    assert!(
        waves_of(&with_job, &order)
            .iter()
            .all(|wave| wave.len() == 1 || !wave.contains(&0)),
        "a stage with jobs goes alone"
    );

    assert!(
        waves_of(&stages[1..4], &order).is_empty(),
        "no two stages side by side: no waves"
    );

    let early = check_waves(&[vec![1, 2], vec![0], vec![3], vec![4]], &stages, &order)
        .err()
        .unwrap_or_default();
    assert!(
        early.contains("stack 'api'") && early.contains("'db'"),
        "{early}"
    );
    let twice = check_waves(&[vec![0, 1], vec![1, 2], vec![3], vec![4]], &stages, &order)
        .err()
        .unwrap_or_default();
    assert!(twice.contains("twice"), "{twice}");
    let shared = check_waves(&[vec![0, 1], vec![2], vec![3, 4]], &stages, &order)
        .err()
        .unwrap_or_default();
    assert!(shared.contains("stack 'worker'"), "{shared}");
    let missing = check_waves(&[vec![0, 1], vec![2], vec![3]], &stages, &order)
        .err()
        .unwrap_or_default();
    assert!(missing.contains("leave out stack 'worker'"), "{missing}");
}

#[test]
fn the_heavy_cluster_example_rolls_out_in_layers() -> TestResult {
    use mm::domain::rollout::{RolloutStage, StageJob, waves_of};

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/cluster-heavy/.config/mm.yaml");
    let config: MmConfig = serde_yaml_ng::from_str(&std::fs::read_to_string(path)?)?;
    let policy = RolloutPolicy::from_config(&config, "production")?;
    assert_eq!(policy.stack_order.len(), 10);
    let stages = policy
        .stack_order
        .iter()
        .map(|stack| RolloutStage {
            stack: stack.clone(),
            batches: vec![vec!["n1".to_owned(), "n2".to_owned(), "n3".to_owned()]],
            start_first: Vec::new(),
            pause_on_failure: Vec::new(),
            jobs: if stack == "api" {
                vec![StageJob {
                    job: "migrate".to_owned(),
                    node: "n1".to_owned(),
                    timeout_seconds: 120,
                }]
            } else {
                Vec::new()
            },
            ahead: Vec::new(),
        })
        .collect::<Vec<_>>();
    let after = config
        .stacks
        .iter()
        .map(|(name, stack)| (name.clone(), stack.after.iter().cloned().collect()))
        .collect();
    let names = |wave: &Vec<usize>| {
        wave.iter()
            .map(|index| stages[*index].stack.as_str())
            .collect::<Vec<_>>()
    };
    let waves = waves_of(&stages, &after);
    assert_eq!(
        waves.iter().map(names).collect::<Vec<_>>(),
        [
            vec!["cache", "data", "queue"],
            vec!["api"],
            vec!["auth", "files", "search"],
            vec!["admin", "web", "worker"],
        ]
    );
    Ok(())
}
