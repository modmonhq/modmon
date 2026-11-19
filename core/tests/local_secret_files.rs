#![cfg(unix)]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::environment::EnvironmentIndexer;
use mm::adapters::runtime::FakeRuntime;
use mm::adapters::secrets::DockerComposeFileSecretDelivery;
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::state::SqliteStateStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{IngressRouteRegistry, RuntimeSecretDelivery, TargetStore};
use mm::app::{Application, ApplicationDependencies, StackManager, WorkspaceSelection};
use mm::domain::config::MmConfig;
use mm::domain::operation::OperationId;
use mm::domain::runtime::{RuntimeKind, RuntimeProbe};
use mm::domain::runtime_secret::{
    HostIds, RuntimeSecretConsumer, RuntimeSecretMaterial, RuntimeSecretPlan,
    RuntimeSecretPlanItem, SecretFileOwnerNote,
};
use mm::domain::secret::SecretMaterial;
use mm::domain::secret_contract::DeliveryDriverId;
use mm::domain::target::{TargetDefinition, TargetId};

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const PROJECT_ID: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de3";
const TARGET_ID: &str = "319c527b-6242-7b43-bfd4-bd60e27c1de3";
const VALUE: &str = "local-file-secret-test-value";

fn this_user() -> HostIds {
    HostIds {
        uid: rustix::process::geteuid().as_raw(),
        gid: rustix::process::getegid().as_raw(),
    }
}

fn consumer(service: &str, ids: &str, mode: u32) -> RuntimeSecretConsumer {
    RuntimeSecretConsumer {
        stack: "app".to_owned(),
        service: service.to_owned(),
        target: "database-password".to_owned(),
        uid: ids.to_owned(),
        gid: ids.to_owned(),
        mode,
    }
}

fn plan(
    consumers: Vec<RuntimeSecretConsumer>,
) -> Result<RuntimeSecretPlan, Box<dyn std::error::Error>> {
    Ok(RuntimeSecretPlan::new(
        PROJECT_ID.to_owned(),
        TARGET_ID.to_owned(),
        "local".to_owned(),
        RuntimeKind::DockerCompose,
        vec![RuntimeSecretPlanItem {
            logical_name: "database.password".to_owned(),
            backend: "age".to_owned(),
            delivery: DeliveryDriverId::new("compose-file-secret")?,
            source_revision: "revision-1".to_owned(),
            previous_source_revision: None,
            consumers,
        }],
    )?)
}

fn material() -> Result<Vec<RuntimeSecretMaterial>, String> {
    Ok(vec![RuntimeSecretMaterial {
        logical_name: "database.password".to_owned(),
        material: SecretMaterial::new(VALUE.to_owned())?,
    }])
}

/// Each service's mount in an overlay, and the file behind it.
fn mounts(
    path: &Path,
) -> Result<BTreeMap<String, (serde_yaml_ng::Value, PathBuf)>, Box<dyn std::error::Error>> {
    let overlay: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(path)?)?;
    let mut mounts = BTreeMap::new();
    for (service, definition) in overlay["services"]
        .as_mapping()
        .ok_or("the overlay has no services")?
    {
        let mount = definition["secrets"][0].clone();
        let key = mount["source"]
            .as_str()
            .ok_or("the service mounts no secret")?;
        let file = overlay["secrets"][key]["file"]
            .as_str()
            .ok_or("the secret has no file")?;
        mounts.insert(
            service
                .as_str()
                .ok_or("a service without a name")?
                .to_owned(),
            (mount.clone(), PathBuf::from(file)),
        );
    }
    Ok(mounts)
}

/// Every file the driver on `run` keeps, but the key that names them.
fn kept_files(run: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut pending = vec![run.with_file_name("run-files")];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else if entry.file_name() != "name.key" {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    Ok(files)
}

/// The file has `mode`, belongs to `owner`, and every directory above it
/// up to `home` is the user's alone.
fn assert_kept(file: &Path, owner: HostIds, mode: u32, home: &Path) -> TestResult {
    let metadata = fs::metadata(file)?;
    assert_eq!(
        metadata.permissions().mode() & 0o777,
        mode,
        "{}",
        file.display()
    );
    assert_eq!((metadata.uid(), metadata.gid()), (owner.uid, owner.gid));
    assert_eq!(fs::read_to_string(file)?, VALUE);
    let mut directory = file.parent();
    while let Some(parent) = directory.filter(|parent| *parent != home) {
        let metadata = fs::metadata(parent)?;
        assert_eq!(
            metadata.permissions().mode() & 0o777,
            0o700,
            "{}",
            parent.display()
        );
        assert_eq!(metadata.uid(), this_user().uid);
        directory = parent.parent();
    }
    Ok(())
}

#[tokio::test]
async fn a_local_file_secret_has_the_mode_its_entry_asks_for_in_a_directory_only_you_reach()
-> TestResult {
    let home = temp_dirs::tempdir()?;
    let run = home.path().join("run");
    let user = this_user();
    let driver = DockerComposeFileSecretDelivery::kept_for(run.clone(), Some(user))?;
    assert_eq!(driver.capability().files_kept_for, Some(user));
    assert!(!driver.capability().supports_uid_gid_mode);
    let plan = plan(vec![
        consumer("api", "0", 0o444),
        consumer("worker", "999", 0o444),
        consumer("db", "999", 0o400),
    ])?;
    let prepared = driver
        .prepare(OperationId::new(), &plan, material()?)
        .await?;
    let mounts = mounts(&prepared.overlays[0].1)?;

    // Every service that keeps mode 0444 mounts the revision's own file,
    // which every user in a container can read.
    let (api, shared) = &mounts["api"];
    assert_eq!(&mounts["worker"].1, shared);
    assert_kept(shared, user, 0o444, home.path())?;
    assert_eq!(api["mode"].as_u64(), Some(0o444));
    assert_eq!(api["target"].as_str(), Some("database-password"));

    // A service that asks for another mode gets a copy with it, which
    // carries its mode itself: the mount names no owner or mode.
    let (db, copy) = &mounts["db"];
    assert_ne!(copy, shared);
    assert_kept(copy, user, 0o400, home.path())?;
    assert!(
        db["uid"].is_null() && db["gid"].is_null() && db["mode"].is_null(),
        "{db:?}"
    );
    assert_ne!(db["source"], api["source"]);
    assert_eq!(kept_files(&run)?.len(), 2);

    // The plan names the files that belong to the user and not to the ids
    // their entries name: the service's own user may be refused only for
    // the copy that keeps other users out.
    let notes = plan.kept_for(user);
    let named = notes
        .iter()
        .map(|note| (note.service.as_str(), note.readable_by_all()))
        .collect::<Vec<_>>();
    assert_eq!(named, [("db", false), ("worker", true)]);
    let line = notes[0].line();
    assert!(line.contains("lets no other user read it"), "{line}");
    assert!(!line.contains(VALUE));
    driver.dispose(&prepared)?;
    Ok(())
}

#[tokio::test]
async fn the_private_file_an_older_release_kept_becomes_readable_under_the_same_secret()
-> TestResult {
    let home = temp_dirs::tempdir()?;
    let run = home.path().join("run");
    let driver = DockerComposeFileSecretDelivery::kept_for(run.clone(), Some(this_user()))?;
    let plan = plan(vec![consumer("api", "0", 0o444)])?;
    let first = driver
        .prepare(OperationId::new(), &plan, material()?)
        .await?;
    let before = fs::read_to_string(&first.overlays[0].1)?;
    let (_, file) = mounts(&first.overlays[0].1)?
        .remove("api")
        .ok_or("api mounts nothing")?;
    driver.dispose(&first)?;
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600))?;

    // The next deployment gives the same file the default mode, and the
    // services keep the same definition: none of them is replaced for it.
    let second = driver
        .prepare(OperationId::new(), &plan, material()?)
        .await?;
    assert_eq!(fs::read_to_string(&second.overlays[0].1)?, before);
    assert_eq!(fs::metadata(&file)?.permissions().mode() & 0o777, 0o444);
    assert_eq!(kept_files(&run)?, [file]);
    driver.dispose(&second)?;
    Ok(())
}

#[tokio::test]
async fn where_mm_runs_as_root_each_copy_belongs_to_the_ids_its_entry_names() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let run = home.path().join("run");
    let driver = DockerComposeFileSecretDelivery::kept_for(run.clone(), None)?;
    assert_eq!(driver.capability().files_kept_for, None);
    assert!(driver.capability().supports_uid_gid_mode);

    // An id no host has is refused before anything is written.
    let impossible = plan(vec![consumer("api", "4294967296", 0o444)])?;
    let error = driver
        .prepare(OperationId::new(), &impossible, material()?)
        .await
        .err()
        .ok_or("an id beyond the host's was accepted")?
        .to_string();
    assert!(
        error.contains(
            "secret file database-password of service 'api': uid 4294967296 is no id of this host"
        ),
        "{error}"
    );
    assert!(kept_files(&run)?.is_empty());

    let plan = plan(vec![
        consumer("api", "0", 0o444),
        consumer("db", "4321", 0o440),
    ])?;
    let prepared = driver.prepare(OperationId::new(), &plan, material()?).await;
    if !rustix::process::geteuid().is_root() {
        let error = prepared.err().ok_or("a file was given to another user")?;
        assert!(
            error.to_string().contains("giving a runtime secret file"),
            "{error}"
        );
        return Ok(());
    }
    let prepared = prepared?;
    let mounts = mounts(&prepared.overlays[0].1)?;
    assert_kept(
        &mounts["api"].1,
        HostIds { uid: 0, gid: 0 },
        0o444,
        home.path(),
    )?;
    assert_kept(
        &mounts["db"].1,
        HostIds {
            uid: 4321,
            gid: 4321,
        },
        0o440,
        home.path(),
    )?;
    assert!(
        plan.kept_for(this_user())
            .iter()
            .all(|note| note.service == "db")
    );
    driver.dispose(&prepared)?;
    Ok(())
}

#[test]
fn a_note_says_what_a_file_kept_for_you_leaves_out() -> TestResult {
    let user = HostIds {
        uid: 1000,
        gid: 1000,
    };
    assert!(SecretFileOwnerNote::of(&consumer("api", "0", 0o444), user).is_none());
    assert!(SecretFileOwnerNote::of(&consumer("api", "1000", 0o400), user).is_none());

    let root_only = SecretFileOwnerNote::of(&consumer("api", "0", 0o400), user)
        .ok_or("a root-only file kept for another user says nothing")?;
    assert!(!root_only.readable_by_all());
    assert_eq!(
        root_only.line(),
        "secret file database-password of service 'api' belongs to uid 1000 and gid 1000, the user mm runs as on this machine, not to uid 0 and gid 0: only root can give a file to other ids, and its mode 0400 lets no other user read it, so the service's user may be refused. Use mode 0444, the default: every user in the container can read the file, and other users of this machine still cannot"
    );
    let named = SecretFileOwnerNote::of(&consumer("worker", "999", 0o444), user)
        .ok_or("a file kept for another user than it names says nothing")?;
    assert!(named.readable_by_all());
    assert!(
        named
            .line()
            .ends_with("Its mode 0444 lets every user in the container read it"),
        "{}",
        named.line()
    );

    let config: MmConfig = serde_yaml_ng::from_str(&format!(
        "schema-version: 1
project: {{id: {PROJECT_ID}, name: local-files}}
target: local
stacks: {{app: {{files: [compose.yaml]}}}}
modules:
  api:
    path: api
    services:
      api:
        stack: app
        secret-files:
          plain: {{secret: database.password}}
          key: {{secret: database.password, uid: \"999\", gid: \"999\", mode: 0o400}}
          staging-only: {{secret: database.password, mode: 0o400, environments: [staging]}}
environments:
  local: {{bindings: {{}}}}
  staging: {{bindings: {{}}}}
"
    ))?;
    let local = SecretFileOwnerNote::in_config(&config, "local", user);
    assert_eq!(
        local
            .iter()
            .map(|note| note.file.as_str())
            .collect::<Vec<_>>(),
        ["key"]
    );
    let staging = SecretFileOwnerNote::in_config(&config, "staging", user);
    assert_eq!(staging.len(), 2);
    Ok(())
}

#[test]
fn config_validate_names_a_local_docker_secret_file_that_belongs_to_you() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let project = temporary.path().join("project");
    fs::create_dir_all(project.join(".config"))?;
    fs::create_dir_all(project.join("api"))?;
    fs::write(
        project.join("api/.env.example"),
        "DB_PASSWORD=${DB.PASSWORD}\n",
    )?;
    fs::write(
        project.join("compose.yaml"),
        "services:\n  api:\n    image: docker.io/library/busybox:1.37\n",
    )?;
    fs::write(
        project.join(".config/mm.yaml"),
        format!(
            "schema-version: 1
default-environment: local
project: {{id: {PROJECT_ID}, name: local-files, development-environment: local}}
target: local
runtime: {{driver: docker-compose}}
stacks: {{app: {{files: [compose.yaml]}}}}
modules:
  api:
    path: api
    env: {{template: .env.example}}
    services:
      api:
        stack: app
        secret-files:
          database-password: {{secret: database.password, uid: \"999\", gid: \"999\", mode: 0o400}}
secrets:
  database.password:
    storage: {{provider: age, location: local/db-password}}
environments:
  local:
    bindings:
      DB.PASSWORD: {{secret: database.password}}
"
        ),
    )?;
    let validation = application(&temporary)?
        .validate(&WorkspaceSelection::new(&project, Some("local".to_owned())))?;
    assert_eq!(validation.runtime_kind, Some(RuntimeKind::DockerCompose));
    let found = validation
        .validation
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "MM_SECRET_FILE_OWNER")
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>();
    match HostIds::unprivileged() {
        Some(user) => {
            assert_eq!(found.len(), 1, "{found:?}");
            assert!(
                found[0].contains(&format!(
                    "secret file database-password of service 'api' belongs to uid {} and gid {}",
                    user.uid, user.gid
                )),
                "{}",
                found[0]
            );
        }
        None => assert!(found.is_empty(), "{found:?}"),
    }
    Ok(())
}

fn application(temporary: &tempfile::TempDir) -> Result<Application, Box<dyn std::error::Error>> {
    let state = Arc::new(SqliteStateStore::open(
        &temporary.path().join("state.sqlite3"),
    )?);
    let targets: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(
        temporary.path().join("targets.yaml"),
    ));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(uuid::Uuid::parse_str(TARGET_ID)?)),
    )?;
    let documents = Arc::new(ConfigLoader);
    let route_registry: Arc<dyn IngressRouteRegistry> = state.clone();
    let stacks = StackManager::with_route_registry(
        documents.clone(),
        Arc::new(FilesystemStackArtifactStore::new(
            temporary.path().join("compose"),
        )),
        route_registry,
        targets,
    );
    Ok(Application::new(ApplicationDependencies {
        documents,
        environment_sources: Arc::new(EnvironmentIndexer),
        runtime: Arc::new(FakeRuntime {
            probe: RuntimeProbe {
                kind: RuntimeKind::DockerCompose,
                available: false,
                engine_version: None,
                compose_provider: None,
                rootless: None,
                socket_path: None,
                capabilities: Vec::new(),
                diagnostics: Vec::new(),
            },
            user_rendered: None,
            rendered: BTreeMap::new(),
            observed: BTreeMap::new(),
        }),
        state,
        stacks,
        clock: Arc::new(FixedClock { unix_ms: 1_000 }),
    }))
}
