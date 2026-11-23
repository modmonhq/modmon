use super::*;
use crate::domain::remote::RemoteScope;
use crate::domain::runtime::RuntimeKind;

fn scope() -> RemoteScope {
    RemoteScope {
        project_id: ProjectId(uuid::Uuid::from_u128(7)),
        environment: "production".to_owned(),
        target: "production".to_owned(),
        target_fingerprint: "a".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: None,
    }
}

fn sections(hashes: &[(&str, &str)]) -> Vec<ClusterPlanSection> {
    hashes
        .iter()
        .map(|(node, digit)| ClusterPlanSection {
            node: (*node).to_owned(),
            plan_hash: digit.repeat(64),
        })
        .collect()
}

fn seal(hashes: &[(&str, &str)], skipped: &[&str]) -> Result<ClusterPlanSeal, String> {
    ClusterPlanSeal::new(
        RemotePlanKind::Deployment,
        &scope(),
        &"c".repeat(40),
        sections(hashes),
        skipped.iter().map(|node| (*node).to_owned()).collect(),
        (None, None),
    )
}

const THREE: [(&str, &str); 3] = [("app-1", "1"), ("app-2", "2"), ("db-1", "3")];

#[test]
fn the_cluster_hash_covers_every_node_section_in_order() -> Result<(), String> {
    let reviewed = seal(&THREE, &[])?;
    reviewed.validate()?;
    assert_eq!(
        reviewed.cluster_plan_hash,
        seal(&THREE, &[])?.cluster_plan_hash
    );

    // Any node's plan changing changes the hash.
    for changed in 0..THREE.len() {
        let mut other = THREE;
        other[changed].1 = "9";
        assert_ne!(
            seal(&other, &[])?.cluster_plan_hash,
            reviewed.cluster_plan_hash,
            "section {changed}"
        );
    }
    // So does the order, a node fewer, a skipped node, the commit, the
    // target, and the kind.
    let reordered = [THREE[1], THREE[0], THREE[2]];
    assert_ne!(
        seal(&reordered, &[])?.cluster_plan_hash,
        reviewed.cluster_plan_hash
    );
    assert_ne!(
        seal(&THREE[..2], &[])?.cluster_plan_hash,
        reviewed.cluster_plan_hash
    );
    assert_ne!(
        seal(&THREE[..2], &["db-1"])?.cluster_plan_hash,
        seal(&THREE[..2], &[])?.cluster_plan_hash
    );
    let mut another_commit = reviewed.clone();
    another_commit.commit = "d".repeat(40);
    assert_ne!(another_commit.canonical_hash()?, reviewed.cluster_plan_hash);
    let mut another_target = reviewed.clone();
    another_target.target_fingerprint = "b".repeat(64);
    assert_ne!(another_target.canonical_hash()?, reviewed.cluster_plan_hash);
    let mut team_sync = reviewed.clone();
    team_sync.kind = RemotePlanKind::TeamSync;
    assert_ne!(team_sync.canonical_hash()?, reviewed.cluster_plan_hash);
    Ok(())
}

#[test]
fn a_seal_that_was_changed_after_review_does_not_validate() -> Result<(), String> {
    let reviewed = seal(&THREE, &[])?;
    let mut swapped = reviewed.clone();
    swapped.sections[1].plan_hash = "9".repeat(64);
    assert!(swapped.validate().is_err());
    let mut dropped = reviewed.clone();
    dropped.sections.pop();
    assert!(dropped.validate().is_err());
    let mut skipped = reviewed.clone();
    skipped.skipped.push("cache-1".to_owned());
    assert!(skipped.validate().is_err());

    assert_eq!(
        reviewed
            .section("app-2")
            .map(|section| section.plan_hash.as_str()),
        Some("2".repeat(64).as_str())
    );
    assert!(reviewed.section("cache-1").is_none());
    Ok(())
}

#[test]
fn a_seal_needs_distinct_valid_nodes_and_at_least_one_section() {
    assert!(seal(&[], &[]).is_err());
    assert!(seal(&[("app-1", "1"), ("app-1", "2")], &[]).is_err());
    assert!(seal(&[("app-1", "1")], &["app-1"]).is_err());
    assert!(seal(&[("App One", "1")], &[]).is_err());
    let short = ClusterPlanSeal::new(
        RemotePlanKind::Deployment,
        &scope(),
        &"c".repeat(40),
        vec![ClusterPlanSection {
            node: "app-1".to_owned(),
            plan_hash: "abc".to_owned(),
        }],
        Vec::new(),
        (None, None),
    );
    assert!(short.is_err());
}

fn report(states: &[(&str, NodeApplyState)]) -> ClusterApplyReport<String> {
    ClusterApplyReport {
        plan_hash: "f".repeat(64),
        target: "production".to_owned(),
        environment: "production".to_owned(),
        nodes: states
            .iter()
            .map(|(node, state)| NodeApplyOutcome {
                node: (*node).to_owned(),
                state: *state,
                result: (*state == NodeApplyState::Applied).then(|| "done".to_owned()),
                error: (*state == NodeApplyState::Failed).then(|| "the engine refused".to_owned()),
                rolled_back: false,
                log: Vec::new(),
                stopped: Vec::new(),
                unchanged: false,
                code: None,
                kept: false,
            })
            .collect(),
        skipped: Vec::new(),
    }
}

#[test]
fn a_failed_deployment_says_what_state_each_node_is_in() {
    let mut rolled = report(&[
        ("app-1", NodeApplyState::RolledBack),
        ("app-2", NodeApplyState::Failed),
        ("db-1", NodeApplyState::NotStarted),
    ]);
    rolled.nodes[1].rolled_back = true;
    rolled.nodes[1].log = (1..=7).map(|line| format!("line {line}")).collect();
    assert!(!rolled.succeeded());
    let failure = rolled.failure("mm deploy plan").unwrap_or_default();
    for expected in [
        "failed on node 'app-2': the engine refused",
        "Its last log lines: line 3 | line 4 | line 5 | line 6 | line 7.",
        "Rolled back to what they ran before: app-1, app-2.",
        "Not changed: db-1.",
        "Fix the cause, then run `mm deploy plan` again",
    ] {
        assert!(failure.contains(expected), "{expected}: {failure}");
    }
    assert!(
        !failure.contains('\n'),
        "an error message is one line: {failure}"
    );

    // A deployment that paused, or a node that could not be rolled back,
    // leaves changed nodes behind and says how to deal with them.
    let paused = report(&[
        ("app-1", NodeApplyState::Unfinished),
        ("app-2", NodeApplyState::Failed),
        ("db-1", NodeApplyState::NotStarted),
    ]);
    let failure = paused.failure("mm deploy plan").unwrap_or_default();
    assert!(
        failure.contains("Changed and left as they are: app-1, app-2."),
        "{failure}"
    );
    assert!(failure.contains("`mm deploy status`"), "{failure}");
    assert!(failure.contains("`mm deploy rollback`"), "{failure}");

    // A node that finished before another failed runs the new deployment.
    let mixed = report(&[
        ("app-1", NodeApplyState::Applied),
        ("app-2", NodeApplyState::Failed),
    ]);
    let failure = mixed.failure("mm deploy plan").unwrap_or_default();
    assert!(
        failure.contains("Run the new deployment: app-1."),
        "{failure}"
    );

    // One that was cancelled was stopped, not failed: there is nothing to
    // fix, and its error is `MM_CANCELLED`.
    let mut cancelled = report(&[
        ("app-1", NodeApplyState::RolledBack),
        ("app-2", NodeApplyState::Failed),
        ("db-1", NodeApplyState::NotStarted),
    ]);
    cancelled.nodes[1].rolled_back = true;
    cancelled.nodes[1].code = Some("MM_CANCELLED".to_owned());
    let failure = cancelled.failure("mm deploy plan").unwrap_or_default();
    assert!(failure.contains("was stopped on node 'app-2'"), "{failure}");
    assert!(
        failure.ends_with(". Run `mm deploy plan` again to deploy"),
        "{failure}"
    );
    assert!(
        matches!(
            cancelled.stopped("mm deploy plan"),
            Some(crate::domain::error::MmError::Cancelled { .. })
        ),
        "a cancelled apply ends with MM_CANCELLED"
    );

    let done = report(&[
        ("app-1", NodeApplyState::Applied),
        ("app-2", NodeApplyState::Applied),
    ]);
    assert!(done.succeeded());
    assert_eq!(done.failure("mm deploy plan"), None);
}

#[test]
fn an_apply_report_has_a_stable_json_form() -> Result<(), serde_json::Error> {
    let stopped = report(&[
        ("app-1", NodeApplyState::Applied),
        ("app-2", NodeApplyState::Failed),
        ("db-1", NodeApplyState::NotStarted),
    ]);
    let json = serde_json::to_value(&stopped)?;
    assert_eq!(json["nodes"][0]["state"], "applied");
    assert_eq!(json["nodes"][0]["result"], "done");
    assert_eq!(json["nodes"][1]["state"], "failed");
    assert_eq!(json["nodes"][1]["error"], "the engine refused");
    assert_eq!(json["nodes"][2]["state"], "not-started");
    assert!(json["nodes"][2].get("result").is_none());
    assert_eq!(
        serde_json::from_value::<ClusterApplyReport<String>>(json)?,
        stopped
    );
    Ok(())
}

fn node(name: &str, state: NodeState, version: Option<&str>, commit: Option<&str>) -> NodeReport {
    NodeReport {
        name: name.to_owned(),
        host: "203.0.113.11".to_owned(),
        port: 22,
        mesh_address: Some(Ipv4Addr::new(10, 90, 0, 1)),
        state,
        reachable: state == NodeState::Ready,
        problem: (state != NodeState::Ready).then(|| "did not answer".to_owned()),
        mmd_version: version.map(str::to_owned),
        deployed: commit.map(|commit| NodeDeployment {
            commit: commit.repeat(40),
            deployed_at_unix_ms: 1,
            deployed_by: "alice".to_owned(),
            runs_services: true,
        }),
        rollout: None,
        engine: None,
        daemon: None,
        secret_store: None,
    }
}

#[test]
fn doctor_reports_each_node_that_is_not_ready_with_what_to_run() {
    let reports = [
        node("app-1", NodeState::Ready, Some("0.2.0"), Some("a")),
        node("app-2", NodeState::Unreachable, None, None),
        node("app-3", NodeState::NotSetUp, None, None),
        node("db-1", NodeState::WrongNode, Some("0.2.0"), None),
    ];
    let diagnostics = node_diagnostics("production", &reports);
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
        [
            "MM_NODE_UNREACHABLE",
            "MM_NODE_NOT_SET_UP",
            "MM_NODE_MISMATCH"
        ]
    );
    assert!(
        diagnostics[0]
            .message
            .contains("node 'app-2' (203.0.113.11:22)")
    );
    assert!(
        diagnostics[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("systemctl status mmd"))
    );
    assert!(
        diagnostics[1]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("mm target setup record"))
    );
    assert!(
        diagnostics[2]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("targets.production.nodes"))
    );
}

#[test]
fn doctor_reports_version_and_commit_skew_between_ready_nodes() {
    let skewed = [
        node("app-1", NodeState::Ready, Some("0.2.0"), Some("a")),
        node("app-2", NodeState::Ready, Some("0.1.0"), Some("b")),
        // A node that does not answer is not part of the comparison.
        node("db-1", NodeState::Unreachable, None, None),
    ];
    let diagnostics = node_diagnostics("production", &skewed);
    let version = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "MM_NODE_VERSION_SKEW");
    assert!(
        version.is_some_and(|diagnostic| diagnostic.message.contains("app-1 0.2.0, app-2 0.1.0")),
        "{diagnostics:?}"
    );
    let commit = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "MM_NODE_COMMIT_SKEW");
    assert!(
        commit.is_some_and(
            |diagnostic| diagnostic.message.contains("app-1 aaaaaaaaaaaa")
                && diagnostic.message.contains("app-2 bbbbbbbbbbbb")
        ),
        "{diagnostics:?}"
    );

    let agreeing = [
        node("app-1", NodeState::Ready, Some("0.2.0"), Some("a")),
        node("app-2", NodeState::Ready, Some("0.2.0"), Some("a")),
    ];
    assert!(node_diagnostics("production", &agreeing).is_empty());
}

#[test]
fn the_subset_hint_names_the_nodes_to_pass() {
    assert_eq!(
        unreachable_help(&["app-2"], &["app-1", "db-1"]),
        "To go ahead without app-2, add `--nodes app-1,db-1`: app-2 keeps what it runs now"
    );
    assert_eq!(
        unreachable_help(&["app-2", "db-1"], &["app-1"]),
        "To go ahead without app-2, db-1, add `--nodes app-1`: app-2, db-1 keep what they run now"
    );
    assert!(unreachable_help(&["app-1"], &[]).contains("No other node"));
}
