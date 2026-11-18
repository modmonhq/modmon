use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};

use crate::domain::config::MmConfig;
use crate::domain::secret::DirectSecretRotationPlan;
use crate::domain::secret_backend::{
    ManagedSecretBackendAction, ManagedSecretBackendPlan, ManagedSecretBackendPlanRequest,
    ManagedSecretBackendStatus, ManagedSecretDataAction, ManagedSecretDataPlan,
    ProviderConfiguration, ProviderConfigurationField, ProviderDescriptor, ProviderInstanceConfig,
    ProviderInstanceDescriptor, ProviderInstanceName, ProviderMode, ProviderOperation,
    RemoteSecretBackendSetupPlan, RemoteSecretBackendSetupRequest, RemoteSecretBackendStatus,
    RemoteSecretBackendStatusRequest,
};
use crate::domain::workspace::ProjectId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretBackendTuiContext {
    pub root: PathBuf,
    pub target: String,
    pub config_hash: String,
    pub project_id: ProjectId,
    pub environment: String,
    pub external_connections: Vec<ConnectionScope>,
    pub managed_connections: Vec<ConnectionScope>,
    pub drivers: Vec<ProviderDescriptor>,
    pub instances: std::collections::BTreeMap<ProviderInstanceName, ProviderInstanceConfig>,
    pub rotatable_secrets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionScope {
    pub instance: ProviderInstanceDescriptor,
    pub configuration: ProviderConfiguration,
    pub name: String,
    pub locations: Vec<String>,
}

impl SecretBackendTuiContext {
    #[must_use]
    pub fn from_config(
        root: PathBuf,
        config_hash: String,
        config: &MmConfig,
        environment: String,
        drivers: Vec<ProviderDescriptor>,
        instances: Vec<ProviderInstanceDescriptor>,
    ) -> Self {
        let scopes = instances
            .into_iter()
            .filter_map(|instance| {
                config
                    .providers
                    .get(&instance.name)
                    .map(|configured| ConnectionScope {
                        locations: provider_locations(config, &instance.name),
                        name: instance.name.to_string(),
                        configuration: configured.configuration.clone(),
                        instance,
                    })
            })
            .collect::<Vec<_>>();
        Self {
            root,
            target: config.target.clone(),
            config_hash,
            project_id: config.project.id,
            environment,
            external_connections: scopes
                .iter()
                .filter(|scope| scope.instance.mode == ProviderMode::External)
                .cloned()
                .collect(),
            managed_connections: scopes
                .iter()
                .filter(|scope| scope.instance.mode == ProviderMode::Managed)
                .cloned()
                .collect(),
            drivers,
            instances: config.providers.clone(),
            rotatable_secrets: config
                .secrets
                .iter()
                .filter(|(_, declaration)| {
                    config
                        .providers
                        .keys()
                        .any(|name| name.as_str() == declaration.storage.provider.as_str())
                })
                .map(|(name, _)| name.clone())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderModel {
    pub context: SecretBackendTuiContext,
    pub screen: ProviderScreen,
    pub selected: usize,
    pub selected_managed: usize,
    pub managed_status: Option<ManagedSecretBackendStatus>,
    pub remote_statuses: Vec<RemoteSecretBackendStatus>,
    pub last_review: Option<ProviderReview>,
    pub managed_release: String,
}

impl ProviderModel {
    #[must_use]
    pub fn new(context: SecretBackendTuiContext) -> Self {
        let managed_release = context
            .managed_connections
            .first()
            .and_then(managed_configuration)
            .map_or_else(String::new, |managed| managed.release);
        Self {
            context,
            screen: ProviderScreen::Dashboard,
            selected: 0,
            selected_managed: 0,
            managed_status: None,
            remote_statuses: Vec::new(),
            last_review: None,
            managed_release,
        }
    }

    fn selected_external(&self) -> Option<&ConnectionScope> {
        self.context.external_connections.get(self.selected)
    }

    fn selected_secret(&self) -> Option<&str> {
        let offset = self.context.external_connections.len();
        self.context
            .rotatable_secrets
            .get(self.selected.checked_sub(offset)?)
            .map(String::as_str)
    }

    fn selected_managed(&self) -> Option<&ConnectionScope> {
        self.context.managed_connections.get(self.selected_managed)
    }

    fn selection_count(&self) -> usize {
        self.context.external_connections.len() + self.context.rotatable_secrets.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderScreen {
    Dashboard,
    ExternalSetup(ExternalSetupForm),
    DataPath(DataPathForm),
    Review(Box<ProviderReview>),
    Applying(String),
    Complete(String),
    Error(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSetupForm {
    pub instance: ProviderInstanceName,
    pub fields: Vec<ProviderConfigurationField>,
    pub values: std::collections::BTreeMap<String, String>,
    pub allow_bootstrap: bool,
    pub field: usize,
}

impl ExternalSetupForm {
    fn new(scope: &ConnectionScope, drivers: &[ProviderDescriptor]) -> Self {
        let fields = drivers
            .iter()
            .find(|driver| driver.driver == scope.instance.driver)
            .and_then(|driver| driver.mode(ProviderMode::External))
            .map_or_else(Vec::new, |mode| mode.configuration.clone());
        let mut values = std::collections::BTreeMap::new();
        if let ProviderConfiguration::External {
            address,
            namespace,
            storage_mount,
            automation_auth_mount,
            trust_bundle,
            expected_trust_sha256,
        } = &scope.configuration
        {
            values.insert("address".to_owned(), address.clone());
            values.insert(
                "namespace".to_owned(),
                namespace.clone().unwrap_or_default(),
            );
            values.insert("storage-mount".to_owned(), storage_mount.clone());
            values.insert(
                "automation-auth-mount".to_owned(),
                automation_auth_mount.clone(),
            );
            values.insert(
                "trust-bundle".to_owned(),
                trust_bundle.display().to_string(),
            );
            values.insert(
                "expected-trust-sha256".to_owned(),
                expected_trust_sha256.clone(),
            );
        }
        Self {
            instance: scope.instance.name.clone(),
            fields,
            values,
            allow_bootstrap: false,
            field: 0,
        }
    }

    pub fn displayed_fields(&self) -> Vec<(&str, &str)> {
        self.fields
            .iter()
            .map(|field| {
                (
                    field.label.as_str(),
                    self.values.get(&field.key).map_or("", String::as_str),
                )
            })
            .collect()
    }

    fn active_mut(&mut self) -> Option<&mut String> {
        let key = self.fields.get(self.field)?.key.clone();
        Some(self.values.entry(key).or_default())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPathForm {
    pub instance: ProviderInstanceName,
    pub action: ManagedSecretDataAction,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderReview {
    ExternalSetup {
        request: RemoteSecretBackendSetupRequest,
        plan: Box<RemoteSecretBackendSetupPlan>,
    },
    Managed {
        request: ManagedSecretBackendPlanRequest,
        plan: ManagedSecretBackendPlan,
    },
    Data(ManagedSecretDataPlan),
    Rotation(DirectSecretRotationPlan),
}

impl ProviderReview {
    #[must_use]
    pub fn title(&self) -> String {
        match self {
            Self::ExternalSetup { plan, .. } => {
                format!(
                    "External provider '{}' {}",
                    plan.request.instance,
                    plan.request.operation.as_str()
                )
            }
            Self::Managed { plan, .. } => {
                format!(
                    "Managed provider '{}' {}",
                    plan.instance,
                    plan.action.as_str()
                )
            }
            Self::Data(plan) => format!(
                "Managed provider '{}' {}",
                plan.instance,
                plan.action.as_str()
            ),
            Self::Rotation(plan) => format!("Rotate secret '{}'", plan.name),
        }
    }

    #[must_use]
    pub fn plan_hash(&self) -> &str {
        match self {
            Self::ExternalSetup { plan, .. } => &plan.plan_hash,
            Self::Managed { plan, .. } => &plan.plan_hash,
            Self::Data(plan) => &plan.plan_hash,
            Self::Rotation(plan) => &plan.plan_hash,
        }
    }

    #[must_use]
    pub fn consequences(&self) -> Vec<String> {
        match self {
            Self::ExternalSetup { plan, .. } => plan.consequences.clone(),
            Self::Managed { plan, .. } => vec![
                format!("Use immutable artifact {}.", plan.immutable_artifact),
                format!("Management access remains {}.", plan.management_access),
                format!("Owned data resource is {}.", plan.data_resource),
                format!("Purge persistent data: {}.", plan.purge_data),
            ],
            Self::Data(plan) => plan.consequences.clone(),
            Self::Rotation(plan) => vec![
                format!(
                    "Replace revision {} with a CAS-guarded write.",
                    plan.current_revision
                ),
                format!("Hidden input required: {}.", plan.requires_hidden_input),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderAction {
    PlanExternal(RemoteSecretBackendSetupRequest),
    TestExternal(RemoteSecretBackendStatusRequest),
    PlanManaged(ManagedSecretBackendPlanRequest),
    ManagedStatus(ProviderInstanceName),
    PlanData(ProviderInstanceName, ManagedSecretDataAction, PathBuf),
    PlanRotation(String),
    ApplyReview,
    RetryApply,
    ReportError(String),
    None,
}

pub fn update(model: &mut ProviderModel, key: KeyEvent) -> Option<ProviderAction> {
    if key.code == KeyCode::Esc
        && matches!(
            model.screen,
            ProviderScreen::ExternalSetup(_) | ProviderScreen::DataPath(_)
        )
    {
        model.screen = ProviderScreen::Dashboard;
        return Some(ProviderAction::None);
    }
    match &mut model.screen {
        ProviderScreen::Dashboard => update_dashboard(model, key),
        ProviderScreen::ExternalSetup(form) => {
            Some(update_external_form(&model.context, form, key))
        }
        ProviderScreen::DataPath(form) => Some(update_data_path(form, key)),
        ProviderScreen::Review(review) => Some(match key.code {
            KeyCode::Enter => {
                let title = review.title();
                model.screen = ProviderScreen::Applying(format!("Applying {title}..."));
                ProviderAction::ApplyReview
            }
            KeyCode::Esc | KeyCode::Char('c') => {
                model.screen = ProviderScreen::Cancelled;
                ProviderAction::None
            }
            KeyCode::Char('b') => {
                model.screen = ProviderScreen::Dashboard;
                ProviderAction::None
            }
            _ => ProviderAction::None,
        }),
        ProviderScreen::Error(_) => Some(match key.code {
            KeyCode::Char('r') | KeyCode::Enter if model.last_review.is_some() => {
                model.screen =
                    ProviderScreen::Applying("Retrying exact reviewed plan...".to_owned());
                ProviderAction::RetryApply
            }
            KeyCode::Esc | KeyCode::Char('b') => {
                model.screen = ProviderScreen::Dashboard;
                ProviderAction::None
            }
            _ => ProviderAction::None,
        }),
        ProviderScreen::Cancelled => Some(match key.code {
            KeyCode::Char('r') => {
                model.screen =
                    ProviderScreen::Applying("Resuming exact reviewed plan...".to_owned());
                ProviderAction::RetryApply
            }
            KeyCode::Esc | KeyCode::Char('b') => {
                model.screen = ProviderScreen::Dashboard;
                ProviderAction::None
            }
            _ => ProviderAction::None,
        }),
        ProviderScreen::Complete(_) => Some(match key.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('b') => {
                model.screen = ProviderScreen::Dashboard;
                ProviderAction::None
            }
            _ => ProviderAction::None,
        }),
        ProviderScreen::Applying(_) => Some(ProviderAction::None),
    }
}

fn update_dashboard(model: &mut ProviderModel, key: KeyEvent) -> Option<ProviderAction> {
    let action = match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            model.selected = (model.selected + 1).min(model.selection_count().saturating_sub(1));
            ProviderAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            model.selected = model.selected.saturating_sub(1);
            ProviderAction::None
        }
        KeyCode::Char('x') => {
            let Some(scope) = model
                .selected_external()
                .or_else(|| model.context.external_connections.first())
            else {
                return Some(ProviderAction::None);
            };
            model.screen = ProviderScreen::ExternalSetup(ExternalSetupForm::new(
                scope,
                &model.context.drivers,
            ));
            ProviderAction::None
        }
        KeyCode::Char('t') => {
            let request = model
                .selected_external()
                .map(|scope| RemoteSecretBackendStatusRequest {
                    instance: scope.instance.name.clone(),
                    project_id: model.context.project_id,
                    environment: model.context.environment.clone(),
                });
            request.map_or(ProviderAction::None, |request| {
                model.screen = ProviderScreen::Applying(
                    "Testing TLS, health, AppRole login, and exact-scope metadata access..."
                        .to_owned(),
                );
                ProviderAction::TestExternal(request)
            })
        }
        KeyCode::Char('s') => {
            model.screen = ProviderScreen::Applying(
                "Refreshing managed health/seal diagnostics...".to_owned(),
            );
            model
                .selected_managed()
                .map_or(ProviderAction::None, |scope| {
                    ProviderAction::ManagedStatus(scope.instance.name.clone())
                })
        }
        _ => return update_managed_dashboard(model, key),
    };
    Some(action)
}

fn update_managed_dashboard(model: &mut ProviderModel, key: KeyEvent) -> Option<ProviderAction> {
    let action = match key.code {
        KeyCode::Char('p') => {
            if !model.context.managed_connections.is_empty() {
                model.selected_managed =
                    (model.selected_managed + 1) % model.context.managed_connections.len();
                model.managed_release = managed_configuration(
                    &model.context.managed_connections[model.selected_managed],
                )
                .map_or_else(String::new, |managed| managed.release);
            }
            ProviderAction::None
        }
        KeyCode::Char('[') => {
            if let Some(scope) = model.selected_managed() {
                let releases = driver_releases(&model.context.drivers, &scope.instance);
                if let Some(release) = releases.first() {
                    model.managed_release = release.clone();
                }
            }
            ProviderAction::None
        }
        KeyCode::Char(']') => {
            if let Some(scope) = model.selected_managed() {
                let releases = driver_releases(&model.context.drivers, &scope.instance);
                if let Some(release) = releases.last() {
                    model.managed_release = release.clone();
                }
            }
            ProviderAction::None
        }
        KeyCode::Char('i') => managed_action(model, ManagedSecretBackendAction::Install, false),
        KeyCode::Char('r') => managed_action(model, ManagedSecretBackendAction::Restart, false),
        KeyCode::Char('u') => managed_action(model, ManagedSecretBackendAction::Upgrade, false),
        KeyCode::Char('z') => managed_action(model, ManagedSecretBackendAction::Seal, false),
        KeyCode::Char('n') => managed_action(model, ManagedSecretBackendAction::Unseal, false),
        KeyCode::Char('d') => managed_action(model, ManagedSecretBackendAction::Remove, false),
        KeyCode::Char('D') => managed_action(model, ManagedSecretBackendAction::Remove, true),
        KeyCode::Char('b') => {
            let scope = model.selected_managed()?;
            if !scope
                .instance
                .operations
                .contains(&ProviderOperation::Backup)
            {
                return Some(ProviderAction::None);
            }
            model.screen = ProviderScreen::DataPath(DataPathForm {
                instance: scope.instance.name.clone(),
                action: ManagedSecretDataAction::Backup,
                path: format!("{}-backup.age", scope.instance.name),
            });
            ProviderAction::None
        }
        KeyCode::Char('o') => {
            let scope = model.selected_managed()?;
            if !scope
                .instance
                .operations
                .contains(&ProviderOperation::Restore)
            {
                return Some(ProviderAction::None);
            }
            model.screen = ProviderScreen::DataPath(DataPathForm {
                instance: scope.instance.name.clone(),
                action: ManagedSecretDataAction::Restore,
                path: format!("{}-backup.age", scope.instance.name),
            });
            ProviderAction::None
        }
        KeyCode::Char('v') => {
            let name = model.selected_secret().map(str::to_owned);
            name.map_or(ProviderAction::None, |name| {
                model.screen = ProviderScreen::Applying(
                    "Inspecting current metadata for a CAS rotation plan...".to_owned(),
                );
                ProviderAction::PlanRotation(name)
            })
        }
        _ => return None,
    };
    Some(action)
}

fn update_external_form(
    context: &SecretBackendTuiContext,
    form: &mut ExternalSetupForm,
    key: KeyEvent,
) -> ProviderAction {
    match key.code {
        KeyCode::Esc => ProviderAction::None,
        KeyCode::Tab | KeyCode::Down => {
            form.field = (form.field + 1).min(form.fields.len());
            ProviderAction::None
        }
        KeyCode::BackTab | KeyCode::Up => {
            form.field = form.field.saturating_sub(1);
            ProviderAction::None
        }
        KeyCode::Enter if form.field < form.fields.len() => {
            form.field += 1;
            ProviderAction::None
        }
        KeyCode::Enter => remote_request(context, form)
            .map_or_else(ProviderAction::ReportError, ProviderAction::PlanExternal),
        KeyCode::Char(' ') if form.field == form.fields.len() => {
            form.allow_bootstrap = !form.allow_bootstrap;
            ProviderAction::None
        }
        KeyCode::Backspace => {
            if let Some(value) = form.active_mut() {
                value.pop();
            }
            ProviderAction::None
        }
        KeyCode::Char(character) if !character.is_control() => {
            if let Some(value) = form.active_mut()
                && value.len() < 4096
            {
                value.push(character);
            }
            ProviderAction::None
        }
        _ => ProviderAction::None,
    }
}

fn update_data_path(form: &mut DataPathForm, key: KeyEvent) -> ProviderAction {
    match key.code {
        KeyCode::Enter => ProviderAction::PlanData(
            form.instance.clone(),
            form.action,
            PathBuf::from(&form.path),
        ),
        KeyCode::Backspace => {
            form.path.pop();
            ProviderAction::None
        }
        KeyCode::Char(character) if !character.is_control() && form.path.len() < 4096 => {
            form.path.push(character);
            ProviderAction::None
        }
        _ => ProviderAction::None,
    }
}

fn remote_request(
    context: &SecretBackendTuiContext,
    form: &ExternalSetupForm,
) -> Result<RemoteSecretBackendSetupRequest, String> {
    let scope = context
        .external_connections
        .iter()
        .find(|scope| scope.instance.name == form.instance)
        .ok_or_else(|| {
            format!(
                "provider instance '{}' has no matching external declaration in mm.yaml",
                form.instance
            )
        })?;
    let value = |key: &str| form.values.get(key).cloned().unwrap_or_default();
    Ok(RemoteSecretBackendSetupRequest {
        instance: scope.instance.name.clone(),
        operation: ProviderOperation::Add,
        address: value("address"),
        namespace: (!value("namespace").is_empty()).then(|| value("namespace")),
        storage_mount: value("storage-mount"),
        automation_auth_mount: value("automation-auth-mount"),
        trust_bundle: {
            let path = PathBuf::from(value("trust-bundle"));
            if path.is_absolute() {
                path
            } else {
                context.root.join(path)
            }
        },
        expected_trust_sha256: value("expected-trust-sha256"),
        project_id: context.project_id,
        environment: context.environment.clone(),
        locations: scope.locations.clone(),
        allow_bootstrap: form.allow_bootstrap,
        target: context.target.clone(),
        config_hash: context.config_hash.clone(),
    })
}

fn managed_action(
    model: &ProviderModel,
    action: ManagedSecretBackendAction,
    purge_data: bool,
) -> ProviderAction {
    let Some(scope) = model.selected_managed() else {
        return ProviderAction::ReportError(
            "no managed OpenBao or Vault connection and logical secret locations are declared in mm.yaml"
                .to_owned(),
        );
    };
    let Some(managed) = managed_configuration(scope) else {
        return ProviderAction::ReportError(
            "selected provider has no managed configuration".to_owned(),
        );
    };
    let required = match action {
        ManagedSecretBackendAction::Install => ProviderOperation::Install,
        ManagedSecretBackendAction::Restart => ProviderOperation::Restart,
        ManagedSecretBackendAction::Upgrade => ProviderOperation::Upgrade,
        ManagedSecretBackendAction::Seal => ProviderOperation::Seal,
        ManagedSecretBackendAction::Unseal => ProviderOperation::Unseal,
        ManagedSecretBackendAction::Remove => ProviderOperation::Remove,
    };
    if !scope.instance.operations.contains(&required) {
        return ProviderAction::ReportError(format!(
            "provider instance '{}' does not support {}",
            scope.instance.name,
            required.as_str()
        ));
    }
    ProviderAction::PlanManaged(ManagedSecretBackendPlanRequest {
        instance: scope.instance.name.clone(),
        action,
        installation: managed.installation,
        project_id: model.context.project_id,
        environment: model.context.environment.clone(),
        locations: scope.locations.clone(),
        release: if action == ManagedSecretBackendAction::Upgrade {
            managed.release
        } else {
            model.managed_release.clone()
        },
        management_port: managed.management_port,
        purge_data,
        target: model.context.target.clone(),
        config_hash: model.context.config_hash.clone(),
    })
}

fn managed_configuration(
    scope: &ConnectionScope,
) -> Option<crate::domain::secret_backend::LocalManagedProvider> {
    scope
        .configuration
        .local_managed(scope.instance.name.as_str())
}

fn driver_releases(
    drivers: &[ProviderDescriptor],
    instance: &ProviderInstanceDescriptor,
) -> Vec<String> {
    drivers
        .iter()
        .find(|driver| driver.driver == instance.driver)
        .and_then(|driver| driver.mode(ProviderMode::Managed))
        .into_iter()
        .flat_map(|mode| &mode.configuration)
        .find(|field| field.key == "release")
        .map_or_else(Vec::new, |field| field.allowed_values.clone())
}

fn provider_locations(config: &MmConfig, provider: &ProviderInstanceName) -> Vec<String> {
    let mut locations = config
        .secrets
        .values()
        .filter(|declaration| declaration.storage.provider.as_str() == provider.as_str())
        .map(|declaration| declaration.storage.location.clone())
        .collect::<Vec<_>>();
    locations.sort();
    locations.dedup();
    locations
}

#[cfg(test)]
mod tests;
