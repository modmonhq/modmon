//! The host side of secrets and variables set over the relay: which keys
//! the deployed commit lets a host hold, the two steps of a stage, what a
//! node reports, and what goes live after a deployment.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::adapters::secrets::{
    PreparedStage, SlotReport, StagedSlot, StoreChange, StoreStagedChange,
};
use crate::app::ports::{Clock, RemoteRequestContext};
use crate::domain::environment::{EnvironmentValueOverride, ValueSource};
use crate::domain::staged::{
    HostValue, NodeValues, STAGE_PREPARE_TTL_MS, StageAnswer, StageState, StageStep, StagedAction,
    StagedChangeKind, StagedPlanItem, ValueChange, ValueKind, ValueOrigin, ValueStage,
};

/// The values a selection's environment gets: on a host, the variables set
/// on it; elsewhere, this machine's overrides.
pub(super) fn values_for(
    selection: &WorkspaceSelection,
    home: &MmHome,
    document: &ConfigDocument,
    sqlite: Arc<SqliteStateStore>,
) -> Arc<dyn EnvironmentValueStore> {
    if !selection.host_values {
        return sqlite;
    }
    Arc::new(HostVariables {
        store: AgeSecretStore::new(
            super::providers::identity_path(home),
            home.secrets_directory
                .join(format!("{}.age", document.config.project.id.0)),
        ),
    })
}

struct HostVariables {
    store: AgeSecretStore,
}

impl EnvironmentValueStore for HostVariables {
    fn list_environment_overrides(
        &self,
        _workspace_id: crate::domain::workspace::WorkspaceId,
        environment: &str,
    ) -> Result<Vec<EnvironmentValueOverride>, MmError> {
        Ok(self
            .store
            .variable_values(environment)?
            .into_iter()
            .map(|variable| {
                let digest = blake3::hash(variable.revision.as_bytes());
                let mut revision = [0_u8; 8];
                revision.copy_from_slice(&digest.as_bytes()[..8]);
                EnvironmentValueOverride {
                    key: variable.key,
                    value: variable.value.to_string(),
                    revision: i64::from_le_bytes(revision),
                    updated_at_unix_ms: variable.changed_at_unix_ms,
                }
            })
            .collect())
    }

    fn set_environment_override(
        &self,
        _workspace_id: crate::domain::workspace::WorkspaceId,
        environment: &str,
        key: &str,
        _value: &str,
        _now_unix_ms: i64,
        _expected_revision: Option<i64>,
    ) -> Result<EnvironmentValueOverride, MmError> {
        Err(set_remotely(environment, key))
    }

    fn unset_environment_override(
        &self,
        _workspace_id: crate::domain::workspace::WorkspaceId,
        environment: &str,
        key: &str,
    ) -> Result<bool, MmError> {
        Err(set_remotely(environment, key))
    }
}

fn set_remotely(environment: &str, key: &str) -> MmError {
    MmError::Environment {
        message: format!(
            "variables of a host are set over the relay: run `mm -e {environment} env set {key} --stdin` on your machine"
        ),
    }
}

/// A key the deployed commit lets a host hold.
struct DeclaredKey {
    kind: ValueKind,
    key: String,
    /// `None` for a secret an external store keeps.
    slot: Option<StagedSlot>,
    provider: Option<String>,
    services: BTreeSet<String>,
    required: bool,
}

/// Every secret the configuration at `selection` declares, and every
/// variable its templates use whose value the repo does not fix: no binding
/// and no service `env:` entry gives it one.
async fn declared(
    selection: &WorkspaceSelection,
    mm_home: &Path,
) -> Result<(ConfigDocument, String, Vec<DeclaredKey>), MmError> {
    let document = documents_on(selection.document_scope()).load_from(&selection.start)?;
    let environment = resolve_environment(&document, selection.environment.as_deref())?;
    let status = RuntimeComposition::new()
        .environment_manager(selection, Some(mm_home))?
        .status(selection, false)
        .await?;
    let mut keys = BTreeMap::<(ValueKind, String), DeclaredKey>::new();
    for (name, declaration) in &document.config.secrets {
        let reference = declaration
            .storage
            .reference()
            .map_err(|message| MmError::Secret { message })?;
        let external = reference.scheme() != "age";
        let managed = kept_in_cluster_store(&document, &reference);
        keys.insert(
            (ValueKind::Secret, name.clone()),
            DeclaredKey {
                kind: ValueKind::Secret,
                key: name.clone(),
                slot: (!external || managed).then(|| StagedSlot::Secret {
                    environment: environment.clone(),
                    reference,
                }),
                provider: external.then(|| declaration.storage.provider.as_str().to_owned()),
                services: BTreeSet::new(),
                required: false,
            },
        );
    }
    let bindings =
        crate::domain::config_resolve::effective_bindings(&document.config, &environment);
    for service in status.modules.iter().flat_map(|module| &module.services) {
        for variable in &service.variables {
            let Some(global_key) = &variable.global_key else {
                continue;
            };
            let required = variable.required && !variable.has_default;
            let entry = match bindings.get(global_key.as_str()) {
                Some(binding) => match binding.secret_name() {
                    Some(secret) => keys.get_mut(&(ValueKind::Secret, secret.to_owned())),
                    None => None,
                },
                None if variable.source == ValueSource::ServiceEnv
                    || crate::domain::release_facts::is_release_key(
                        &document.config,
                        global_key,
                    ) =>
                {
                    None
                }
                None => Some(
                    keys.entry((ValueKind::Variable, global_key.clone()))
                        .or_insert_with(|| DeclaredKey {
                            kind: ValueKind::Variable,
                            key: global_key.clone(),
                            slot: Some(StagedSlot::Variable {
                                environment: environment.clone(),
                                key: global_key.clone(),
                            }),
                            provider: None,
                            services: BTreeSet::new(),
                            required: false,
                        }),
                ),
            };
            if let Some(entry) = entry {
                entry.services.insert(service.service.clone());
                entry.required |= required;
            }
        }
    }
    for module in document.config.modules.values() {
        for (name, service) in &module.services {
            if !service.is_active(&environment) {
                continue;
            }
            for (_, file) in service.secret_files_in(&environment) {
                if let Some(entry) = keys.get_mut(&(ValueKind::Secret, file.secret.clone())) {
                    entry.services.insert(name.clone());
                    entry.required = true;
                }
            }
        }
    }
    Ok((document, environment, keys.into_values().collect()))
}

/// Whether `reference` names the managed store of cluster environments.
fn kept_in_cluster_store(
    document: &ConfigDocument,
    reference: &crate::domain::secret::SecretReference,
) -> bool {
    document
        .config
        .providers
        .iter()
        .any(|(provider, instance)| {
            provider.as_str() == reference.scheme()
                && crate::domain::secret_store::is_cluster_store(instance)
        })
}

fn store(mm_home: &Path, context: &RemoteRequestContext) -> Result<AgeSecretStore, MmError> {
    super::host_secrets::host_store(mm_home, context.scope.project_id)
}

pub(super) fn secret_recipient(
    mm_home: &Path,
    context: &RemoteRequestContext,
) -> Result<String, MmError> {
    store(mm_home, context)?.ensure_recipient()
}

pub(super) async fn host_values(
    selection: &WorkspaceSelection,
    mm_home: &Path,
    context: &RemoteRequestContext,
) -> Result<NodeValues, MmError> {
    let (_, _, keys) = declared(selection, mm_home).await?;
    let reports = reports(mm_home, context, &keys).await?;
    Ok(NodeValues {
        node: context.scope.node.clone(),
        commit: String::new(),
        values: keys
            .into_iter()
            .zip(reports)
            .map(|(key, report)| HostValue {
                origin: if key.provider.is_some() && key.slot.is_none() {
                    Some(ValueOrigin::ExternalStore)
                } else {
                    report.origin
                },
                key: key.key,
                kind: key.kind,
                provider: key.provider,
                services: key.services.into_iter().collect(),
                required: key.required,
                active: report.active,
                staged: report.staged,
                defaulted: false,
            })
            .collect(),
    })
}

async fn reports(
    mm_home: &Path,
    context: &RemoteRequestContext,
    keys: &[DeclaredKey],
) -> Result<Vec<SlotReport>, MmError> {
    let slots = keys
        .iter()
        .filter_map(|key| key.slot.clone())
        .collect::<Vec<_>>();
    let mut reports = store(mm_home, context)?.slot_reports(&slots)?.into_iter();
    let mut all = keys
        .iter()
        .map(|key| {
            if key.slot.is_some() {
                reports.next().unwrap_or_default()
            } else {
                SlotReport::default()
            }
        })
        .collect::<Vec<_>>();
    for (key, report) in keys.iter().zip(all.iter_mut()) {
        if let Some(StagedSlot::Secret {
            environment,
            reference,
        }) = &key.slot
            && reference.scheme() != "age"
        {
            report.active = live_in_store(mm_home, context, (environment, reference), report)
                .await
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "the managed secret store did not say what it holds");
                    None
                });
        }
    }
    Ok(all)
}

/// The version a managed cluster store holds live, with who changed it
/// when this host staged that revision.
async fn live_in_store(
    mm_home: &Path,
    context: &RemoteRequestContext,
    (environment, reference): (&str, &crate::domain::secret::SecretReference),
    report: &SlotReport,
) -> Result<Option<crate::domain::staged::ValueRevision>, MmError> {
    let home = MmHome::resolve(Some(mm_home))?;
    let live =
        super::providers::cluster_store(&home, context.scope.project_id, reference.scheme())?
            .live(
                &crate::domain::secret::SecretScope::new(environment),
                reference,
            )
            .await?;
    let Some(revision) = live
        .revision
        .filter(|_| live.status == crate::domain::secret::SecretStatus::Set)
    else {
        return Ok(None);
    };
    let recorded = report
        .active
        .as_ref()
        .filter(|recorded| recorded.revision == revision);
    Ok(Some(crate::domain::staged::ValueRevision {
        changed_by: recorded.and_then(|recorded| recorded.changed_by.clone()),
        changed_at_unix_ms: live.created_at_unix_ms,
        revision,
    }))
}

pub(super) async fn stage_values(
    selections: &[WorkspaceSelection],
    mm_home: &Path,
    context: &RemoteRequestContext,
    stage: ValueStage,
) -> Result<StageAnswer, MmError> {
    let store = store(mm_home, context)?;
    let now = crate::adapters::clock::SystemClock.now_unix_ms();
    let environment = &context.scope.environment;
    let answer = |state, changes| StageAnswer {
        stage: stage.stage,
        state,
        changes,
    };
    match stage.step {
        StageStep::Prepare { changes, sealed } => {
            let keys = declared_anywhere(selections, mm_home, &changes).await?;
            for change in &changes {
                settable(&keys, change, environment)?;
            }
            if let Some(sealed) = &sealed {
                store
                    .open_staged_values(sealed.as_bytes())?
                    .check_matches(context.scope.project_id, environment, stage.stage, &changes)
                    .map_err(|message| MmError::Secret { message })?;
            }
            store.prepare_stage(
                stage.stage,
                PreparedStage {
                    environment: environment.clone(),
                    principal: context.principal.clone(),
                    changes: changes.clone(),
                    sealed,
                    expires_at_unix_ms: now.saturating_add(STAGE_PREPARE_TTL_MS),
                },
                now,
            )?;
            Ok(answer(StageState::Prepared, changes))
        }
        StageStep::Commit => {
            let prepared = store
                .take_prepared(stage.stage, now)?
                .filter(|prepared| {
                    &prepared.environment == environment && prepared.principal == context.principal
                })
                .ok_or_else(|| MmError::Environment {
                    message: format!(
                        "this node holds no prepared stage {} of yours; it expired or was reverted. Run the command again",
                        stage.stage
                    ),
                })?;
            let keys = declared_anywhere(selections, mm_home, &prepared.changes).await?;
            let mut values = match &prepared.sealed {
                Some(sealed) => store
                    .open_staged_values(sealed.as_bytes())?
                    .values
                    .into_iter()
                    .map(|value| ((value.kind, value.key), value.value))
                    .collect::<BTreeMap<_, _>>(),
                None => BTreeMap::new(),
            };
            let mut changes = Vec::with_capacity(prepared.changes.len());
            for change in &prepared.changes {
                let slot = settable(&keys, change, environment)?;
                let change = match &change.action {
                    StagedAction::Set { revision } => StoreChange::Set {
                        revision: revision.clone(),
                        value: values
                            .remove(&(change.kind, change.key.clone()))
                            .ok_or_else(|| MmError::Secret {
                                message: format!("the sealed values lack '{}'", change.key),
                            })?,
                    },
                    StagedAction::Unset { revision } => StoreChange::Unset {
                        revision: revision.clone(),
                    },
                    StagedAction::Discard => StoreChange::Discard,
                };
                changes.push(StoreStagedChange { slot, change });
            }
            store.stage_values(stage.stage, changes, &context.principal, now)?;
            Ok(answer(StageState::Committed, prepared.changes))
        }
        StageStep::Revert => {
            store.revert_stage(stage.stage)?;
            Ok(answer(StageState::Reverted, Vec::new()))
        }
    }
}

/// The keys of the deployed commit, and of newer checkouts for the
/// changes it does not declare.
async fn declared_anywhere(
    selections: &[WorkspaceSelection],
    mm_home: &Path,
    changes: &[ValueChange],
) -> Result<Vec<DeclaredKey>, MmError> {
    let mut keys = Vec::new();
    for (index, selection) in selections.iter().enumerate() {
        let found = match declared(selection, mm_home).await {
            Ok((_, _, found)) => found,
            Err(error) if index == 0 => return Err(error),
            Err(_) => continue,
        };
        for key in found {
            if !keys
                .iter()
                .any(|known: &DeclaredKey| known.kind == key.kind && known.key == key.key)
            {
                keys.push(key);
            }
        }
        let missing = changes.iter().any(|change| {
            !keys
                .iter()
                .any(|key| key.kind == change.kind && key.key == change.key)
        });
        if !missing {
            break;
        }
    }
    Ok(keys)
}

/// The slot a change lands in, when the deployed commit lets this host
/// hold the key.
fn settable(
    keys: &[DeclaredKey],
    change: &ValueChange,
    environment: &str,
) -> Result<StagedSlot, MmError> {
    let found = keys
        .iter()
        .find(|key| key.kind == change.kind && key.key == change.key);
    match found {
        Some(DeclaredKey {
            slot: Some(slot), ..
        }) => Ok(slot.clone()),
        Some(DeclaredKey {
            provider: Some(provider),
            ..
        }) if provider == "file" => Err(MmError::Secret {
            message: format!(
                "secret '{}' is read from a file on the machine that runs mm, and hosts never read it, so it serves builds made there. To give a service of {environment} the value, keep it in a secret with `provider: age`, bind that one for {environment}, and set it with `mm -e {environment} secret set NAME --stdin`",
                change.key
            ),
        }),
        Some(DeclaredKey {
            provider: Some(provider),
            ..
        }) => Err(MmError::Secret {
            message: format!(
                "secret '{}' is read from the store '{provider}', not kept on the hosts; write a new version there and run `mm -e {environment} deploy redeploy`",
                change.key
            ),
        }),
        _ => Err(undeclared(keys, change, environment)),
    }
}

fn undeclared(keys: &[DeclaredKey], change: &ValueChange, environment: &str) -> MmError {
    let listed = keys
        .iter()
        .filter(|key| key.kind == change.kind && key.slot.is_some())
        .map(|key| key.key.as_str())
        .collect::<Vec<_>>();
    let known = if listed.is_empty() {
        "it declares none".to_owned()
    } else {
        format!("it declares {}", listed.join(", "))
    };
    use crate::domain::release_facts::{FACT_PREFIX, INPUT_PREFIX};
    if change.kind == ValueKind::Variable
        && (change.key.starts_with(FACT_PREFIX) || change.key.starts_with(INPUT_PREFIX))
    {
        return MmError::Environment {
            message: format!(
                "'{}' is a release fact: each deployment takes it from its commit, its build, and the --release and --release-input given to `mm -e {environment} deploy plan`, so hosts hold no value for it",
                change.key
            ),
        };
    }
    MmError::Environment {
        message: match change.kind {
            ValueKind::Secret => format!(
                "neither the commit deployed to '{environment}' nor one planned on this host declares a secret '{}' ({known}). Add it under `secrets:` in .config/mm.yaml, run `mm -e {environment} deploy plan` for that commit, then set it",
                change.key
            ),
            ValueKind::Variable => format!(
                "'{}' is not a variable '{environment}' lets you set on its hosts ({known}). A value fixed in .config/mm.yaml is changed in the repo; a new variable needs a template that uses it, planned here first",
                change.key
            ),
        },
    }
}

/// The staged changes a plan of the checkout in `selection` makes live.
pub(super) async fn staged_plan(
    selection: &WorkspaceSelection,
    mm_home: &Path,
    context: &RemoteRequestContext,
) -> Result<Vec<StagedPlanItem>, MmError> {
    let (_, _, keys) = declared(selection, mm_home).await?;
    let reports = reports(mm_home, context, &keys).await?;
    Ok(keys
        .into_iter()
        .zip(reports)
        .filter_map(|(key, report)| {
            let staged = report.staged?;
            let change = if staged.removal {
                StagedChangeKind::Removed
            } else if report.active.is_some() {
                StagedChangeKind::Changed
            } else {
                StagedChangeKind::Added
            };
            Some(StagedPlanItem {
                key: key.key,
                kind: key.kind,
                change,
                revision: staged.revision,
                staged_by: staged.staged_by,
                services: key.services.into_iter().collect(),
            })
        })
        .collect())
}

/// After a deployment of the checkout in `selection`: its planned staged
/// changes go live.
pub(super) async fn go_live(
    selection: &WorkspaceSelection,
    mm_home: &Path,
    context: &RemoteRequestContext,
    staged: &[StagedPlanItem],
) -> Result<(), MmError> {
    let (_, _, keys) = declared(selection, mm_home).await?;
    let (elsewhere, here): (Vec<_>, Vec<_>) = staged
        .iter()
        .filter_map(|item| {
            keys.iter()
                .find(|key| key.kind == item.kind && key.key == item.key)
                .and_then(|key| key.slot.clone())
                .map(|slot| (slot, item.revision.clone()))
        })
        .partition(|(slot, _)| slot.kept_elsewhere());
    let now = crate::adapters::clock::SystemClock.now_unix_ms();
    let store = store(mm_home, context)?;
    store.go_live(&here, now)?;
    let home = MmHome::resolve(Some(mm_home))?;
    for (slot, revision) in elsewhere {
        let StagedSlot::Secret {
            environment,
            reference,
        } = &slot
        else {
            continue;
        };
        let Some(change) = store.staged_change(&slot, &revision)? else {
            continue;
        };
        let live =
            super::providers::cluster_store(&home, context.scope.project_id, reference.scheme())?;
        let scope = crate::domain::secret::SecretScope::new(environment.as_str());
        match change {
            Some(value) => {
                live.write_live(&scope, reference, (&value, &revision), now)
                    .await?;
            }
            None => {
                live.remove_live(&scope, reference).await?;
            }
        }
        store.settle_staged(&slot, &revision)?;
    }
    Ok(())
}

/// The keys that are required and would be unset on this node after its
/// staged changes go live, for a plan to refuse.
pub(super) async fn unset_required(
    selection: &WorkspaceSelection,
    mm_home: &Path,
    context: &RemoteRequestContext,
) -> Result<Vec<(ValueKind, String, Vec<String>)>, MmError> {
    let (_, _, keys) = declared(selection, mm_home).await?;
    let reports = reports(mm_home, context, &keys).await?;
    Ok(keys
        .into_iter()
        .zip(reports)
        .filter(|(key, report)| {
            key.required
                && key.slot.is_some()
                && match &report.staged {
                    Some(staged) => staged.removal,
                    None => report.active.is_none(),
                }
        })
        .map(|(key, _)| (key.kind, key.key, key.services.into_iter().collect()))
        .collect())
}

/// Seals staged values with age to each node's store recipient.
pub struct AgeValueSealer;

impl crate::app::ValueSealer for AgeValueSealer {
    fn seal(
        &self,
        recipient: &str,
        payload: &crate::domain::staged::StagedPayload,
    ) -> Result<String, MmError> {
        crate::adapters::secrets::seal_staged_values(recipient, payload)
    }
}

/// `mm secret migrate` on this host.
pub(super) async fn migrate_to_store(
    selection: &WorkspaceSelection,
    mm_home: &Path,
    context: &RemoteRequestContext,
    provider: &str,
) -> Result<crate::domain::secret_store::StoreMigrated, MmError> {
    let (document, environment, keys) = declared(selection, mm_home).await?;
    if !document.config.providers.iter().any(|(name, instance)| {
        name.as_str() == provider && crate::domain::secret_store::is_cluster_store(instance)
    }) {
        return Err(MmError::Environment {
            message: format!(
                "'{provider}' is not the managed store of '{environment}' in the commit this host runs; declare it under providers: with `mode: managed` and `scope: environment`, deploy, then migrate"
            ),
        });
    }
    let home = MmHome::resolve(Some(mm_home))?;
    let target = super::providers::cluster_store(&home, context.scope.project_id, provider)?;
    let destination_provider = crate::domain::secret_contract::StorageProviderId::new(provider)
        .map_err(|message| MmError::Secret { message })?;
    let age = store(mm_home, context)?;
    let scope = crate::domain::secret::SecretScope::new(environment.as_str());
    let now = crate::adapters::clock::SystemClock.now_unix_ms();
    let mut secrets = Vec::new();
    for key in keys {
        let Some(StagedSlot::Secret { reference, .. }) = &key.slot else {
            continue;
        };
        if reference.scheme() != "age" {
            continue;
        }
        let location = reference.locator().to_owned();
        let Some((revision, value)) = age.live_secret(&scope, reference)? else {
            secrets.push(crate::domain::secret_store::MigratedSecret {
                key: key.key,
                location,
                revision: None,
                written: false,
            });
            continue;
        };
        let destination =
            crate::domain::secret::SecretReference::new(destination_provider.clone(), &location)
                .map_err(|message| MmError::Secret { message })?;
        let written = target
            .write_live(&scope, &destination, (&value, &revision), now)
            .await?;
        secrets.push(crate::domain::secret_store::MigratedSecret {
            key: key.key,
            location,
            revision: Some(revision),
            written,
        });
    }
    Ok(crate::domain::secret_store::StoreMigrated {
        node: context.scope.node.clone().unwrap_or_default(),
        provider: provider.to_owned(),
        secrets,
    })
}

/// A plan refused because the node would deploy without a required value.
pub(super) fn unset_on_host(
    environment: &str,
    unset: &[(crate::domain::staged::ValueKind, String, Vec<String>)],
) -> MmError {
    let listed = unset
        .iter()
        .map(|(kind, key, services)| {
            let command = match kind {
                crate::domain::staged::ValueKind::Secret => "secret",
                crate::domain::staged::ValueKind::Variable => "env",
            };
            format!(
                "{} '{key}' (used by {}): run `mm -e {environment} {command} set {key}`",
                kind.as_str(),
                services.join(", ")
            )
        })
        .collect::<Vec<_>>();
    MmError::Environment {
        message: format!(
            "this host has no value for what the deployment needs: {}. Then plan again",
            listed.join("; ")
        ),
    }
}
