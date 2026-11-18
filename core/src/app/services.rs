use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::app::TargetIngressManager;
use crate::app::ports::{
    Clock, DashboardHostSource, GatePolicySource, ServiceStackArtifactInput, ServiceStackExecutor,
    ServiceStackStore, TargetOperationLock, TargetStore,
};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::error::MmError;
use crate::domain::ownership::{ResourceKind, ResourceOwnership};
use crate::domain::runtime::RuntimeKind;
use crate::domain::service_stack::{
    CAP_HOST, CAP_IMAGE, CAP_PORT, CAP_VERSION, DASHBOARD_BACKEND_PORT, DASHBOARD_HOST,
    DashboardServiceConfig, MAILPIT_HOST, MAILPIT_IMAGE, MAILPIT_SMTP_PORT, MAILPIT_UI_PORT,
    MAILPIT_VERSION, PersistentVolumeOwner, SERVICE_STACK_SCHEMA, SERVICE_STACK_VERSION,
    ServiceComponentDescriptor, ServiceComponentKind, ServiceStackAction, ServiceStackJournalState,
    ServiceStackPlan, ServiceStackResult, ServiceStackState, ServiceStackStatus, VALKEY_IMAGE,
    VALKEY_VERSION, cap_valkey_data_volume_name, development_network_name,
    mailpit_data_volume_name, services_project_name,
};
use crate::domain::target::NamedTarget;
use crate::domain::target_ingress::{
    ingress_acme_volume_name, ingress_dynamic_config_volume_name, ingress_network_name,
};
use crate::domain::traefik_dynamic::{
    TRAEFIK_VERIFY_ENTRYPOINT, TRAEFIK_VERIFY_HEADER, TRAEFIK_VERIFY_HEADER_VALUE,
    TRAEFIK_VERIFY_MIDDLEWARE,
};

mod rendering;
pub(crate) use rendering::render_dashboard_service_unit;
use rendering::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServicePlanRequest {
    pub action: ServiceStackAction,
    pub host_smtp_port: Option<u16>,
    pub purge_data: bool,
    pub include_gate: bool,
}

#[derive(Clone)]
pub struct ServiceStackManager {
    runtime: RuntimeKind,
    targets: Arc<dyn TargetStore>,
    ingress: TargetIngressManager,
    store: Arc<dyn ServiceStackStore>,
    executor: Arc<dyn ServiceStackExecutor>,
    locks: Arc<dyn TargetOperationLock>,
    clock: Arc<dyn Clock>,
    dashboard_ingress_token: String,
    dashboard_service: DashboardServiceConfig,
    dashboard_hosts: Option<Arc<dyn DashboardHostSource>>,
    gate_policies: Option<Arc<dyn GatePolicySource>>,
}

#[derive(Clone)]
pub struct ServiceStackManagerConfig {
    pub runtime: RuntimeKind,
    pub targets: Arc<dyn TargetStore>,
    pub ingress: TargetIngressManager,
    pub store: Arc<dyn ServiceStackStore>,
    pub executor: Arc<dyn ServiceStackExecutor>,
    pub locks: Arc<dyn TargetOperationLock>,
    pub clock: Arc<dyn Clock>,
    pub dashboard_ingress_token: String,
    pub dashboard_service: DashboardServiceConfig,
}

impl std::fmt::Debug for ServiceStackManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceStackManager")
            .finish_non_exhaustive()
    }
}

impl ServiceStackManager {
    #[must_use]
    pub fn new(config: ServiceStackManagerConfig) -> Self {
        Self {
            runtime: config.runtime,
            targets: config.targets,
            ingress: config.ingress,
            store: config.store,
            executor: config.executor,
            locks: config.locks,
            clock: config.clock,
            dashboard_ingress_token: config.dashboard_ingress_token,
            dashboard_service: config.dashboard_service,
            dashboard_hosts: None,
            gate_policies: None,
        }
    }

    #[must_use]
    pub fn with_dashboard_hosts(mut self, source: Arc<dyn DashboardHostSource>) -> Self {
        self.dashboard_hosts = Some(source);
        self
    }

    #[must_use]
    pub fn with_gate_policies(mut self, source: Arc<dyn GatePolicySource>) -> Self {
        self.gate_policies = Some(source);
        self
    }

    pub fn plan(&self, request: ServicePlanRequest) -> Result<ServiceStackPlan, MmError> {
        let target = self.local_target()?;
        let current = self.store.load_state()?;
        validate_request(request, current.as_ref(), self.runtime, &target)?;
        let host_smtp_port = request
            .host_smtp_port
            .or_else(|| current.as_ref().and_then(|state| state.host_smtp_port));
        let previous_state_digest = current
            .as_ref()
            .map(ServiceStackState::digest)
            .transpose()
            .map_err(|message| MmError::State { message })?
            .unwrap_or_else(|| "absent".to_owned());
        let ingress = self
            .ingress
            .plan_for_runtime("local", self.runtime)?
            .artifact;
        let development_network = development_network_name(target.definition.id);
        let mailpit_data_volume = mailpit_data_volume_name(target.definition.id);
        let gate_enabled = request.include_gate
            || self
                .gate_policies
                .as_ref()
                .map_or(Ok(false), |source| source.gate_components_required())?;
        let cap_valkey_data_volume =
            gate_enabled.then(|| cap_valkey_data_volume_name(target.definition.id));
        let cap_admin_key_file = if gate_enabled {
            Some(self.store.ensure_cap_admin_key_file()?)
        } else {
            None
        };
        let components = components(
            &target,
            &mailpit_data_volume,
            cap_valkey_data_volume.as_deref(),
        );
        let compose = render_compose(
            &target,
            &development_network,
            &mailpit_data_volume,
            host_smtp_port,
            cap_valkey_data_volume.as_deref(),
            cap_admin_key_file.as_deref(),
        )?;
        let dashboard_hosts = self.dashboard_hosts()?;
        validate_dashboard_ingress_token(&self.dashboard_ingress_token)?;
        let route = render_route(
            &dashboard_hosts,
            &self.dashboard_ingress_token,
            gate_enabled,
        );
        let dashboard_unit = self.dashboard_unit(&ingress)?;
        let identity = artifact_identity(
            request,
            self.runtime,
            &target,
            &previous_state_digest,
            [&compose, &route, &dashboard_unit],
            &ingress.manifest.artifact_hash,
        );
        let artifact = self.publish_artifact(
            &identity,
            target.definition.id,
            &components,
            &compose,
            &route,
            &dashboard_unit,
        )?;
        let mut plan = ServiceStackPlan {
            schema: SERVICE_STACK_SCHEMA,
            action: request.action,
            stack_version: SERVICE_STACK_VERSION.to_owned(),
            runtime: self.runtime,
            target_id: target.definition.id,
            target_fingerprint: target.definition.canonical_fingerprint(),
            components,
            dashboard_hosts: dashboard_hosts.into_iter().collect(),
            dashboard_service: Some(self.dashboard_service.clone()),
            development_network,
            mailpit_data_volume,
            cap_valkey_data_volume,
            cap_admin_key_file,
            previous_host_smtp_port: current.as_ref().and_then(|state| state.host_smtp_port),
            host_smtp_port,
            purge_data: request.purge_data,
            previous_state_digest,
            ingress,
            artifact,
            plan_hash: String::new(),
        };
        plan.plan_hash = plan
            .canonical_hash()
            .map_err(|message| MmError::State { message })?;
        plan.validate()
            .map_err(|message| MmError::Security { message })?;
        Ok(plan)
    }

    /// Setup repairs an interrupted journal before proposing new convergence.
    pub fn setup_action(&self) -> Result<ServiceStackAction, MmError> {
        Ok(
            if self
                .store
                .load_state()?
                .is_some_and(|state| state.journal_state != ServiceStackJournalState::Applied)
            {
                ServiceStackAction::Repair
            } else {
                ServiceStackAction::Apply
            },
        )
    }

    fn local_target(&self) -> Result<NamedTarget, MmError> {
        self.targets
            .resolve_target("local")
            .map_err(|_| MmError::Target {
                message:
                    "the local target is missing; run `mm dev setup` before planning shared services"
                        .to_owned(),
            })
    }

    /// The dashboard service follows the local ingress platform: a systemd unit on
    /// Linux, a launchd agent on macOS.
    fn dashboard_unit(
        &self,
        ingress: &crate::domain::target_ingress::PreparedTargetIngressArtifact,
    ) -> Result<Vec<u8>, MmError> {
        render_dashboard_service_unit(
            &self.dashboard_service,
            self.runtime,
            ingress.manifest.macos_host.as_ref(),
        )
    }

    fn dashboard_hosts(&self) -> Result<BTreeSet<String>, MmError> {
        let mut hosts = self.dashboard_hosts.as_ref().map_or_else(
            || Ok(BTreeSet::new()),
            |source| source.project_dashboard_hosts(),
        )?;
        hosts.insert(DASHBOARD_HOST.to_owned());
        Ok(hosts)
    }

    fn publish_artifact(
        &self,
        identity: &str,
        target_id: crate::domain::target::TargetId,
        components: &[ServiceComponentDescriptor],
        compose: &[u8],
        route: &[u8],
        dashboard_unit: &[u8],
    ) -> Result<crate::domain::service_stack::PreparedServiceStackArtifact, MmError> {
        let manifest = serde_json::to_vec_pretty(&serde_json::json!({
            "schema": SERVICE_STACK_SCHEMA,
            "stack_version": SERVICE_STACK_VERSION,
            "runtime": self.runtime,
            "target_id": target_id,
            "components": components,
            "compose_digest": blake3::hash(compose).to_hex().to_string(),
            "route_digest": blake3::hash(route).to_hex().to_string(),
            "dashboard_unit_digest": blake3::hash(dashboard_unit).to_hex().to_string(),
        }))
        .map_err(|error| MmError::State {
            message: format!("could not serialize service artifact manifest: {error}"),
        })?;
        self.store.publish(ServiceStackArtifactInput {
            plan_identity: identity,
            manifest: &manifest,
            compose,
            route,
            dashboard_unit,
        })
    }

    pub async fn execute_reviewed(
        &self,
        request: ServicePlanRequest,
        expected_plan_hash: &str,
    ) -> Result<ServiceStackResult, MmError> {
        let preview = self.plan(request)?;
        require_hash(&preview, expected_plan_hash)?;
        let _guard = self.locks.acquire(preview.target_id)?;
        let plan = self.plan(request)?;
        require_hash(&plan, expected_plan_hash)?;
        let current = self.store.load_state()?;
        if request.action == ServiceStackAction::Repair
            && current.as_ref().is_some_and(|state| {
                matches!(
                    state.journal_state,
                    ServiceStackJournalState::Applying
                        | ServiceStackJournalState::Removing
                        | ServiceStackJournalState::Failed
                )
            })
        {
            return self
                .recover(
                    current.ok_or_else(|| MmError::State {
                        message: "interrupted service state disappeared".to_owned(),
                    })?,
                    &plan,
                )
                .await;
        }
        self.executor.preflight(&plan).await?;
        let operation_id = uuid::Uuid::new_v4();
        let transition = ServiceStackState {
            schema: SERVICE_STACK_SCHEMA,
            installation_id: current
                .as_ref()
                .map_or_else(uuid::Uuid::new_v4, |state| state.installation_id),
            runtime: plan.runtime,
            target_id: plan.target_id,
            stack_version: plan.stack_version.clone(),
            plan_hash: plan.plan_hash.clone(),
            artifact: plan.artifact.clone(),
            ingress: plan.ingress.clone(),
            development_network: plan.development_network.clone(),
            mailpit_data_volume: plan.mailpit_data_volume.clone(),
            cap_valkey_data_volume: plan.cap_valkey_data_volume.clone(),
            host_smtp_port: plan.host_smtp_port,
            journal_state: if plan.action == ServiceStackAction::Remove {
                ServiceStackJournalState::Removing
            } else {
                ServiceStackJournalState::Applying
            },
            operation_id,
            updated_at_unix_ms: self.clock.now_unix_ms(),
            previous: current.clone().map(Box::new),
            failure_code: None,
            applied_plan: Box::new(plan.clone()),
        };
        self.store.save_state(&transition)?;
        let result = if plan.action == ServiceStackAction::Remove {
            self.executor.remove(&plan).await
        } else {
            self.executor.apply(&plan).await
        };
        match result {
            Ok(()) => {
                if plan.action == ServiceStackAction::Remove {
                    self.store.remove_state()?;
                    return Ok(ServiceStackResult {
                        action: plan.action,
                        operation_id,
                        plan_hash: plan.plan_hash,
                        state: ServiceStackJournalState::Removed,
                        rolled_back: false,
                        data_purged: plan.purge_data,
                    });
                }
                let mut applied = transition;
                applied.journal_state = ServiceStackJournalState::Applied;
                applied.previous = None;
                applied.updated_at_unix_ms = self.clock.now_unix_ms();
                self.store.save_state(&applied)?;
                Ok(ServiceStackResult {
                    action: plan.action,
                    operation_id,
                    plan_hash: plan.plan_hash,
                    state: ServiceStackJournalState::Applied,
                    rolled_back: false,
                    data_purged: false,
                })
            }
            Err(error) => self.rollback_failed(transition, error).await,
        }
    }

    pub async fn status(&self) -> Result<ServiceStackStatus, MmError> {
        Self::inspect_status(self.runtime, self.store.as_ref(), self.executor.as_ref()).await
    }

    /// Status needs neither credentials nor mutable state repositories.
    pub async fn inspect_status(
        runtime: RuntimeKind,
        store: &dyn ServiceStackStore,
        executor: &dyn ServiceStackExecutor,
    ) -> Result<ServiceStackStatus, MmError> {
        let Some(state) = store.load_state()? else {
            return Ok(ServiceStackStatus {
                installed: false,
                stack_version: None,
                runtime: None,
                target_id: None,
                route: format!("https://{MAILPIT_HOST}"),
                internal_smtp: format!("mailpit:{MAILPIT_SMTP_PORT}"),
                host_smtp_port: None,
                components: Vec::new(),
                diagnostics: vec![
                    Diagnostic::warning(
                        "MM_SERVICES_NOT_INSTALLED",
                        "the installation-wide local service stack is not installed",
                    )
                    .with_help("run `mm dev services plan`, review the hash, then apply it"),
                ],
            });
        };
        let mut diagnostics = Vec::new();
        if state.runtime != runtime {
            diagnostics.push(Diagnostic::error(
                "MM_SERVICES_RUNTIME_MISMATCH",
                format!(
                    "the stack is owned by {:?}, not the selected {:?} runtime",
                    state.runtime, runtime
                ),
            ));
            return Ok(status_from_state(&state, Vec::new(), diagnostics));
        }
        if state.stack_version != SERVICE_STACK_VERSION {
            diagnostics.push(
                Diagnostic::error(
                    "MM_SERVICES_VERSION_DRIFT",
                    format!(
                        "installed service schema {} differs from supported {}",
                        state.stack_version, SERVICE_STACK_VERSION
                    ),
                )
                .with_help("review `mm dev services plan --operation upgrade`"),
            );
        }
        if state.journal_state != ServiceStackJournalState::Applied {
            diagnostics.push(
                Diagnostic::error(
                    "MM_SERVICES_RECOVERY_REQUIRED",
                    format!("service journal is {:?}", state.journal_state),
                )
                .with_help("review and run a services repair plan"),
            );
        }
        let observed = executor.status(&state.applied_plan).await?;
        diagnostics.extend(observed.diagnostics);
        Ok(status_from_state(&state, observed.components, diagnostics))
    }

    async fn recover(
        &self,
        state: ServiceStackState,
        reviewed: &ServiceStackPlan,
    ) -> Result<ServiceStackResult, MmError> {
        let operation_id = state.operation_id;
        if let Some(previous) = state.previous.as_deref() {
            self.executor.apply(&previous.applied_plan).await?;
            let mut restored = previous.clone();
            restored.journal_state = ServiceStackJournalState::Applied;
            restored.previous = None;
            restored.updated_at_unix_ms = self.clock.now_unix_ms();
            self.store.save_state(&restored)?;
        } else {
            return self.complete_interrupted_install(state, reviewed).await;
        }
        Ok(ServiceStackResult {
            action: ServiceStackAction::Repair,
            operation_id,
            plan_hash: reviewed.plan_hash.clone(),
            state: ServiceStackJournalState::Applied,
            rolled_back: true,
            data_purged: false,
        })
    }

    async fn complete_interrupted_install(
        &self,
        mut state: ServiceStackState,
        plan: &ServiceStackPlan,
    ) -> Result<ServiceStackResult, MmError> {
        // Shared networks may already have project consumers. First-install
        // recovery converges forward; it must never tear down those networks.
        self.executor.preflight(plan).await?;
        state.runtime = plan.runtime;
        state.target_id = plan.target_id;
        state.stack_version = plan.stack_version.clone();
        state.plan_hash = plan.plan_hash.clone();
        state.artifact = plan.artifact.clone();
        state.ingress = plan.ingress.clone();
        state.development_network = plan.development_network.clone();
        state.mailpit_data_volume = plan.mailpit_data_volume.clone();
        state.cap_valkey_data_volume = plan.cap_valkey_data_volume.clone();
        state.host_smtp_port = plan.host_smtp_port;
        *state.applied_plan = plan.clone();
        state.previous = None;
        state.journal_state = ServiceStackJournalState::Applying;
        state.failure_code = None;
        state.updated_at_unix_ms = self.clock.now_unix_ms();
        self.store.save_state(&state)?;
        if let Err(error) = self.executor.apply(plan).await {
            state.journal_state = ServiceStackJournalState::Failed;
            state.failure_code = Some(error.code().to_owned());
            if let Err(journal) = self.store.save_state(&state) {
                return Err(MmError::operation_failure(
                    state.operation_id.to_string(),
                    crate::domain::error::MutationStatus::Unknown,
                    error,
                    vec![journal.to_string()],
                    "repair shared services again after fixing journal storage; existing resources were retained",
                ));
            }
            return Err(error);
        }
        state.journal_state = ServiceStackJournalState::Applied;
        state.updated_at_unix_ms = self.clock.now_unix_ms();
        self.store.save_state(&state)?;
        Ok(ServiceStackResult {
            action: ServiceStackAction::Repair,
            operation_id: state.operation_id,
            plan_hash: plan.plan_hash.clone(),
            state: ServiceStackJournalState::Applied,
            rolled_back: false,
            data_purged: false,
        })
    }

    async fn rollback_failed(
        &self,
        mut transition: ServiceStackState,
        error: MmError,
    ) -> Result<ServiceStackResult, MmError> {
        let rollback = if let Some(previous) = transition.previous.as_deref() {
            self.executor.apply(&previous.applied_plan).await
        } else {
            self.executor.remove(&transition.applied_plan).await
        };
        match rollback {
            Ok(()) => {
                if let Some(previous) = transition.previous.take() {
                    self.store.save_state(&previous)?;
                } else {
                    self.store.remove_state()?;
                }
                Err(error)
            }
            Err(rollback) => {
                transition.journal_state = ServiceStackJournalState::Failed;
                transition.failure_code = Some(error.code().to_owned());
                transition.updated_at_unix_ms = self.clock.now_unix_ms();
                self.store.save_state(&transition)?;
                Err(MmError::Security {
                    message: format!(
                        "service operation failed with {} ({error}) and rollback failed with {} ({rollback}); run a reviewed repair",
                        error.code(),
                        rollback.code()
                    ),
                })
            }
        }
    }
}

fn validate_request(
    request: ServicePlanRequest,
    current: Option<&ServiceStackState>,
    runtime: RuntimeKind,
    target: &NamedTarget,
) -> Result<(), MmError> {
    if request.include_gate
        && !matches!(
            request.action,
            ServiceStackAction::Apply | ServiceStackAction::Upgrade
        )
    {
        return Err(MmError::Security {
            message: "gate service preparation is supported only for apply and upgrade plans"
                .to_owned(),
        });
    }
    if let Some(state) = current {
        if state.target_id != target.definition.id || state.runtime != runtime {
            return Err(MmError::Security {
                message: "a service stack owned by another target/runtime is already recorded; select its exact runtime and remove it first"
                    .to_owned(),
            });
        }
        if matches!(
            state.journal_state,
            ServiceStackJournalState::Applying
                | ServiceStackJournalState::Removing
                | ServiceStackJournalState::Failed
        ) && request.action != ServiceStackAction::Repair
        {
            return Err(MmError::State {
                message: "the service journal requires a reviewed repair before another operation"
                    .to_owned(),
            });
        }
    }
    match request.action {
        ServiceStackAction::Upgrade => {
            let state = current.ok_or_else(|| MmError::State {
                message: "cannot upgrade an uninstalled service stack".to_owned(),
            })?;
            let installed =
                semver::Version::parse(&state.stack_version).map_err(|error| MmError::State {
                    message: format!("installed service stack version is invalid: {error}"),
                })?;
            let desired =
                semver::Version::parse(SERVICE_STACK_VERSION).map_err(|error| MmError::State {
                    message: format!("compiled service stack version is invalid: {error}"),
                })?;
            if installed >= desired {
                return Err(MmError::State {
                    message: "the installed service stack is already at the current version"
                        .to_owned(),
                });
            }
        }
        ServiceStackAction::Repair | ServiceStackAction::Remove if current.is_none() => {
            return Err(MmError::State {
                message: format!(
                    "cannot {} an uninstalled service stack",
                    request.action.as_str()
                ),
            });
        }
        ServiceStackAction::Apply | ServiceStackAction::Repair | ServiceStackAction::Remove => {}
    }
    if request.purge_data && request.action != ServiceStackAction::Remove {
        return Err(MmError::Security {
            message: "data purge is valid only for a reviewed remove plan".to_owned(),
        });
    }
    if let Some(port) = request.host_smtp_port
        && matches!(port, 80 | 443 | MAILPIT_UI_PORT)
    {
        return Err(MmError::Security {
            message: format!("host SMTP port {port} conflicts with a reserved service port"),
        });
    }
    Ok(())
}
