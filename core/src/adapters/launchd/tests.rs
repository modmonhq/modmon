use tempfile::TempDir;
use uuid::Uuid;

use super::testing::FakeLaunchd;
use super::*;

/// Canonical so symlinked temp roots (macOS `/var`) pass the path policy.
fn root() -> Result<(TempDir, PathBuf), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let path = dunce::canonicalize(temp.path())?;
    Ok((temp, path))
}

fn installation(root: &Path, plist: &str, config: &str) -> AgentInstallation {
    AgentInstallation {
        label: "local.mm.test".to_owned(),
        target_id: TargetId(Uuid::from_u128(5)),
        artifact_hash: "a".repeat(64),
        owner_path: root.join("state/local.mm.test.owner.json"),
        plist: AgentFile {
            destination: root.join("LaunchAgents/local.mm.test.plist"),
            bytes: plist.as_bytes().to_vec(),
            mode: 0o644,
        },
        files: vec![AgentFile {
            destination: root.join("state/config.cfg"),
            bytes: config.as_bytes().to_vec(),
            mode: 0o644,
        }],
    }
}

#[test]
fn print_output_maps_to_running_exited_and_never_exited() {
    assert_eq!(
        parse_print("x = {\n\tstate = running\n\tpid = 9\n}"),
        LaunchdJobState::Running
    );
    let failed = parse_print("x = {\n\tstate = not running\n\tlast exit code = 1\n}");
    assert_eq!(failed, LaunchdJobState::Exited { code: Some(1) });
    assert!(failed.failed() && failed.loaded());
    let fresh = parse_print("x = {\n\tstate = not running\n\tlast exit code = (never exited)\n}");
    assert_eq!(fresh, LaunchdJobState::Exited { code: None });
    assert!(!fresh.failed());
    assert!(!LaunchdJobState::Unloaded.loaded());
}

#[tokio::test]
async fn bootout_of_an_unloaded_job_succeeds_and_other_errors_surface()
-> Result<(), Box<dyn std::error::Error>> {
    let fake = Arc::new(FakeLaunchd::default());
    let launchctl = Launchctl::current_user(fake.clone());
    launchctl.bootout("local.mm.absent").await?;
    assert_eq!(
        launchctl.state("local.mm.absent").await?,
        LaunchdJobState::Unloaded
    );
    assert!(
        launchctl
            .bootstrap(Path::new("/nonexistent/local.mm.absent.plist"))
            .await
            .is_err()
    );
    let calls = fake.calls.lock().map_err(|_| "poisoned")?;
    assert!(calls[0][1].starts_with("gui/") && calls[0][1].ends_with("/local.mm.absent"));
    Ok(())
}

#[tokio::test]
async fn install_is_idempotent_and_reloads_only_on_change() -> Result<(), Box<dyn std::error::Error>>
{
    let (_temp, root) = root()?;
    let fake = Arc::new(FakeLaunchd::default());
    let launchctl = Launchctl::current_user(fake.clone());

    let first = install_agent(&launchctl, &installation(&root, "plist-1", "config-1")).await?;
    assert!(first.changed());
    assert_eq!(
        fs::read_to_string(root.join("state/config.cfg"))?,
        "config-1"
    );
    assert_eq!(fake.verbs(), ["print", "bootstrap"]);

    let unchanged = install_agent(&launchctl, &installation(&root, "plist-1", "config-1")).await?;
    assert!(!unchanged.changed());
    assert_eq!(fake.verbs(), ["print", "bootstrap", "print"]);

    install_agent(&launchctl, &installation(&root, "plist-1", "config-2")).await?;
    assert_eq!(
        fake.verbs(),
        [
            "print",
            "bootstrap",
            "print",
            "print",
            "bootout",
            "bootstrap"
        ]
    );
    assert!(
        fs::read_to_string(root.join("state/local.mm.test.owner.json"))?
            .contains("00000000-0000-0000-0000-000000000005")
    );
    Ok(())
}

#[tokio::test]
async fn failed_jobs_with_current_files_are_restarted_in_place()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, root) = root()?;
    let fake = Arc::new(FakeLaunchd {
        print_stdout: Some("x = {\n\tstate = not running\n\tlast exit code = 78\n}\n"),
        ..FakeLaunchd::default()
    });
    let launchctl = Launchctl::current_user(fake.clone());
    install_agent(&launchctl, &installation(&root, "plist", "config")).await?;
    install_agent(&launchctl, &installation(&root, "plist", "config")).await?;
    assert_eq!(fake.verbs(), ["print", "bootstrap", "print", "kickstart"]);
    Ok(())
}

#[tokio::test]
async fn unmanaged_and_foreign_agents_are_refused() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, root) = root()?;
    let launchctl = Launchctl::current_user(Arc::new(FakeLaunchd::default()));
    fs::create_dir_all(root.join("LaunchAgents"))?;
    fs::write(
        root.join("LaunchAgents/local.mm.test.plist"),
        "someone else's",
    )?;
    assert!(
        install_agent(&launchctl, &installation(&root, "plist", "config"))
            .await
            .is_err()
    );
    assert!(
        uninstall_agent(&launchctl, &installation(&root, "plist", "config"))
            .await
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(root.join("LaunchAgents/local.mm.test.plist"))?,
        "someone else's"
    );

    fs::remove_file(root.join("LaunchAgents/local.mm.test.plist"))?;
    install_agent(&launchctl, &installation(&root, "plist", "config")).await?;
    let mut foreign = installation(&root, "plist", "config");
    foreign.target_id = TargetId(Uuid::from_u128(6));
    assert!(install_agent(&launchctl, &foreign).await.is_err());
    assert!(uninstall_agent(&launchctl, &foreign).await.is_err());
    Ok(())
}

#[tokio::test]
async fn failed_load_restores_previous_files_and_job() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, root) = root()?;
    let working = Arc::new(FakeLaunchd::default());
    install_agent(
        &Launchctl::current_user(working.clone()),
        &installation(&root, "plist-1", "config-1"),
    )
    .await?;

    let failing = Arc::new(FakeLaunchd {
        fail_bootstrap: true,
        ..FakeLaunchd::default()
    });
    let result = install_agent(
        &Launchctl::current_user(failing.clone()),
        &installation(&root, "plist-2", "config-2"),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(root.join("LaunchAgents/local.mm.test.plist"))?,
        "plist-1"
    );
    assert_eq!(
        fs::read_to_string(root.join("state/config.cfg"))?,
        "config-1"
    );
    Ok(())
}

#[tokio::test]
async fn uninstall_unloads_and_removes_only_owned_files() -> Result<(), Box<dyn std::error::Error>>
{
    let (_temp, root) = root()?;
    let fake = Arc::new(FakeLaunchd::default());
    let launchctl = Launchctl::current_user(fake.clone());
    uninstall_agent(&launchctl, &installation(&root, "plist", "config")).await?;
    install_agent(&launchctl, &installation(&root, "plist", "config")).await?;
    fs::write(root.join("state/unrelated"), "keep")?;
    uninstall_agent(&launchctl, &installation(&root, "plist", "config")).await?;
    assert!(!root.join("LaunchAgents/local.mm.test.plist").exists());
    assert!(!root.join("state/config.cfg").exists());
    assert!(!root.join("state/local.mm.test.owner.json").exists());
    assert!(root.join("state/unrelated").exists());
    assert_eq!(
        launchctl.state("local.mm.test").await?,
        LaunchdJobState::Unloaded
    );
    Ok(())
}
