#![cfg(unix)]

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm::adapters::secrets::{DockerComposeFileSecretDelivery, PodmanSecretDelivery};
use mm::app::ports::RuntimeSecretDelivery;
use mm::domain::operation::OperationId;
use mm::domain::runtime::RuntimeKind;
use mm::domain::runtime_secret::{
    HostIds, RuntimeSecretConsumer, RuntimeSecretMaterial, RuntimeSecretPlan,
    RuntimeSecretPlanItem, RuntimeSecretStoreKind,
};
use mm::domain::secret::SecretMaterial;
use mm::domain::secret_contract::DeliveryDriverId;

mod temp_dirs;

const SENTINEL: &str = "mm-compose-secret-sentinel-f938de";

fn this_user() -> HostIds {
    HostIds {
        uid: rustix::process::geteuid().as_raw(),
        gid: rustix::process::getegid().as_raw(),
    }
}

#[test]
fn capability_matrix_is_exact() -> Result<(), Box<dyn std::error::Error>> {
    struct NoProcess;
    #[async_trait::async_trait]
    impl mm::app::ports::ProcessRunner for NoProcess {
        async fn run(
            &self,
            _specification: mm::app::ports::CommandSpec,
        ) -> Result<mm::app::ports::CommandOutput, mm::domain::error::MmError> {
            Err(mm::domain::error::MmError::RuntimeResponse {
                message: "unexpected process".to_owned(),
            })
        }
    }
    let home = temp_dirs::tempdir()?;
    let process: Arc<dyn mm::app::ports::ProcessRunner> = Arc::new(NoProcess);
    let podman = PodmanSecretDelivery::new(process, home.path().join("podman"))?;
    let compose =
        DockerComposeFileSecretDelivery::kept_for(home.path().join("compose"), Some(this_user()))?;
    let as_root = DockerComposeFileSecretDelivery::kept_for(home.path().join("root"), None)?;

    assert_eq!(podman.capability().runtime, RuntimeKind::PodmanCompose);
    assert_eq!(podman.capability().store, RuntimeSecretStoreKind::Podman);
    assert!(podman.capability().native_storage);
    assert!(podman.capability().immutable_revisions);
    assert!(podman.capability().supports_uid_gid_mode);
    assert_eq!(compose.capability().runtime, RuntimeKind::DockerCompose);
    assert_eq!(compose.capability().store, RuntimeSecretStoreKind::None);
    assert!(!compose.capability().native_storage);
    assert!(compose.capability().immutable_revisions);
    assert!(!compose.capability().supports_uid_gid_mode);
    assert_eq!(compose.capability().files_kept_for, Some(this_user()));
    assert!(compose.capability().plaintext_file_lifetime.is_some());
    assert!(as_root.capability().supports_uid_gid_mode);
    assert_eq!(as_root.capability().files_kept_for, None);
    assert_eq!(
        DockerComposeFileSecretDelivery::new(home.path().join("default"))?
            .capability()
            .files_kept_for,
        HostIds::unprivileged()
    );
    Ok(())
}

/// Every regular file under `root`.
fn files_under(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    Ok(files)
}

/// The revision files a driver on `run` keeps: everything beside the
/// operation directories but the key that names them.
fn revision_files(run: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    Ok(files_under(&run.with_file_name("run-files"))?
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name != "name.key"))
        .collect())
}

fn overlay(path: &Path) -> Result<serde_yaml_ng::Value, Box<dyn std::error::Error>> {
    Ok(serde_yaml_ng::from_str(&fs::read_to_string(path)?)?)
}

/// The key of the Compose secret `service` mounts, and the file behind it.
fn mounted(
    overlay: &serde_yaml_ng::Value,
    service: &str,
) -> Result<(String, PathBuf), Box<dyn std::error::Error>> {
    let key = overlay["services"][service]["secrets"][0]["source"]
        .as_str()
        .ok_or("the service mounts no secret")?
        .to_owned();
    let file = overlay["secrets"][key.as_str()]["file"]
        .as_str()
        .ok_or("the secret has no file")?;
    Ok((key, PathBuf::from(file)))
}

fn material(logical: &str, value: &str) -> Result<RuntimeSecretMaterial, String> {
    Ok(RuntimeSecretMaterial {
        logical_name: logical.to_owned(),
        material: SecretMaterial::new(value.to_owned())?,
    })
}

/// `database.password` for `api` and `cache.token` for `worker`.
fn two_secrets(
    database: (&str, Option<&str>),
    cache: &str,
) -> Result<RuntimeSecretPlan, Box<dyn std::error::Error>> {
    let item = |logical: &str, service: &str, revision: &str, previous: Option<&str>| {
        Ok::<_, Box<dyn std::error::Error>>(RuntimeSecretPlanItem {
            logical_name: logical.to_owned(),
            backend: "team-secrets".to_owned(),
            delivery: DeliveryDriverId::new("compose-file-secret")?,
            source_revision: revision.to_owned(),
            previous_source_revision: previous.map(str::to_owned),
            consumers: vec![RuntimeSecretConsumer {
                stack: "app".to_owned(),
                service: service.to_owned(),
                target: logical.replace('.', "-"),
                uid: "0".to_owned(),
                gid: "0".to_owned(),
                mode: 0o444,
            }],
        })
    };
    Ok(RuntimeSecretPlan::new(
        "019c527b-6242-7b43-bfd4-bd60e27c1de3".to_owned(),
        "319c527b-6242-7b43-bfd4-bd60e27c1de3".to_owned(),
        "integration".to_owned(),
        RuntimeKind::DockerCompose,
        vec![
            item("database.password", "api", database.0, database.1)?,
            item("cache.token", "worker", cache, None)?,
        ],
    )?)
}

#[tokio::test]
async fn compose_file_delivery_keeps_one_file_per_revision_in_a_private_directory()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let run = home.path().join("run");
    let driver = DockerComposeFileSecretDelivery::kept_for(run.clone(), Some(this_user()))?;
    let plan = plan(
        RuntimeKind::DockerCompose,
        "compose-file-secret",
        "revision-1",
    )?;
    let prepared = driver
        .prepare(
            OperationId::new(),
            &plan,
            vec![material("database.password", SENTINEL)?],
        )
        .await?;

    assert_eq!(driver.capability().store, RuntimeSecretStoreKind::None);
    assert!(!driver.capability().native_storage);
    assert!(!driver.capability().supports_uid_gid_mode);
    let rendered = overlay(&prepared.overlays[0].1)?;
    let (key, file) = mounted(&rendered, "api")?;
    assert_eq!(revision_files(&run)?, std::slice::from_ref(&file));
    assert_eq!(fs::metadata(&file)?.permissions().mode() & 0o777, 0o444);
    assert_eq!(fs::read_to_string(&file)?, SENTINEL);
    let mut directory = file.parent();
    while let Some(parent) = directory.filter(|parent| *parent != home.path()) {
        assert_eq!(fs::metadata(parent)?.permissions().mode() & 0o777, 0o700);
        directory = parent.parent();
    }
    assert_eq!(
        file.file_name().and_then(|name| name.to_str()),
        Some(prepared.secrets[0].runtime_name.as_str())
    );
    // Nothing in a name or in the overlay says what the value or the
    // store's revision is, and the plan names no file.
    let text = fs::read_to_string(&prepared.overlays[0].1)?;
    let plain_digest = blake3::hash(b"revision-1").to_hex().to_string();
    for forbidden in [SENTINEL, "revision-1", &plain_digest[..12]] {
        assert!(!text.contains(forbidden), "overlay holds {forbidden}");
        assert!(!key.contains(forbidden));
        for path in files_under(home.path())? {
            assert!(!path.to_string_lossy().contains(forbidden));
        }
    }
    assert!(text.contains("target: database-password"));
    let serialized = serde_json::to_string(&plan)?;
    assert!(!serialized.contains(SENTINEL));
    assert!(!serialized.contains(&prepared.secrets[0].runtime_name));

    // The operation's overlays go; the file a container mounts stays.
    driver.dispose(&prepared)?;
    assert!(!prepared.root.exists());
    assert_eq!(fs::read_to_string(&file)?, SENTINEL);

    // The same revision again is the same file, also for a driver started
    // later; the same revision with other content is refused.
    let later = DockerComposeFileSecretDelivery::kept_for(run.clone(), Some(this_user()))?;
    let again = later
        .prepare(
            OperationId::new(),
            &plan,
            vec![material("database.password", SENTINEL)?],
        )
        .await?;
    assert_eq!(
        mounted(&overlay(&again.overlays[0].1)?, "api")?,
        (key, file)
    );
    later.dispose(&again)?;
    let refused = later
        .prepare(
            OperationId::new(),
            &plan,
            vec![material("database.password", "another value")?],
        )
        .await
        .err()
        .ok_or("a kept revision was overwritten")?
        .to_string();
    assert!(refused.contains("write a new version"), "{refused}");
    assert!(!refused.contains("another value") && !refused.contains(SENTINEL));
    assert_eq!(revision_files(&run)?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_rotated_file_secret_changes_only_its_consumers_and_keeps_the_old_revision_for_rollback()
-> Result<(), Box<dyn std::error::Error>> {
    const ROTATED: &str = "mm-compose-secret-rotated-41c07a";
    let home = temp_dirs::tempdir()?;
    let run = home.path().join("run");
    let driver = DockerComposeFileSecretDelivery::new(run.clone())?;
    let initial = two_secrets(("revision-1", None), "revision-7")?;
    let first = driver
        .prepare(
            OperationId::new(),
            &initial,
            vec![
                material("database.password", SENTINEL)?,
                material("cache.token", SENTINEL)?,
            ],
        )
        .await?;
    let before = overlay(&first.overlays[0].1)?;
    // With nothing to return to, a rollback mounts what the deployment does.
    assert_eq!(overlay(&first.rollback_overlays[0].1)?, before);
    driver.dispose(&first)?;

    let rotated = two_secrets(("revision-2", Some("revision-1")), "revision-7")?;
    let second = driver
        .prepare(
            OperationId::new(),
            &rotated,
            vec![
                material("database.password", ROTATED)?,
                material("cache.token", SENTINEL)?,
            ],
        )
        .await?;
    let after = overlay(&second.overlays[0].1)?;
    let (old_key, old_file) = mounted(&before, "api")?;
    let (new_key, new_file) = mounted(&after, "api")?;
    // The service that uses the rotated secret has another definition...
    assert_ne!(old_key, new_key);
    assert_ne!(old_file, new_file);
    assert_ne!(before["services"]["api"], after["services"]["api"]);
    assert_eq!(fs::read_to_string(&new_file)?, ROTATED);
    // ...the one that does not has the same, down to the file.
    assert_eq!(before["services"]["worker"], after["services"]["worker"]);
    assert_eq!(mounted(&before, "worker")?, mounted(&after, "worker")?);
    // The old revision is still there, and the rollback overlay is the
    // definition the services had before.
    assert_eq!(fs::read_to_string(&old_file)?, SENTINEL);
    assert_eq!(overlay(&second.rollback_overlays[0].1)?, before);
    assert_eq!(revision_files(&run)?.len(), 3);

    // Before the deployment succeeded both revisions are retained.
    let both = BTreeMap::from([
        (
            "database.password".to_owned(),
            BTreeSet::from(["revision-1".to_owned(), "revision-2".to_owned()]),
        ),
        (
            "cache.token".to_owned(),
            BTreeSet::from(["revision-7".to_owned()]),
        ),
    ]);
    let active = HashSet::from([second.operation_id]);
    assert!(driver.reconcile(&rotated, &both, &active).await?.is_empty());
    assert_eq!(revision_files(&run)?.len(), 3);
    driver.dispose(&second)?;

    // After it succeeded only what the plan uses is: the old file goes.
    let current = BTreeMap::from([
        (
            "database.password".to_owned(),
            BTreeSet::from(["revision-2".to_owned()]),
        ),
        (
            "cache.token".to_owned(),
            BTreeSet::from(["revision-7".to_owned()]),
        ),
    ]);
    let removed = driver
        .reconcile(&rotated, &current, &HashSet::new())
        .await?;
    assert_eq!(
        removed,
        [old_file
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("the old revision has no name")?
            .to_owned()]
    );
    assert!(!old_file.exists());
    assert_eq!(fs::read_to_string(&new_file)?, ROTATED);
    assert_eq!(revision_files(&run)?.len(), 2);

    // Another environment's files are not this plan's to remove.
    let mut elsewhere = two_secrets(("revision-2", None), "revision-7")?;
    elsewhere.environment = "staging".to_owned();
    elsewhere.plan_hash = elsewhere.canonical_hash()?;
    assert!(
        driver
            .reconcile(&elsewhere, &BTreeMap::new(), &HashSet::new())
            .await?
            .is_empty()
    );
    assert_eq!(revision_files(&run)?.len(), 2);

    // A previous revision an older release delivered is not on the host:
    // the rotation still applies, and a rollback keeps the new revision.
    let fresh = home.path().join("fresh").join("run");
    fs::create_dir_all(home.path().join("fresh"))?;
    let upgraded = DockerComposeFileSecretDelivery::new(fresh)?;
    let third = upgraded
        .prepare(
            OperationId::new(),
            &rotated,
            vec![
                material("database.password", ROTATED)?,
                material("cache.token", SENTINEL)?,
            ],
        )
        .await?;
    assert_eq!(
        overlay(&third.rollback_overlays[0].1)?,
        overlay(&third.overlays[0].1)?
    );
    Ok(())
}

#[tokio::test]
async fn compose_failure_cleanup_recovery_and_impossible_owners_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let run = home.path().join("run");
    let driver = DockerComposeFileSecretDelivery::kept_for(run.clone(), None)?;
    let mut unsupported = plan(
        RuntimeKind::DockerCompose,
        "compose-file-secret",
        "revision-1",
    )?;
    unsupported.items[0].consumers[0].uid = "4294967296".to_owned();
    unsupported.plan_hash = unsupported.canonical_hash()?;
    let result = driver
        .prepare(
            OperationId::new(),
            &unsupported,
            vec![material("database.password", SENTINEL)?],
        )
        .await;
    assert!(result.is_err());
    if run.exists() {
        assert_eq!(fs::read_dir(&run)?.count(), 0);
    }
    assert!(revision_files(&run)?.is_empty());

    // An operation that never finished leaves its overlays and the file of
    // a revision no deployment uses: both go when nothing retains them.
    let plan = plan(
        RuntimeKind::DockerCompose,
        "compose-file-secret",
        "revision-2",
    )?;
    let orphan = driver
        .prepare(
            OperationId::new(),
            &plan,
            vec![material("database.password", SENTINEL)?],
        )
        .await?;
    let removed = driver
        .reconcile(&plan, &BTreeMap::new(), &HashSet::new())
        .await?;
    assert_eq!(
        removed,
        [
            orphan.operation_id.0.to_string(),
            orphan.secrets[0].runtime_name.clone()
        ]
    );
    assert!(!orphan.root.exists());
    assert!(revision_files(&run)?.is_empty());
    Ok(())
}

#[test]
fn canonical_plan_covers_revision_consumers_and_rejects_duplicates()
-> Result<(), Box<dyn std::error::Error>> {
    let first = plan(
        RuntimeKind::PodmanCompose,
        "podman-native-secret",
        "revision-1",
    )?;
    let second = plan(
        RuntimeKind::PodmanCompose,
        "podman-native-secret",
        "revision-2",
    )?;
    assert_ne!(first.plan_hash, second.plan_hash);
    assert_eq!(first.plan_hash, first.canonical_hash()?);
    let state = temp_dirs::tempdir()?;
    let database = state.path().join("state.db");
    let connection = rusqlite::Connection::open(&database)?;
    connection.execute("CREATE TABLE plans (plan_json TEXT NOT NULL)", [])?;
    connection.execute(
        "INSERT INTO plans (plan_json) VALUES (?1)",
        [serde_json::to_string(&first)?],
    )?;
    drop(connection);
    assert!(
        !fs::read(database)?
            .windows(SENTINEL.len())
            .any(|bytes| bytes == SENTINEL.as_bytes())
    );

    let mut duplicate = first.items.clone();
    duplicate.push(duplicate[0].clone());
    assert!(
        RuntimeSecretPlan::new(
            first.project_id,
            first.target_id,
            first.environment,
            first.runtime,
            duplicate,
        )
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn mismatched_runtime_fails_before_creating_files() -> Result<(), Box<dyn std::error::Error>>
{
    let home = temp_dirs::tempdir()?;
    let driver = DockerComposeFileSecretDelivery::new(home.path().join("run"))?;
    let plan = plan(
        RuntimeKind::PodmanCompose,
        "podman-native-secret",
        "revision-1",
    )?;
    assert!(
        driver
            .prepare(
                OperationId::new(),
                &plan,
                vec![RuntimeSecretMaterial {
                    logical_name: "database.password".to_owned(),
                    material: SecretMaterial::new(SENTINEL.to_owned())?,
                }],
            )
            .await
            .is_err()
    );
    assert!(!home.path().join("run").exists());
    Ok(())
}

fn plan(
    runtime: RuntimeKind,
    driver: &str,
    revision: &str,
) -> Result<RuntimeSecretPlan, Box<dyn std::error::Error>> {
    Ok(RuntimeSecretPlan::new(
        "019c527b-6242-7b43-bfd4-bd60e27c1de3".to_owned(),
        "319c527b-6242-7b43-bfd4-bd60e27c1de3".to_owned(),
        "integration".to_owned(),
        runtime,
        vec![RuntimeSecretPlanItem {
            logical_name: "database.password".to_owned(),
            backend: "age".to_owned(),
            delivery: DeliveryDriverId::new(driver)?,
            source_revision: revision.to_owned(),
            previous_source_revision: None,
            consumers: vec![RuntimeSecretConsumer {
                stack: "app".to_owned(),
                service: "api".to_owned(),
                target: "database-password".to_owned(),
                uid: "0".to_owned(),
                gid: "0".to_owned(),
                mode: 0o444,
            }],
        }],
    )?)
}
