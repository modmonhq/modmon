use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tempfile::TempDir;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::*;
use crate::adapters::clock::FixedClock;
use crate::adapters::services::FilesystemServiceStackStore;
use crate::adapters::target::{FilesystemTargetIngressArtifactStore, FilesystemTargetStore};
use crate::app::ports::{
    CommandOutput, CommandSpec, DashboardServiceInstaller, ProcessRunner, ServiceExecutorStatus,
    ServiceStackExecutor, TargetOperationLock, TargetOperationLockGuard, TargetStore,
};
use crate::app::{
    ServicePlanRequest, ServiceStackManager, ServiceStackManagerConfig, TargetIngressManager,
};
use crate::domain::error::MmError;
use crate::domain::runtime::RuntimeKind;
use crate::domain::service_stack::{DashboardServiceConfig, ServiceStackAction, ServiceStackPlan};
use crate::domain::target::{TargetDefinition, TargetId};

#[tokio::test]
async fn installs_reports_and_removes_the_reviewed_user_service()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let process = Arc::new(FakeProcess::default());
    let installer =
        SystemdDashboardServiceInstaller::with_home(process.clone(), fixture.home.clone());
    installer.install(&fixture.plan).await?;
    let status = installer.status(&fixture.plan).await?;
    assert!(status.installed);
    assert!(status.running);
    assert_eq!(
        fs::read(fixture.unit_path())?,
        fs::read(
            fixture
                .plan
                .artifact
                .dashboard_unit_path
                .as_ref()
                .ok_or("dashboard unit artifact is missing")?
        )?
    );
    installer.uninstall(&fixture.plan).await?;
    assert!(!fixture.unit_path().exists());
    assert!(!fixture.owner_path().exists());
    assert_eq!(
        process.commands()?,
        [
            "/usr/bin/systemctl --user daemon-reload",
            "/usr/bin/systemctl --user enable mm-dashboard.service",
            "/usr/bin/systemctl --user restart mm-dashboard.service",
            "/usr/bin/systemctl --user is-active --quiet mm-dashboard.service",
            "/usr/bin/systemctl --user disable --now mm-dashboard.service",
            "/usr/bin/systemctl --user daemon-reload",
        ]
    );
    Ok(())
}

#[tokio::test]
async fn activation_failure_restores_an_absent_installation()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let process = Arc::new(FakeProcess {
        fail_at: Some(1),
        ..FakeProcess::default()
    });
    let installer =
        SystemdDashboardServiceInstaller::with_home(process.clone(), fixture.home.clone());
    assert!(installer.install(&fixture.plan).await.is_err());
    assert!(!fixture.unit_path().exists());
    assert!(!fixture.owner_path().exists());
    assert_eq!(
        process.commands()?,
        [
            "/usr/bin/systemctl --user daemon-reload",
            "/usr/bin/systemctl --user enable mm-dashboard.service",
            "/usr/bin/systemctl --user disable --now mm-dashboard.service",
            "/usr/bin/systemctl --user daemon-reload",
        ]
    );
    Ok(())
}

#[tokio::test]
async fn refuses_to_adopt_an_unmanaged_unit() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fs::create_dir_all(
        fixture
            .unit_path()
            .parent()
            .ok_or("dashboard unit has no parent")?,
    )?;
    fs::write(fixture.unit_path(), "unmanaged")?;
    let process = Arc::new(FakeProcess::default());
    let installer =
        SystemdDashboardServiceInstaller::with_home(process.clone(), fixture.home.clone());
    assert!(matches!(
        installer.preflight(&fixture.plan).await,
        Err(MmError::Security { .. })
    ));
    assert!(process.commands()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn rejects_a_tampered_reviewed_unit() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fs::write(
        fixture
            .plan
            .artifact
            .dashboard_unit_path
            .as_ref()
            .ok_or("dashboard unit artifact is missing")?,
        "tampered",
    )?;
    let process = Arc::new(FakeProcess::default());
    let installer =
        SystemdDashboardServiceInstaller::with_home(process.clone(), fixture.home.clone());
    assert!(matches!(
        installer.preflight(&fixture.plan).await,
        Err(MmError::Security { .. })
    ));
    assert!(process.commands()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn status_rejects_missing_invalid_and_foreign_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let process = Arc::new(FakeProcess::default());
    let installer =
        SystemdDashboardServiceInstaller::with_home(process.clone(), fixture.home.clone());
    let absent = installer.status(&fixture.plan).await?;
    assert!(!absent.installed);
    assert!(!absent.running);

    prepare_parent(&fixture.unit_path())?;
    fs::write(fixture.unit_path(), b"unmanaged")?;
    assert!(installer.status(&fixture.plan).await.is_err());
    fs::remove_file(fixture.unit_path())?;

    prepare_parent(&fixture.owner_path())?;
    fs::write(fixture.owner_path(), b"not-json")?;
    assert!(installer.status(&fixture.plan).await.is_err());
    let foreign = DashboardServiceOwner {
        target_id: TargetId(Uuid::from_u128(999)),
        plan_hash: fixture.plan.plan_hash.clone(),
    };
    atomic_write(&fixture.owner_path(), &serde_json::to_vec(&foreign)?, 0o600)?;
    assert!(installer.status(&fixture.plan).await.is_err());
    Ok(())
}

#[tokio::test]
async fn removal_failure_restores_previous_managed_installation()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let setup = Arc::new(FakeProcess::default());
    let installer = SystemdDashboardServiceInstaller::with_home(setup, fixture.home.clone());
    installer.install(&fixture.plan).await?;
    let previous_unit = fs::read(fixture.unit_path())?;
    let previous_owner = fs::read(fixture.owner_path())?;

    let failing = Arc::new(FakeProcess {
        fail_at: Some(0),
        ..FakeProcess::default()
    });
    let installer =
        SystemdDashboardServiceInstaller::with_home(failing.clone(), fixture.home.clone());
    assert!(installer.uninstall(&fixture.plan).await.is_err());
    assert_eq!(fs::read(fixture.unit_path())?, previous_unit);
    assert_eq!(fs::read(fixture.owner_path())?, previous_owner);
    assert_eq!(failing.commands()?.len(), 4);

    let inactive = Arc::new(FakeProcess {
        fail_at: Some(0),
        ..FakeProcess::default()
    });
    let installer = SystemdDashboardServiceInstaller::with_home(inactive, fixture.home.clone());
    let status = installer.status(&fixture.plan).await?;
    assert!(status.installed);
    assert!(!status.running);
    Ok(())
}

#[tokio::test]
async fn service_free_and_tampered_plans_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let process = Arc::new(FakeProcess::default());
    let installer = SystemdDashboardServiceInstaller::with_home(process, fixture.home.clone());
    installer.install(&fixture.plan).await?;

    let mut without_service = fixture.plan.clone();
    without_service.dashboard_service = None;
    without_service.artifact.dashboard_unit_path = None;
    without_service.artifact.dashboard_unit_digest = None;
    without_service.plan_hash = without_service.canonical_hash()?;
    assert!(installer.install(&without_service).await.is_err());
    assert!(installer.uninstall(&without_service).await.is_err());
    assert!(fixture.unit_path().exists());
    assert!(fixture.owner_path().exists());

    let mut invalid = without_service;
    invalid.plan_hash = "0".repeat(64);
    assert!(installer.uninstall(&invalid).await.is_err());
    Ok(())
}

#[tokio::test]
async fn constructor_debug_and_helper_io_failures_are_covered()
-> Result<(), Box<dyn std::error::Error>> {
    let process = Arc::new(FakeProcess::default());
    let installer = SystemdDashboardServiceInstaller::new(process)?;
    assert!(format!("{installer:?}").contains("SystemdDashboardServiceInstaller"));

    let temporary = crate::test_support::tempdir()?;
    let missing = temporary.path().join("missing-executable");
    assert!(verify_executable(&missing).is_err());
    let invalid_owner = temporary.path().join("owner.json");
    fs::write(&invalid_owner, b"not-json")?;
    assert!(
        validate_owner(
            &invalid_owner,
            TargetId(Uuid::nil()),
            &temporary.path().join("unit")
        )
        .is_err()
    );
    let blocked = temporary.path().join("blocked");
    fs::write(&blocked, b"file")?;
    assert!(prepare_parent(&blocked.join("child")).is_err());
    assert!(atomic_write(&blocked.join("child"), b"value", 0o600).is_err());
    Ok(())
}

#[tokio::test]
async fn failed_removal_rollback_is_reported() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let installer = SystemdDashboardServiceInstaller::with_home(
        Arc::new(FakeProcess::default()),
        fixture.home.clone(),
    );
    installer.install(&fixture.plan).await?;
    let failing = SystemdDashboardServiceInstaller::with_home(
        Arc::new(FakeProcess {
            fail_all: true,
            ..FakeProcess::default()
        }),
        fixture.home.clone(),
    );
    assert!(
        failing
            .uninstall(&fixture.plan)
            .await
            .is_err_and(|error| error.to_string().contains("rollback failed"))
    );
    Ok(())
}

#[test]
fn owner_parent_and_atomic_publication_helpers_cover_remaining_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let owner_path = temporary.path().join("owner.json");
    let unit_path = temporary.path().join("unit.service");
    let owner = DashboardServiceOwner {
        target_id: TargetId(Uuid::from_u128(1)),
        plan_hash: "plan".to_owned(),
    };
    atomic_write(&owner_path, &serde_json::to_vec(&owner)?, 0o600)?;
    assert!(validate_owner(&owner_path, TargetId(Uuid::from_u128(2)), &unit_path).is_err());
    assert!(prepare_parent(Path::new("/")).is_err());
    assert!(atomic_write(Path::new("/proc/mm-dashboard-test"), b"value", 0o600).is_err());
    let directory = temporary.path().join("directory");
    fs::create_dir(&directory)?;
    assert!(atomic_write(&directory, b"value", 0o600).is_err());
    Ok(())
}

#[test]
fn executable_and_file_helpers_enforce_types_modes_and_idempotent_restore()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    assert!(verify_executable(PathBuf::from("relative").as_path()).is_err());
    assert!(verify_executable(temporary.path()).is_err());
    let executable = temporary.path().join("mm");
    fs::write(&executable, b"binary")?;
    assert!(verify_executable(&executable).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
        verify_executable(&executable)?;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o777))?;
        assert!(verify_executable(&executable).is_err());
    }

    let path = temporary.path().join("restored");
    restore_file(&path, None, 0o600)?;
    restore_file(&path, Some(b"before"), 0o600)?;
    assert_eq!(read_existing(&path)?.as_deref(), Some(b"before".as_slice()));
    restore_file(&path, None, 0o600)?;
    assert!(!path.exists());

    let mut failure = None;
    retain_first(&mut failure, Ok(()));
    retain_first(
        &mut failure,
        Err(MmError::State {
            message: "first".to_owned(),
        }),
    );
    retain_first(
        &mut failure,
        Err(MmError::State {
            message: "second".to_owned(),
        }),
    );
    assert!(failure.is_some());
    assert!(
        format!(
            "{}",
            rollback_failure(
                "test",
                failure.as_ref().ok_or("failure missing")?,
                &MmError::State {
                    message: "rollback".to_owned()
                }
            )
        )
        .contains("dashboard service test")
    );
    Ok(())
}

struct Fixture {
    _temporary: TempDir,
    home: PathBuf,
    plan: ServiceStackPlan,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Self::build(false)
    }

    /// A plan whose local ingress is macOS/launchd, rooted at a canonical path
    /// so the macOS `/var` symlink does not trip the path policy.
    fn macos() -> Result<Self, Box<dyn std::error::Error>> {
        Self::build(true)
    }

    fn build(macos: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let temporary = crate::test_support::tempdir()?;
        let root = dunce::canonicalize(temporary.path())?;
        let home = root.join("home");
        let mm_home = root.join("mm-home");
        let executable = root.join("bin/mm");
        fs::create_dir(&home)?;
        fs::create_dir(&mm_home)?;
        fs::create_dir(executable.parent().ok_or("executable has no parent")?)?;
        fs::write(&executable, "test executable")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
        }
        let targets = Arc::new(FilesystemTargetStore::new(root.join("targets.yaml")));
        let target_id = TargetId(Uuid::from_u128(0x319c_527b_6242_7b43_bfd4_bd60_e27c_1de3));
        targets.insert_target("local", TargetDefinition::local(target_id))?;
        let mut ingress = TargetIngressManager::new(
            targets.clone(),
            Arc::new(FilesystemTargetIngressArtifactStore::new(
                root.join("ingress"),
            )),
        );
        if macos {
            let home_text = home.to_str().ok_or("home is not UTF-8")?.to_owned();
            ingress = ingress.with_local_host_profile(
                crate::domain::target_ingress::IngressHostProfile::MacosLaunchd(
                    crate::domain::target_ingress::MacosHostFacts {
                        haproxy_executable: "/opt/homebrew/opt/haproxy/bin/haproxy".to_owned(),
                        docker_executable: "/usr/local/bin/docker".to_owned(),
                        docker_host: format!("unix://{home_text}/.docker/run/docker.sock"),
                        home: home_text,
                    },
                ),
            );
        }
        let manager = ServiceStackManager::new(ServiceStackManagerConfig {
            runtime: RuntimeKind::DockerCompose,
            ingress,
            targets,
            store: Arc::new(FilesystemServiceStackStore::new(root.join("services"))),
            executor: Arc::new(NoopExecutor),
            locks: Arc::new(NoopLock),
            clock: Arc::new(FixedClock { unix_ms: 0 }),
            dashboard_ingress_token: "a".repeat(64),
            dashboard_service: DashboardServiceConfig {
                executable,
                mm_home,
            },
        });
        let plan = manager.plan(ServicePlanRequest {
            action: ServiceStackAction::Apply,
            host_smtp_port: None,
            purge_data: false,
            include_gate: false,
        })?;
        Ok(Self {
            _temporary: temporary,
            home,
            plan,
        })
    }

    fn unit_path(&self) -> PathBuf {
        self.home.join(".config/systemd/user/mm-dashboard.service")
    }

    fn owner_path(&self) -> PathBuf {
        self.home.join(".local/share/mm/dashboard/owner.json")
    }
}

struct NoopLock;
struct NoopLockGuard;

impl TargetOperationLockGuard for NoopLockGuard {}

impl TargetOperationLock for NoopLock {
    fn acquire(&self, _target_id: TargetId) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Ok(Box::new(NoopLockGuard))
    }
}

struct NoopExecutor;

#[async_trait]
impl ServiceStackExecutor for NoopExecutor {
    async fn preflight(&self, _plan: &ServiceStackPlan) -> Result<(), MmError> {
        Ok(())
    }
    async fn apply(&self, _plan: &ServiceStackPlan) -> Result<(), MmError> {
        Ok(())
    }
    async fn status(&self, _plan: &ServiceStackPlan) -> Result<ServiceExecutorStatus, MmError> {
        Ok(ServiceExecutorStatus {
            components: Vec::new(),
            diagnostics: Vec::new(),
        })
    }
    async fn remove(&self, _plan: &ServiceStackPlan) -> Result<(), MmError> {
        Ok(())
    }
}

#[derive(Default)]
struct FakeProcess {
    calls: AtomicUsize,
    fail_at: Option<usize>,
    fail_all: bool,
    commands: Mutex<Vec<String>>,
}

impl FakeProcess {
    fn commands(&self) -> Result<Vec<String>, MmError> {
        self.commands
            .lock()
            .map(|commands| commands.clone())
            .map_err(|_| MmError::State {
                message: "dashboard test process lock was poisoned".to_owned(),
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
                message: "dashboard test process lock was poisoned".to_owned(),
            })?
            .push(specification.redacted_display());
        let failed = self.fail_all || self.fail_at == Some(call);
        Ok(CommandOutput {
            code: Some(if failed { 1 } else { 0 }),
            stdout: Zeroizing::new(String::new()),
            stderr: Zeroizing::new(if failed {
                "simulated systemd failure".to_owned()
            } else {
                String::new()
            }),
            truncated: false,
        })
    }
}

#[tokio::test]
async fn launchd_dashboard_agent_installs_reports_stops_and_removes()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::adapters::launchd::testing::FakeLaunchd;

    let fixture = Fixture::macos()?;
    let fake = Arc::new(FakeLaunchd::default());
    let installer = LaunchdDashboardServiceInstaller::with_home(fake.clone(), fixture.home.clone());
    installer.preflight(&fixture.plan).await?;
    assert!(!installer.status(&fixture.plan).await?.installed);

    installer.install(&fixture.plan).await?;
    let plist = fixture
        .home
        .join("Library/LaunchAgents/local.mm.dashboard.plist");
    let text = fs::read_to_string(&plist)?;
    assert!(text.contains("<string>127.0.0.1:42831</string>"));
    let status = installer.status(&fixture.plan).await?;
    assert!(status.installed && status.running);

    installer.install(&fixture.plan).await?;
    assert_eq!(fake.verbs(), ["print", "bootstrap", "print", "print"]);

    fs::write(&plist, "tampered")?;
    assert!(installer.status(&fixture.plan).await.is_err());
    fs::write(&plist, text)?;

    installer.uninstall(&fixture.plan).await?;
    assert!(!plist.exists());
    assert!(!installer.status(&fixture.plan).await?.installed);
    Ok(())
}

#[tokio::test]
async fn launchd_dashboard_agent_refuses_linux_plans_and_unmanaged_plists()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::adapters::launchd::testing::FakeLaunchd;

    let linux = Fixture::new()?;
    let installer = LaunchdDashboardServiceInstaller::with_home(
        Arc::new(FakeLaunchd::default()),
        linux.home.clone(),
    );
    assert!(installer.install(&linux.plan).await.is_err());

    let fixture = Fixture::macos()?;
    let installer = LaunchdDashboardServiceInstaller::with_home(
        Arc::new(FakeLaunchd::default()),
        fixture.home.clone(),
    );
    let agents = fixture.home.join("Library/LaunchAgents");
    fs::create_dir_all(&agents)?;
    fs::write(agents.join("local.mm.dashboard.plist"), "someone else's")?;
    assert!(installer.preflight(&fixture.plan).await.is_err());
    assert!(installer.install(&fixture.plan).await.is_err());
    Ok(())
}
