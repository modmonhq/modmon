use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tempfile::TempDir;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::SystemdRootlessIngressInstaller;
use crate::adapters::target::{FilesystemTargetIngressArtifactStore, FilesystemTargetStore};
use crate::app::TargetIngressManager;
use crate::app::ports::{
    CommandOutput, CommandSpec, ProcessRunner, RootlessIngressInstaller, TargetStore,
};
use crate::domain::error::MmError;
use crate::domain::target::{TargetDefinition, TargetId};
use crate::domain::target_ingress::TargetIngressPlan;

#[tokio::test]
async fn installs_reviewed_files_and_enables_the_user_unit()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let process = Arc::new(FakeProcess::default());
    let installer =
        SystemdRootlessIngressInstaller::with_home(process.clone(), fixture.home.clone());

    installer.install(&fixture.plan).await?;

    assert_eq!(
        fs::read(
            fixture
                .home
                .join(".local/share/mm/ingress/current/compose.yaml")
        )?,
        fs::read(&fixture.plan.artifact.compose_path)?
    );
    assert_eq!(
        fs::read(fixture.home.join(".config/systemd/user/mm-ingress.service"))?,
        fs::read(support_path(&fixture.plan, "mm-ingress.service")?)?
    );
    assert_eq!(process.calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn refuses_to_adopt_an_unmanaged_user_unit() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let unit = fixture.home.join(".config/systemd/user/mm-ingress.service");
    fs::create_dir_all(
        unit.parent()
            .ok_or_else(|| std::io::Error::other("unit has no parent"))?,
    )?;
    fs::write(&unit, "unmanaged")?;
    let process = Arc::new(FakeProcess::default());
    let installer =
        SystemdRootlessIngressInstaller::with_home(process.clone(), fixture.home.clone());

    let result = installer.install(&fixture.plan).await;

    assert!(matches!(result, Err(MmError::Security { .. })));
    assert_eq!(process.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn repairs_a_partial_same_owner_installation() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let initial = Arc::new(FakeProcess::default());
    SystemdRootlessIngressInstaller::with_home(initial, fixture.home.clone())
        .install(&fixture.plan)
        .await?;
    let unit = fixture.home.join(".config/systemd/user/mm-ingress.service");
    fs::remove_file(&unit)?;
    let retry = Arc::new(FakeProcess::default());
    let installer = SystemdRootlessIngressInstaller::with_home(retry.clone(), fixture.home.clone());

    installer.install(&fixture.plan).await?;

    assert_eq!(
        fs::read(unit)?,
        fs::read(support_path(&fixture.plan, "mm-ingress.service")?)?
    );
    assert_eq!(retry.calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn removes_a_failed_first_installation() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let failing = Arc::new(FakeProcess {
        fail_at: vec![1],
        ..FakeProcess::default()
    });
    let installer =
        SystemdRootlessIngressInstaller::with_home(failing.clone(), fixture.home.clone());

    assert!(installer.install(&fixture.plan).await.is_err());

    assert!(
        !fixture
            .home
            .join(".local/share/mm/ingress/owner.json")
            .exists()
    );
    assert!(
        !fixture
            .home
            .join(".local/share/mm/ingress/current/compose.yaml")
            .exists()
    );
    assert!(
        !fixture
            .home
            .join(".config/systemd/user/mm-ingress.service")
            .exists()
    );
    assert_eq!(
        failing.commands()?,
        [
            "/usr/bin/systemctl --user daemon-reload",
            "/usr/bin/systemctl --user enable --now mm-ingress.service",
            "/usr/bin/systemctl --user disable --now mm-ingress.service",
            "/usr/bin/systemctl --user daemon-reload",
        ]
    );
    Ok(())
}

#[tokio::test]
async fn restores_previous_files_when_systemd_reload_fails()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let successful = Arc::new(FakeProcess::default());
    SystemdRootlessIngressInstaller::with_home(successful, fixture.home.clone())
        .install(&fixture.plan)
        .await?;
    let compose = fixture
        .home
        .join(".local/share/mm/ingress/current/compose.yaml");
    let unit = fixture.home.join(".config/systemd/user/mm-ingress.service");
    fs::write(&compose, "previous compose")?;
    fs::write(&unit, "previous unit")?;
    let failing = Arc::new(FakeProcess {
        fail_at: vec![0],
        ..FakeProcess::default()
    });
    let installer =
        SystemdRootlessIngressInstaller::with_home(failing.clone(), fixture.home.clone());

    assert!(installer.install(&fixture.plan).await.is_err());
    assert_eq!(fs::read_to_string(compose)?, "previous compose");
    assert_eq!(fs::read_to_string(unit)?, "previous unit");
    assert_eq!(
        failing.commands()?,
        [
            "/usr/bin/systemctl --user daemon-reload",
            "/usr/bin/systemctl --user daemon-reload",
            "/usr/bin/systemctl --user enable mm-ingress.service",
            "/usr/bin/systemctl --user restart mm-ingress.service",
        ]
    );
    Ok(())
}

#[tokio::test]
async fn restores_previous_service_when_activation_fails() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new()?;
    let successful = Arc::new(FakeProcess::default());
    SystemdRootlessIngressInstaller::with_home(successful, fixture.home.clone())
        .install(&fixture.plan)
        .await?;
    let compose = fixture
        .home
        .join(".local/share/mm/ingress/current/compose.yaml");
    let unit = fixture.home.join(".config/systemd/user/mm-ingress.service");
    fs::write(&compose, "previous compose")?;
    fs::write(&unit, "previous unit")?;
    let failing = Arc::new(FakeProcess {
        fail_at: vec![1],
        ..FakeProcess::default()
    });
    let installer =
        SystemdRootlessIngressInstaller::with_home(failing.clone(), fixture.home.clone());

    assert!(installer.install(&fixture.plan).await.is_err());
    assert_eq!(fs::read_to_string(compose)?, "previous compose");
    assert_eq!(fs::read_to_string(unit)?, "previous unit");
    assert_eq!(
        failing.commands()?,
        [
            "/usr/bin/systemctl --user daemon-reload",
            "/usr/bin/systemctl --user enable --now mm-ingress.service",
            "/usr/bin/systemctl --user daemon-reload",
            "/usr/bin/systemctl --user enable mm-ingress.service",
            "/usr/bin/systemctl --user restart mm-ingress.service",
        ]
    );
    Ok(())
}

#[tokio::test]
async fn uninstalls_only_the_reviewed_target_and_removes_managed_files()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let process = Arc::new(FakeProcess::default());
    let installer =
        SystemdRootlessIngressInstaller::with_home(process.clone(), fixture.home.clone());
    installer.install(&fixture.plan).await?;

    installer.uninstall(&fixture.plan).await?;

    assert!(
        !fixture
            .home
            .join(".local/share/mm/ingress/owner.json")
            .exists()
    );
    assert!(
        !fixture
            .home
            .join(".local/share/mm/ingress/current/compose.yaml")
            .exists()
    );
    assert!(
        !fixture
            .home
            .join(".config/systemd/user/mm-ingress.service")
            .exists()
    );
    assert_eq!(
        process.commands()?,
        [
            "/usr/bin/systemctl --user daemon-reload",
            "/usr/bin/systemctl --user enable --now mm-ingress.service",
            "/usr/bin/systemctl --user disable --now mm-ingress.service",
            "/usr/bin/systemctl --user daemon-reload",
        ]
    );
    Ok(())
}

#[tokio::test]
async fn restores_rootless_ingress_when_uninstall_reload_fails()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    SystemdRootlessIngressInstaller::with_home(
        Arc::new(FakeProcess::default()),
        fixture.home.clone(),
    )
    .install(&fixture.plan)
    .await?;
    let process = Arc::new(FakeProcess {
        fail_at: vec![1],
        ..FakeProcess::default()
    });
    let installer =
        SystemdRootlessIngressInstaller::with_home(process.clone(), fixture.home.clone());

    assert!(installer.uninstall(&fixture.plan).await.is_err());

    assert!(
        fixture
            .home
            .join(".local/share/mm/ingress/owner.json")
            .exists()
    );
    assert!(
        fixture
            .home
            .join(".local/share/mm/ingress/current/compose.yaml")
            .exists()
    );
    assert!(
        fixture
            .home
            .join(".config/systemd/user/mm-ingress.service")
            .exists()
    );
    assert_eq!(
        process.commands()?,
        [
            "/usr/bin/systemctl --user disable --now mm-ingress.service",
            "/usr/bin/systemctl --user daemon-reload",
            "/usr/bin/systemctl --user daemon-reload",
            "/usr/bin/systemctl --user enable mm-ingress.service",
            "/usr/bin/systemctl --user restart mm-ingress.service",
        ]
    );
    Ok(())
}

#[tokio::test]
async fn installation_preserves_one_rollback_failure() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let failing = Arc::new(FakeProcess {
        fail_at: vec![1, 2],
        ..FakeProcess::default()
    });
    let installer = SystemdRootlessIngressInstaller::with_home(failing, fixture.home.clone());

    let error = installer
        .install(&fixture.plan)
        .await
        .err()
        .ok_or_else(|| {
            std::io::Error::other("injected ingress and rollback failures unexpectedly succeeded")
        })?;

    assert!(matches!(
        error,
        MmError::OperationFailure {
            cleanup_failures,
            ..
        } if cleanup_failures.len() == 1
    ));
    Ok(())
}

#[tokio::test]
async fn installation_preserves_multiple_rollback_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let failing = Arc::new(FakeProcess {
        fail_at: vec![1, 2, 3],
        ..FakeProcess::default()
    });
    let installer = SystemdRootlessIngressInstaller::with_home(failing, fixture.home.clone());

    let error = installer
        .install(&fixture.plan)
        .await
        .err()
        .ok_or_else(|| {
            std::io::Error::other("injected ingress and rollback failures unexpectedly succeeded")
        })?;

    match error {
        MmError::OperationFailure {
            cleanup_failures,
            mutation_status,
            ..
        } => {
            assert_eq!(cleanup_failures.len(), 2);
            assert_eq!(
                mutation_status,
                crate::domain::error::MutationStatus::Unknown
            );
        }
        other => return Err(format!("expected aggregated operation failure, got {other}").into()),
    }
    Ok(())
}

#[tokio::test]
async fn plan_artifacts_and_ownership_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let installer = SystemdRootlessIngressInstaller::with_home(
        Arc::new(FakeProcess::default()),
        fixture.home.clone(),
    );
    assert!(format!("{installer:?}").contains(fixture.home.to_string_lossy().as_ref()));

    let mut wrong_target = fixture.plan.clone();
    wrong_target.target_id = TargetId(Uuid::new_v4());
    assert!(matches!(
        installer.install(&wrong_target).await,
        Err(MmError::Security { .. })
    ));

    let mut missing_unit = fixture.plan.clone();
    missing_unit.artifact.support_paths.clear();
    assert!(matches!(
        installer.install(&missing_unit).await,
        Err(MmError::Security { .. })
    ));

    fs::write(&fixture.plan.artifact.compose_path, b"tampered compose")?;
    assert!(installer.install(&fixture.plan).await.is_err());

    let fixture = Fixture::new()?;
    let installer = SystemdRootlessIngressInstaller::with_home(
        Arc::new(FakeProcess::default()),
        fixture.home.clone(),
    );
    fs::write(
        support_path(&fixture.plan, "mm-ingress.service")?,
        b"tampered unit",
    )?;
    assert!(matches!(
        installer.install(&fixture.plan).await,
        Err(MmError::Security { .. })
    ));

    let fixture = Fixture::new()?;
    let installer = SystemdRootlessIngressInstaller::with_home(
        Arc::new(FakeProcess::default()),
        fixture.home.clone(),
    );
    fs::remove_file(&fixture.plan.artifact.compose_path)?;
    assert!(installer.install(&fixture.plan).await.is_err());
    Ok(())
}

#[tokio::test]
async fn uninstall_rejects_unmanaged_invalid_and_foreign_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let process = Arc::new(FakeProcess::default());
    let installer =
        SystemdRootlessIngressInstaller::with_home(process.clone(), fixture.home.clone());
    installer.uninstall(&fixture.plan).await?;
    assert_eq!(process.calls.load(Ordering::SeqCst), 0);

    let compose = fixture
        .home
        .join(".local/share/mm/ingress/current/compose.yaml");
    fs::create_dir_all(compose.parent().ok_or("compose parent missing")?)?;
    fs::write(&compose, b"unmanaged")?;
    assert!(matches!(
        installer.uninstall(&fixture.plan).await,
        Err(MmError::Security { .. })
    ));
    fs::remove_file(compose)?;

    let owner = fixture.home.join(".local/share/mm/ingress/owner.json");
    fs::create_dir_all(owner.parent().ok_or("owner parent missing")?)?;
    fs::write(&owner, b"not-json")?;
    assert!(matches!(
        installer.uninstall(&fixture.plan).await,
        Err(MmError::Security { .. })
    ));
    assert!(matches!(
        installer.install(&fixture.plan).await,
        Err(MmError::Security { .. })
    ));

    fs::write(
        &owner,
        serde_json::to_vec(&super::RootlessIngressOwner {
            target_id: TargetId(Uuid::new_v4()),
            artifact_hash: "foreign".to_owned(),
        })?,
    )?;
    assert!(matches!(
        installer.uninstall(&fixture.plan).await,
        Err(MmError::Security { .. })
    ));
    assert!(matches!(
        installer.install(&fixture.plan).await,
        Err(MmError::Security { .. })
    ));
    Ok(())
}

#[test]
fn filesystem_helpers_restore_remove_and_reject_invalid_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let existing = temporary.path().join("existing");
    fs::write(&existing, b"current")?;
    super::restore_file(&existing, Some(b"previous"), 0o600)?;
    assert_eq!(fs::read(&existing)?, b"previous");
    super::restore_file(&existing, None, 0o600)?;
    assert!(!existing.exists());
    super::restore_file(&existing, None, 0o600)?;

    assert!(super::prepare_parent(Path::new("/")).is_err());
    assert!(
        super::atomic_write(
            &temporary.path().join("missing-parent/file"),
            b"value",
            0o600,
        )
        .is_err()
    );
    assert_eq!(super::read_existing(&existing)?, None);
    assert!(super::prepare_parent(Path::new("/proc/mm-rootless/file")).is_err());
    assert!(super::atomic_write(Path::new("/proc/mm-rootless-file"), b"value", 0o600).is_err());
    let directory = temporary.path().join("directory-destination");
    fs::create_dir(&directory)?;
    assert!(super::atomic_write(&directory, b"value", 0o600).is_err());
    assert!(super::set_mode(&temporary.path().join("missing"), 0o600).is_err());
    Ok(())
}

#[test]
fn public_constructor_uses_the_resolved_home_and_redacts_the_runner()
-> Result<(), Box<dyn std::error::Error>> {
    let installer = SystemdRootlessIngressInstaller::new(Arc::new(FakeProcess::default()))?;
    assert!(format!("{installer:?}").contains("SystemdRootlessIngressInstaller"));
    Ok(())
}

struct Fixture {
    _temporary: TempDir,
    home: PathBuf,
    plan: TargetIngressPlan,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temporary = crate::test_support::tempdir()?;
        let home = temporary.path().join("home");
        fs::create_dir(&home)?;
        let targets = Arc::new(FilesystemTargetStore::new(
            temporary.path().join("targets.yaml"),
        ));
        targets.ensure_target(
            "local",
            TargetDefinition::local(TargetId(Uuid::from_u128(
                0x319c527b62427b43bfd4bd60e27c1de3,
            ))),
        )?;
        let artifacts = Arc::new(FilesystemTargetIngressArtifactStore::new(
            temporary.path().join("artifacts"),
        ));
        let plan = TargetIngressManager::new(targets, artifacts).plan("local")?;
        Ok(Self {
            _temporary: temporary,
            home,
            plan,
        })
    }
}

#[derive(Default)]
struct FakeProcess {
    calls: AtomicUsize,
    fail_at: Vec<usize>,
    commands: Mutex<Vec<String>>,
}

impl FakeProcess {
    fn commands(&self) -> Result<Vec<String>, MmError> {
        self.commands
            .lock()
            .map(|commands| commands.clone())
            .map_err(|_| MmError::State {
                message: "fake process command lock was poisoned".to_owned(),
            })
    }
}

#[async_trait]
impl ProcessRunner for FakeProcess {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        self.commands
            .lock()
            .map_err(|_| MmError::State {
                message: "fake process command lock was poisoned".to_owned(),
            })?
            .push(specification.redacted_display());
        let fails = self.fail_at.contains(&call);
        Ok(CommandOutput {
            code: Some(if fails { 1 } else { 0 }),
            stdout: Zeroizing::new(String::new()),
            stderr: Zeroizing::new(if fails {
                "simulated systemd failure".to_owned()
            } else {
                String::new()
            }),
            truncated: false,
        })
    }
}

fn support_path<'a>(
    plan: &'a TargetIngressPlan,
    name: &str,
) -> Result<&'a Path, Box<dyn std::error::Error>> {
    plan.artifact
        .support_paths
        .iter()
        .find(|path| path.file_name().and_then(|value| value.to_str()) == Some(name))
        .map(PathBuf::as_path)
        .ok_or_else(|| format!("support file '{name}' is missing").into())
}
