    use std::collections::{BTreeMap, BTreeSet, HashSet};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use zeroize::Zeroizing;

    use super::*;
    use crate::app::ports::{CommandOutput, CommandSpec, ProcessRunner, RuntimeSecretDelivery};
    use crate::domain::error::MmError;
    use crate::domain::operation::OperationId;
    use crate::domain::runtime::RuntimeKind;
    use crate::domain::runtime_secret::{
        RuntimeSecretConsumer, RuntimeSecretMaterial, RuntimeSecretPlan, RuntimeSecretPlanItem,
    };
    use crate::domain::secret::SecretMaterial;
    use crate::domain::secret_contract::DeliveryDriverId;

    const SENTINEL: &str = "mm-native-secret-sentinel-05e342";

    #[derive(Debug, Default)]
    struct MemoryEngine {
        secrets: Mutex<BTreeMap<String, BTreeMap<String, String>>>,
        records: Mutex<Vec<crate::app::ports::RedactedCommandRecord>>,
        create_count: Mutex<usize>,
    }

    impl MemoryEngine {
        fn output(code: i32, stdout: String, stderr: String) -> CommandOutput {
            CommandOutput {
                code: Some(code),
                stdout: Zeroizing::new(stdout),
                stderr: Zeroizing::new(stderr),
                truncated: false,
            }
        }
    }

    #[async_trait]
    impl ProcessRunner for MemoryEngine {
        async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
            let args = specification
                .arguments()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            self.records
                .lock()
                .map_err(lock_error)?
                .push(specification.redacted_record());
            match args.as_slice() {
                [secret, inspect, format, json, name]
                    if secret == "secret"
                        && inspect == "inspect"
                        && format == "--format"
                        && json == "json" =>
                {
                    let secrets = self.secrets.lock().map_err(lock_error)?;
                    let Some(labels) = secrets.get(name) else {
                        return Ok(Self::output(1, String::new(), "no such secret".to_owned()));
                    };
                    let value = serde_json::json!([{
                        "CreatedAt": "2026-08-23T12:34:56.789Z",
                        "Spec": {"Name": name, "Labels": labels}
                    }]);
                    Ok(Self::output(0, value.to_string(), String::new()))
                }
                [secret, create, rest @ ..] if secret == "secret" && create == "create" => {
                    let name = rest
                        .get(rest.len().saturating_sub(2))
                        .ok_or_else(|| MmError::RuntimeResponse {
                            message: "mock create omitted name".to_owned(),
                        })?
                        .clone();
                    let mut labels = BTreeMap::new();
                    let mut index = 0;
                    while index + 1 < rest.len().saturating_sub(2) {
                        if rest[index] != "--label" {
                            return Err(MmError::RuntimeResponse {
                                message: "mock create received an unsafe option".to_owned(),
                            });
                        }
                        let (key, value) = rest[index + 1].split_once('=').ok_or_else(|| {
                            MmError::RuntimeResponse {
                                message: "mock create received an invalid label".to_owned(),
                            }
                        })?;
                        labels.insert(key.to_owned(), value.to_owned());
                        index += 2;
                    }
                    let mut secrets = self.secrets.lock().map_err(lock_error)?;
                    if secrets.insert(name.clone(), labels).is_some() {
                        return Ok(Self::output(
                            125,
                            String::new(),
                            "secret already exists".to_owned(),
                        ));
                    }
                    *self.create_count.lock().map_err(lock_error)? += 1;
                    Ok(Self::output(0, name, String::new()))
                }
                [secret, remove, name] if secret == "secret" && remove == "rm" => {
                    self.secrets.lock().map_err(lock_error)?.remove(name);
                    Ok(Self::output(0, name.clone(), String::new()))
                }
                [secret, list, rest @ ..] if secret == "secret" && list == "ls" => {
                    if rest.last().map(String::as_str) != Some("{{.Name}}") {
                        return Err(MmError::RuntimeResponse {
                            message: "mock list omitted fixed format".to_owned(),
                        });
                    }
                    let names = self
                        .secrets
                        .lock()
                        .map_err(lock_error)?
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(Self::output(0, names, String::new()))
                }
                _ => Err(MmError::RuntimeResponse {
                    message: format!("unexpected mock command: {args:?}"),
                }),
            }
        }
    }

    #[tokio::test]
    async fn native_create_rotate_rollback_retention_remove_and_redaction()
    -> Result<(), Box<dyn std::error::Error>> {
        let home = crate::test_support::tempdir()?;
        let engine = Arc::new(MemoryEngine::default());
        let process: Arc<dyn ProcessRunner> = engine.clone();
        let driver = PodmanSecretDelivery::new(process, home.path().join("run"))?;
        let initial = plan("revision-1")?;
        let first = driver
            .prepare(OperationId::new(), &initial, materials()?)
            .await?;
        assert_eq!(*engine.create_count.lock().map_err(lock_error)?, 1);
        assert_eq!(engine.secrets.lock().map_err(lock_error)?.len(), 1);
        assert!(first.secrets[0].created_at_unix_ms.is_some());
        assert!(!fs_contains(&first.root, SENTINEL)?);

        let rotated = plan("revision-2")?;
        let second = driver
            .prepare(OperationId::new(), &rotated, materials()?)
            .await?;
        assert_eq!(*engine.create_count.lock().map_err(lock_error)?, 2);
        assert_eq!(engine.secrets.lock().map_err(lock_error)?.len(), 2);
        assert_ne!(
            first.secrets[0].runtime_name,
            second.secrets[0].runtime_name
        );

        // A failed workload convergence keeps both immutable revisions so the
        // caller can redeploy the previous overlay without recreating material.
        driver.dispose(&second)?;
        assert_eq!(engine.secrets.lock().map_err(lock_error)?.len(), 2);
        assert!(
            driver
                .remove_revision(&rotated, "database.password", "revision-1")
                .await?
        );
        assert_eq!(engine.secrets.lock().map_err(lock_error)?.len(), 1);
        driver.dispose(&first)?;

        let records = serde_json::to_string(&*engine.records.lock().map_err(lock_error)?)?;
        assert!(!records.contains(SENTINEL));
        assert!(!format!("{first:?}").contains(SENTINEL));
        Ok(())
    }

    #[tokio::test]
    async fn foreign_collision_and_reconciliation_are_ownership_safe()
    -> Result<(), Box<dyn std::error::Error>> {
        let home = crate::test_support::tempdir()?;
        let engine = Arc::new(MemoryEngine::default());
        let process: Arc<dyn ProcessRunner> = engine.clone();
        let driver = PodmanSecretDelivery::new(process, home.path().join("run"))?;
        let plan = plan("revision-1")?;
        let collision = runtime_name(&plan, "database.password", "revision-1");
        engine
            .secrets
            .lock()
            .map_err(lock_error)?
            .insert(collision, BTreeMap::new());
        assert!(
            driver
                .prepare(OperationId::new(), &plan, materials()?)
                .await
                .is_err()
        );
        assert_eq!(*engine.create_count.lock().map_err(lock_error)?, 0);

        engine.secrets.lock().map_err(lock_error)?.clear();
        let prepared = driver
            .prepare(OperationId::new(), &plan, materials()?)
            .await?;
        let retained = BTreeMap::from([(
            "database.password".to_owned(),
            BTreeSet::from(["revision-1".to_owned()]),
        )]);
        assert!(
            driver
                .reconcile(&plan, &retained, &HashSet::new())
                .await?
                .is_empty()
        );
        driver.dispose(&prepared)?;
        assert_eq!(
            driver
                .reconcile(&plan, &BTreeMap::new(), &HashSet::new())
                .await?
                .len(),
            1
        );
        assert!(engine.secrets.lock().map_err(lock_error)?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_same_revision_creation_has_one_winner()
    -> Result<(), Box<dyn std::error::Error>> {
        let home = crate::test_support::tempdir()?;
        let engine = Arc::new(MemoryEngine::default());
        let process: Arc<dyn ProcessRunner> = engine.clone();
        let driver = PodmanSecretDelivery::new(process, home.path().join("run"))?;
        let plan = plan("revision-cas")?;
        let left = driver.prepare(OperationId::new(), &plan, materials()?);
        let right = driver.prepare(OperationId::new(), &plan, materials()?);
        let (left, right) = tokio::join!(left, right);
        assert!(left.is_ok() || right.is_ok());
        assert_eq!(*engine.create_count.lock().map_err(lock_error)?, 1);
        assert_eq!(engine.secrets.lock().map_err(lock_error)?.len(), 1);
        if let Ok(prepared) = left {
            driver.dispose(&prepared)?;
        }
        if let Ok(prepared) = right {
            driver.dispose(&prepared)?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn missing_rollback_revision_removes_new_candidate()
    -> Result<(), Box<dyn std::error::Error>> {
        let home = crate::test_support::tempdir()?;
        let engine = Arc::new(MemoryEngine::default());
        let process: Arc<dyn ProcessRunner> = engine.clone();
        let driver = PodmanSecretDelivery::new(process, home.path().join("run"))?;

        assert!(
            driver
                .prepare(OperationId::new(), &plan("revision-2")?, materials()?)
                .await
                .is_err()
        );
        assert_eq!(*engine.create_count.lock().map_err(lock_error)?, 1);
        assert!(engine.secrets.lock().map_err(lock_error)?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn rollback_overlay_keeps_unchanged_secrets() -> Result<(), Box<dyn std::error::Error>> {
        let home = crate::test_support::tempdir()?;
        let engine = Arc::new(MemoryEngine::default());
        let process: Arc<dyn ProcessRunner> = engine;
        let driver = PodmanSecretDelivery::new(process, home.path().join("run"))?;
        let initial = two_secret_plan(false)?;
        let first = driver
            .prepare(OperationId::new(), &initial, two_materials()?)
            .await?;
        let rotated = two_secret_plan(true)?;
        let second = driver
            .prepare(OperationId::new(), &rotated, two_materials()?)
            .await?;

        let yaml: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(&std::fs::read_to_string(&second.rollback_overlays[0].1)?)?;
        assert_eq!(
            yaml["services"]["api"]["secrets"]
                .as_sequence()
                .map(Vec::len),
            Some(2)
        );
        driver.dispose(&second)?;
        driver.dispose(&first)?;
        Ok(())
    }

    #[test]
    fn artifact_helpers_cover_private_roots_overlays_disposal_and_reconciliation()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = crate::test_support::tempdir()?;
        let run = temp.path().join("run");
        let operation_id = OperationId::new();
        let operation = operation_root(&run, operation_id)?;
        assert!(operation_root(&run, operation_id).is_err());

        let secret_plan = plan("revision-1")?;
        let prepared = vec![prepared_item(
            &secret_plan.items[0],
            "runtime-secret".to_owned(),
            Some(123),
        )];
        let overlays = render_overlays(&operation, &prepared, true)?;
        assert_eq!(overlays.len(), 1);
        let compose_root = operation.join("compose");
        create_private_directory(&compose_root)?;
        let compose_overlays =
            render_compose_overlays(&compose_root, &prepared, Path::new("/run/secrets"))?;
        assert_eq!(compose_overlays.len(), 1);
        assert_ne!(overlay_key("a"), overlay_key("b"));

        let duplicate = operation.join("duplicate");
        write_private_file(&duplicate, b"one")?;
        assert!(write_private_file(&duplicate, b"two").is_err());
        let prepared_operation = crate::domain::runtime_secret::PreparedRuntimeSecrets {
            operation_id,
            plan_hash: secret_plan.plan_hash.clone(),
            root: operation.clone(),
            overlays,
            rollback_overlays: compose_overlays,
            secrets: prepared,
        };
        let malformed = crate::domain::runtime_secret::PreparedRuntimeSecrets {
            root: temp.path().join("outside"),
            ..prepared_operation.clone()
        };
        assert!(dispose_operation(&run, &malformed).is_err());
        dispose_operation(&run, &prepared_operation)?;
        dispose_operation(&run, &prepared_operation)?;

        let missing = temp.path().join("missing-run");
        assert!(reconcile_operation_files(&missing, &HashSet::new())?.is_empty());
        create_private_directory(&run)?;
        std::fs::write(run.join("not-a-directory"), b"invalid")?;
        assert!(reconcile_operation_files(&run, &HashSet::new()).is_err());
        std::fs::remove_file(run.join("not-a-directory"))?;

        std::fs::create_dir(run.join("invalid-id"))?;
        assert!(reconcile_operation_files(&run, &HashSet::new()).is_err());
        std::fs::remove_dir(run.join("invalid-id"))?;
        let active = OperationId::new();
        let stale = OperationId::new();
        std::fs::create_dir(run.join(active.0.to_string()))?;
        std::fs::create_dir(run.join(stale.0.to_string()))?;
        let removed = reconcile_operation_files(&run, &HashSet::from([active]))?;
        assert_eq!(removed, vec![stale.0.to_string()]);
        assert!(run.join(active.0.to_string()).is_dir());
        Ok(())
    }

    fn plan(revision: &str) -> Result<RuntimeSecretPlan, Box<dyn std::error::Error>> {
        Ok(RuntimeSecretPlan::new(
            "019c527b-6242-7b43-bfd4-bd60e27c1de3".to_owned(),
            "319c527b-6242-7b43-bfd4-bd60e27c1de3".to_owned(),
            "integration".to_owned(),
            RuntimeKind::PodmanCompose,
            vec![RuntimeSecretPlanItem {
                logical_name: "database.password".to_owned(),
                backend: "age".to_owned(),
                delivery: DeliveryDriverId::new("podman-native-secret")?,
                source_revision: revision.to_owned(),
                previous_source_revision: (revision == "revision-2")
                    .then(|| "revision-1".to_owned()),
                consumers: vec![RuntimeSecretConsumer {
                    stack: "app".to_owned(),
                    service: "api".to_owned(),
                    target: "database-password".to_owned(),
                    uid: "1000".to_owned(),
                    gid: "1000".to_owned(),
                    mode: 0o400,
                }],
            }],
        )?)
    }

    fn materials() -> Result<Vec<RuntimeSecretMaterial>, MmError> {
        Ok(vec![RuntimeSecretMaterial {
            logical_name: "database.password".to_owned(),
            material: SecretMaterial::new(SENTINEL.to_owned())
                .map_err(|message| MmError::Secret { message })?,
        }])
    }

    fn two_secret_plan(rotated: bool) -> Result<RuntimeSecretPlan, Box<dyn std::error::Error>> {
        let item = |logical_name: &str, target: &str, revision: &str, previous| {
            Ok::<_, Box<dyn std::error::Error>>(RuntimeSecretPlanItem {
                logical_name: logical_name.to_owned(),
                backend: "age".to_owned(),
                delivery: DeliveryDriverId::new("podman-native-secret")?,
                source_revision: revision.to_owned(),
                previous_source_revision: previous,
                consumers: vec![RuntimeSecretConsumer {
                    stack: "app".to_owned(),
                    service: "api".to_owned(),
                    target: target.to_owned(),
                    uid: "1000".to_owned(),
                    gid: "1000".to_owned(),
                    mode: 0o400,
                }],
            })
        };
        Ok(RuntimeSecretPlan::new(
            "019c527b-6242-7b43-bfd4-bd60e27c1de3".to_owned(),
            "319c527b-6242-7b43-bfd4-bd60e27c1de3".to_owned(),
            "integration".to_owned(),
            RuntimeKind::PodmanCompose,
            vec![
                item(
                    "database.password",
                    "database-password",
                    if rotated { "revision-2" } else { "revision-1" },
                    rotated.then(|| "revision-1".to_owned()),
                )?,
                item("api.token", "api-token", "revision-1", None)?,
            ],
        )?)
    }

    fn two_materials() -> Result<Vec<RuntimeSecretMaterial>, MmError> {
        ["database.password", "api.token"]
            .into_iter()
            .map(|logical_name| {
                Ok(RuntimeSecretMaterial {
                    logical_name: logical_name.to_owned(),
                    material: SecretMaterial::new(SENTINEL.to_owned())
                        .map_err(|message| MmError::Secret { message })?,
                })
            })
            .collect()
    }

    fn fs_contains(root: &std::path::Path, sentinel: &str) -> Result<bool, std::io::Error> {
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if fs_contains(&entry.path(), sentinel)? {
                    return Ok(true);
                }
            } else if std::fs::read(entry.path())?
                .windows(sentinel.len())
                .any(|bytes| bytes == sentinel.as_bytes())
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn lock_error<T>(_: std::sync::PoisonError<T>) -> MmError {
        MmError::State {
            message: "memory engine lock poisoned".to_owned(),
        }
    }
