//! The command log: records kept per project with secrets redacted, the
//! oldest dropped past the limits, concurrent writers, the reads people and
//! agents use, and a command whose record cannot be kept still succeeding.

mod temp_dirs;

use std::sync::Arc;

use mm::adapters::command_log::FileCommandLog;
use mm::app::CommandHistory;
use mm::domain::command_log::{
    CommandFilter, CommandOutcome, CommandPrincipal, CommandRecord, REDACTED, redact_arguments,
};
use mm::domain::progress::{ProgressEvent, StepOutcome};
use mm::domain::workspace::ProjectId;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const KIND: &str = "OPENSSH";

fn record(id: &str, environment: &str, outcome: CommandOutcome, output: &str) -> CommandRecord {
    CommandRecord {
        id: id.to_owned(),
        arguments: vec!["deploy".to_owned(), "plan".to_owned()],
        environment: Some(environment.to_owned()),
        target: None,
        runtime: None,
        principal: CommandPrincipal::User {
            name: "dev".to_owned(),
        },
        started_at_unix_ms: 1_000,
        finished_at_unix_ms: 3_500,
        outcome,
        exit_code: Some(u8::from(outcome != CommandOutcome::Succeeded)),
        error_code: None,
        output: output.to_owned(),
        output_truncated: false,
        events: vec![mm::domain::command_log::RecordedEvent {
            at_ms: 10,
            event: ProgressEvent::finished("plan", StepOutcome::Done, None),
        }],
        result: Some(serde_json::json!({"plan_hash": "abc", "deploy-key": {"private_key": "x"}})),
    }
}

/// A project made from the minimal fixture, and an empty mm home.
fn project_and_home() -> Result<(tempfile::TempDir, tempfile::TempDir), Box<dyn std::error::Error>>
{
    let project = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal");
    for file in [".config/mm.yaml", "compose.yaml", "api/.env.example"] {
        let destination = project.path().join(file);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source.join(file), destination)?;
    }
    Ok((project, home))
}

fn history(root: &std::path::Path) -> CommandHistory {
    CommandHistory::new(
        Arc::new(FileCommandLog::new(root.join("history"))),
        ProjectId(uuid::Uuid::from_u128(7)),
    )
}

#[test]
fn records_are_listed_newest_first_filtered_and_read_by_a_prefix() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let history = history(home.path());
    history.record(record(
        "20261115T100000000Z-aaaa",
        "staging",
        CommandOutcome::Succeeded,
        "one\n",
    ))?;
    history.record(record(
        "20261115T110000000Z-bbbb",
        "production",
        CommandOutcome::Failed,
        "two\n",
    ))?;
    history.record(record(
        "20261115T120000000Z-cccc",
        "production",
        CommandOutcome::Succeeded,
        "three\n",
    ))?;

    let (all, next) = history.list(&CommandFilter::default(), None, 2)?;
    assert_eq!(
        all.iter()
            .map(|command| command.id.as_str())
            .collect::<Vec<_>>(),
        ["20261115T120000000Z-cccc", "20261115T110000000Z-bbbb"]
    );
    assert_eq!(next.as_deref(), Some("20261115T110000000Z-bbbb"));
    let (rest, last) = history.list(&CommandFilter::default(), next.as_deref(), 2)?;
    assert_eq!(rest.len(), 1);
    assert_eq!(last, None);

    let failed = CommandFilter {
        outcome: Some(CommandOutcome::Failed),
        ..CommandFilter::default()
    };
    let (found, _) = history.list(&failed, None, 10)?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].duration_ms, 2_500);
    let staging = CommandFilter {
        environment: Some("staging".to_owned()),
        ..CommandFilter::default()
    };
    assert_eq!(history.list(&staging, None, 10)?.0.len(), 1);

    let one = history.get("20261115T12")?;
    assert_eq!(one.output, "three\n");
    assert_eq!(one.events.len(), 1);
    assert_eq!(
        one.result
            .as_ref()
            .and_then(|result| result["deploy-key"]["private_key"].as_str()),
        Some(REDACTED),
        "a secret-bearing key of the result is scrubbed"
    );
    let ambiguous = history
        .get("20261115T1")
        .err()
        .ok_or("an ambiguous prefix")?;
    assert!(
        ambiguous.to_string().contains("starts 3 command IDs"),
        "{ambiguous}"
    );
    assert!(history.get("nothing").is_err());
    assert!(history.get("../../etc").is_err());
    Ok(())
}

#[test]
fn secrets_never_reach_a_record() -> TestResult {
    let arguments = [
        "secret",
        "set",
        "API_TOKEN=hunter2",
        "--password",
        "hunter3",
        "--token=hunter4",
        "--stdin",
        "--plan",
        "abc",
    ]
    .map(str::to_owned);
    let redacted = redact_arguments(&arguments);
    assert_eq!(
        redacted,
        [
            "secret",
            "set",
            "API_TOKEN=[REDACTED]",
            "--password",
            "[REDACTED]",
            "--token=[REDACTED]",
            "--stdin",
            "--plan",
            "abc"
        ]
    );
    let home = temp_dirs::tempdir()?;
    let history = history(home.path());
    history.record(record(
        "20261115T100000000Z-aaaa",
        "local",
        CommandOutcome::Succeeded,
        &format!("db_password=hunter5 kept\n-----BEGIN {KIND} PRIVATE KEY-----\nAAAA\n-----END {KIND} PRIVATE KEY-----\nafter\n"),
    ))?;
    let kept = std::fs::read_to_string(
        home.path()
            .join("history")
            .join(uuid::Uuid::from_u128(7).to_string())
            .join("20261115T100000000Z-aaaa.json"),
    )?;
    for secret in ["hunter5", "AAAA", "\"x\""] {
        assert!(!kept.contains(secret), "{secret} in {kept}");
    }
    let output = history.get("20261115T100000000Z-aaaa")?.output;
    assert_eq!(output, "db_password=[REDACTED] kept\n[REDACTED]\nafter\n");
    Ok(())
}

#[test]
fn the_oldest_records_go_past_the_count_and_the_size() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let log = FileCommandLog::new(home.path().join("history")).keeping(3, 1_000_000);
    let history = CommandHistory::new(Arc::new(log), ProjectId(uuid::Uuid::from_u128(7)));
    for minute in 0..5 {
        history.record(record(
            &format!("20261115T10{minute:02}00000Z-aaaa"),
            "local",
            CommandOutcome::Succeeded,
            "x\n",
        ))?;
    }
    let (kept, _) = history.list(&CommandFilter::default(), None, 10)?;
    assert_eq!(
        kept.iter()
            .map(|command| command.id.as_str())
            .collect::<Vec<_>>(),
        [
            "20261115T100400000Z-aaaa",
            "20261115T100300000Z-aaaa",
            "20261115T100200000Z-aaaa"
        ]
    );
    let small = FileCommandLog::new(home.path().join("small")).keeping(100, 2_500);
    let history = CommandHistory::new(Arc::new(small), ProjectId(uuid::Uuid::from_u128(7)));
    for minute in 0..5 {
        history.record(record(
            &format!("20261115T10{minute:02}00000Z-aaaa"),
            "local",
            CommandOutcome::Succeeded,
            &"y".repeat(600),
        ))?;
    }
    let (kept, _) = history.list(&CommandFilter::default(), None, 10)?;
    assert!(kept.len() < 5 && !kept.is_empty(), "{}", kept.len());
    assert_eq!(kept[0].id, "20261115T100400000Z-aaaa");
    Ok(())
}

#[test]
fn concurrent_writers_keep_every_record() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let root = home.path().to_path_buf();
    let writers = (0..8)
        .map(|writer| {
            let root = root.clone();
            std::thread::spawn(move || {
                let history = history(&root);
                for index in 0..10 {
                    history.record(record(
                        &format!("20261115T1000{writer:02}{index:03}Z-{writer:04}"),
                        "local",
                        CommandOutcome::Succeeded,
                        "z\n",
                    ))?;
                }
                Ok::<(), mm::MmError>(())
            })
        })
        .collect::<Vec<_>>();
    for writer in writers {
        writer.join().map_err(|_| "a writer panicked")??;
    }
    let (all, _) = history(&root).list(&CommandFilter::default(), None, 100)?;
    assert_eq!(all.len(), 80);
    Ok(())
}

#[test]
fn every_command_in_a_project_is_recorded_and_readable() -> TestResult {
    let (project, home) = project_and_home()?;
    std::fs::create_dir(home.path().join("history"))?;
    let run = |arguments: &[&str]| {
        temp_dirs::mm()
            .arg("--root")
            .arg(project.path())
            .arg("--mm-home")
            .arg(home.path())
            .args(arguments)
            .output()
    };
    assert!(run(&["config", "validate"])?.status.success());
    let failed = run(&["--output", "json", "deploy", "apply", "--plan", "nope"])?;
    assert!(!failed.status.success());
    let listed = run(&["--output", "json", "history"])?;
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&listed.stdout)?;
    let commands = value["commands"].as_array().ok_or("commands")?;
    assert_eq!(commands.len(), 2, "{value}");
    assert_eq!(commands[0]["outcome"], "failed");
    assert!(
        commands[1]["arguments"]
            .as_array()
            .is_some_and(|arguments| arguments.iter().any(|argument| argument == "validate")),
        "{value}"
    );
    let id = commands[1]["id"].as_str().ok_or("id")?;
    let shown = run(&["history", "show", id])?;
    let text = String::from_utf8(shown.stdout)?;
    assert!(text.starts_with("mm --root"), "{text}");
    assert!(!text.contains('\u{1b}'), "no escape codes: {text}");
    Ok(())
}

#[test]
fn a_command_whose_record_cannot_be_kept_still_succeeds() -> TestResult {
    let (project, home) = project_and_home()?;
    std::fs::write(home.path().join("history"), "not a directory")?;
    let output = temp_dirs::mm()
        .arg("--root")
        .arg(project.path())
        .arg("--mm-home")
        .arg(home.path())
        .args(["config", "validate"])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
