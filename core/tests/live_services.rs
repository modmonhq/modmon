use std::collections::BTreeSet;
use std::io;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use mm::adapters::ca::FilesystemLocalCaStore;
use mm::adapters::clock::SystemClock;
use mm::adapters::process::TokioProcessRunner;
use mm::adapters::runtime::{RuntimeFactory, TcpIngressVerifier};
use mm::adapters::service_stack_runtime::{
    LocalServiceStackExecutor, LocalServiceStackExecutorConfig,
};
use mm::adapters::services::FilesystemServiceStackStore;
use mm::adapters::target::{
    FilesystemTargetIngressArtifactStore, FilesystemTargetOperationLock, FilesystemTargetStore,
};
use mm::app::ports::{
    DashboardHostSource, DashboardServiceInstaller, DashboardServiceStatus,
    PrivilegedIngressInstaller, RootlessIngressInstaller, ServiceStackStore, TargetStore,
};
use mm::app::{
    ServicePlanRequest, ServiceStackManager, ServiceStackManagerConfig, TargetIngressManager,
};
use mm::domain::error::MmError;
use mm::domain::runtime::RuntimeKind;
use mm::domain::service_stack::{
    DASHBOARD_BACKEND_PORT, DASHBOARD_HOST, DashboardServiceConfig, MAILPIT_HOST,
    ServiceStackAction, ServiceStackPlan, services_project_name,
};
use mm::domain::target::TargetId;
use mm::domain::target_ingress::TargetIngressPlan;

const BUSYBOX_IMAGE: &str = "docker.io/library/busybox@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0";

#[derive(Debug)]
struct NoopIngressInstaller;

#[derive(Debug, Default)]
struct LiveDashboardInstaller {
    child: Mutex<Option<Child>>,
    fail_next_install: AtomicBool,
}

impl LiveDashboardInstaller {
    fn fail_next_install(&self) {
        self.fail_next_install.store(true, Ordering::SeqCst);
    }

    fn stop(&self) -> Result<(), MmError> {
        let mut child = self.child.lock().map_err(lock_error)?;
        if let Some(mut process) = child.take() {
            process.kill().map_err(|source| MmError::Io {
                context: "stopping the live dashboard process".to_owned(),
                source,
            })?;
            process.wait().map_err(|source| MmError::Io {
                context: "waiting for the live dashboard process".to_owned(),
                source,
            })?;
        }
        Ok(())
    }
}

impl Drop for LiveDashboardInstaller {
    fn drop(&mut self) {
        if let Ok(child) = self.child.get_mut()
            && let Some(process) = child.as_mut()
        {
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

#[async_trait]
impl DashboardServiceInstaller for LiveDashboardInstaller {
    async fn preflight(&self, plan: &ServiceStackPlan) -> Result<(), MmError> {
        if let Some(service) = &plan.dashboard_service
            && !service.executable.is_file()
        {
            return Err(MmError::Security {
                message: "live dashboard executable is missing".to_owned(),
            });
        }
        Ok(())
    }

    async fn install(&self, plan: &ServiceStackPlan) -> Result<(), MmError> {
        self.stop()?;
        let Some(service) = &plan.dashboard_service else {
            return Ok(());
        };
        if self.fail_next_install.swap(false, Ordering::SeqCst) {
            return Err(MmError::RuntimeResponse {
                message: "injected live dashboard activation failure".to_owned(),
            });
        }
        let child = Command::new(&service.executable)
            .args(["--mm-home"])
            .arg(&service.mm_home)
            .args([
                "--runtime",
                "docker-compose",
                "dashboard",
                "serve",
                "--listen",
                "0.0.0.0:42831",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|source| MmError::Io {
                context: "starting the live dashboard executable".to_owned(),
                source,
            })?;
        *self.child.lock().map_err(lock_error)? = Some(child);
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if TcpStream::connect((Ipv4Addr::LOCALHOST, DASHBOARD_BACKEND_PORT)).is_ok() {
                return Ok(());
            }
            if let Some(status) = self
                .child
                .lock()
                .map_err(lock_error)?
                .as_mut()
                .and_then(|child| child.try_wait().ok().flatten())
            {
                return Err(MmError::RuntimeResponse {
                    message: format!("live dashboard exited before readiness with {status}"),
                });
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(MmError::RuntimeResponse {
            message: "live dashboard did not become ready".to_owned(),
        })
    }

    async fn status(&self, plan: &ServiceStackPlan) -> Result<DashboardServiceStatus, MmError> {
        let expected = plan.dashboard_service.is_some();
        let running = self
            .child
            .lock()
            .map_err(lock_error)?
            .as_mut()
            .is_some_and(|child| child.try_wait().ok().flatten().is_none());
        Ok(DashboardServiceStatus {
            installed: expected && running,
            running,
        })
    }

    async fn uninstall(&self, _plan: &ServiceStackPlan) -> Result<(), MmError> {
        self.stop()
    }
}

struct StaticDashboardHosts;

impl DashboardHostSource for StaticDashboardHosts {
    fn project_dashboard_hosts(&self) -> Result<BTreeSet<String>, MmError> {
        Ok(["mm.live-dashboard.test".to_owned()].into_iter().collect())
    }
}

#[async_trait]
impl RootlessIngressInstaller for NoopIngressInstaller {
    async fn install(&self, _plan: &TargetIngressPlan) -> Result<(), MmError> {
        Ok(())
    }

    async fn uninstall(&self, _plan: &TargetIngressPlan) -> Result<(), MmError> {
        Ok(())
    }
}

#[async_trait]
impl PrivilegedIngressInstaller for NoopIngressInstaller {
    async fn install(&self, _plan: &TargetIngressPlan) -> Result<(), MmError> {
        Ok(())
    }

    async fn uninstall(&self, _plan: &TargetIngressPlan) -> Result<(), MmError> {
        Ok(())
    }
}

#[derive(Debug)]
struct LiveCleanup {
    engine: &'static str,
    target_id: TargetId,
    plan: Option<ServiceStackPlan>,
}

impl Drop for LiveCleanup {
    fn drop(&mut self) {
        let Some(plan) = &self.plan else {
            return;
        };
        for (root, compose, project) in [
            (
                &plan.artifact.root,
                &plan.artifact.compose_path,
                services_project_name(plan.target_id),
            ),
            (
                &plan.ingress.root,
                &plan.ingress.compose_path,
                plan.ingress.manifest.compose_project_name.clone(),
            ),
        ] {
            let _ = Command::new(self.engine)
                .args(["compose", "--project-directory"])
                .arg(root)
                .args(["--file"])
                .arg(compose)
                .args([
                    "--project-name",
                    project.as_str(),
                    "down",
                    "--remove-orphans",
                ])
                .status();
        }
        remove_labeled_containers(self.engine, self.target_id);
        for network in [
            &plan.development_network,
            &plan.ingress.manifest.network_name,
        ] {
            let _ = Command::new(self.engine)
                .args(["network", "rm", network])
                .status();
        }
        for volume in [
            &plan.mailpit_data_volume,
            &plan.ingress.manifest.dynamic_config_volume_name,
            &plan.ingress.manifest.acme_volume_name,
        ] {
            let _ = Command::new(self.engine)
                .args(["volume", "rm", volume])
                .status();
        }
    }
}

#[tokio::test]
#[ignore = "requires MM_LIVE_SERVICES_ROOT and a live Docker or Podman Compose engine"]
async fn live_mailpit_tls_smtp_and_owned_cleanup() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("MM_LIVE_SERVICES_ROOT")?);
    let project_root = root.with_extension("project");
    let runtime = match std::env::var("MM_LIVE_SERVICES_RUNTIME")?.as_str() {
        "docker-compose" => RuntimeKind::DockerCompose,
        "podman-compose" => RuntimeKind::PodmanCompose,
        other => return Err(format!("unsupported live services runtime '{other}'").into()),
    };
    let engine = match runtime {
        RuntimeKind::DockerCompose => "docker",
        RuntimeKind::PodmanCompose => "podman",
    };
    std::fs::create_dir_all(&root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    }
    let executable = std::env::var_os("MM_LIVE_MM_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_mm")))
        .canonicalize()?;
    initialize_project(&executable, &root, &project_root)?;
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(root.join("targets.yaml")));
    let target_id = targets.resolve_target("local")?.definition.id;
    let mut cleanup = LiveCleanup {
        engine,
        target_id,
        plan: None,
    };
    let ca = Arc::new(FilesystemLocalCaStore::new(root.join("ca")));
    ca.install()?;
    let process = Arc::new(TokioProcessRunner);
    let factory = RuntimeFactory::new(process);
    let deployment = factory.deployment(runtime)?;
    let inspector = factory.driver(runtime);
    let dashboard = Arc::new(LiveDashboardInstaller::default());
    let executor = Arc::new(LocalServiceStackExecutor::new(
        LocalServiceStackExecutorConfig {
            routes: Arc::new(mm::adapters::state::SqliteIngressRouteReader {
                path: root.join("state.sqlite3"),
            }),
            dynamic_configs: Arc::new(
                mm::adapters::target::FilesystemTraefikDynamicConfigStore::new(
                    root.join("dynamic-configs"),
                ),
            ),
            targets: Arc::clone(&targets),
            runtime: deployment,
            inspector,
            ca: ca.clone(),
            verifier: Arc::new(TcpIngressVerifier::default()),
            rootless: Arc::new(NoopIngressInstaller),
            privileged: Arc::new(NoopIngressInstaller),
            dashboard: dashboard.clone(),
        },
    ));
    let store = Arc::new(FilesystemServiceStackStore::new(root.join("services")));
    let dashboard_ingress_token = store.load_or_create_dashboard_token()?;
    let manager = ServiceStackManager::new(ServiceStackManagerConfig {
        runtime,
        targets: Arc::clone(&targets),
        ingress: TargetIngressManager::new(
            Arc::clone(&targets),
            Arc::new(FilesystemTargetIngressArtifactStore::new(
                root.join("ingress"),
            )),
        ),
        store: store.clone(),
        executor: executor.clone(),
        locks: Arc::new(FilesystemTargetOperationLock::new(root.join("locks"))),
        clock: Arc::new(SystemClock),
        dashboard_ingress_token,
        dashboard_service: DashboardServiceConfig {
            executable,
            mm_home: root.clone(),
        },
    })
    .with_dashboard_hosts(Arc::new(StaticDashboardHosts));

    let apply_request = request(ServiceStackAction::Apply, None, false);
    let plan = manager.plan(apply_request)?;
    cleanup.plan = Some(plan.clone());
    if let Err(error) = manager
        .execute_reviewed(apply_request, &plan.plan_hash)
        .await
    {
        let _ = Command::new(engine)
            .args(["compose", "--project-directory"])
            .arg(&plan.ingress.root)
            .args(["--file"])
            .arg(&plan.ingress.compose_path)
            .args([
                "--project-name",
                &plan.ingress.manifest.compose_project_name,
                "logs",
                "--no-color",
            ])
            .status();
        return Err(error.into());
    }
    let status = manager.status().await?;
    assert!(status.installed);
    assert!(status.components.iter().all(|component| component.healthy));
    let forwarder = TlsForwarder::start(plan.ingress.manifest.proxy_https_port)?;
    let installation = trusted_dashboard_curl(&root, DASHBOARD_HOST, "/")
        .map_err(|error| format!("installation dashboard request failed: {error}"))?;
    assert!(installation.contains("mm local dashboard"));
    let installation_api = trusted_dashboard_curl(&root, DASHBOARD_HOST, "/api/v1/dashboard")
        .map_err(|error| format!("installation API request failed: {error}"))?;
    assert!(installation_api.contains("\"slug\":\"live-dashboard\""));
    let project = trusted_dashboard_curl(&root, "mm.live-dashboard.test", "/")
        .map_err(|error| format!("project dashboard request failed: {error}"))?;
    assert!(project.contains("mm local dashboard"));
    let events = trusted_sse(&root, DASHBOARD_HOST)
        .map_err(|error| format!("dashboard event stream request failed: {error}"))?;
    assert!(events.contains("event: connected"));
    assert!(events.contains("\"type\":\"connected\""));

    let second = manager.plan(apply_request)?;
    manager
        .execute_reviewed(apply_request, &second.plan_hash)
        .await?;
    trusted_mailpit_curl(&root, &second, "/")?;
    send_mail(engine, &second, "project-one")?;
    send_mail(engine, &second, "project-two")?;
    let messages = trusted_mailpit_curl(&root, &second, "/api/v1/messages")?;
    assert!(messages.contains("project-one"));
    assert!(messages.contains("project-two"));

    let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let occupied_port = occupied.local_addr()?.port();
    let blocked_request = request(ServiceStackAction::Apply, Some(occupied_port), false);
    let blocked_plan = manager.plan(blocked_request)?;
    assert!(
        manager
            .execute_reviewed(blocked_request, &blocked_plan.plan_hash)
            .await
            .is_err()
    );
    assert!(manager.status().await?.installed);
    drop(occupied);

    let previous = store.load_state()?.ok_or("live service state is missing")?;
    dashboard.fail_next_install();
    let failed_apply = request(ServiceStackAction::Apply, None, false);
    let failed_plan = manager.plan(failed_apply)?;
    assert!(
        manager
            .execute_reviewed(failed_apply, &failed_plan.plan_hash)
            .await
            .is_err()
    );
    assert!(dashboard.status(&previous.applied_plan).await?.running);
    let repair = request(ServiceStackAction::Repair, None, false);
    let repair_plan = manager.plan(repair)?;
    manager
        .execute_reviewed(repair, &repair_plan.plan_hash)
        .await?;
    assert!(manager.status().await?.installed);

    let smtp_port = free_loopback_port()?;
    let smtp_request = request(ServiceStackAction::Apply, Some(smtp_port), false);
    let smtp_plan = manager.plan(smtp_request)?;
    cleanup.plan = Some(smtp_plan.clone());
    manager
        .execute_reviewed(smtp_request, &smtp_plan.plan_hash)
        .await?;
    assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, smtp_port)).is_ok());

    let remove_request = request(ServiceStackAction::Remove, None, false);
    let remove_plan = manager.plan(remove_request)?;
    manager
        .execute_reviewed(remove_request, &remove_plan.plan_hash)
        .await?;
    assert!(resource_exists(
        engine,
        "volume",
        &remove_plan.mailpit_data_volume
    ));

    let reinstall = request(ServiceStackAction::Apply, None, false);
    let reinstall_plan = manager.plan(reinstall)?;
    cleanup.plan = Some(reinstall_plan.clone());
    manager
        .execute_reviewed(reinstall, &reinstall_plan.plan_hash)
        .await?;
    let purge = request(ServiceStackAction::Remove, None, true);
    let purge_plan = manager.plan(purge)?;
    manager
        .execute_reviewed(purge, &purge_plan.plan_hash)
        .await?;
    assert!(!resource_exists(
        engine,
        "volume",
        &purge_plan.mailpit_data_volume
    ));
    assert!(!dashboard.status(&purge_plan).await?.running);
    drop(forwarder);
    cleanup.plan = None;
    remove_directory_contents(&root)?;
    std::fs::remove_dir_all(project_root)?;
    Ok(())
}

fn request(
    action: ServiceStackAction,
    host_smtp_port: Option<u16>,
    purge_data: bool,
) -> ServicePlanRequest {
    ServicePlanRequest {
        action,
        host_smtp_port,
        purge_data,
        include_gate: false,
    }
}

fn trusted_mailpit_curl(
    root: &Path,
    plan: &ServiceStackPlan,
    path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let port = plan.ingress.manifest.proxy_https_port;
    trusted_curl(
        root,
        &format!("{MAILPIT_HOST}:{port}:127.0.0.1"),
        &format!("https://{MAILPIT_HOST}:{port}{path}"),
    )
}

fn trusted_dashboard_curl(
    root: &Path,
    host: &str,
    path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    trusted_curl(
        root,
        &format!("{host}:443:127.0.0.1"),
        &format!("https://{host}{path}"),
    )
}

fn trusted_curl(
    root: &Path,
    resolution: &str,
    url: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let ca = root.join("ca/ca.crt");
    let mut last_error = String::new();
    for _ in 0..40 {
        let output = Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--noproxy", "*"])
            .env("SSL_CERT_FILE", &ca)
            .env("CURL_CA_BUNDLE", &ca)
            .args(["--resolve", resolution])
            .arg(url)
            .output()?;
        if output.status.success() {
            return Ok(String::from_utf8(output.stdout)?);
        }
        last_error = String::from_utf8_lossy(&output.stderr).into_owned();
        thread::sleep(Duration::from_millis(250));
    }
    Err(last_error.into())
}

fn trusted_sse(root: &Path, host: &str) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--no-buffer",
            "--max-time",
            "3",
            "--noproxy",
            "*",
        ])
        .env("SSL_CERT_FILE", root.join("ca/ca.crt"))
        .env("CURL_CA_BUNDLE", root.join("ca/ca.crt"))
        .args(["--resolve", &format!("{host}:443:127.0.0.1")])
        .arg(format!("https://{host}/api/v1/events"))
        .output()?;
    if !output.status.success() && output.status.code() != Some(28) {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn initialize_project(
    executable: &Path,
    root: &Path,
    project_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir(project_root)?;
    std::fs::write(project_root.join("compose.yaml"), "services: {}\n")?;
    let review = Command::new(executable)
        .args(["--root"])
        .arg(project_root)
        .args(["--mm-home"])
        .arg(root)
        .args(["--output", "json", "init", "--name", "live-dashboard"])
        .output()?;
    if !review.status.success() {
        return Err(String::from_utf8_lossy(&review.stderr).into_owned().into());
    }
    let review: serde_json::Value = serde_json::from_slice(&review.stdout)?;
    let plan = review["plan_hash"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("initialization plan hash is missing"))?;
    let output = Command::new(executable)
        .args(["--root"])
        .arg(project_root)
        .args(["--mm-home"])
        .arg(root)
        .args([
            "--output",
            "json",
            "init",
            "--name",
            "live-dashboard",
            "--plan",
            plan,
            "--yes",
        ])
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(())
}

struct TlsForwarder {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl TlsForwarder {
    fn start(target_port: u16) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 443))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((client, _)) => {
                        thread::spawn(move || proxy_connection(client, target_port));
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(25));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for TlsForwarder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn proxy_connection(mut client: TcpStream, target_port: u16) {
    let Ok(mut target) = TcpStream::connect((Ipv4Addr::LOCALHOST, target_port)) else {
        return;
    };
    let (Ok(mut client_reader), Ok(mut target_writer)) = (client.try_clone(), target.try_clone())
    else {
        return;
    };
    let upload = thread::spawn(move || io::copy(&mut client_reader, &mut target_writer));
    let _ = io::copy(&mut target, &mut client);
    let _ = upload.join();
}

fn send_mail(
    engine: &str,
    plan: &ServiceStackPlan,
    subject: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let script = format!(
        "printf 'HELO live\\r\\nMAIL FROM:<live@mm.test>\\r\\nRCPT TO:<mail@mm.test>\\r\\nDATA\\r\\nSubject: {subject}\\r\\n\\r\\nbody\\r\\n.\\r\\nQUIT\\r\\n' | nc -w 10 mailpit 1025"
    );
    let status = Command::new(engine)
        .args([
            "run",
            "--rm",
            "--network",
            &plan.development_network,
            "--cpus",
            "0.25",
            "--memory",
            "64m",
            "--pids-limit",
            "32",
            "--read-only",
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges",
            BUSYBOX_IMAGE,
            "sh",
            "-c",
            &script,
        ])
        .status()?;
    if !status.success() {
        return Err(format!("{engine} SMTP sender failed with {status}").into());
    }
    Ok(())
}

fn free_loopback_port() -> Result<u16, std::io::Error> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    listener.local_addr().map(|address| address.port())
}

fn resource_exists(engine: &str, kind: &str, name: &str) -> bool {
    Command::new(engine)
        .args([kind, "inspect", name])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn remove_labeled_containers(engine: &str, target_id: TargetId) {
    let Ok(output) = Command::new(engine)
        .args([
            "ps",
            "--all",
            "--quiet",
            "--filter",
            &format!("label=io.mm.target-id={}", target_id.0),
        ])
        .output()
    else {
        return;
    };
    for id in String::from_utf8_lossy(&output.stdout).split_whitespace() {
        let _ = Command::new(engine).args(["rm", "--force", id]).status();
    }
}

fn lock_error<T>(_error: std::sync::PoisonError<T>) -> MmError {
    MmError::State {
        message: "live dashboard process lock was poisoned".to_owned(),
    }
}

fn remove_directory_contents(path: &Path) -> io::Result<()> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
