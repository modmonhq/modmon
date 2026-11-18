//! `development: on | optional | off`, and the optional units one machine
//! turns on with `mm dev enable` and `mm dev up --with`.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use mm::adapters::config::{ConfigLoader, OVERRIDES_RELATIVE_PATH, validate};
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{ProjectDocumentStore, TargetStore};
use mm::app::{DevelopmentDocuments, DevelopmentUnitService, StackManager, WorkspaceSelection};
use mm::domain::config::ConfigDocument;
use mm::domain::development_units::{Development, UnitKind};
use mm::domain::diagnostic::Severity;
use mm::domain::target::{TargetDefinition, TargetId};
use tempfile::TempDir;
use uuid::Uuid;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const COMPOSE: &str = "services:\n  api:\n    image: example/api\n  status-web:\n    image: example/status\n    depends_on: [status-queue]\n  status-queue:\n    image: example/queue\n  legacy:\n    image: example/legacy\n";

const MODULES: &str = "modules:\n  api:\n    path: api\n    services:\n      api: {stack: app}\n  status:\n    path: status\n    development: optional\n    services:\n      status-web: {stack: app}\n  queue:\n    path: queue\n    development: optional\n    services:\n      status-queue: {stack: app}\n  legacy:\n    path: legacy\n    services:\n      legacy: {stack: app, development: off}\njobs:\n  report:\n    module: status\n    service: status-web\n    command: [/bin/true]\n  cleanup:\n    module: api\n    service: api\n    development: optional\n    command: [/bin/true]\n";

fn project(compose: &str) -> Result<TempDir, Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path();
    for directory in [".config", "api", "status", "queue", "legacy"] {
        fs::create_dir_all(root.join(directory))?;
    }
    fs::write(root.join("compose.yaml"), compose)?;
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "# shared\nschema-version: 1\nproject:\n  id: 019c527b-6242-7b43-bfd4-bd60e27c1de4\n  name: optional\n  development-environment: local\ndefault-environment: local\ntarget: local\nruntime:\n  driver: docker-compose\ningress:\n  automatic-local: false\nstacks:\n  app:\n    files: [compose.yaml]\n    ingress:\n      - domain: status\n        service: status-web\n        port: 8080\n{MODULES}environments:\n  local:\n    domains: {{status: status.optional.test}}\n  production:\n    domains: {{status: status.example.com}}\n"
        ),
    )?;
    Ok(temporary)
}

fn off(document: &ConfigDocument, module: &str, service: &str) -> Option<(Development, String)> {
    document.config.modules[module].services[service]
        .off_locally_in("local")
        .map(|off| (off.development, off.unit.clone()))
}

fn compiled(
    documents: Arc<dyn ProjectDocumentStore>,
    root: &Path,
    environment: &str,
) -> Result<(mm::domain::stack::StackCompilationReport, String), Box<dyn std::error::Error>> {
    let errors = validate(&documents.load_from(root)?, environment)
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Err(format!("{errors:?}").into());
    }
    let home = temp_dirs::tempdir()?;
    let store: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    store.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::parse_str(
            "319c527b-6242-7b43-bfd4-bd60e27c1de4",
        )?)),
    )?;
    let manager = StackManager::new(
        documents,
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        store,
    );
    let report = manager.compile(&WorkspaceSelection::new(root, Some(environment.to_owned())))?;
    let overlay = fs::read_to_string(&report.stacks[0].generated_overlay_path)?;
    Ok((report, overlay))
}

#[test]
fn optional_and_off_units_stay_out_of_development_only() -> TestResult {
    let project = project(COMPOSE)?;
    let document = ConfigLoader.load_from(project.path())?;
    assert_eq!(off(&document, "api", "api"), None);
    assert_eq!(
        off(&document, "status", "status-web"),
        Some((Development::Optional, "status".to_owned()))
    );
    assert_eq!(
        off(&document, "legacy", "legacy"),
        Some((Development::Off, "legacy/legacy".to_owned()))
    );
    let status = &document.config.modules["status"].services["status-web"];
    assert!(!status.is_active("local") && !status.is_in_environment("local"));
    assert!(status.is_active("production"));
    let jobs = &document.config.jobs;
    assert!(!jobs["report"].runs_in("local") && jobs["report"].runs_in("production"));
    assert!(!jobs["cleanup"].runs_in("local") && jobs["cleanup"].runs_in("production"));

    let (report, overlay) = compiled(Arc::new(ConfigLoader), project.path(), "local")?;
    let excluded = report
        .excluded_services
        .iter()
        .map(|service| {
            (
                service.service.as_str(),
                service
                    .off_locally
                    .as_ref()
                    .and_then(|off| off.enable_command()),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        excluded,
        [
            ("legacy", None),
            ("status-queue", Some("mm dev enable queue".to_owned())),
            ("status-web", Some("mm dev enable status".to_owned())),
        ]
    );
    assert!(!overlay.contains("status-web"));
    assert!(report.route_claims.is_empty());
    let ingress = validate(&document, "local")
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code.starts_with("MM_INGRESS"))
        .collect::<Vec<_>>();
    assert!(ingress.is_empty(), "{ingress:?}");
    let (production, _) = compiled(Arc::new(ConfigLoader), project.path(), "production")?;
    assert!(production.excluded_services.is_empty());
    assert_eq!(production.route_claims[0].service, "status-web");

    DevelopmentUnitService::new(Arc::new(ConfigLoader)).set(
        project.path(),
        &["status".to_owned()],
        true,
    )?;
    let (enabled, _) = compiled(Arc::new(ConfigLoader), project.path(), "local")?;
    assert_eq!(enabled.route_claims[0].host, "status.optional.test");
    Ok(())
}

#[test]
fn enabling_a_unit_brings_what_it_needs_and_is_kept_in_the_local_overrides() -> TestResult {
    let project = project(COMPOSE)?;
    let root = project.path();
    let overrides = root.join(OVERRIDES_RELATIVE_PATH);
    fs::write(&overrides, "# mine\nbuild:\n  platforms: [linux/amd64]\n")?;
    let service = DevelopmentUnitService::new(Arc::new(ConfigLoader));

    let change = service.set(root, &["status".to_owned()], true)?;
    assert_eq!(
        change.started,
        ["queue/status-queue", "report", "status/status-web"]
    );
    assert!(change.stopped.is_empty());
    assert_eq!(
        fs::read_to_string(&overrides)?,
        "# mine\nbuild:\n  platforms: [linux/amd64]\ndevelopment:\n  enabled:\n    - status\n"
    );
    let document = ConfigLoader.load_from(root)?;
    assert_eq!(off(&document, "status", "status-web"), None);
    assert_eq!(off(&document, "queue", "status-queue"), None);
    assert_eq!(document.config.build.platforms, ["linux/amd64"]);
    let units = service.list(root)?.units;
    let status = units
        .iter()
        .find(|unit| unit.name == "status")
        .ok_or("no status unit")?;
    assert!(status.kind == UnitKind::Module && status.on && status.enabled);
    let queue = units
        .iter()
        .find(|unit| unit.name == "queue")
        .ok_or("no queue unit")?;
    assert!(queue.on && !queue.enabled);

    let again = service.set(root, &["status".to_owned()], true)?;
    assert!(again.started.is_empty());

    let change = service.set(root, &["status".to_owned()], false)?;
    assert_eq!(
        change.stopped,
        ["queue/status-queue", "report", "status/status-web"]
    );
    assert_eq!(
        fs::read_to_string(&overrides)?,
        "# mine\nbuild:\n  platforms: [linux/amd64]\n"
    );
    Ok(())
}

#[test]
fn names_that_turn_nothing_on_are_refused_with_the_next_step() -> TestResult {
    let project = project(COMPOSE)?;
    let root = project.path();
    let service = DevelopmentUnitService::new(Arc::new(ConfigLoader));
    let unknown = service
        .set(root, &["nope".to_owned()], true)
        .err()
        .ok_or("an unknown unit was enabled")?;
    assert!(
        unknown
            .to_string()
            .contains("the optional ones are queue, status, cleanup"),
        "{unknown}"
    );
    let off = service
        .set(root, &["legacy".to_owned()], true)
        .err()
        .ok_or("an off unit was enabled")?;
    assert!(off.to_string().contains("`development: off`"), "{off}");
    let always = service.set(root, &["api/api".to_owned()], true)?;
    assert_eq!(always.notes, ["api/api always runs in local"]);
    assert!(!root.join(OVERRIDES_RELATIVE_PATH).exists());
    let disable = service
        .set(root, &["api/api".to_owned()], false)
        .err()
        .ok_or("an always-on unit was disabled")?;
    assert!(disable.to_string().contains("always runs in development"));
    assert!(service.check_with(root, &["nope".to_owned()]).is_err());
    service.check_with(root, &["queue".to_owned(), "api/api".to_owned()])?;
    Ok(())
}

#[test]
fn a_unit_on_for_one_run_does_not_change_the_overrides() -> TestResult {
    let project = project(COMPOSE)?;
    let root = project.path();
    let documents = DevelopmentDocuments::new(Arc::new(ConfigLoader), vec!["cleanup".to_owned()]);
    let document = documents.load_from(root)?;
    assert!(document.config.jobs["cleanup"].runs_in("local"));
    assert_eq!(
        off(&document, "status", "status-web").map(|(development, _)| development),
        Some(Development::Optional)
    );
    let with_queue = DevelopmentDocuments::new(Arc::new(ConfigLoader), vec!["queue".to_owned()]);
    let (report, _) = compiled(Arc::new(with_queue), root, "local")?;
    assert!(
        !report
            .excluded_services
            .iter()
            .any(|service| service.service == "status-queue")
    );
    assert!(!root.join(OVERRIDES_RELATIVE_PATH).exists());
    Ok(())
}

#[test]
fn a_running_unit_that_needs_an_optional_one_that_is_off_is_invalid() -> TestResult {
    let compose = COMPOSE.replace(
        "  api:\n    image: example/api\n",
        "  api:\n    image: example/api\n    depends_on:\n      status-web: {condition: service_started}\n",
    );
    let project = project(&compose)?;
    let root = project.path();
    let errors = |environment: &str| -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let document = ConfigLoader.load_from(root)?;
        Ok(validate(&document, environment)
            .diagnostics
            .into_iter()
            .filter(|diagnostic| {
                diagnostic.code == "MM_DEVELOPMENT_DEPENDENCY"
                    && diagnostic.severity == Severity::Error
            })
            .map(|diagnostic| diagnostic.message)
            .collect())
    };
    let local = errors("local")?;
    assert_eq!(local.len(), 1, "{local:?}");
    assert!(
        local[0].contains("api/api runs in local and needs status/status-web")
            && local[0].contains("Run `mm dev enable status`"),
        "{}",
        local[0]
    );
    assert!(errors("production")?.is_empty());
    DevelopmentUnitService::new(Arc::new(ConfigLoader)).set(root, &["status".to_owned()], true)?;
    assert!(errors("local")?.is_empty());
    Ok(())
}

#[test]
fn the_enabled_list_belongs_to_the_local_overrides() -> TestResult {
    let project = project(COMPOSE)?;
    let root = project.path();
    let path = root.join(".config/mm.yaml");
    let shared = fs::read_to_string(&path)?;
    fs::write(
        &path,
        format!("{shared}development:\n  enabled: [status]\n"),
    )?;
    let error = ConfigLoader
        .load_from(root)
        .err()
        .ok_or("mm.yaml set the enabled list")?;
    assert!(
        error
            .to_string()
            .contains("belongs in .config/mm.overrides.yaml"),
        "{error}"
    );
    fs::write(&path, shared)?;
    fs::write(
        root.join(OVERRIDES_RELATIVE_PATH),
        "development:\n  enabled: [statsu]\n",
    )?;
    let document = ConfigLoader.load_from(root)?;
    let warnings = validate(&document, "local")
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "MM_DEVELOPMENT_UNIT")
        .map(|diagnostic| diagnostic.message)
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("run `mm dev disable statsu`"));
    let change = DevelopmentUnitService::new(Arc::new(ConfigLoader)).set(
        root,
        &["statsu".to_owned()],
        false,
    )?;
    assert!(change.optional.iter().all(|unit| !unit.enabled));
    assert_eq!(
        fs::read_to_string(root.join(OVERRIDES_RELATIVE_PATH))?,
        "{}\n"
    );
    Ok(())
}

#[test]
fn the_overrides_are_only_created_where_git_ignores_them() -> TestResult {
    let project = project(COMPOSE)?;
    let root = project.path();
    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(root)
        .status()?;
    assert!(status.success());
    let service = DevelopmentUnitService::new(Arc::new(ConfigLoader));
    let error = service
        .set(root, &["status".to_owned()], true)
        .err()
        .ok_or("an unignored overrides file was created")?;
    assert!(error.to_string().contains("git does not ignore"), "{error}");
    assert!(!root.join(OVERRIDES_RELATIVE_PATH).exists());
    fs::write(root.join(".gitignore"), "/.config/mm.overrides.yaml\n")?;
    service.set(root, &["status".to_owned()], true)?;
    assert!(root.join(OVERRIDES_RELATIVE_PATH).exists());
    Ok(())
}
