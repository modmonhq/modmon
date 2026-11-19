//! The inline UI of the command line.

use std::ffi::OsString;
use std::time::{Duration, Instant};

use mm::domain::cluster_setup::{ClusterSetupPlan, ClusterSetupResult};
use mm::domain::progress::{NodeState, ProgressEvent, StepOutcome};
use mm::presentation::cli::OutputFormat;
use mm::presentation::inline::{Card, Mark, Palette, Surface, Tasks, Value, plain, write_lines};
use mm::presentation::output::{setup_plan_card, setup_result_card};
use ratatui::backend::TestBackend;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::{Terminal, TerminalOptions, Viewport};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn surface(format: OutputFormat, terminals: bool, variables: &[(&str, &str)]) -> Surface {
    Surface::decide(format, terminals, |name| {
        variables
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(value))
    })
}

#[test]
fn one_rule_picks_live_plain_or_quiet() {
    let color = [("TERM", "xterm-256color")];
    assert_eq!(surface(OutputFormat::Human, true, &color), Surface::Live);
    assert_eq!(surface(OutputFormat::Human, false, &color), Surface::Plain);
    assert_eq!(surface(OutputFormat::Json, true, &color), Surface::Quiet);
    assert_eq!(surface(OutputFormat::Json, false, &color), Surface::Quiet);
    for (name, value) in [("CI", "true"), ("NO_COLOR", "1"), ("MM_PLAIN", "1")] {
        assert_eq!(
            surface(OutputFormat::Human, true, &[color[0], (name, value)]),
            Surface::Plain,
            "{name}"
        );
    }
    assert_eq!(
        surface(OutputFormat::Human, true, &[("TERM", "dumb")]),
        Surface::Plain
    );
    assert_eq!(
        surface(OutputFormat::Human, true, &[color[0], ("CI", "")]),
        Surface::Live,
        "an empty CI is unset"
    );
}

fn lines_of(lines: &[Line<'static>]) -> Vec<String> {
    lines.iter().map(plain).collect()
}

/// Draws `lines` in an inline viewport of a `width`-column test terminal and returns its rows.
fn drawn(lines: Vec<Line<'static>>, width: u16) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let height = u16::try_from(lines.len().max(1))?;
    let mut terminal = Terminal::with_options(
        TestBackend::new(width, height + 2),
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )?;
    terminal.draw(|frame| frame.render_widget(Paragraph::new(lines), frame.area()))?;
    let buffer = terminal.backend().buffer().clone();
    Ok((0..height)
        .map(|row| {
            (0..width)
                .map(|column| buffer[(column, row)].symbol().to_owned())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect())
}

fn setup_tasks(start: Instant) -> Tasks {
    let mut tasks = Tasks::new(Palette::color());
    for (key, label) in [
        ("prepare-nodes", "Preparing 2 nodes"),
        ("provision-nodes", "Provisioning 2 nodes"),
        ("connect-nodes", "Connecting the mesh"),
        ("push-secrets", "Pushing 68 secrets"),
        ("verify", "Verifying 2 nodes"),
    ] {
        tasks.plan(key, label);
    }
    let at = |seconds: u64| start + Duration::from_secs(seconds);
    tasks.apply(
        ProgressEvent::started("prepare-nodes", "Preparing 2 nodes"),
        at(0),
    );
    tasks.apply(
        ProgressEvent::finished(
            "prepare-nodes",
            StepOutcome::Done,
            Some("Prepared 2 nodes: names, node facts, mesh tools".to_owned()),
        ),
        at(3),
    );
    tasks.apply(
        ProgressEvent::started("provision-nodes", "Provisioning 2 nodes"),
        at(3),
    );
    tasks.apply(
        ProgressEvent::node(
            "provision-nodes",
            "app-1",
            NodeState::Working,
            Some("installing podman-compose and mmd"),
        ),
        at(3),
    );
    tasks.apply(
        ProgressEvent::node("provision-nodes", "app-1", NodeState::Done, None),
        at(40),
    );
    tasks.apply(
        ProgressEvent::node(
            "provision-nodes",
            "app-2",
            NodeState::Working,
            Some("installing podman-compose and mmd"),
        ),
        at(40),
    );
    tasks
}

#[test]
fn the_viewport_holds_the_running_step_its_nodes_and_what_is_to_come() -> TestResult {
    let start = Instant::now();
    let mut tasks = setup_tasks(start);
    let records = lines_of(&tasks.take_records(80));
    assert_eq!(records.len(), 1, "{records:?}");
    assert!(records[0].starts_with("✓ Prepared 2 nodes: names, node facts, mesh tools"));
    assert!(records[0].ends_with("3.0s"), "{records:?}");
    assert_eq!(
        records[0].chars().count(),
        80,
        "the time sits at the right edge"
    );

    let live = tasks.live(start + Duration::from_secs(52), 0, 80, 12);
    let rows = drawn(live, 80)?;
    save(
        "setup-apply-live-80",
        &tasks.live(start + Duration::from_secs(52), 0, 80, 12),
    );
    assert!(rows[0].starts_with("⠋ Provisioning 2 nodes"), "{rows:#?}");
    assert!(rows[0].ends_with("49s"), "{rows:#?}");
    assert!(
        rows[1].contains("app-1  ✓ installing podman-compose and mmd"),
        "{rows:#?}"
    );
    assert!(
        rows[1].ends_with("37s"),
        "a done node keeps the time it took: {rows:#?}"
    );
    assert!(rows[2].contains("app-2  ⠋ installing"), "{rows:#?}");
    assert!(rows[2].ends_with("12s"), "{rows:#?}");
    assert_eq!(
        rows[3..].to_vec(),
        [
            "○ Connecting the mesh",
            "○ Pushing 68 secrets",
            "○ Verifying 2 nodes"
        ]
    );
    let short = lines_of(&tasks.live(start + Duration::from_secs(52), 0, 80, 5));
    assert_eq!(short.len(), 5);
    assert_eq!(short[4], "  2 more steps", "{short:#?}");
    Ok(())
}

#[test]
fn narrow_terminals_cut_with_an_ellipsis_and_keep_the_time() -> TestResult {
    let start = Instant::now();
    let tasks = setup_tasks(start);
    let rows = drawn(tasks.live(start + Duration::from_secs(52), 0, 40, 12), 40)?;
    save(
        "setup-apply-live-40",
        &tasks.live(start + Duration::from_secs(52), 0, 40, 12),
    );
    for row in &rows {
        assert!(row.chars().count() <= 40, "{row}");
    }
    assert!(rows[1].contains('…'), "{rows:#?}");
    assert!(rows[0].ends_with("49s"), "{rows:#?}");
    Ok(())
}

#[test]
fn plain_lines_are_one_per_finished_step_with_its_time() {
    let start = Instant::now();
    let mut tasks = setup_tasks(start);
    tasks.apply(
        ProgressEvent::node(
            "provision-nodes",
            "app-2",
            NodeState::Failed,
            Some("the playbook failed"),
        ),
        start + Duration::from_secs(61),
    );
    tasks.apply(
        ProgressEvent::finished("provision-nodes", StepOutcome::Failed, None),
        start + Duration::from_secs(61),
    );
    assert_eq!(
        tasks.take_plain(),
        [
            "✓ Prepared 2 nodes: names, node facts, mesh tools (3.0s)",
            "✗ Provisioning 2 nodes (58s)",
            "    ✗ app-2: the playbook failed",
        ]
    );
    assert!(tasks.take_plain().is_empty(), "a record is written once");
}

#[test]
fn an_interrupted_step_is_recorded_as_such() {
    let start = Instant::now();
    let mut tasks = setup_tasks(start);
    let _ = tasks.take_plain();
    tasks.interrupt(start + Duration::from_secs(70));
    assert_eq!(
        tasks.take_plain(),
        ["✗ Provisioning 2 nodes (stopped) (1m07s)"]
    );
    assert!(!tasks.running());
}

#[test]
fn a_bar_shows_an_amount_its_rate_and_what_is_left() {
    let start = Instant::now();
    let mut tasks = Tasks::new(Palette::plain());
    tasks.apply(
        ProgressEvent::started("upload", "Sending the bundle"),
        start,
    );
    tasks.apply(ProgressEvent::bytes("upload", 0, Some(40_000_000)), start);
    tasks.apply(
        ProgressEvent::bytes("upload", 10_000_000, Some(40_000_000)),
        start + Duration::from_secs(2),
    );
    let lines = lines_of(&tasks.live(start + Duration::from_secs(2), 3, 80, 8));
    assert_eq!(lines.len(), 2, "{lines:#?}");
    assert!(
        lines[1].starts_with("    ━━━━━━──────────────────  9.5 MiB of 38.1 MiB · 4.7 MiB/s"),
        "{lines:#?}"
    );
    assert!(lines[1].contains("/s · 6.0s left"), "{lines:#?}");
}

fn plan() -> Result<ClusterSetupPlan, serde_json::Error> {
    serde_json::from_value(serde_json::json!({
        "project-id": "00000000-0000-0000-0000-000000000001",
        "project-name": "shop",
        "config-hash": "c".repeat(64),
        "environment": "integration",
        "target": "shop",
        "nodes": [
            {"name": "app-1", "host": "203.0.113.10", "port": 22,
             "private-address": "10.90.0.1", "host-key": "ssh-ed25519 AAAA",
             "host-key-fingerprint": "SHA256:Qx3fK9mZ0w1xYp7c8d9e0f1a2b3c4d5e6f7a8b9c0d1",
             "host-key-origin": "recorded"},
            {"name": "app-2", "host": "203.0.113.11", "port": 22,
             "private-address": "10.90.0.2", "host-key": "ssh-ed25519 BBBB",
             "host-key-fingerprint": "SHA256:RiPRx7zh4vPDa1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6",
             "host-key-origin": "first-use"}
        ],
        "cluster": {},
        "mesh": {
            "interface": "mm0", "subnet": "10.90.0.0/24", "port": 51820, "mtu": 1420,
            "nodes": [
                {"node": "app-1", "address": "10.90.0.1",
                 "endpoint": "203.0.113.10:51820", "public-key": "k1", "origin": "current",
                 "peers": [{"name": "app-2", "action": "add",
                            "endpoint": "203.0.113.11:51820", "allowed-ips": ["10.90.0.2/32"]}]},
                {"node": "app-2", "address": "10.90.0.2",
                 "endpoint": "203.0.113.11:51820", "origin": "new",
                 "peers": [{"name": "app-1", "action": "add", "public-key": "k1",
                            "endpoint": "203.0.113.10:51820", "allowed-ips": ["10.90.0.1/32"]}]}
            ]
        },
        "storage": "app-1",
        "runtime": "podman-compose",
        "setup": {"user": "root", "origin": "git@example.com:shop.git",
                  "deploy-key": {"secret": "DEPLOY_KEY", "public-key": "ssh-ed25519 CCCC"}},
        "input-files": [],
        "binaries": [{"architecture": "x86_64", "directory": "/opt/mm/bin",
                      "mm-digest": "a".repeat(64), "mmd-digest": "b".repeat(64),
                      "source": "the directory of this mm"}],
        "secrets": (0..68).map(|index| serde_json::json!({
            "name": format!("SECRET_{index}"), "reference": format!("age://SECRET_{index}"),
            "revision": format!("r{index}")
        })).collect::<Vec<_>>(),
        "unset-secrets": ["GHCR_TOKEN"],
        "notices": ["registry ghcr.io: its login secret GHCR_TOKEN has no value in integration; set it with `mm -e integration secret set GHCR_TOKEN` before the first deployment pulls from it"],
        "deploy-key": {"secret": "DEPLOY_KEY", "revision": "r1", "public-key": "ssh-ed25519 CCCC"},
        "access-commit": "4c1e9a2b7d10f00dbeefcafe0123456789abcdef",
        "steps": ["prepare-nodes", "provision-nodes", "connect-nodes", "serve-storage",
                  "push-secrets", "sync-access", "verify"],
        "plan-hash": "9f2c41d07a3e".to_owned() + &"e".repeat(52),
    }))
}

#[test]
fn the_setup_plan_card_leads_with_changes_and_keeps_hashes_short() -> TestResult {
    let plan = plan()?;
    let card = setup_plan_card(&plan);
    save("setup-plan-card-80", &card.lines(Palette::color(), 80));
    save("setup-plan-card-120", &card.lines(Palette::color(), 120));
    let lines = lines_of(&card.lines(Palette::color(), 80));
    let text = lines.join("\n");
    assert!(
        lines[0].starts_with("Cluster setup for integration"),
        "{text}"
    );
    assert!(lines[0].ends_with("plan 9f2c41d07a3e"), "{text}");
    assert!(
        text.contains("  target    shop · 2 nodes · podman-compose"),
        "{text}"
    );
    assert!(text.contains("SHA256:Qx3fK9mZ0w1x…"), "{text}");
    assert!(
        text.contains("+ app-2  joins the mesh and makes its key; adds peer app-1"),
        "{text}"
    );
    assert!(text.contains("~ app-1  adds peer app-2"), "{text}");
    assert!(!text.contains("00000000-0000"), "no project UUID: {text}");
    assert!(!text.contains(&"a".repeat(64)), "no 64-hex digest: {text}");
    let command = lines.last().ok_or("no command")?;
    assert!(command.contains(&plan.plan_hash), "{text}");
    assert!(command.ends_with("--yes"), "{text}");
    for line in &lines[..lines.len() - 1] {
        assert!(line.chars().count() <= 80, "{line}");
    }
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("  ▲ registry ghcr.io")),
        "{text}"
    );
    Ok(())
}

#[test]
fn nothing_to_change_folds_into_one_line() -> TestResult {
    let mut plan = plan()?;
    if let Some(mesh) = plan.mesh.as_mut() {
        for node in &mut mesh.nodes {
            node.origin = mm::domain::mesh::MeshNodeOrigin::Current;
            for peer in &mut node.peers {
                peer.action = mm::domain::mesh::PeerAction::Keep;
            }
        }
    }
    let text = lines_of(&setup_plan_card(&plan).lines(Palette::plain(), 80)).join("\n");
    assert!(text.contains("Changes\n  · 2 nodes unchanged"), "{text}");
    Ok(())
}

#[test]
fn the_setup_result_card_names_each_node_and_the_record_command() -> TestResult {
    let result: ClusterSetupResult = serde_json::from_value(serde_json::json!({
        "project-id": "00000000-0000-0000-0000-000000000001",
        "environment": "integration",
        "target": "shop",
        "plan-hash": "f".repeat(64),
        "nodes": [
            {"name": "app-1", "private-address": "10.90.0.1", "host-key": "ssh-ed25519 AAAA",
             "daemon-fingerprint": "SHA256:dd11ee22ff33aa44bb55cc66dd77ee88ff99", "version": "0.9.0"},
            {"name": "app-2", "private-address": "10.90.0.2", "host-key": "ssh-ed25519 BBBB",
             "daemon-fingerprint": "SHA256:0011223344556677889900aabbccddeeff", "version": "0.9.0"}
        ],
        "secrets-changed": 3,
        "access-commit": "4c1e9a2b7d10f00dbeefcafe0123456789abcdef",
        "record-command": "mm --environment integration target setup record"
    }))?;
    let card = setup_result_card(&result, Duration::from_secs(134));
    save("setup-result-card-80", &card.lines(Palette::color(), 80));
    let text = lines_of(&card.lines(Palette::color(), 80)).join("\n");
    assert!(text.starts_with("Set up shop for integration"), "{text}");
    assert!(
        text.lines()
            .next()
            .is_some_and(|line| line.ends_with("2m14s")),
        "{text}"
    );
    assert!(
        text.contains("app-2  10.90.0.2  0.9.0  SHA256:001122334455…"),
        "{text}"
    );
    assert!(
        text.ends_with("  mm --environment integration target setup record"),
        "{text}"
    );
    Ok(())
}

#[test]
fn a_table_gives_way_from_its_widest_column() {
    let mut card = Card::new("Nodes");
    card.table(
        &["node", "detail"],
        vec![vec![
            Value::new("app-1"),
            Value::new("a very long explanation that does not fit into forty columns at all"),
        ]],
    );
    card.changes(
        vec![(Mark::Remove, "app-2".to_owned(), "leaves".to_owned())],
        None,
    );
    let lines = lines_of(&card.lines(Palette::plain(), 40));
    for line in &lines {
        assert!(line.chars().count() <= 40, "{line}");
    }
    assert!(lines[3].ends_with('…'), "{lines:#?}");
    assert_eq!(lines.last().map(String::as_str), Some("  - app-2  leaves"));
}

/// Writes `lines` styled to `target/tmp/inline-ui/<name>.ansi` for a look.
fn save(name: &str, lines: &[Line<'static>]) {
    let directory = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("inline-ui");
    let mut bytes = Vec::new();
    if std::fs::create_dir_all(&directory).is_ok() && write_lines(&mut bytes, lines, true).is_ok() {
        let _ = std::fs::write(directory.join(format!("{name}.ansi")), bytes);
    }
}
