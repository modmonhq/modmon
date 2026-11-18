use std::fs;

use tempfile::TempDir;
use uuid::Uuid;

use super::*;
use crate::adapters::launchd::testing::FakeLaunchd;
use crate::adapters::target::{FilesystemTargetIngressArtifactStore, FilesystemTargetStore};
use crate::app::TargetIngressManager;
use crate::app::ports::TargetStore as _;
use crate::domain::target::{TargetDefinition, TargetId};
use crate::domain::target_ingress::{IngressHostProfile, MacosHostFacts};

struct Fixture {
    _temp: TempDir,
    home: PathBuf,
    plan: TargetIngressPlan,
}

fn fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    // Canonical so the macOS `/var` symlink does not trip the path policy.
    let home = dunce::canonicalize(temp.path())?.join("Dev Home");
    fs::create_dir(&home)?;
    fs::create_dir(home.join(".mm"))?;
    let target_id = TargetId(Uuid::from_u128(0x5eed));
    let targets = Arc::new(FilesystemTargetStore::new(home.join(".mm/targets.yaml")));
    targets.insert_target("local", TargetDefinition::local(target_id))?;
    let home_text = home.to_str().ok_or("home is not UTF-8")?.to_owned();
    let plan = TargetIngressManager::new(
        targets,
        Arc::new(FilesystemTargetIngressArtifactStore::new(
            home.join(".mm/ingress"),
        )),
    )
    .with_local_host_profile(IngressHostProfile::MacosLaunchd(MacosHostFacts {
        haproxy_executable: "/opt/homebrew/opt/haproxy/bin/haproxy".to_owned(),
        docker_executable: "/usr/local/bin/docker".to_owned(),
        docker_host: format!("unix://{home_text}/.docker/run/docker.sock"),
        home: home_text,
    }))
    .plan("local")?;
    Ok(Fixture {
        _temp: temp,
        home,
        plan,
    })
}

fn reachable() -> Result<(), MmError> {
    Ok(())
}

fn unreachable_forwarder() -> Result<(), MmError> {
    Err(MmError::RuntimeResponse {
        message: "no response".to_owned(),
    })
}

#[tokio::test]
async fn autostart_installs_private_compose_and_agent_then_removes_them()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let fake = Arc::new(FakeLaunchd::default());
    let installer = LaunchdAutostartInstaller::with_home(fake.clone(), fixture.home.clone());
    installer.install(&fixture.plan).await?;

    let compose = fixture
        .home
        .join("Library/Application Support/mm/ingress/current/compose.yaml");
    assert_eq!(
        fs::read(&compose)?,
        fs::read(&fixture.plan.artifact.compose_path)?
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(fs::metadata(&compose)?.permissions().mode() & 0o777, 0o600);
    }
    let plist = fixture
        .home
        .join("Library/LaunchAgents/local.mm.ingress.plist");
    assert!(fs::read_to_string(&plist)?.contains("<string>--remove-orphans</string>"));
    assert!(fixture.home.join("Library/Logs/mm").is_dir());
    assert_eq!(fake.verbs(), ["print", "bootstrap"]);

    installer.uninstall(&fixture.plan).await?;
    assert!(!compose.exists() && !plist.exists());
    Ok(())
}

#[tokio::test]
async fn plans_for_other_users_or_with_altered_hashes_are_refused()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let fake = Arc::new(FakeLaunchd::default());
    let elsewhere = LaunchdAutostartInstaller::with_home(fake.clone(), fixture.home.join("other"));
    assert!(matches!(
        elsewhere.install(&fixture.plan).await,
        Err(MmError::Security { .. })
    ));
    let mut altered = fixture.plan.clone();
    altered.plan_hash = "0".repeat(64);
    let installer = LaunchdForwarderInstaller::with_home(fake.clone(), fixture.home, reachable);
    assert!(matches!(
        installer.install(&altered).await,
        Err(MmError::Security { .. })
    ));
    assert!(fake.verbs().is_empty());
    Ok(())
}

#[tokio::test]
async fn forwarder_validates_haproxy_config_before_installing()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let fake = Arc::new(FakeLaunchd {
        other_status: 1,
        ..FakeLaunchd::default()
    });
    let installer =
        LaunchdForwarderInstaller::with_home(fake.clone(), fixture.home.clone(), reachable);
    assert!(installer.install(&fixture.plan).await.is_err());
    let calls = fake.calls.lock().map_err(|_| "poisoned")?.clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0][0], "/opt/homebrew/opt/haproxy/bin/haproxy");
    assert_eq!(calls[0][1..3], ["-c", "-f"]);
    assert!(calls[0][3].ends_with("/haproxy.cfg"));
    assert!(!fixture.home.join("Library/LaunchAgents").exists());
    Ok(())
}

#[tokio::test]
async fn unverified_forwarder_is_rolled_back() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let fake = Arc::new(FakeLaunchd::default());
    let installer = LaunchdForwarderInstaller::with_home(
        fake.clone(),
        fixture.home.clone(),
        unreachable_forwarder,
    );
    let error = installer
        .install(&fixture.plan)
        .await
        .err()
        .ok_or("verification should fail")?;
    assert!(error.to_string().contains("lsof -nP -iTCP:80"));
    assert!(
        !fixture
            .home
            .join("Library/LaunchAgents/local.mm.ingress-forwarder.plist")
            .exists()
    );
    assert!(fake.loaded.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}

#[tokio::test]
async fn stop_keeps_the_installed_agent_and_start_reloads_it()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let fake = Arc::new(FakeLaunchd::default());
    let control = LaunchdPublicIngressControl::with_home(fake.clone(), fixture.home.clone());
    let absent = control.inspect().await?;
    assert!(!absent.installed && !absent.running);
    control
        .set_state(PublicIngressState::Stopped, false)
        .await?;
    assert!(
        control
            .set_state(PublicIngressState::Running, false)
            .await
            .is_err()
    );

    LaunchdForwarderInstaller::with_home(fake.clone(), fixture.home.clone(), reachable)
        .install(&fixture.plan)
        .await?;
    assert!(control.inspect().await?.running);

    control
        .set_state(PublicIngressState::Stopped, false)
        .await?;
    let stopped = control.inspect().await?;
    assert!(stopped.installed && !stopped.running && !stopped.failed);
    assert!(
        fixture
            .home
            .join("Library/LaunchAgents/local.mm.ingress-forwarder.plist")
            .is_file()
    );

    control
        .set_state(PublicIngressState::Running, false)
        .await?;
    assert!(control.inspect().await?.running);
    control
        .set_state(PublicIngressState::Running, false)
        .await?;
    assert_eq!(
        fake.verbs()
            .iter()
            .filter(|verb| *verb == "bootstrap")
            .count(),
        2
    );
    Ok(())
}

#[tokio::test]
async fn a_forwarder_that_cannot_stay_running_is_left_stopped()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    LaunchdForwarderInstaller::with_home(
        Arc::new(FakeLaunchd::default()),
        fixture.home.clone(),
        reachable,
    )
    .install(&fixture.plan)
    .await?;
    let crashing = Arc::new(FakeLaunchd {
        print_stdout: Some("x = {\n\tstate = not running\n\tlast exit code = 1\n}\n"),
        ..FakeLaunchd::default()
    });
    let control = LaunchdPublicIngressControl::with_home(crashing.clone(), fixture.home.clone());
    let error = control
        .set_state(PublicIngressState::Running, false)
        .await
        .err()
        .ok_or("start should fail")?;
    assert!(error.to_string().contains("did not stay running"));
    assert!(crashing.loaded.lock().map_err(|_| "poisoned")?.is_empty());
    assert_eq!(crashing.verbs().last().map(String::as_str), Some("bootout"));
    Ok(())
}

#[tokio::test]
async fn starting_a_launchctl_disabled_forwarder_explains_how_to_enable_it()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    LaunchdForwarderInstaller::with_home(
        Arc::new(FakeLaunchd::default()),
        fixture.home.clone(),
        reachable,
    )
    .install(&fixture.plan)
    .await?;
    let disabled = Arc::new(FakeLaunchd {
        disabled: vec![LAUNCHD_FORWARDER_LABEL],
        ..FakeLaunchd::default()
    });
    let control = LaunchdPublicIngressControl::with_home(disabled, fixture.home.clone());
    let error = control
        .set_state(PublicIngressState::Running, false)
        .await
        .err()
        .ok_or("start should fail")?;
    assert!(error.to_string().contains("launchctl enable gui/"));
    Ok(())
}
