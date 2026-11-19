//! Autoscale: the configuration as the owner wrote it, the rules
//! every node applies the same way (share, decider, newest count, the
//! decision, one node's next step), and the refusals that name the key to
//! change.

use std::time::Duration;

use mm::domain::autoscale::{
    AutoscaleConfig, Decision, Moment, NodeStep, OwnContainer, ScaleChange, ScalePolicy,
    ScaleReport, ScaledService, ServiceScale, Want, cluster_tally, decide, decider, newest_want,
    node_step, settled_everywhere, share,
};
use mm::domain::metrics::{Metric, Percent, Tally};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const OWNER_SAMPLE: &str = "
scale:
  min: 1
  max: 4
metric:
  type: cpu
  up:
    greater: 75%
    for: 30s
  down:
    less: 40%
    for: 5m
";

fn nodes(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn policy(config: &str, replicas: Option<u32>, placed: usize) -> Result<ScalePolicy, String> {
    let config: AutoscaleConfig =
        serde_yaml_ng::from_str(config).map_err(|error| error.to_string())?;
    ScalePolicy::resolve(&config, replicas, placed).map_err(|problems| {
        problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    })
}

#[test]
fn the_owners_sample_reads_as_written() -> TestResult {
    let policy = policy(OWNER_SAMPLE, None, 3)?;
    assert_eq!((policy.min, policy.max, policy.initial), (1, 4, 1));
    assert_eq!(policy.metric, Metric::Cpu);
    assert_eq!(policy.up.threshold, Percent::from_whole(75));
    assert_eq!(policy.up.hold(), Duration::from_secs(30));
    assert_eq!(policy.down.threshold, Percent::from_whole(40));
    assert_eq!(policy.down.hold(), Duration::from_secs(300));
    Ok(())
}

#[test]
fn only_max_is_required_and_min_defaults_to_replicas() -> TestResult {
    // Without `replicas`: one container on each placement node.
    let defaults = policy("scale: {max: 4}", None, 3)?;
    assert_eq!((defaults.min, defaults.max, defaults.initial), (3, 4, 3));
    assert_eq!(defaults.up.threshold, Percent::from_whole(75));
    assert_eq!(defaults.down.hold(), Duration::from_secs(300));
    // With `replicas: 2`, `min` is 2 and the service starts at 2.
    let replicas = policy("scale: {max: 4}", Some(2), 3)?;
    assert_eq!((replicas.min, replicas.initial), (2, 2));
    // `replicas` within an explicit range is where the service starts.
    let inside = policy("scale: {min: 1, max: 4}", Some(3), 3)?;
    assert_eq!((inside.min, inside.initial), (1, 3));
    // `type: memory` and a `for:` that is not whole steps rounds up.
    let memory = policy(
        "{scale: {max: 2}, metric: {type: memory, up: {for: 12s}}}",
        Some(1),
        2,
    )?;
    assert_eq!(memory.metric, Metric::Memory);
    assert_eq!(memory.up.hold(), Duration::from_secs(15));
    Ok(())
}

#[test]
fn every_problem_names_the_key_to_change() {
    let cases = [
        (
            "scale: {min: 5, max: 4}",
            None,
            "`autoscale.scale.min` is 5, above",
        ),
        (
            "scale: {max: 2}",
            None,
            "`autoscale.scale.min` is not set, so it is the service's `replicas`, 3 here",
        ),
        ("scale: {max: 0}", None, "`autoscale.scale.max` is 0"),
        (
            "scale: {min: 0, max: 2}",
            None,
            "`autoscale.scale.min` is 0",
        ),
        (
            "scale: {min: 1, max: 4}",
            Some(6),
            "`replicas` is 6, outside `autoscale.scale` 1..4",
        ),
        (
            "{scale: {max: 4}, metric: {down: {less: 80%}}}",
            Some(1),
            "`autoscale.metric.down.less` is 80%, not below `autoscale.metric.up.greater` (75%)",
        ),
        (
            "{scale: {max: 4}, metric: {up: {for: 5h}}}",
            Some(1),
            "`autoscale.metric.up.for` is longer than 4h, the longest `for:`",
        ),
        (
            "{scale: {max: 4}, metric: {up: {for: 0s}}}",
            Some(1),
            "`autoscale.metric.up.for` is 0s",
        ),
        (
            "{scale: {max: 4}, metric: {up: {greater: lots}}}",
            Some(1),
            "`autoscale.metric.up.greater` is 'lots'; write a percentage, such as 75%",
        ),
        (
            "{scale: {max: 4}, metric: {down: {less: 0%}}}",
            Some(1),
            "`autoscale.metric.down.less` is 0%",
        ),
        (
            "scale: {max: 1000}",
            Some(1),
            "`autoscale.scale.max` is 1000, which puts up to 334 containers on one",
        ),
    ];
    for (config, replicas, expected) in cases {
        let found = policy(config, replicas, 3).err().unwrap_or_default();
        assert!(found.contains(expected), "{config}: {found}");
    }
    // A number where a percentage goes is refused while reading, with the
    // way to write it.
    let number = serde_yaml_ng::from_str::<AutoscaleConfig>(
        "{scale: {max: 4}, metric: {up: {greater: 75}}}",
    )
    .err()
    .map(|error| error.to_string())
    .unwrap_or_default();
    assert!(
        number.contains("write it as a percentage, such as `75%`"),
        "{number}"
    );
    assert!(serde_yaml_ng::from_str::<AutoscaleConfig>("scale: {max: 4, maximum: 5}").is_ok());
    assert!(
        serde_yaml_ng::from_str::<AutoscaleConfig>("{metric: {}}").is_err(),
        "max is required"
    );
}

#[test]
fn a_count_is_shared_fewest_first_in_placement_order() {
    let placed = nodes(&["app-1", "app-2", "app-3"]);
    let shares = |count| {
        placed
            .iter()
            .map(|node| share(count, &placed, node))
            .collect::<Vec<_>>()
    };
    assert_eq!(shares(0), [0, 0, 0]);
    assert_eq!(shares(1), [1, 0, 0]);
    assert_eq!(shares(2), [1, 1, 0]);
    assert_eq!(shares(3), [1, 1, 1]);
    assert_eq!(shares(4), [2, 1, 1]);
    assert_eq!(shares(5), [2, 2, 1]);
    assert_eq!(
        share(3, &placed, "db-1"),
        0,
        "a node outside the placement runs none"
    );
    for count in 0..40_u32 {
        let now = shares(count);
        let next = shares(count + 1);
        assert_eq!(now.iter().sum::<u32>(), count);
        // One container more lands on exactly one node: the first with the
        // fewest. One less leaves the last node with the most.
        let grew = (0..3)
            .filter(|index| next[*index] == now[*index] + 1)
            .collect::<Vec<_>>();
        assert_eq!(grew.len(), 1);
        let fewest = now.iter().copied().min().unwrap_or_default();
        assert_eq!(Some(grew[0]), now.iter().position(|share| *share == fewest));
        let most = next.iter().copied().max().unwrap_or_default();
        assert_eq!(Some(grew[0]), next.iter().rposition(|share| *share == most));
    }
}

#[test]
fn the_decider_is_the_first_node_that_answers_and_the_next_one_takes_over() {
    let placed = nodes(&["app-1", "app-2", "app-3"]);
    assert_eq!(decider(&placed, |_| true), Some("app-1"));
    // app-1's mmd stopped: app-2 carries on; app-1 back decides again.
    assert_eq!(decider(&placed, |node| node != "app-1"), Some("app-2"));
    assert_eq!(decider(&placed, |node| node == "app-3"), Some("app-3"));
    assert_eq!(decider(&placed, |_| false), None);
}

fn want(count: u32, generation: u64, by: &str) -> Want {
    Want {
        count,
        generation,
        by: by.to_owned(),
        change: None,
    }
}

#[test]
fn the_newest_count_wins_and_two_of_a_generation_go_by_placement() {
    let placed = nodes(&["app-1", "app-2", "app-3"]);
    let wants = [
        want(2, 4, "app-2"),
        want(3, 5, "app-2"),
        want(1, 3, "app-1"),
    ];
    assert_eq!(newest_want(&wants, &placed).map(|want| want.count), Some(3));
    // Two deciders at once while they did not hear each other: the earlier
    // node's want is the one every node goes by.
    let split = [want(4, 6, "app-2"), want(2, 6, "app-1")];
    assert_eq!(
        newest_want(&split, &placed).map(|want| want.by.as_str()),
        Some("app-1")
    );
    assert!(newest_want(&[], &placed).is_none());
}

fn calm(count: u32) -> Moment {
    Moment {
        count,
        up_held: Duration::ZERO,
        down_held: Duration::ZERO,
        settled: Some(Duration::from_secs(3600)),
        deciding: Duration::from_secs(3600),
        up_average: Some(Percent::from_whole(50)),
        down_average: Some(Percent::from_whole(50)),
    }
}

#[test]
fn a_rule_that_held_for_its_window_adds_or_removes_one_container() -> TestResult {
    let policy = policy(OWNER_SAMPLE, None, 3)?;
    let busy = Moment {
        up_held: Duration::from_secs(30),
        up_average: Some(Percent::from_whole(82)),
        ..calm(2)
    };
    assert_eq!(
        decide(&policy, &busy),
        Decision::Change {
            to: 3,
            reason: "cpu 82% above 75% for 30s".to_owned()
        }
    );
    let idle = Moment {
        down_held: Duration::from_secs(300),
        down_average: Some(Percent::from_whole(12)),
        ..calm(3)
    };
    assert_eq!(
        decide(&policy, &idle),
        Decision::Change {
            to: 2,
            reason: "cpu 12% below 40% for 5m".to_owned()
        }
    );
    // Short of the window, nothing changes, and the status says why.
    let almost = Moment {
        up_held: Duration::from_secs(25),
        ..busy
    };
    assert_eq!(
        decide(&policy, &almost),
        Decision::Keep {
            reason: "cpu 82% above 75% for 25s of 30s".to_owned()
        }
    );
    assert_eq!(
        decide(&policy, &calm(2)),
        Decision::Keep {
            reason: "cpu 50%, between 40% and 75%".to_owned()
        }
    );
    // At the edges of the range the count stays and the reason says so.
    let at_max = Moment { count: 4, ..busy };
    assert!(
        matches!(decide(&policy, &at_max), Decision::Keep { reason } if reason.ends_with("and 4 is `scale.max`"))
    );
    let at_min = Moment { count: 1, ..idle };
    assert!(
        matches!(decide(&policy, &at_min), Decision::Keep { reason } if reason.ends_with("and 1 is `scale.min`"))
    );
    // A deployment that changed the range brings the count into it.
    assert!(matches!(
        decide(&policy, &calm(7)),
        Decision::Change { to: 4, .. }
    ));
    Ok(())
}

#[test]
fn no_step_until_the_last_one_settled_and_held_for_a_whole_window_since() -> TestResult {
    let policy = policy(OWNER_SAMPLE, None, 3)?;
    let busy = Moment {
        up_held: Duration::from_secs(600),
        ..calm(2)
    };
    // A new container is starting, or a leaving one is not gone yet.
    let changing = Moment {
        settled: None,
        ..busy
    };
    assert_eq!(
        decide(&policy, &changing),
        Decision::Keep {
            reason: "waiting for the last change to settle on every node".to_owned()
        }
    );
    // Settled 10 s ago: the rule has to hold 30 s with the new count.
    let just_settled = Moment {
        settled: Some(Duration::from_secs(10)),
        ..busy
    };
    assert_eq!(
        decide(&policy, &just_settled),
        Decision::Keep {
            reason: "cpu 50% above 75% for 10s of 30s".to_owned()
        }
    );
    let settled = Moment {
        settled: Some(Duration::from_secs(30)),
        ..busy
    };
    assert!(matches!(
        decide(&policy, &settled),
        Decision::Change { to: 3, .. }
    ));
    Ok(())
}

#[test]
fn a_new_decider_watches_one_whole_window_before_it_changes_anything() -> TestResult {
    let policy = policy(OWNER_SAMPLE, None, 3)?;
    // The node took over 20 s ago; the cluster has been busy far longer.
    let took_over = Moment {
        up_held: Duration::from_secs(20),
        deciding: Duration::from_secs(20),
        ..calm(2)
    };
    assert_eq!(
        decide(&policy, &took_over),
        Decision::Keep {
            reason: "watching one whole `for:` before changing anything".to_owned()
        }
    );
    let watched = Moment {
        up_held: Duration::from_secs(30),
        deciding: Duration::from_secs(30),
        ..took_over
    };
    assert!(matches!(
        decide(&policy, &watched),
        Decision::Change { to: 3, .. }
    ));
    // The down rule waits for its own, longer window.
    let quiet = Moment {
        down_held: Duration::from_secs(300),
        deciding: Duration::from_secs(60),
        ..calm(3)
    };
    assert!(matches!(decide(&policy, &quiet), Decision::Keep { .. }));
    Ok(())
}

#[test]
fn thresholds_that_overlap_are_refused_so_the_count_cannot_flap() {
    let flapping = policy(
        "{scale: {max: 4}, metric: {up: {greater: 50%}, down: {less: 50%}}}",
        Some(1),
        3,
    )
    .err()
    .unwrap_or_default();
    assert!(
        flapping.contains("would add and remove containers in turn"),
        "{flapping}"
    );
}

fn container(id: &str, created: &str) -> OwnContainer {
    OwnContainer {
        id: id.to_owned(),
        created: created.to_owned(),
        running: true,
        healthy: true,
        leaving: false,
    }
}

#[test]
fn a_node_takes_one_step_at_a_time_toward_its_share() {
    let one = [container("aaa", "2026-11-14T10:00:00Z")];
    assert_eq!(node_step(1, &one), NodeStep::Settled);
    // Below its share: Compose is asked for one more than it counts.
    assert_eq!(node_step(2, &one), NodeStep::Start { scale_to: 2 });
    assert_eq!(node_step(1, &[]), NodeStep::Start { scale_to: 1 });
    // The new one counts once it is healthy; nothing else starts until then.
    let starting = [
        container("aaa", "2026-11-14T10:00:00Z"),
        OwnContainer {
            healthy: false,
            ..container("bbb", "2026-11-14T10:05:00Z")
        },
    ];
    assert!(matches!(node_step(3, &starting), NodeStep::Wait { .. }));
    // A stopped container is counted by Compose, not by the share.
    let stopped = [
        container("aaa", "2026-11-14T10:00:00Z"),
        OwnContainer {
            running: false,
            healthy: false,
            ..container("ccc", "2026-11-14T09:00:00Z")
        },
    ];
    assert_eq!(node_step(2, &stopped), NodeStep::Start { scale_to: 3 });
}

#[test]
fn a_container_that_restarts_in_place_is_starting_even_while_stopped() {
    let seen = |running, healthy, leave| {
        OwnContainer::seen(
            ("aaa".to_owned(), "2026-11-14T10:00:00Z".to_owned()),
            running,
            healthy,
            leave,
        )
    };
    // `mm service restart` drains it, then the engine stops and starts it:
    // in between the engine shows it stopped. The node waits for it and
    // starts no other (found by the Podman live run, where that moment is
    // seconds long).
    for running in [true, false] {
        let restarting = [seen(running, true, Some(true))];
        assert!(
            matches!(node_step(1, &restarting), NodeStep::Wait { reason } if reason == "a container is starting"),
            "running: {running}"
        );
    }
    // A removal is leaving, and a container without a leave is as the
    // engine shows it.
    assert!(seen(true, true, Some(false)).leaving);
    assert!(!seen(true, true, Some(false)).healthy);
    assert_eq!(
        seen(true, true, None),
        container("aaa", "2026-11-14T10:00:00Z")
    );
    assert_eq!(
        node_step(1, &[seen(false, false, None)]),
        NodeStep::Start { scale_to: 2 }
    );
}

#[test]
fn the_newest_container_leaves_and_nothing_else_moves_until_it_is_gone() {
    let three = [
        container("old", "2026-11-14T10:00:00Z"),
        container("newest", "2026-11-14T10:09:00Z"),
        container("middle", "2026-11-14T10:05:00Z"),
    ];
    assert_eq!(
        node_step(2, &three),
        NodeStep::Leave {
            id: "newest".to_owned()
        }
    );
    // Marked, confirmed, draining: the container still runs and serves
    // what it has, and the node waits; it is not settled.
    let leaving = [
        container("old", "2026-11-14T10:00:00Z"),
        container("middle", "2026-11-14T10:05:00Z"),
        OwnContainer {
            leaving: true,
            ..container("newest", "2026-11-14T10:09:00Z")
        },
    ];
    assert!(
        matches!(node_step(2, &leaving), NodeStep::Wait { reason } if reason == "a container is leaving")
    );
    assert!(
        matches!(node_step(1, &leaving), NodeStep::Wait { .. }),
        "one leave at a time"
    );
    // Gone: settled at two.
    assert_eq!(node_step(2, &leaving[..2]), NodeStep::Settled);
    // A count that grows back while one leaves starts a new one at once:
    // the leaving one is counted by Compose until it is gone.
    assert_eq!(node_step(3, &leaving), NodeStep::Start { scale_to: 4 });
}

fn scaled(autoscale: Option<ScalePolicy>, replicas: u32) -> ScaledService {
    ScaledService {
        key: mm::domain::autoscale::service_key(
            mm::domain::workspace::ProjectId(uuid::Uuid::nil()),
            "production",
            "app",
            "web",
        ),
        stack: "app".to_owned(),
        service: "web".to_owned(),
        compose_project: "mm-app-production-app".to_owned(),
        nodes: nodes(&["app-1", "app-2", "app-3"]),
        replicas,
        autoscale,
        drain_ms: 60_000,
        now: None,
    }
}

#[test]
fn a_deployment_keeps_the_count_the_nodes_agreed_on_within_the_range() -> TestResult {
    let autoscaled = scaled(Some(policy(OWNER_SAMPLE, None, 3)?), 3);
    assert_eq!(
        autoscaled.key(),
        "00000000-0000-0000-0000-000000000000/production/app/web",
        "the same on every node, whatever its Compose project is called"
    );
    // Before anything scaled it: `min` (the initial count).
    assert_eq!(autoscaled.count(None), 1);
    assert_eq!(autoscaled.share("app-1", None), 1);
    assert_eq!(autoscaled.share("app-2", None), 0);
    // Scaled to 3: a deployment runs 3, not `min`.
    assert_eq!(autoscaled.count(Some(3)), 3);
    assert_eq!(autoscaled.share("app-3", Some(3)), 1);
    // A range that shrank bounds it.
    assert_eq!(autoscaled.count(Some(9)), 4);
    // Without autoscale `replicas` is the count, whatever was agreed.
    let fixed = scaled(None, 5);
    assert_eq!(fixed.count(Some(1)), 5);
    assert_eq!(fixed.share("app-1", None), 2);
    Ok(())
}

#[test]
fn the_count_of_the_moment_is_not_part_of_a_plans_hash() -> TestResult {
    let mut node = mm::domain::deployment::NodeProjectPlan {
        node: "app-1".to_owned(),
        services: Vec::new(),
        starting: Vec::new(),
        leaving: Vec::new(),
        dropped_dependencies: Vec::new(),
        networks: Vec::new(),
        restart_defaults: Vec::new(),
        shared_volumes: Vec::new(),
        ingress: None,
        scaled: vec![scaled(Some(policy(OWNER_SAMPLE, None, 3)?), 3)],
        health: Vec::new(),
    };
    node.scaled[0].now = Some(3);
    let hashed = node.hashed();
    assert_eq!(hashed.scaled[0].now, None);
    node.scaled[0].now = Some(1);
    assert_eq!(
        node.hashed(),
        hashed,
        "the count changes with load, the plan does not"
    );
    Ok(())
}

#[test]
fn two_nodes_answers_merge_into_the_newer_count_and_the_deciders_reading() {
    let base = ServiceScale {
        wanted: 2,
        running: 2,
        min: 1,
        max: 4,
        autoscale: true,
        generation: 3,
        decider: Some("app-1".to_owned()),
        last_change: Some(ScaleChange {
            from: 1,
            to: 2,
            reason: "cpu 82% above 75% for 30s".to_owned(),
            at_unix_ms: 1,
        }),
        holding: None,
        reading: None,
    };
    let newer = ServiceScale {
        wanted: 3,
        running: 1,
        generation: 4,
        ..base.clone()
    };
    let merged = base.clone().merged(newer);
    assert_eq!(
        (merged.wanted, merged.generation, merged.running),
        (3, 4, 2)
    );
}

/// A project whose service `web` (on network `backend`, placed on every
/// node of the three-node target `production`) has `service` added to its
/// entry, and whose Compose file has `compose` added to `web`.
fn project(service: &str, compose: &str) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let root = tempfile::TempDir::new()?;
    std::fs::create_dir_all(root.path().join(".config"))?;
    std::fs::write(
        root.path().join("compose.yaml"),
        format!("services:\n  web:\n    image: docker.io/library/nginx:1.27\n{compose}"),
    )?;
    std::fs::write(
        root.path().join(".config/mm.yaml"),
        format!(
            "schema-version: 1
default-environment: production
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: scaled
target: production
runtime:
  driver: docker-compose
stacks:
  app:
    files: [compose.yaml]
networks:
  backend: {{}}
modules:
  web:
    path: .
    services:
      web:
        stack: app
        placement: all
        networks: [backend]
{service}
targets:
  production:
    nodes:
      - {{name: app-1, host: 203.0.113.11}}
      - {{name: app-2, host: 203.0.113.12}}
      - {{name: app-3, host: 203.0.113.13}}
  single:
    host: 203.0.113.20
environments:
  production: {{}}
  single:
    target: single
"
        ),
    )?;
    Ok(root)
}

/// The `MM_SCALE` findings of the project in `environment`.
fn findings(
    root: &tempfile::TempDir,
    environment: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let document = mm::adapters::config::ConfigLoader.load_from(root.path())?;
    let report = mm::adapters::config::validate(&document, environment);
    Ok(report
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "MM_SCALE")
        .map(|diagnostic| diagnostic.message)
        .collect())
}

const SAMPLE_ON_WEB: &str = "        autoscale:
          scale:
            min: 1
            max: 4
          metric:
            type: cpu
            up:
              greater: 75%
              for: 30s
            down:
              less: 40%
              for: 5m";

#[test]
fn the_sample_validates_on_a_cluster_and_max_alone_does_too() -> TestResult {
    let sample = project(SAMPLE_ON_WEB, "")?;
    assert_eq!(findings(&sample, "production")?, Vec::<String>::new());
    let shortest = project(
        "        replicas: 1\n        autoscale:\n          scale:\n            max: 4",
        "",
    )?;
    assert_eq!(findings(&shortest, "production")?, Vec::<String>::new());
    Ok(())
}

#[test]
fn a_one_host_target_is_told_how_to_become_a_cluster() -> TestResult {
    let sample = project(SAMPLE_ON_WEB, "")?;
    let found = findings(&sample, "single")?;
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("environment 'single' deploys to target 'single', one host. Write targets.single as a cluster: `nodes: [{name: <node>, host: <address>}]`"),
        "{found:?}"
    );
    Ok(())
}

#[test]
fn a_range_problem_names_the_entry_and_the_key() -> TestResult {
    let project = project(
        "        autoscale:\n          scale:\n            max: 2",
        "",
    )?;
    let found = findings(&project, "production")?;
    assert!(
        found.iter().any(|finding| finding.starts_with("modules.web.services.web.autoscale.scale.min is not set, so it is the service's `replicas`, 3 here")),
        "{found:?}"
    );
    Ok(())
}

#[test]
fn compose_keys_that_cannot_work_with_several_containers_are_refused() -> TestResult {
    let pinned = project(
        SAMPLE_ON_WEB,
        "    container_name: web\n    ports: [\"8080:80\"]\n",
    )?;
    let found = findings(&pinned, "production")?;
    assert!(
        found
            .iter()
            .any(|finding| finding.contains("sets `container_name`")),
        "{found:?}"
    );
    assert!(
        found.iter().any(|finding| finding.contains("publishes a host port, and modules.web.services.web.autoscale.scale.max (4) is more than the 3 node(s)")),
        "{found:?}"
    );
    // At most one per node: a host port is fine.
    let one_each = project(
        "        autoscale:\n          scale:\n            min: 1\n            max: 3",
        "    ports: [\"8080:80\"]\n",
    )?;
    assert_eq!(findings(&one_each, "production")?, Vec::<String>::new());
    Ok(())
}

#[test]
fn compose_deploy_replicas_on_a_cluster_points_at_replicas_in_mm_yaml() -> TestResult {
    let swarm = project("", "    deploy:\n      replicas: 3\n")?;
    let document = mm::adapters::config::ConfigLoader.load_from(swarm.path())?;
    let report = mm::adapters::config::validate(&document, "production");
    assert!(
        report.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("write `replicas: 3` under modules.web.services.web in mm.yaml")),
        "{:?}",
        report.diagnostics
    );
    Ok(())
}

#[test]
fn an_autoscaled_service_needs_a_network_its_nodes_share() -> TestResult {
    let root = project(SAMPLE_ON_WEB, "")?;
    let path = root.path().join(".config/mm.yaml");
    let config = std::fs::read_to_string(&path)?.replace("        networks: [backend]\n", "");
    std::fs::write(&path, config)?;
    let found = findings(&root, "production")?;
    assert!(
        found
            .iter()
            .any(|finding| finding.contains("is on no network of mm.yaml")
                && finding.contains("`networks: [<name>]`")),
        "{found:?}"
    );
    Ok(())
}

fn report(running: u32, settled: bool, busy: &[u16]) -> ScaleReport {
    ScaleReport {
        service: "00000000-0000-0000-0000-000000000000/production/app/web".to_owned(),
        tally: busy.iter().fold(Tally::NONE, |sum, percent| {
            sum.plus(Tally::one(Percent::from_whole(*percent)))
        }),
        running,
        settled,
        want: None,
    }
}

#[test]
fn a_node_not_heard_from_is_left_out_never_counted_as_zero() {
    let own = report(1, true, &[90]);
    let app_2 = report(1, true, &[80]);
    // app-3 runs a container too, but has not been heard from for a while:
    // the step averages what was heard, 85%, not (90 + 80 + 0) / 3.
    let heard = [("app-2", &app_2)];
    let total = cluster_tally(own.tally, &heard);
    assert_eq!(total.count, 2);
    assert_eq!(total.average(), Some(Percent::from_whole(85)));
}

#[test]
fn the_count_settles_on_the_nodes_that_answer_and_waits_for_none_that_do_not() {
    let placed = nodes(&["app-1", "app-2", "app-3"]);
    let own = report(1, true, &[]);
    let app_2 = report(1, true, &[]);
    // 3 wanted, 1 on each node: app-3 does not answer, so it is not waited for.
    assert!(settled_everywhere(
        3,
        &placed,
        ("app-1", &own),
        &[("app-2", &app_2)],
        &["app-1", "app-2"]
    ));
    // app-3 answers and has said nothing about the service yet.
    assert!(!settled_everywhere(
        3,
        &placed,
        ("app-1", &own),
        &[("app-2", &app_2)],
        &["app-1", "app-2", "app-3"]
    ));
    // app-2 is still starting its container.
    let starting = report(0, false, &[]);
    assert!(!settled_everywhere(
        3,
        &placed,
        ("app-1", &own),
        &[("app-2", &starting)],
        &["app-1", "app-2"]
    ));
    // app-2 runs what an older count gave it.
    assert!(!settled_everywhere(
        2,
        &placed,
        ("app-1", &own),
        &[("app-2", &report(2, true, &[]))],
        &["app-1", "app-2"]
    ));
}

#[test]
fn podman_compose_scales_like_docker_compose() -> TestResult {
    // A cluster node's Podman runs Docker Compose against Podman's socket,
    // which honours `--scale`: nothing is refused.
    let root = project(SAMPLE_ON_WEB, "")?;
    let path = root.path().join(".config/mm.yaml");
    let config =
        std::fs::read_to_string(&path)?.replace("driver: docker-compose", "driver: podman-compose");
    std::fs::write(&path, config)?;
    assert_eq!(findings(&root, "production")?, Vec::<String>::new());
    Ok(())
}

fn deployed_with(services: Vec<ScaledService>) -> mm::adapters::autoscale::DeployedScaling {
    mm::adapters::autoscale::DeployedScaling {
        scope: mm::domain::remote::RemoteScope {
            project_id: mm::domain::workspace::ProjectId(uuid::Uuid::nil()),
            environment: "production".to_owned(),
            target: "production".to_owned(),
            target_fingerprint: "0".repeat(64),
            runtime: mm::domain::runtime::RuntimeKind::DockerCompose,
            node: Some("app-1".to_owned()),
        },
        root: std::path::PathBuf::from("/srv/mm/projects/x/production"),
        plan_hash: "a".repeat(64),
        services,
    }
}

fn want_of(count: u32) -> Want {
    Want {
        count,
        generation: 1,
        by: "app-1".to_owned(),
        change: None,
    }
}

#[test]
fn a_count_goes_with_its_service() -> TestResult {
    use mm::adapters::autoscale::store::{deployed, keep_deployed, keep_want, sweep_wants, want};
    let root = tempfile::TempDir::new()?;
    let root = root.path();
    let web = scaled(Some(policy(OWNER_SAMPLE, None, 3)?), 1);
    let worker = ScaledService {
        key: mm::domain::autoscale::service_key(
            mm::domain::workspace::ProjectId(uuid::Uuid::nil()),
            "production",
            "app",
            "worker",
        ),
        service: "worker".to_owned(),
        ..web.clone()
    };
    keep_deployed(root, &deployed_with(vec![web.clone(), worker.clone()]))?;
    keep_want(root, &web.key(), &want_of(3))?;
    keep_want(root, &worker.key(), &want_of(2))?;

    // The next deployment no longer autoscales the worker (it keeps
    // `replicas`): its count is removed, web's stays.
    let replicas_only = ScaledService {
        autoscale: None,
        ..worker.clone()
    };
    keep_deployed(root, &deployed_with(vec![web.clone(), replicas_only]))?;
    assert_eq!(want(root, &worker.key())?, None);
    assert_eq!(want(root, &web.key())?.map(|want| want.count), Some(3));

    // A count a step wrote after its service went is swept at the next one.
    keep_want(root, &worker.key(), &want_of(2))?;
    sweep_wants(root, &deployed(root)?)?;
    assert_eq!(want(root, &worker.key())?, None);
    assert!(want(root, &web.key())?.is_some(), "web is still autoscaled");

    // The environment places nothing here any more: the last count goes
    // with its record.
    keep_deployed(root, &deployed_with(Vec::new()))?;
    assert_eq!(want(root, &web.key())?, None);
    assert!(deployed(root)?.is_empty());
    Ok(())
}
