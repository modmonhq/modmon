use std::path::PathBuf;

use crate::app::ports::{StateStore, TargetStore};
use crate::domain::config::ConfigDocument;
use crate::domain::config::RuntimeConfig;
use crate::domain::error::MmError;
use crate::domain::runtime::RuntimeKind;
use crate::domain::target::{NamedTarget, TargetTransport};
use crate::domain::workspace::ProjectId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionSource {
    Cli,
    LocalEnvironmentOverride,
    LocalProjectOverride,
    EnvironmentConfig,
    ProjectConfig,
    TargetDefault,
}

impl SelectionSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::LocalEnvironmentOverride => "local-environment",
            Self::LocalProjectOverride => "local-project",
            Self::EnvironmentConfig => "environment-config",
            Self::ProjectConfig => "project-config",
            Self::TargetDefault => "target-default",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSelectionPreference {
    pub runtime: Option<RuntimeKind>,
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSelection {
    pub environment: String,
    pub target: String,
    pub target_source: SelectionSource,
    pub runtime: RuntimeKind,
    pub runtime_source: SelectionSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOperationContext {
    pub project_root: PathBuf,
    pub project_id: ProjectId,
    pub environment: String,
    pub target: NamedTarget,
    pub runtime: RuntimeKind,
    pub transport: TargetTransport,
    pub selection: ResolvedSelection,
    pub config_hash: String,
}

impl ResolvedOperationContext {
    #[must_use]
    pub fn from_resolved(
        document: &ConfigDocument,
        selection: ResolvedSelection,
        target: NamedTarget,
    ) -> Self {
        Self {
            project_root: document.root.clone(),
            project_id: document.config.project.id,
            environment: selection.environment.clone(),
            runtime: selection.runtime,
            transport: target.definition.transport.clone(),
            target,
            selection,
            config_hash: document.source_hash.clone(),
        }
    }
}

pub fn resolve_target_name(
    document: &ConfigDocument,
    environment: &str,
    cli_target: Option<&str>,
    environment_pref: Option<&LocalSelectionPreference>,
    project_pref: Option<&LocalSelectionPreference>,
) -> Result<(String, SelectionSource), MmError> {
    let environment_config = document.config.environments.get(environment);
    first_present([
        cli_target.map(|value| (value.to_owned(), SelectionSource::Cli)),
        environment_pref.and_then(|pref| {
            pref.target
                .clone()
                .map(|value| (value, SelectionSource::LocalEnvironmentOverride))
        }),
        project_pref.and_then(|pref| {
            pref.target
                .clone()
                .map(|value| (value, SelectionSource::LocalProjectOverride))
        }),
        environment_config.and_then(|config| {
            config
                .target
                .clone()
                .map(|value| (value, SelectionSource::EnvironmentConfig))
        }),
        Some((
            document.config.target.clone(),
            SelectionSource::ProjectConfig,
        )),
    ])
    .ok_or_else(|| missing_selection("target"))
}

pub fn resolve_runtime(
    document: &ConfigDocument,
    environment: &str,
    cli_runtime: Option<RuntimeKind>,
    environment_pref: Option<&LocalSelectionPreference>,
    project_pref: Option<&LocalSelectionPreference>,
    target: &NamedTarget,
) -> Result<(RuntimeKind, SelectionSource), MmError> {
    let environment_config = document.config.environments.get(environment);
    first_present([
        cli_runtime.map(|value| (value, SelectionSource::Cli)),
        environment_pref.and_then(|pref| {
            pref.runtime
                .map(|value| (value, SelectionSource::LocalEnvironmentOverride))
        }),
        project_pref.and_then(|pref| {
            pref.runtime
                .map(|value| (value, SelectionSource::LocalProjectOverride))
        }),
        environment_config.and_then(|config| {
            runtime_driver(config.runtime.as_ref())
                .map(|value| (value, SelectionSource::EnvironmentConfig))
        }),
        runtime_driver(Some(&document.config.runtime))
            .map(|value| (value, SelectionSource::ProjectConfig)),
        target
            .definition
            .default_runtime
            .map(|value| (value, SelectionSource::TargetDefault)),
    ])
    .ok_or(MmError::Setup {
        issue: crate::domain::readiness::SetupIssue::MissingRuntime,
    })
}

pub fn resolve_runtime_and_target(
    document: &ConfigDocument,
    environment: &str,
    cli_runtime: Option<RuntimeKind>,
    cli_target: Option<&str>,
    environment_pref: Option<&LocalSelectionPreference>,
    project_pref: Option<&LocalSelectionPreference>,
    target: &NamedTarget,
) -> Result<ResolvedSelection, MmError> {
    let target_name = resolve_target_name(
        document,
        environment,
        cli_target,
        environment_pref,
        project_pref,
    )?;
    let runtime = resolve_runtime(
        document,
        environment,
        cli_runtime,
        environment_pref,
        project_pref,
        target,
    )?;
    ensure_runtime_available(target, runtime.0)?;
    Ok(ResolvedSelection {
        environment: environment.to_owned(),
        target: target_name.0,
        target_source: target_name.1,
        runtime: runtime.0,
        runtime_source: runtime.1,
    })
}

pub fn resolve_operation_context(
    document: &ConfigDocument,
    environment: &str,
    cli_runtime: Option<RuntimeKind>,
    cli_target: Option<&str>,
    environment_pref: Option<&LocalSelectionPreference>,
    project_pref: Option<&LocalSelectionPreference>,
    targets: &dyn TargetStore,
) -> Result<ResolvedOperationContext, MmError> {
    let (selection, target) = resolve_selection(
        document,
        environment,
        cli_runtime,
        cli_target,
        environment_pref,
        project_pref,
        targets,
    )?;
    Ok(ResolvedOperationContext::from_resolved(
        document, selection, target,
    ))
}

/// The context of a remote request: the environment's runtime, `fallback` only when none is named.
pub fn resolve_request_context(
    document: &ConfigDocument,
    environment: &str,
    (cli_runtime, fallback): (Option<RuntimeKind>, RuntimeKind),
    cli_target: Option<&str>,
    (environment_pref, project_pref): (
        Option<&LocalSelectionPreference>,
        Option<&LocalSelectionPreference>,
    ),
    targets: &dyn TargetStore,
) -> Result<ResolvedOperationContext, MmError> {
    let resolve = |runtime: Option<RuntimeKind>| {
        resolve_operation_context(
            document,
            environment,
            runtime,
            cli_target,
            environment_pref,
            project_pref,
            targets,
        )
    };
    match resolve(cli_runtime) {
        Err(MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::MissingRuntime,
        }) => resolve(Some(fallback)),
        resolved => resolved,
    }
}

pub fn resolve_selection(
    document: &ConfigDocument,
    environment: &str,
    cli_runtime: Option<RuntimeKind>,
    cli_target: Option<&str>,
    environment_pref: Option<&LocalSelectionPreference>,
    project_pref: Option<&LocalSelectionPreference>,
    targets: &dyn TargetStore,
) -> Result<(ResolvedSelection, NamedTarget), MmError> {
    let (target_name, target_source) = resolve_target_name(
        document,
        environment,
        cli_target,
        environment_pref,
        project_pref,
    )?;
    let target = resolve_named_target(document, &target_name, targets)?;
    let (runtime, runtime_source) = resolve_runtime(
        document,
        environment,
        cli_runtime,
        environment_pref,
        project_pref,
        &target,
    )?;
    ensure_runtime_available(&target, runtime)?;
    Ok((
        ResolvedSelection {
            environment: environment.to_owned(),
            target: target_name,
            target_source,
            runtime,
            runtime_source,
        },
        target,
    ))
}

/// The target called `name`: this machine's private catalog entry when it
/// has one (for example, a jump host), else the target the repo declares,
/// built from its public facts. A repo target that lists `nodes:` resolves
/// to a target that names every node.
pub fn resolve_named_target(
    document: &ConfigDocument,
    name: &str,
    targets: &dyn TargetStore,
) -> Result<NamedTarget, MmError> {
    match targets.resolve_target(name) {
        Ok(target) => Ok(target),
        Err(MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::MissingTarget { .. },
        }) if document.config.targets.contains_key(name) => document.config.targets[name]
            .named_target(document.config.project.id, name)
            .map_err(|message| MmError::Target { message }),
        Err(error) => Err(error),
    }
}

pub fn load_selection_preferences(
    state: Option<&dyn StateStore>,
    document: &ConfigDocument,
    environment: &str,
) -> Result<
    (
        Option<LocalSelectionPreference>,
        Option<LocalSelectionPreference>,
    ),
    MmError,
> {
    let Some(state) = state else {
        return Ok((None, None));
    };
    Ok((
        state.local_environment_preference(document.config.project.id, environment)?,
        state.local_project_preference(document.config.project.id)?,
    ))
}

fn ensure_runtime_available(target: &NamedTarget, runtime: RuntimeKind) -> Result<(), MmError> {
    if target.definition.supports_runtime(runtime) {
        return Ok(());
    }
    Err(MmError::Target {
        message: format!(
            "target '{}' does not offer runtime {:?}",
            target.name, runtime
        ),
    })
}

fn runtime_driver(config: Option<&RuntimeConfig>) -> Option<RuntimeKind> {
    config.and_then(|config| config.driver)
}

fn first_present<T, const N: usize>(
    candidates: [Option<(T, SelectionSource)>; N],
) -> Option<(T, SelectionSource)> {
    candidates.into_iter().flatten().next()
}

fn missing_selection(kind: &str) -> MmError {
    MmError::Environment {
        message: format!(
            "no {kind} was selected; pass --{kind}, set a private local preference, or configure a project, environment, or target default"
        ),
    }
}

#[cfg(test)]
mod tests;
