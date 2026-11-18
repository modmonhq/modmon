use std::fs;
use std::path::Path;
use std::sync::Arc;

use assert_cmd::Command;
use mm::adapters::config::ConfigLoader;
use mm::adapters::runtime::FakeRuntime;
use mm::adapters::state::{SqliteStateInspector, SqliteStateRecovery, SqliteStateStore};
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{ReadinessProbe, StateRecoveryStore, TargetStore};
use mm::app::{ReadinessService, TargetManager, WorkspaceSelection};
use mm::domain::diagnostic::Diagnostic;
use mm::domain::error::MmError;
use mm::domain::readiness::{CheckStatus, SetupIssue};
use mm::domain::runtime::{RuntimeKind, RuntimeProbe};
use mm::domain::target::{TargetDefinition, TargetId};
use tempfile::TempDir;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct MissingNetwork(std::sync::atomic::AtomicUsize);

#[async_trait::async_trait]
impl mm::app::ports::ProcessRunner for MissingNetwork {
    async fn run(
        &self,
        specification: mm::app::ports::CommandSpec,
    ) -> Result<mm::app::ports::CommandOutput, MmError> {
        let record = specification.redacted_record();
        assert_eq!(&record.arguments[..2], &["network", "inspect"]);
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(mm::app::ports::CommandOutput {
            code: Some(1),
            stdout: String::new().into(),
            stderr: "Error: No such network".to_owned().into(),
            truncated: false,
        })
    }
}

struct Fixture {
    root: TempDir,
    home: TempDir,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let root = temp_dirs::tempdir()?;
        let home = temp_dirs::tempdir()?;
        fs::create_dir(root.path().join(".config"))?;
        fs::create_dir(root.path().join("api"))?;
        fs::write(
            root.path().join("compose.yaml"),
            "services: {api: {expose: [8080]}}\n",
        )?;
        fs::write(
            root.path().join(".config/mm.yaml"),
            "schema-version: 1\nproject:\n  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3\n  name: readiness\ndefault-environment: development\ntarget: local\nstacks:\n  app:\n    files: [compose.yaml]\nmodules:\n  api:\n    path: api\n    build:\n      enabled: true\n      image: example/api\n    services:\n      api:\n        stack: app\nenvironments:\n  development: {}\n",
        )?;
        Ok(Self { root, home })
    }

    fn service(&self, available: bool) -> ReadinessService {
        let mut probe = RuntimeProbe::unavailable(
            RuntimeKind::DockerCompose,
            Diagnostic::info("TEST", "test runtime"),
        );
        probe.available = available;
        ReadinessService::new(
            Arc::new(ConfigLoader),
            Arc::new(SqliteStateInspector {
                path: self.home.path().join("state.sqlite3"),
            }),
            Arc::new(FilesystemTargetStore::new(
                self.home.path().join("targets.yaml"),
            )),
            vec![Arc::new(FakeRuntime {
                probe,
                user_rendered: None,
                rendered: Default::default(),
                observed: Default::default(),
            })],
        )
    }

    fn command(&self) -> Command {
        let mut command = temp_dirs::mm();
        command
            .arg("--root")
            .arg(self.root.path())
            .arg("--mm-home")
            .arg(self.home.path());
        command
    }

    fn local(&self) -> Result<(), MmError> {
        let store = Arc::new(FilesystemTargetStore::new(
            self.home.path().join("targets.yaml"),
        ));
        TargetManager::new(Arc::new(ConfigLoader), store).ensure_local()?;
        Ok(())
    }
}

fn legacy(path: &Path) -> Result<rusqlite::Connection, rusqlite::Error> {
    let connection = rusqlite::Connection::open(path)?;
    connection.execute_batch(
        "CREATE TABLE workspaces (name TEXT); INSERT INTO workspaces VALUES ('preserve me');",
    )?;
    Ok(connection)
}

#[tokio::test]
async fn collects_independent_failures_and_marks_unknown_checks_blocked() -> TestResult {
    let fixture = Fixture::new()?;
    let database = fixture.home.path().join("state.sqlite3");
    drop(legacy(&database)?);
    let before = fs::read(&database)?;
    let report = fixture
        .service(true)
        .inspect(&WorkspaceSelection::new(fixture.root.path(), None))
        .await;
    let readiness = report.readiness.ok_or("missing readiness")?;
    assert!(
        readiness
            .checks
            .iter()
            .any(|check| matches!(check.issue, Some(SetupIssue::LegacyState { .. })))
    );
    assert!(
        readiness
            .checks
            .iter()
            .any(|check| matches!(check.issue, Some(SetupIssue::MissingTarget { .. })))
    );
    assert!(
        readiness
            .checks
            .iter()
            .any(|check| check.name == "images" && check.status == CheckStatus::Blocked)
    );
    assert!(readiness.candidates.is_empty());
    assert_eq!(before, fs::read(&database)?);
    assert!(!fixture.home.path().join("targets.yaml").exists());
    assert!(!fixture.home.path().join("compose").exists());
    Ok(())
}

#[tokio::test]
async fn missing_runtime_and_images_are_reported_together_without_saving_a_choice() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.local()?;
    for available in [true, false] {
        let report = fixture
            .service(available)
            .inspect(&WorkspaceSelection::new(fixture.root.path(), None))
            .await;
        let readiness = report.readiness.ok_or("missing readiness")?;
        assert!(
            readiness
                .checks
                .iter()
                .any(|check| matches!(check.issue, Some(SetupIssue::MissingRuntime)))
        );
        assert!(
            readiness
                .checks
                .iter()
                .any(|check| matches!(check.issue, Some(SetupIssue::MissingImages { .. })))
        );
        assert_eq!(readiness.candidates.len(), 1);
        assert_eq!(readiness.candidates[0].probe.available, available);
        assert!(!fixture.home.path().join("state.sqlite3").exists());
    }
    Ok(())
}

struct ExtraProbe;

#[async_trait::async_trait]
impl ReadinessProbe for ExtraProbe {
    fn name(&self) -> &'static str {
        "extension"
    }
    async fn inspect(&self, _: &WorkspaceSelection) -> Result<Vec<Diagnostic>, MmError> {
        Ok(vec![Diagnostic::error(
            "TEST_EXTENSION",
            "extension failure",
        )])
    }
}

#[tokio::test]
async fn extensions_run_even_when_core_checks_fail() -> TestResult {
    let fixture = Fixture::new()?;
    let report = fixture
        .service(true)
        .with_probe(Arc::new(ExtraProbe))
        .inspect(&WorkspaceSelection::new(fixture.root.path(), None))
        .await;
    assert!(
        report
            .validation
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TEST_EXTENSION")
    );
    assert!(
        report
            .readiness
            .ok_or("missing readiness")?
            .checks
            .iter()
            .any(|check| check.name == "extension" && check.status == CheckStatus::Failed)
    );
    Ok(())
}

#[test]
fn recovery_backs_up_committed_wal_and_replaces_only_legacy_state() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let path = directory.path().join("state.sqlite3");
    let connection = legacy(&path)?;
    connection
        .execute_batch("PRAGMA journal_mode=WAL; INSERT INTO workspaces VALUES ('in WAL');")?;
    let recovery = SqliteStateRecovery { path: path.clone() };
    let plan = recovery.plan()?;
    let backup = recovery.recover(&plan.plan_hash)?;
    let archived = rusqlite::Connection::open(backup)?;
    let rows: i64 = archived.query_row("SELECT count(*) FROM workspaces", [], |row| row.get(0))?;
    assert_eq!(rows, 2);
    assert!(SqliteStateStore::inspect(&path)?.is_some());
    assert!(recovery.plan().is_err());
    Ok(())
}

#[test]
fn stale_and_unknown_state_cannot_be_reset() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let path = directory.path().join("state.sqlite3");
    let connection = legacy(&path)?;
    let recovery = SqliteStateRecovery { path: path.clone() };
    let plan = recovery.plan()?;
    connection.execute("INSERT INTO workspaces VALUES ('new data')", [])?;
    assert!(recovery.recover(&plan.plan_hash).is_err());
    let count: i64 =
        connection.query_row("SELECT count(*) FROM workspaces", [], |row| row.get(0))?;
    assert_eq!(count, 2);
    connection.execute_batch("CREATE TABLE schema_migrations (version TEXT); INSERT INTO schema_migrations VALUES ('9999_future');")?;
    assert!(recovery.plan().is_err());
    Ok(())
}

#[test]
fn doctor_is_read_only_and_local_target_fix_is_idempotent() -> TestResult {
    let fixture = Fixture::new()?;
    let output = fixture
        .command()
        .env("PATH", fixture.home.path())
        .args(["doctor", "--output", "json"])
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert!(
        report["readiness"]["checks"]
            .as_array()
            .ok_or("checks")?
            .iter()
            .any(|check| check["issue"]["kind"] == "missing_target")
    );
    assert_eq!(fs::read_dir(fixture.home.path())?.count(), 0);
    fixture
        .command()
        .args(["target", "local"])
        .assert()
        .success();
    let before = fs::read(fixture.home.path().join("targets.yaml"))?;
    fixture
        .command()
        .args(["target", "local"])
        .assert()
        .success();
    assert_eq!(before, fs::read(fixture.home.path().join("targets.yaml"))?);
    fixture
        .command()
        .args(["target", "preference", "set-runtime", "docker-compose"])
        .assert()
        .success();
    Ok(())
}

#[test]
fn doctor_ends_with_each_step_once_in_plain_words() -> TestResult {
    let fixture = Fixture::new()?;
    let output = fixture
        .command()
        .env("PATH", fixture.home.path())
        .arg("doctor")
        .output()?;
    let report = String::from_utf8(output.stdout)?;
    assert_eq!(output.status.code(), Some(2), "{report}");
    for marker in [
        "  checks:      ",
        " passed, ",
        " failed",
        "development needs ",
        " before it can start:",
        "  1. Register this machine\n     mm has no record of this machine",
        "Run `mm` in a terminal to do these one at a time.",
    ] {
        assert!(report.contains(marker), "missing {marker:?} in:\n{report}");
    }
    let run = report
        .lines()
        .find(|line| line.starts_with("     run: "))
        .ok_or("no command for the first step")?;
    assert!(run.ends_with(" target local"), "{run}");
    for repeated in [
        "[Passed]",
        "[Failed]",
        "MM_TARGET_MISSING",
        "is not configured in",
    ] {
        assert!(
            !report.contains(repeated),
            "{repeated:?} repeats a step in:\n{report}"
        );
    }
    // The steps are the last thing printed, where the prompt comes back.
    let steps = report.find("development needs ").ok_or("steps")?;
    assert!(report.find("  checks:").ok_or("checks")? < steps);
    Ok(())
}

#[test]
fn local_registration_does_not_replace_an_existing_target() -> TestResult {
    let fixture = Fixture::new()?;
    let store = FilesystemTargetStore::new(fixture.home.path().join("targets.yaml"));
    let definition = TargetDefinition::local(TargetId(uuid::Uuid::new_v4()));
    let existing = store.insert_target("other", definition)?;
    fixture.local()?;
    assert_eq!(store.resolve_target("other")?, existing);
    Ok(())
}

#[tokio::test]
async fn shared_services_check_matches_mail_injection_and_honors_opt_out() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.local()?;
    let config = fixture.root.path().join(".config/mm.yaml");
    let source = fs::read_to_string(&config)?.replace(
        "  name: readiness",
        "  name: readiness\n  development-environment: development",
    );
    fs::write(&config, &source)?;
    let runner = Arc::new(MissingNetwork(std::sync::atomic::AtomicUsize::new(0)));
    let factory = mm::adapters::runtime::RuntimeFactory::new(runner.clone());
    let check = mm::app::DevelopmentServicesCheck::new(
        Arc::new(ConfigLoader),
        Arc::new(SqliteStateInspector {
            path: fixture.home.path().join("state.sqlite3"),
        }),
        Arc::new(FilesystemTargetStore::new(
            fixture.home.path().join("targets.yaml"),
        )),
        vec![(
            RuntimeKind::DockerCompose,
            factory.deployment(RuntimeKind::DockerCompose)?,
        )],
    );
    let mut selection = WorkspaceSelection::new(fixture.root.path(), None);
    selection.runtime = Some(RuntimeKind::DockerCompose);
    assert!(matches!(
        check.inspect(&selection).await,
        Err(MmError::Setup {
            issue: SetupIssue::MissingDevelopmentServices { .. }
        })
    ));
    assert_eq!(runner.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    fs::write(
        config,
        format!("{source}\ndevelopment-mail:\n  enabled: false\n"),
    )?;
    assert!(check.inspect(&selection).await?.is_empty());
    assert_eq!(runner.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn recovery_commands_preserve_scope_and_parse() -> TestResult {
    use clap::Parser;
    use mm::presentation::cli::Cli;
    let cli = Cli::try_parse_from([
        "mm",
        "--root",
        "/work/project with spaces",
        "--mm-home",
        "/private/mm",
        "--environment",
        "development",
        "--target",
        "local",
        "--runtime",
        "docker-compose",
        "doctor",
    ])?;
    let context = cli.recovery_context();
    for issue in [
        SetupIssue::LegacyState {
            path: "/private/mm/state.sqlite3".into(),
        },
        SetupIssue::MissingTarget {
            name: "local".to_owned(),
            path: "/private/mm/targets.yaml".into(),
        },
        SetupIssue::MissingRuntime,
        SetupIssue::LocalTlsRequired,
        SetupIssue::MissingImages {
            services: vec!["api/api".to_owned()],
        },
        SetupIssue::MissingDevelopmentServices {
            network: "example".to_owned(),
        },
    ] {
        let parsed = Cli::try_parse_from(context.command(&issue))?;
        assert_eq!(parsed.root, cli.root);
        assert_eq!(parsed.mm_home, cli.mm_home);
        assert_eq!(parsed.environment, cli.environment);
        assert_eq!(parsed.target, cli.target);
        assert_eq!(parsed.runtime, cli.runtime);
        assert!(
            context
                .display_command(&issue)
                .contains("'/work/project with spaces'")
        );
    }
    Ok(())
}

#[test]
fn automatic_project_domains_and_missing_tls_need_no_domain_entries() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.local()?;
    let config = fixture.root.path().join(".config/mm.yaml");
    let source = fs::read_to_string(&config)?
        .replace(
            "  name: readiness",
            "  name: store\n  development-environment: development",
        )
        .replace("enabled: true", "enabled: false");
    fs::write(&config, &source)?;
    let output = fixture
        .command()
        .env("PATH", fixture.home.path())
        .args(["--runtime", "docker-compose", "doctor", "--output", "json"])
        .output()?;
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert!(
        report["readiness"]["checks"]
            .as_array()
            .ok_or("checks")?
            .iter()
            .any(|check| check["issue"]["kind"] == "local_tls_required"),
        "{report}"
    );
    assert!(
        report["ingress"]
            .as_array()
            .ok_or("ingress")?
            .iter()
            .any(|route| route["url"] == "https://api.store.test"),
        "{report}"
    );
    assert!(!fixture.home.path().join("ca").exists());
    fs::write(
        config,
        format!(
            "{source}\ningress:\n  automatic-local: false\ndevelopment-mail:\n  enabled: false\n"
        ),
    )?;
    let output = fixture
        .command()
        .env("PATH", fixture.home.path())
        .args(["--runtime", "docker-compose", "doctor", "--output", "json"])
        .output()?;
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert!(
        !report["readiness"]["checks"]
            .as_array()
            .ok_or("checks")?
            .iter()
            .any(|check| check["issue"]["kind"] == "local_tls_required")
    );
    Ok(())
}
