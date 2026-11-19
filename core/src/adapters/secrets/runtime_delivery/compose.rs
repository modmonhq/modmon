/// File secrets for Docker Compose, which has no secret store of its own.
///
/// Every revision of a secret is one file, kept for root only beside the
/// operation directories and named by a keyed digest of the store's
/// revision (never of the value). The overlay of an operation gives each
/// consumer a Compose secret whose key carries that digest, so a new
/// revision changes the definition of the services that use it and of no
/// other. The file of the previous revision stays until a deployment that
/// no longer uses it has succeeded: a rollback mounts it again.
///
/// On rootful Podman, which Docker Compose drives on a remote host, mm
/// knows the host ids of each service's containers: every consumer gets a
/// copy of the revision given to the host ids its `uid` and `gid` are in
/// its user namespace, with its `mode`, in the same root-only directory.
#[derive(Debug, Clone)]
pub struct DockerComposeFileSecretDelivery {
    run_directory: PathBuf,
    revision_directory: PathBuf,
    capability: RuntimeSecretCapability,
}

impl DockerComposeFileSecretDelivery {
    pub fn new(run_directory: PathBuf) -> Result<Self, MmError> {
        Self::serving(run_directory, RuntimeKind::DockerCompose)
    }

    /// File secrets on rootful Podman, given to each consumer's user.
    pub fn for_podman(run_directory: PathBuf) -> Result<Self, MmError> {
        Self::serving(run_directory, RuntimeKind::PodmanCompose)
    }

    fn serving(run_directory: PathBuf, runtime: RuntimeKind) -> Result<Self, MmError> {
        // Operation directories are swept by operation id; revisions
        // outlive operations, so they are a sibling, not a child.
        let mut name = run_directory
            .file_name()
            .map(std::ffi::OsStr::to_os_string)
            .unwrap_or_default();
        name.push("-files");
        Ok(Self {
            revision_directory: run_directory.with_file_name(name),
            run_directory,
            capability: compose_capability(runtime)?,
        })
    }

    /// The consumers of `item` by the owner of their copy. Docker Engine
    /// ignores owner and mode of a file secret: one root-only file.
    fn consumers_by_owner(
        &self,
        plan: &RuntimeSecretPlan,
        item: &RuntimeSecretPlanItem,
    ) -> Result<BTreeMap<Option<FileOwner>, Vec<RuntimeSecretConsumer>>, MmError> {
        let mut owners = BTreeMap::<_, Vec<_>>::new();
        for consumer in &item.consumers {
            let owner = if self.capability.supports_uid_gid_mode {
                Some(FileOwner::of(plan, consumer)?)
            } else if consumer.uid != "0" || consumer.gid != "0" || consumer.mode != 0o444 {
                return Err(MmError::Secret {
                    message: format!(
                        "Docker Compose file secrets cannot enforce uid/gid/mode; secret '{}' must use uid 0, gid 0, mode 0444",
                        item.logical_name
                    ),
                });
            } else {
                None
            };
            owners.entry(owner).or_default().push(consumer.clone());
        }
        Ok(owners)
    }

    fn stage(
        &self,
        operation_id: OperationId,
        plan: &RuntimeSecretPlan,
        materials: Vec<RuntimeSecretMaterial>,
        root: &Path,
    ) -> Result<PreparedRuntimeSecrets, MmError> {
        let owners = plan
            .items
            .iter()
            .map(|item| self.consumers_by_owner(plan, item))
            .collect::<Result<Vec<_>, _>>()?;
        let files = RevisionFiles::open(&self.revision_directory, plan)?;
        let mut by_name = materials
            .into_iter()
            .map(|material| (material.logical_name.clone(), material))
            .collect::<BTreeMap<_, _>>();
        let mut prepared = Vec::with_capacity(plan.items.len());
        let mut rollback = Vec::with_capacity(plan.items.len());
        for (item, owners) in plan.items.iter().zip(owners) {
            let material = by_name
                .remove(&item.logical_name)
                .ok_or_else(|| MmError::Secret {
                    message: format!(
                        "runtime secret '{}' has no authorized material",
                        item.logical_name
                    ),
                })?;
            let name = files.name(plan, &item.logical_name, &item.source_revision);
            let value = material.material.into_zeroizing();
            // What a rollback mounts: the revision that ran before, which
            // is still here unless an older mm delivered it.
            let before = item
                .previous_source_revision
                .as_ref()
                .map(|previous| files.name(plan, &item.logical_name, previous));
            for (owner, consumers) in owners {
                let file = FileOwner::file_name(owner, &name);
                files.keep(operation_id, &item.logical_name, &file, value.as_bytes(), owner)?;
                let current = PreparedRuntimeSecret {
                    consumers,
                    ..prepared_item(item, file, None)
                };
                rollback.push(
                    match before.as_ref().map(|before| FileOwner::file_name(owner, before)) {
                        Some(before) if files.path(&before).is_file() => PreparedRuntimeSecret {
                            runtime_name: before,
                            ..current.clone()
                        },
                        Some(_) => {
                            tracing::warn!(
                                secret = %item.logical_name,
                                "the previous revision of a file secret is not kept on this host; a rollback keeps the new one"
                            );
                            current.clone()
                        }
                        None => current.clone(),
                    },
                );
                prepared.push(current);
            }
        }
        let overlays = render_compose_overlays(root, &prepared, &files.directory)?;
        let rollback_root = root.join("rollback");
        create_private_directory(&rollback_root)?;
        let rollback_overlays = render_compose_overlays(&rollback_root, &rollback, &files.directory)?;
        Ok(PreparedRuntimeSecrets {
            operation_id,
            plan_hash: plan.plan_hash.clone(),
            root: root.to_path_buf(),
            overlays,
            rollback_overlays,
            secrets: prepared,
        })
    }
}

#[async_trait]
impl RuntimeSecretDelivery for DockerComposeFileSecretDelivery {
    fn capability(&self) -> &RuntimeSecretCapability {
        &self.capability
    }

    async fn prepare(
        &self,
        operation_id: OperationId,
        plan: &RuntimeSecretPlan,
        materials: Vec<RuntimeSecretMaterial>,
    ) -> Result<PreparedRuntimeSecrets, MmError> {
        validate_request(&self.capability, plan, &materials)?;
        let root = operation_root(&self.run_directory, operation_id)?;
        let result = self.stage(operation_id, plan, materials, &root);
        if let Err(primary) = result {
            // A revision file written before the failure stays: the next
            // deployment's reconcile removes it unless it is used.
            return match fs::remove_dir_all(&root) {
                Ok(()) => Err(primary),
                Err(source) => Err(MmError::State {
                    message: format!(
                        "{primary}; runtime secret staging cleanup also failed for {}: {source}",
                        root.display()
                    ),
                }),
            };
        }
        result
    }

    /// Removes the operation's overlays. The revision files stay.
    fn dispose(&self, prepared: &PreparedRuntimeSecrets) -> Result<(), MmError> {
        dispose_operation(&self.run_directory, prepared)
    }

    async fn remove_revision(
        &self,
        plan: &RuntimeSecretPlan,
        logical_name: &str,
        source_revision: &str,
    ) -> Result<bool, MmError> {
        validate_plan_for(&self.capability, plan)?;
        plan_item(plan, logical_name)?;
        let Some(files) = RevisionFiles::existing(&self.revision_directory, plan)? else {
            return Ok(false);
        };
        let name = files.name(plan, logical_name, source_revision);
        let mut removed = false;
        for copy in files.copies(|revision| revision == name)? {
            match fs::remove_file(files.path(&copy)) {
                Ok(()) => removed = true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(MmError::Io {
                        context: "removing a runtime secret revision file".to_owned(),
                        source,
                    });
                }
            }
        }
        Ok(removed)
    }

    async fn reconcile(
        &self,
        plan: &RuntimeSecretPlan,
        retained: &BTreeMap<String, BTreeSet<String>>,
        active_operations: &HashSet<OperationId>,
    ) -> Result<Vec<String>, MmError> {
        validate_plan_for(&self.capability, plan)?;
        let mut removed = reconcile_operation_files(&self.run_directory, active_operations)?;
        removed.extend(prune_revision_files(
            &self.revision_directory,
            plan,
            retained,
        )?);
        Ok(removed)
    }
}

pub fn podman_native_secret_descriptor() -> Result<DeliveryCapabilityDescriptor, MmError> {
    delivery_descriptor(
        "podman-native-secret",
        DeliveryOperation::RuntimeNativeSecret,
        vec![RuntimeKind::PodmanCompose],
        PlaintextPersistence::ProviderManaged,
    )
}

/// Docker Engine's file secrets, and rootful Podman's, whose services have
/// user ids mm knows.
pub fn compose_file_secret_descriptor() -> Result<DeliveryCapabilityDescriptor, MmError> {
    delivery_descriptor(
        "compose-file-secret",
        DeliveryOperation::MountedFile,
        vec![RuntimeKind::DockerCompose, RuntimeKind::PodmanCompose],
        PlaintextPersistence::RevisionScopedPrivateFile,
    )
}

fn delivery_descriptor(
    id: &str,
    operation: DeliveryOperation,
    runtimes: Vec<RuntimeKind>,
    persistence: PlaintextPersistence,
) -> Result<DeliveryCapabilityDescriptor, MmError> {
    Ok(DeliveryCapabilityDescriptor {
        id: DeliveryDriverId::new(id).map_err(|message| MmError::Security { message })?,
        operation,
        environment_scoping: DeliveryEnvironmentScoping::Workload,
        material_kinds: vec![SecretMaterialKind::Text, SecretMaterialKind::Binary],
        max_material_bytes: MAX_NATIVE_SECRET_BYTES,
        max_materialization_name_bytes: 255,
        runtimes,
        workload_kinds: vec![SecretWorkloadKind::Service],
        plaintext_persistence: persistence,
    })
}

fn native_capability(engine: NativeEngine) -> Result<RuntimeSecretCapability, MmError> {
    Ok(RuntimeSecretCapability {
        delivery_driver: DeliveryDriverId::new(engine.driver())
            .map_err(|message| MmError::Security { message })?,
        runtime: engine.runtime(),
        store: engine.store(),
        native_storage: true,
        immutable_revisions: true,
        max_material_bytes: MAX_NATIVE_SECRET_BYTES,
        max_runtime_name_bytes: 253,
        supports_uid_gid_mode: true,
        plaintext_file_lifetime: None,
    })
}

fn compose_capability(runtime: RuntimeKind) -> Result<RuntimeSecretCapability, MmError> {
    let owned = runtime == RuntimeKind::PodmanCompose;
    Ok(RuntimeSecretCapability {
        delivery_driver: DeliveryDriverId::new("compose-file-secret")
            .map_err(|message| MmError::Security { message })?,
        runtime,
        store: RuntimeSecretStoreKind::None,
        native_storage: false,
        immutable_revisions: true,
        max_material_bytes: MAX_NATIVE_SECRET_BYTES,
        max_runtime_name_bytes: 253,
        supports_uid_gid_mode: owned,
        plaintext_file_lifetime: Some(if owned {
            "revision; kept on the host for the container's user, in a directory only root reaches, until a deployment that no longer uses it succeeded".to_owned()
        } else {
            "revision; kept for root on the host until a deployment that no longer uses it succeeded"
                .to_owned()
        }),
    })
}

#[derive(Debug)]
struct InspectedSecret {
    name: String,
    labels: BTreeMap<String, String>,
    created_at_unix_ms: Option<i64>,
}

fn parse_inspected_secret(output: CommandOutput) -> Result<InspectedSecret, MmError> {
    let value: serde_json::Value =
        serde_json::from_str(&output.stdout).map_err(|error| MmError::RuntimeResponse {
            message: format!("engine returned invalid bounded secret inspection JSON: {error}"),
        })?;
    let row = value
        .as_array()
        .and_then(|rows| (rows.len() == 1).then(|| &rows[0]))
        .ok_or_else(|| MmError::RuntimeResponse {
            message: "engine secret inspection did not return exactly one secret".to_owned(),
        })?;
    let spec = row.get("Spec").unwrap_or(row);
    let name = spec
        .get("Name")
        .or_else(|| row.get("Name"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| MmError::RuntimeResponse {
            message: "engine secret inspection omitted its name".to_owned(),
        })?
        .to_owned();
    validate_runtime_name(&name)?;
    let labels = spec
        .get("Labels")
        .or_else(|| row.get("Labels"))
        .and_then(serde_json::Value::as_object)
        .map(|labels| {
            labels
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default();
    let created_at_unix_ms = row
        .get("CreatedAt")
        .or_else(|| spec.get("CreatedAt"))
        .and_then(serde_json::Value::as_str)
        .and_then(|created_at| {
            match time::OffsetDateTime::parse(
                created_at,
                &time::format_description::well_known::Rfc3339,
            ) {
                Ok(value) => Some(value),
                Err(error) => {
                    tracing::debug!(%error, "ignoring invalid runtime secret creation timestamp");
                    None
                }
            }
        })
        .and_then(|created_at| {
            match i64::try_from(created_at.unix_timestamp_nanos() / 1_000_000) {
                Ok(value) => Some(value),
                Err(error) => {
                    tracing::debug!(%error, "ignoring out-of-range runtime secret creation timestamp");
                    None
                }
            }
        });
    Ok(InspectedSecret {
        name,
        labels,
        created_at_unix_ms,
    })
}

fn ownership_labels(
    plan: &RuntimeSecretPlan,
    item: &RuntimeSecretPlanItem,
    revision: &str,
) -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        (LABEL_PROJECT, plan.project_id.clone()),
        (LABEL_TARGET, plan.target_id.clone()),
        (LABEL_ENVIRONMENT, plan.environment.clone()),
        (LABEL_LOGICAL, item.logical_name.clone()),
        (LABEL_REVISION, revision_digest(revision)),
        (LABEL_DRIVER, item.delivery.to_string()),
        (LABEL_GENERATION, "1".to_owned()),
    ])
}

fn require_owned(
    plan: &RuntimeSecretPlan,
    item: &RuntimeSecretPlanItem,
    revision: &str,
    name: &str,
    inspected: &InspectedSecret,
) -> Result<(), MmError> {
    if inspected.name != name
        || ownership_labels(plan, item, revision)
            .iter()
            .any(|(key, value)| inspected.labels.get(*key) != Some(value))
    {
        return Err(MmError::Security {
            message: format!(
                "runtime secret name collision for logical secret '{}'; the existing resource is not exactly owned by this project, target, environment, delivery, and revision",
                item.logical_name
            ),
        });
    }
    Ok(())
}

fn require_plan_ownership(
    plan: &RuntimeSecretPlan,
    driver: &str,
    name: &str,
    inspected: &InspectedSecret,
) -> Result<(), MmError> {
    let exact = inspected.name == name
        && inspected.labels.get(LABEL_PROJECT) == Some(&plan.project_id)
        && inspected.labels.get(LABEL_TARGET) == Some(&plan.target_id)
        && inspected.labels.get(LABEL_ENVIRONMENT) == Some(&plan.environment)
        && inspected
            .labels
            .get(LABEL_DRIVER)
            .is_some_and(|value| value == driver)
        && inspected
            .labels
            .get(LABEL_GENERATION)
            .is_some_and(|value| value == "1");
    if exact {
        Ok(())
    } else {
        Err(MmError::Security {
            message: format!(
                "refusing to reconcile foreign or ambiguously owned runtime secret '{name}'"
            ),
        })
    }
}

fn runtime_name(plan: &RuntimeSecretPlan, logical: &str, revision: &str) -> String {
    let owner = blake3::hash(
        format!(
            "{}\0{}\0{}\0{logical}",
            plan.project_id, plan.target_id, plan.environment
        )
        .as_bytes(),
    );
    format!(
        "mm-{}-{}",
        &owner.to_hex()[..40],
        &revision_digest(revision)[..20]
    )
}

fn revision_digest(revision: &str) -> String {
    blake3::hash(revision.as_bytes()).to_hex().to_string()
}

fn validate_runtime_name(name: &str) -> Result<(), MmError> {
    if name.is_empty()
        || name.len() > 253
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        Err(MmError::RuntimeResponse {
            message: "engine returned an invalid secret name".to_owned(),
        })
    } else {
        Ok(())
    }
}

fn validate_request(
    capability: &RuntimeSecretCapability,
    plan: &RuntimeSecretPlan,
    materials: &[RuntimeSecretMaterial],
) -> Result<(), MmError> {
    validate_plan_for(capability, plan)?;
    if plan
        .canonical_hash()
        .map_err(|message| MmError::Security { message })?
        != plan.plan_hash
    {
        return Err(MmError::State {
            message: "runtime secret plan changed after review".to_owned(),
        });
    }
    if !capability.immutable_revisions
        && plan
            .items
            .iter()
            .any(|item| item.previous_source_revision.is_some())
    {
        return Err(MmError::Secret {
            message: format!(
                "delivery '{}' cannot reconstruct a previous operation-scoped file revision; rotation must use a rollback-capable source workflow",
                capability.delivery_driver
            ),
        });
    }
    let material_names = materials
        .iter()
        .map(|material| material.logical_name.as_str())
        .collect::<BTreeSet<_>>();
    let planned_names = plan
        .items
        .iter()
        .map(|item| item.logical_name.as_str())
        .collect::<BTreeSet<_>>();
    if materials.len() != material_names.len() || material_names != planned_names {
        return Err(MmError::Secret {
            message: "authorized runtime secret material does not exactly match the reviewed plan"
                .to_owned(),
        });
    }
    for material in materials {
        let within_limit = material
            .material
            .expose(|value| !value.is_empty() && value.len() <= capability.max_material_bytes);
        if !within_limit {
            return Err(MmError::Secret {
                message: format!(
                    "runtime secret '{}' exceeds the {} byte capability limit",
                    material.logical_name, capability.max_material_bytes
                ),
            });
        }
    }
    Ok(())
}

fn validate_plan_for(
    capability: &RuntimeSecretCapability,
    plan: &RuntimeSecretPlan,
) -> Result<(), MmError> {
    if plan.runtime != capability.runtime
        || plan
            .items
            .iter()
            .any(|item| item.delivery != capability.delivery_driver)
    {
        return Err(MmError::Secret {
            message: format!(
                "runtime secret delivery '{}' does not support {:?}; unsupported paths fail before mutation",
                capability.delivery_driver, plan.runtime
            ),
        });
    }
    Ok(())
}

fn plan_item<'a>(
    plan: &'a RuntimeSecretPlan,
    logical: &str,
) -> Result<&'a RuntimeSecretPlanItem, MmError> {
    plan.items
        .iter()
        .find(|item| item.logical_name == logical)
        .ok_or_else(|| MmError::Secret {
            message: format!("logical secret '{logical}' is not in the reviewed runtime plan"),
        })
}
