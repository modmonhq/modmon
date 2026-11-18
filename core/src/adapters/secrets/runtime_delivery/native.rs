use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_owned_by_current_user,
    ensure_private_permissions_supported,
};
use crate::app::ports::{CommandOutput, CommandSpec, ProcessRunner, RuntimeSecretDelivery};
use crate::domain::error::MmError;
use crate::domain::operation::OperationId;
use crate::domain::runtime::{
    PODMAN_INHERITED_ENVIRONMENT, RuntimeKind,
};
use crate::domain::runtime_secret::{
    PreparedRuntimeSecret, PreparedRuntimeSecrets, RuntimeSecretCapability, RuntimeSecretConsumer,
    RuntimeSecretMaterial, RuntimeSecretPlan, RuntimeSecretPlanItem, RuntimeSecretStoreKind,
};
use crate::domain::secret_contract::{
    DeliveryCapabilityDescriptor, DeliveryDriverId, DeliveryEnvironmentScoping, DeliveryOperation,
    PlaintextPersistence, SecretMaterialKind, SecretWorkloadKind,
};
use async_trait::async_trait;
use serde_yaml_ng::{Mapping, Value};

const MAX_NATIVE_SECRET_BYTES: usize = 500 * 1024;
const MAX_ENGINE_OUTPUT_BYTES: usize = 1024 * 1024;
const LABEL_PROJECT: &str = "io.mm.project-id";
const LABEL_TARGET: &str = "io.mm.target-id";
const LABEL_ENVIRONMENT: &str = "io.mm.environment";
const LABEL_LOGICAL: &str = "io.mm.logical-secret";
const LABEL_REVISION: &str = "io.mm.source-revision-digest";
const LABEL_DRIVER: &str = "io.mm.delivery-driver";
const LABEL_GENERATION: &str = "io.mm.generation";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeEngine {
    Podman,
}

impl NativeEngine {
    const fn executable(self) -> &'static str {
        match self {
            Self::Podman => "podman",
        }
    }

    const fn runtime(self) -> RuntimeKind {
        match self {
            Self::Podman => RuntimeKind::PodmanCompose,
        }
    }

    const fn driver(self) -> &'static str {
        match self {
            Self::Podman => "podman-native-secret",
        }
    }

    const fn store(self) -> RuntimeSecretStoreKind {
        match self {
            Self::Podman => RuntimeSecretStoreKind::Podman,
        }
    }

    const fn inherited_environment(self) -> &'static [&'static str] {
        match self {
            Self::Podman => PODMAN_INHERITED_ENVIRONMENT,
        }
    }
}

#[derive(Clone)]
struct NativeSecretDelivery {
    process: Arc<dyn ProcessRunner>,
    run_directory: PathBuf,
    engine: NativeEngine,
    capability: RuntimeSecretCapability,
}

impl std::fmt::Debug for NativeSecretDelivery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeSecretDelivery")
            .field("run_directory", &self.run_directory)
            .field("engine", &self.engine)
            .finish_non_exhaustive()
    }
}

impl NativeSecretDelivery {
    fn new(
        process: Arc<dyn ProcessRunner>,
        run_directory: PathBuf,
        engine: NativeEngine,
    ) -> Result<Self, MmError> {
        Ok(Self {
            process,
            run_directory,
            engine,
            capability: native_capability(engine)?,
        })
    }

    fn command(&self, args: impl IntoIterator<Item = impl Into<String>>) -> CommandSpec {
        CommandSpec::new(self.engine.executable())
            .args(args)
            .inherit_environment(self.engine.inherited_environment())
            .timeout(Duration::from_secs(30))
            .with_output_limit(MAX_ENGINE_OUTPUT_BYTES)
    }

    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        self.process.run(specification).await
    }

    fn success(&self, output: CommandOutput) -> Result<CommandOutput, MmError> {
        if output.success() && !output.truncated {
            Ok(output)
        } else if output.truncated {
            Err(MmError::RuntimeResponse {
                message: format!(
                    "{} secret output exceeded the inspection limit",
                    self.engine.executable()
                ),
            })
        } else {
            Err(MmError::ProcessFailed {
                program: self.engine.executable().to_owned(),
                code: output.code,
                stderr: output.stderr.trim().to_owned(),
            })
        }
    }

    async fn inspect(
        &self,
        plan: &RuntimeSecretPlan,
        item: &RuntimeSecretPlanItem,
        revision: &str,
    ) -> Result<Option<InspectedSecret>, MmError> {
        let name = runtime_name(plan, &item.logical_name, revision);
        let output = self
            .run(self.command(["secret", "inspect", "--format", "json", name.as_str()]))
            .await?;
        if !output.success() {
            let missing = output.code == Some(1)
                && output
                    .stderr
                    .to_ascii_lowercase()
                    .contains("no such secret");
            if missing {
                return Ok(None);
            }
            self.success(output)?;
            return Err(MmError::RuntimeResponse {
                message: "secret inspection returned an invalid result".to_owned(),
            });
        }
        let inspected = parse_inspected_secret(self.success(output)?)?;
        require_owned(plan, item, revision, &name, &inspected)?;
        Ok(Some(inspected))
    }

    async fn create(
        &self,
        plan: &RuntimeSecretPlan,
        item: &RuntimeSecretPlanItem,
        material: RuntimeSecretMaterial,
    ) -> Result<(PreparedRuntimeSecret, bool), MmError> {
        if let Some(inspected) = self.inspect(plan, item, &item.source_revision).await? {
            return Ok((
                prepared_item(item, inspected.name, inspected.created_at_unix_ms),
                false,
            ));
        }
        let material = material.material.into_zeroizing();
        if material.len() > MAX_NATIVE_SECRET_BYTES {
            return Err(MmError::Secret {
                message: format!(
                    "{} accepts secret material up to {MAX_NATIVE_SECRET_BYTES} bytes",
                    self.engine.executable()
                ),
            });
        }
        let name = runtime_name(plan, &item.logical_name, &item.source_revision);
        let labels = ownership_labels(plan, item, &item.source_revision);
        let mut args = vec!["secret".to_owned(), "create".to_owned()];
        for (key, value) in labels {
            args.extend(["--label".to_owned(), format!("{key}={value}")]);
        }
        args.extend([name.clone(), "-".to_owned()]);
        let output = self
            .run(self.command(args).sensitive_stdin(material))
            .await?;
        self.success(output)?;
        let inspected = match self.inspect(plan, item, &item.source_revision).await {
            Ok(Some(inspected)) => inspected,
            Ok(None) => {
                let primary = MmError::RuntimeResponse {
                    message: "created runtime secret was not inspectable".to_owned(),
                };
                return Err(self.cleanup_created_name(name.as_str(), primary).await);
            }
            Err(error) => {
                return Err(self.cleanup_created_name(name.as_str(), error).await);
            }
        };
        Ok((
            prepared_item(item, inspected.name, inspected.created_at_unix_ms),
            true,
        ))
    }

    async fn cleanup_created_name(&self, name: &str, primary: MmError) -> MmError {
        let cleanup = self
            .run(self.command(["secret", "rm", name]))
            .await
            .and_then(|output| self.success(output).map(|_| ()));
        native_cleanup_error(primary, cleanup)
    }

    async fn remove_created(
        &self,
        plan: &RuntimeSecretPlan,
        created: &[(String, String)],
    ) -> Result<(), MmError> {
        let mut failures = Vec::new();
        for (logical_name, revision) in created.iter().rev() {
            match plan_item(plan, logical_name) {
                Ok(item) => {
                    if let Err(error) = self.remove(plan, item, revision).await {
                        failures.push(error.to_string());
                    }
                }
                Err(error) => failures.push(error.to_string()),
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(MmError::State {
                message: format!("runtime secret cleanup failed: {}", failures.join("; ")),
            })
        }
    }

    async fn remove(
        &self,
        plan: &RuntimeSecretPlan,
        item: &RuntimeSecretPlanItem,
        revision: &str,
    ) -> Result<bool, MmError> {
        let Some(inspected) = self.inspect(plan, item, revision).await? else {
            return Ok(false);
        };
        let output = self
            .run(self.command(["secret", "rm", inspected.name.as_str()]))
            .await?;
        self.success(output)?;
        Ok(true)
    }

    async fn owned_names(&self, plan: &RuntimeSecretPlan) -> Result<Vec<String>, MmError> {
        let filters = [
            format!("label={LABEL_PROJECT}={}", plan.project_id),
            format!("label={LABEL_TARGET}={}", plan.target_id),
            format!("label={LABEL_ENVIRONMENT}={}", plan.environment),
            format!("label={LABEL_DRIVER}={}", self.engine.driver()),
        ];
        let mut args = vec!["secret".to_owned(), "ls".to_owned()];
        for filter in filters {
            args.extend(["--filter".to_owned(), filter]);
        }
        args.extend(["--format".to_owned(), "{{.Name}}".to_owned()]);
        let output = self.success(self.run(self.command(args)).await?)?;
        let mut names = Vec::new();
        for name in output
            .stdout
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            validate_runtime_name(name)?;
            names.push(name.to_owned());
        }
        names.sort();
        names.dedup();
        Ok(names)
    }
}

pub struct PodmanSecretDelivery(NativeSecretDelivery);

impl PodmanSecretDelivery {
    pub fn new(process: Arc<dyn ProcessRunner>, run_directory: PathBuf) -> Result<Self, MmError> {
        NativeSecretDelivery::new(process, run_directory, NativeEngine::Podman).map(Self)
    }
}

macro_rules! native_delivery {
    ($type:ty) => {
        #[async_trait]
        impl RuntimeSecretDelivery for $type {
            fn capability(&self) -> &RuntimeSecretCapability {
                &self.0.capability
            }

            async fn prepare(
                &self,
                operation_id: OperationId,
                plan: &RuntimeSecretPlan,
                materials: Vec<RuntimeSecretMaterial>,
            ) -> Result<PreparedRuntimeSecrets, MmError> {
                validate_request(&self.0.capability, plan, &materials)?;
                let mut by_name = materials
                    .into_iter()
                    .map(|material| (material.logical_name.clone(), material))
                    .collect::<BTreeMap<_, _>>();
                let mut prepared = Vec::with_capacity(plan.items.len());
                let mut created = Vec::new();
                for item in &plan.items {
                    let material = by_name.remove(&item.logical_name).ok_or_else(|| MmError::Secret {
                        message: format!("runtime secret '{}' has no authorized material", item.logical_name),
                    })?;
                    match self.0.create(plan, item, material).await {
                        Ok((secret, was_created)) => {
                            if was_created {
                                created.push((item.logical_name.clone(), item.source_revision.clone()));
                            }
                            prepared.push(secret);
                        }
                        Err(error) => {
                            return Err(native_cleanup_error(
                                error,
                                self.0.remove_created(plan, &created).await,
                            ));
                        }
                    }
                }
                let mut rollback = Vec::new();
                if plan
                    .items
                    .iter()
                    .any(|item| item.previous_source_revision.is_some())
                {
                    for (item, current) in plan.items.iter().zip(&prepared) {
                        let Some(previous) = &item.previous_source_revision else {
                            rollback.push(current.clone());
                            continue;
                        };
                        let inspected = match self.0.inspect(plan, item, previous).await {
                            Ok(Some(inspected)) => inspected,
                            Ok(None) => {
                                let error = MmError::Secret {
                                    message: format!("previous runtime revision for secret '{}' is unavailable; refusing a non-rollback-safe update", item.logical_name),
                                };
                                return Err(native_cleanup_error(
                                    error,
                                    self.0.remove_created(plan, &created).await,
                                ));
                            }
                            Err(error) => {
                                return Err(native_cleanup_error(
                                    error,
                                    self.0.remove_created(plan, &created).await,
                                ));
                            }
                        };
                        rollback.push(prepared_item(
                            item,
                            inspected.name,
                            inspected.created_at_unix_ms,
                        ));
                    }
                }
                let rendered = (|| {
                    let root = operation_root(&self.0.run_directory, operation_id)?;
                    let overlays = render_overlays(&root, &prepared, true)?;
                    let rollback_overlays = if rollback.is_empty() {
                        Vec::new()
                    } else {
                        let rollback_root = root.join("rollback");
                        create_private_directory(&rollback_root)?;
                        render_overlays(&rollback_root, &rollback, true)?
                    };
                    Ok(PreparedRuntimeSecrets {
                        operation_id,
                        plan_hash: plan.plan_hash.clone(),
                        root: root.clone(),
                        overlays,
                        rollback_overlays,
                        secrets: prepared,
                    })
                })();
                match rendered {
                    Ok(prepared) => Ok(prepared),
                    Err(error) => {
                        Err(native_cleanup_error(
                            error,
                            self.0.remove_created(plan, &created).await,
                        ))
                    }
                }
            }

            fn dispose(&self, prepared: &PreparedRuntimeSecrets) -> Result<(), MmError> {
                dispose_operation(&self.0.run_directory, prepared)
            }

            async fn remove_revision(
                &self,
                plan: &RuntimeSecretPlan,
                logical_name: &str,
                source_revision: &str,
            ) -> Result<bool, MmError> {
                validate_plan_for(&self.0.capability, plan)?;
                let item = plan_item(plan, logical_name)?;
                self.0.remove(plan, item, source_revision).await
            }

            async fn reconcile(
                &self,
                plan: &RuntimeSecretPlan,
                retained: &BTreeMap<String, BTreeSet<String>>,
                _active_operations: &HashSet<OperationId>,
            ) -> Result<Vec<String>, MmError> {
                validate_plan_for(&self.0.capability, plan)?;
                let names = self.0.owned_names(plan).await?;
                let retained_names = retained
                    .iter()
                    .flat_map(|(logical, revisions)| {
                        revisions.iter().map(move |revision| runtime_name(plan, logical, revision))
                    })
                    .collect::<BTreeSet<_>>();
                let mut removed = Vec::new();
                for name in names {
                    if retained_names.contains(&name) {
                        continue;
                    }
                    let inspected = parse_inspected_secret(self.0.success(
                        self.0.run(self.0.command(["secret", "inspect", "--format", "json", name.as_str()])).await?
                    )?)?;
                    require_plan_ownership(plan, self.0.engine.driver(), &name, &inspected)?;
                    self.0.success(self.0.run(self.0.command(["secret", "rm", name.as_str()])).await?)?;
                    removed.push(name);
                }
                Ok(removed)
            }
        }
    };
}

fn native_cleanup_error(primary: MmError, cleanup: Result<(), MmError>) -> MmError {
    match cleanup {
        Ok(()) => primary,
        Err(cleanup) => MmError::State {
            message: format!("{primary}; runtime secret cleanup also failed: {cleanup}"),
        },
    }
}

native_delivery!(PodmanSecretDelivery);
