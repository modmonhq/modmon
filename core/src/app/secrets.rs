use std::path::Path;
use std::sync::Arc;

use crate::app::generate_secret;
use crate::app::ports::{
    Clock, ProjectDocumentStore, SecretBatchReplacer, SecretCreator,
    SecretDeliveryCapabilityCatalog, SecretMetadataInspector, SecretRemover, SecretReplacer,
    SecretStorageCapabilityCatalog,
};
use crate::domain::config::{
    ConfigDocument, SecretDeclaration, SecretRotationStrategy, resolve_environment,
};
use crate::domain::error::MmError;
use crate::domain::secret::{
    DirectSecretRotationPlan, SecretCapabilitySnapshot, SecretExpectedState, SecretMaterial,
    SecretMetadata, SecretReference, SecretReplacement, SecretRotationApplyReport,
    SecretRotationPlan, SecretRotationPlanItem, SecretRotationPlanStatus, SecretRotationResultItem,
    SecretScope, SecretStatus,
};
use crate::domain::secret_contract::{
    AuthenticationKind, RevisionSemantics, RotationCapability, StorageOperation,
};

mod generation;
pub use generation::generated_secrets;

const DAY_MILLISECONDS: i64 = 24 * 60 * 60 * 1_000;

#[derive(Clone)]
pub struct SecretManager {
    documents: Arc<dyn ProjectDocumentStore>,
    inspector: Arc<dyn SecretMetadataInspector>,
    creator: Arc<dyn SecretCreator>,
    replacer: Arc<dyn SecretReplacer>,
    batch_replacer: Arc<dyn SecretBatchReplacer>,
    remover: Arc<dyn SecretRemover>,
    storage_capabilities: Arc<dyn SecretStorageCapabilityCatalog>,
    delivery_capabilities: Arc<dyn SecretDeliveryCapabilityCatalog>,
    clock: Arc<dyn Clock>,
    author: Option<String>,
}

#[derive(Clone)]
pub struct SecretManagerDependencies {
    pub documents: Arc<dyn ProjectDocumentStore>,
    pub inspector: Arc<dyn SecretMetadataInspector>,
    pub creator: Arc<dyn SecretCreator>,
    pub replacer: Arc<dyn SecretReplacer>,
    pub batch_replacer: Arc<dyn SecretBatchReplacer>,
    pub remover: Arc<dyn SecretRemover>,
    pub storage_capabilities: Arc<dyn SecretStorageCapabilityCatalog>,
    pub delivery_capabilities: Arc<dyn SecretDeliveryCapabilityCatalog>,
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for SecretManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretManager")
            .finish_non_exhaustive()
    }
}

impl SecretManager {
    #[must_use]
    pub fn new(dependencies: SecretManagerDependencies) -> Self {
        Self {
            documents: dependencies.documents,
            inspector: dependencies.inspector,
            creator: dependencies.creator,
            replacer: dependencies.replacer,
            batch_replacer: dependencies.batch_replacer,
            remover: dependencies.remover,
            storage_capabilities: dependencies.storage_capabilities,
            delivery_capabilities: dependencies.delivery_capabilities,
            clock: dependencies.clock,
            author: None,
        }
    }

    /// Records `author` as the writer of every revision this manager writes.
    #[must_use]
    pub fn written_by(mut self, author: String) -> Self {
        self.author = Some(author);
        self
    }

    #[must_use]
    pub fn capabilities(&self) -> SecretCapabilitySnapshot {
        SecretCapabilitySnapshot {
            storage: self.storage_capabilities.storage_capabilities(),
            delivery: self.delivery_capabilities.delivery_capabilities(),
        }
    }

    pub fn input_prompt(&self, start: &Path, name: &str) -> Result<String, MmError> {
        let document = self.documents.load_from(start)?;
        let declaration = declaration(&document, name)?;
        ensure_single_source(name, declaration)?;
        self.ensure_writable(name, declaration, StorageOperation::Create)?;
        if let Some(input) = &declaration.input {
            ensure_safe_prompt(name, &input.prompt)?;
            Ok(input.prompt.clone())
        } else {
            Ok("Secret value".to_owned())
        }
    }

    pub fn rotation_prompt(&self, start: &Path, name: &str) -> Result<Option<String>, MmError> {
        let document = self.documents.load_from(start)?;
        let declaration = declaration(&document, name)?;
        ensure_single_source(name, declaration)?;
        self.ensure_writable(name, declaration, StorageOperation::Replace)?;
        if declaration.rotation.strategy != SecretRotationStrategy::Manual {
            return Err(MmError::Secret {
                message: format!(
                    "secret '{name}' uses rotation strategy '{}', which requires deployment orchestration",
                    rotation_strategy(declaration.rotation.strategy)
                ),
            });
        }
        if declaration.generate.is_some() {
            return Ok(None);
        }
        if let Some(input) = &declaration.input {
            ensure_safe_prompt(name, &input.prompt)?;
            Ok(Some(input.prompt.clone()))
        } else {
            Ok(Some("Secret value".to_owned()))
        }
    }

    pub async fn set(
        &self,
        start: &Path,
        environment: Option<&str>,
        name: &str,
        material: SecretMaterial,
    ) -> Result<SecretRotationResultItem, MmError> {
        let document = self.documents.load_from(start)?;
        let scope = secret_scope(&document, environment)?;
        let declaration = declaration(&document, name)?;
        ensure_single_source(name, declaration)?;
        self.ensure_writable(name, declaration, StorageOperation::Create)?;
        let reference = secret_reference(declaration)?;
        let metadata = self.inspector.inspect(&scope, &reference).await?;
        ensure_no_candidate(name, &metadata)?;
        let now_unix_ms = self.clock.now_unix_ms();
        let revision = match metadata.status {
            SecretStatus::Unset => {
                let result = self
                    .creator
                    .create(
                        &scope,
                        &reference,
                        material,
                        now_unix_ms,
                        self.author.as_deref(),
                    )
                    .await?;
                if !result.changed {
                    return Err(lifecycle_changed());
                }
                result.revision
            }
            SecretStatus::Set | SecretStatus::Invalid => {
                if metadata.revision.is_none() {
                    return Err(lifecycle_changed());
                }
                self.replacer
                    .replace(
                        &scope,
                        &reference,
                        material,
                        metadata.expected_state(),
                        now_unix_ms,
                        self.author.as_deref(),
                    )
                    .await?
            }
            status => return Err(unavailable_for_mutation(name, status)),
        };
        Ok(SecretRotationResultItem {
            name: name.to_owned(),
            provider: reference.scheme().to_owned(),
            revision,
        })
    }

    pub async fn unset(
        &self,
        start: &Path,
        environment: Option<&str>,
        name: &str,
    ) -> Result<(String, bool), MmError> {
        let document = self.documents.load_from(start)?;
        let scope = secret_scope(&document, environment)?;
        let declaration = declaration(&document, name)?;
        self.ensure_writable(name, declaration, StorageOperation::Delete)?;
        let reference = secret_reference(declaration)?;
        let metadata = self.inspector.inspect(&scope, &reference).await?;
        ensure_no_candidate(name, &metadata)?;
        let removed = match metadata.status {
            SecretStatus::Unset => false,
            SecretStatus::Set | SecretStatus::Invalid => {
                if metadata.revision.is_none() {
                    return Err(lifecycle_changed());
                }
                self.remover
                    .unset(&scope, &reference, metadata.expected_state())
                    .await?
            }
            status => return Err(unavailable_for_mutation(name, status)),
        };
        Ok((reference.scheme().to_owned(), removed))
    }

    pub async fn rotate(
        &self,
        start: &Path,
        environment: Option<&str>,
        name: &str,
        supplied_material: Option<SecretMaterial>,
    ) -> Result<SecretRotationResultItem, MmError> {
        let document = self.documents.load_from(start)?;
        let scope = secret_scope(&document, environment)?;
        let declaration = declaration(&document, name)?;
        ensure_single_source(name, declaration)?;
        self.ensure_writable(name, declaration, StorageOperation::Replace)?;
        if declaration.rotation.strategy != SecretRotationStrategy::Manual {
            return Err(MmError::Secret {
                message: format!(
                    "secret '{name}' uses rotation strategy '{}', which requires deployment orchestration",
                    rotation_strategy(declaration.rotation.strategy)
                ),
            });
        }
        let reference = secret_reference(declaration)?;
        let metadata = self.inspector.inspect(&scope, &reference).await?;
        if metadata.status != SecretStatus::Set {
            return Err(MmError::Secret {
                message: format!("secret '{name}' must be set before it can be rotated"),
            });
        }
        if metadata.candidate_revision.is_some() {
            return Err(MmError::Secret {
                message: format!("secret '{name}' already has a staged candidate"),
            });
        }
        if metadata.revision.is_none() {
            return Err(lifecycle_changed());
        }
        let expected = metadata.expected_state();
        let material = if let Some(generator) = &declaration.generate {
            if supplied_material.is_some() {
                return Err(MmError::Secret {
                    message: format!("secret '{name}' is generated and does not accept input"),
                });
            }
            generate_secret(generator)?
        } else {
            supplied_material.ok_or_else(|| MmError::Secret {
                message: format!("secret '{name}' requires hidden input or --stdin"),
            })?
        };
        let revision = self
            .replacer
            .replace(
                &scope,
                &reference,
                material,
                expected,
                self.clock.now_unix_ms(),
                self.author.as_deref(),
            )
            .await?;
        Ok(SecretRotationResultItem {
            name: name.to_owned(),
            provider: reference.scheme().to_owned(),
            revision,
        })
    }

    pub async fn plan_direct_rotation(
        &self,
        start: &Path,
        environment: Option<&str>,
        name: &str,
    ) -> Result<DirectSecretRotationPlan, MmError> {
        let document = self.documents.load_from(start)?;
        let scope = secret_scope(&document, environment)?;
        let declaration = declaration(&document, name)?;
        ensure_single_source(name, declaration)?;
        self.ensure_writable(name, declaration, StorageOperation::Replace)?;
        if declaration.rotation.strategy != SecretRotationStrategy::Manual {
            return Err(MmError::Secret {
                message: format!(
                    "secret '{name}' uses rotation strategy '{}', which requires deployment orchestration",
                    rotation_strategy(declaration.rotation.strategy)
                ),
            });
        }
        let reference = secret_reference(declaration)?;
        let metadata = self.inspector.inspect(&scope, &reference).await?;
        if metadata.status != SecretStatus::Set || metadata.candidate_revision.is_some() {
            return Err(MmError::Secret {
                message: format!("secret '{name}' is not in a directly rotatable state"),
            });
        }
        let current_revision = metadata.revision.ok_or_else(lifecycle_changed)?;
        let requires_hidden_input = declaration.generate.is_none();
        let canonical = serde_json::to_vec(&(
            &document.source_hash,
            document.config.project.id,
            scope.environment(),
            name,
            reference.canonical(),
            &current_revision,
            requires_hidden_input,
        ))
        .map_err(|error| MmError::State {
            message: format!("could not canonicalize direct secret rotation plan: {error}"),
        })?;
        Ok(DirectSecretRotationPlan {
            project_name: document.config.project.name,
            environment: scope.environment().to_owned(),
            name: name.to_owned(),
            provider: reference.scheme().to_owned(),
            current_revision,
            requires_hidden_input,
            plan_hash: blake3::hash(&canonical).to_hex().to_string(),
        })
    }

    pub async fn apply_direct_rotation(
        &self,
        start: &Path,
        environment: Option<&str>,
        name: &str,
        reviewed_plan_hash: &str,
        supplied_material: Option<SecretMaterial>,
    ) -> Result<SecretRotationResultItem, MmError> {
        let plan = self.plan_direct_rotation(start, environment, name).await?;
        if plan.plan_hash != reviewed_plan_hash {
            return Err(MmError::State {
                message: format!(
                    "secret rotation plan changed (expected {reviewed_plan_hash}, current {}); review the new plan",
                    plan.plan_hash
                ),
            });
        }
        self.rotate(start, environment, name, supplied_material)
            .await
    }

    pub async fn rotation_plan(
        &self,
        start: &Path,
        environment: Option<&str>,
    ) -> Result<SecretRotationPlan, MmError> {
        let (_, plan) = self.build_rotation_plan(start, environment).await?;
        Ok(plan)
    }

    pub async fn apply_rotation(
        &self,
        start: &Path,
        environment: Option<&str>,
        approved_plan_hash: &str,
    ) -> Result<SecretRotationApplyReport, MmError> {
        let (document, plan) = self.build_rotation_plan(start, environment).await?;
        if plan.plan_hash != approved_plan_hash {
            return Err(MmError::Secret {
                message: "rotation plan changed; review a new plan before applying it".to_owned(),
            });
        }
        if plan.secrets.iter().any(|item| {
            matches!(
                item.status,
                SecretRotationPlanStatus::InputRequired
                    | SecretRotationPlanStatus::UnsupportedStrategy
            )
        }) {
            return Err(MmError::Secret {
                message: "rotation plan contains due secrets that require explicit input or deployment orchestration"
                    .to_owned(),
            });
        }
        let ready = plan
            .secrets
            .iter()
            .filter(|item| item.status == SecretRotationPlanStatus::Ready)
            .collect::<Vec<_>>();
        if ready.is_empty() {
            return Err(MmError::Secret {
                message: "rotation plan has no due generated secrets that can be applied safely"
                    .to_owned(),
            });
        }

        let now_unix_ms = self.clock.now_unix_ms();
        let scope = secret_scope(&document, environment)?;
        let mut pending = Vec::with_capacity(ready.len());
        let mut replacements = Vec::with_capacity(ready.len());
        for item in ready {
            let declaration = declaration(&document, &item.name)?;
            let generator = declaration
                .generate
                .as_ref()
                .ok_or_else(|| MmError::Secret {
                    message: format!("secret '{}' no longer has a generator", item.name),
                })?;
            if item.current_revision.is_none() || item.candidate_revision.is_some() {
                return Err(lifecycle_changed());
            }
            let material = generate_secret(generator)?;
            let reference = secret_reference(declaration)?;
            pending.push((item.name.clone(), reference.scheme().to_owned()));
            replacements.push(SecretReplacement {
                scope: scope.clone(),
                reference: reference.clone(),
                material,
                expected: SecretExpectedState {
                    active_revision: item.current_revision.clone(),
                    candidate_revision: item.candidate_revision.clone(),
                },
            });
        }
        let revisions = self
            .batch_replacer
            .replace_batch(replacements, now_unix_ms, self.author.as_deref())
            .await?;
        if revisions.len() != pending.len() {
            return Err(MmError::Secret {
                message: "secret provider returned an incomplete rotation result".to_owned(),
            });
        }
        let rotated = pending
            .into_iter()
            .zip(revisions)
            .map(|((name, provider), revision)| SecretRotationResultItem {
                name,
                provider,
                revision,
            })
            .collect();
        Ok(SecretRotationApplyReport {
            project_name: document.config.project.name,
            plan_hash: plan.plan_hash,
            rotated,
        })
    }

    async fn build_rotation_plan(
        &self,
        start: &Path,
        environment: Option<&str>,
    ) -> Result<(ConfigDocument, SecretRotationPlan), MmError> {
        let document = self.documents.load_from(start)?;
        let scope = secret_scope(&document, environment)?;
        let now_unix_ms = self.clock.now_unix_ms();
        let mut secrets = Vec::with_capacity(document.config.secrets.len());
        for (name, declaration) in &document.config.secrets {
            let reference = secret_reference(declaration)?;
            let descriptor = self
                .storage_capabilities
                .storage_capability(reference.provider())?;
            if descriptor.rotation == RotationCapability::None {
                if declaration.rotation.interval_days.is_some() {
                    return Err(MmError::Secret {
                        message: format!(
                            "secret '{name}' uses storage backend '{}' without rotation support; supported operations are {:?}",
                            descriptor.id, descriptor.operations
                        ),
                    });
                }
                secrets.push(SecretRotationPlanItem {
                    name: name.clone(),
                    provider: reference.scheme().to_owned(),
                    strategy: rotation_strategy(declaration.rotation.strategy).to_owned(),
                    status: SecretRotationPlanStatus::NotScheduled,
                    current_revision: None,
                    candidate_revision: None,
                    due_at_unix_ms: None,
                });
                continue;
            }
            if descriptor.revision_semantics != RevisionSemantics::CompareAndSwap
                && declaration.rotation.interval_days.is_some()
            {
                return Err(MmError::Secret {
                    message: format!(
                        "secret '{name}' uses backend '{}' with {:?} revisions; scheduled rotation requires compare-and-swap revisions",
                        descriptor.id, descriptor.revision_semantics
                    ),
                });
            }
            let metadata = self.inspector.inspect(&scope, &reference).await?;
            let due_at_unix_ms = declaration.rotation.interval_days.and_then(|days| {
                metadata.created_at_unix_ms.map(|created_at| {
                    created_at.saturating_add(i64::from(days).saturating_mul(DAY_MILLISECONDS))
                })
            });
            let status = rotation_plan_status(declaration, &metadata, due_at_unix_ms, now_unix_ms);
            if status == SecretRotationPlanStatus::Ready
                && !descriptor.supports(StorageOperation::ReplaceBatch)
            {
                return Err(MmError::Secret {
                    message: format!(
                        "secret '{name}' uses backend '{}' without atomic batch replacement; scheduled rotation supports backends with {:?}",
                        descriptor.id,
                        StorageOperation::ReplaceBatch
                    ),
                });
            }
            secrets.push(SecretRotationPlanItem {
                name: name.clone(),
                provider: reference.scheme().to_owned(),
                strategy: rotation_strategy(declaration.rotation.strategy).to_owned(),
                status,
                current_revision: metadata.revision,
                candidate_revision: metadata.candidate_revision,
                due_at_unix_ms,
            });
        }
        let plan_hash = rotation_plan_hash(&document.source_hash, &secrets);
        let plan = SecretRotationPlan {
            project_name: document.config.project.name.clone(),
            generated_at_unix_ms: now_unix_ms,
            plan_hash,
            secrets,
        };
        Ok((document, plan))
    }

    fn ensure_writable(
        &self,
        name: &str,
        declaration: &SecretDeclaration,
        operation: StorageOperation,
    ) -> Result<(), MmError> {
        let descriptor = self
            .storage_capabilities
            .storage_capability(&declaration.storage.provider)?;
        if descriptor.supports(operation) {
            return Ok(());
        }
        let location = &declaration.storage.location;
        let message = match descriptor.authentication {
            AuthenticationKind::LocalFile => {
                format!("secret '{name}' is read from {location}; edit that file")
            }
            AuthenticationKind::ProcessEnvironment => format!(
                "secret '{name}' is read from the environment variable {location}; set it there"
            ),
            _ => format!(
                "secret '{name}' uses storage backend '{}' which does not support {operation:?}; supported operations are {:?}",
                descriptor.id, descriptor.operations
            ),
        };
        Err(MmError::Secret { message })
    }
}

fn secret_scope(
    document: &ConfigDocument,
    requested: Option<&str>,
) -> Result<SecretScope, MmError> {
    let environment = resolve_environment(document, requested)?;
    Ok(SecretScope::new(environment))
}

fn declaration<'a>(
    document: &'a ConfigDocument,
    name: &str,
) -> Result<&'a SecretDeclaration, MmError> {
    document
        .config
        .secrets
        .get(name)
        .ok_or_else(|| MmError::Secret {
            message: format!("secret '{name}' is not configured"),
        })
}

fn ensure_single_source(name: &str, declaration: &SecretDeclaration) -> Result<(), MmError> {
    if declaration.generate.is_some() && declaration.input.is_some() {
        Err(MmError::Secret {
            message: format!("secret '{name}' configures both generation and prompted input"),
        })
    } else {
        Ok(())
    }
}

fn ensure_no_candidate(name: &str, metadata: &SecretMetadata) -> Result<(), MmError> {
    if metadata.candidate_revision.is_some() {
        Err(MmError::Secret {
            message: format!("secret '{name}' has a staged candidate and cannot be replaced"),
        })
    } else {
        Ok(())
    }
}

fn unavailable_for_mutation(name: &str, status: SecretStatus) -> MmError {
    let next = if status == SecretStatus::ProviderUnavailable {
        "; initialize its provider on this machine first (`mm secret init age` for the age provider; `mm secret status` shows each secret's provider)"
    } else {
        ""
    };
    MmError::Secret {
        message: format!("secret '{name}' is not writable while its status is {status:?}{next}"),
    }
}

fn lifecycle_changed() -> MmError {
    MmError::Secret {
        message: "secret lifecycle changed since it was inspected; retry from current metadata"
            .to_owned(),
    }
}

fn secret_reference(declaration: &SecretDeclaration) -> Result<SecretReference, MmError> {
    declaration
        .storage
        .reference()
        .map_err(|message| MmError::Secret { message })
}

fn ensure_safe_prompt(name: &str, prompt: &str) -> Result<(), MmError> {
    if prompt.trim().is_empty() || prompt.chars().any(char::is_control) {
        Err(MmError::Secret {
            message: format!("secret '{name}' has an empty or unsafe input prompt"),
        })
    } else {
        Ok(())
    }
}

fn rotation_plan_status(
    declaration: &SecretDeclaration,
    metadata: &SecretMetadata,
    due_at_unix_ms: Option<i64>,
    now_unix_ms: i64,
) -> SecretRotationPlanStatus {
    match metadata.status {
        SecretStatus::ProviderUnavailable
        | SecretStatus::AccessDenied
        | SecretStatus::NotChecked => {
            return SecretRotationPlanStatus::ProviderUnavailable;
        }
        SecretStatus::Unset => return SecretRotationPlanStatus::Unset,
        SecretStatus::Invalid => return SecretRotationPlanStatus::Invalid,
        SecretStatus::Set => {}
    }
    if metadata.candidate_revision.is_some() {
        return SecretRotationPlanStatus::CandidateStaged;
    }
    let Some(due_at_unix_ms) = due_at_unix_ms else {
        return SecretRotationPlanStatus::NotScheduled;
    };
    if now_unix_ms < due_at_unix_ms {
        return SecretRotationPlanStatus::NotDue;
    }
    if declaration.rotation.strategy != SecretRotationStrategy::Manual {
        return SecretRotationPlanStatus::UnsupportedStrategy;
    }
    if declaration.generate.is_some() && declaration.input.is_none() {
        SecretRotationPlanStatus::Ready
    } else {
        SecretRotationPlanStatus::InputRequired
    }
}

const fn rotation_strategy(strategy: SecretRotationStrategy) -> &'static str {
    match strategy {
        SecretRotationStrategy::Manual => "manual",
        SecretRotationStrategy::ReplaceAndRedeploy => "replace-and-redeploy",
        SecretRotationStrategy::Coordinated => "coordinated",
    }
}

fn rotation_plan_hash(config_hash: &str, items: &[SecretRotationPlanItem]) -> String {
    let mut hasher = blake3::Hasher::new();
    hash_field(&mut hasher, "secret-rotation-plan");
    hash_field(&mut hasher, config_hash);
    for item in items {
        hash_field(&mut hasher, &item.name);
        hash_field(&mut hasher, &item.provider);
        hash_field(&mut hasher, &item.strategy);
        hash_field(&mut hasher, rotation_status_name(item.status));
        hash_optional(&mut hasher, item.current_revision.as_deref());
        hash_optional(&mut hasher, item.candidate_revision.as_deref());
        let due_at = item.due_at_unix_ms.map(|value| value.to_string());
        hash_optional(&mut hasher, due_at.as_deref());
    }
    hasher.finalize().to_hex().to_string()
}

fn hash_optional(hasher: &mut blake3::Hasher, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update(&[1]);
            hash_field(hasher, value);
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

fn hash_field(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

const fn rotation_status_name(status: SecretRotationPlanStatus) -> &'static str {
    match status {
        SecretRotationPlanStatus::NotScheduled => "not-scheduled",
        SecretRotationPlanStatus::NotDue => "not-due",
        SecretRotationPlanStatus::Ready => "ready",
        SecretRotationPlanStatus::InputRequired => "input-required",
        SecretRotationPlanStatus::UnsupportedStrategy => "unsupported-strategy",
        SecretRotationPlanStatus::Unset => "unset",
        SecretRotationPlanStatus::Invalid => "invalid",
        SecretRotationPlanStatus::ProviderUnavailable => "provider-unavailable",
        SecretRotationPlanStatus::CandidateStaged => "candidate-staged",
    }
}
