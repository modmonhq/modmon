//! The optional units of local development: which run on this machine, and
//! turning them on and off for it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_yaml_ng::Value;

use crate::app::ports::{ProjectDocumentStore, ProjectInitialization};
use crate::domain::config::{ConfigDocument, MmConfig};
use crate::domain::development_units::{
    ComposeDependencies, DevelopmentChange, DevelopmentUnits, OptionalUnit, UnitRefusal,
    check_unit, mark_development, off_locally, optional_units, select_units,
};
use crate::domain::error::MmError;
use crate::domain::path_policy::read_regular_file;

const MAX_COMPOSE_FILE_BYTES: usize = 16 * 1024 * 1024;

/// Marks what does not run in the development environment on this machine.
pub fn mark(document: &mut ConfigDocument) {
    let choices = &document.config.development;
    let dependencies = if choices.enabled.is_empty() && choices.with.is_empty() {
        ComposeDependencies::new()
    } else {
        compose_dependencies(document)
    };
    mark_development(&mut document.config, &dependencies);
}

/// For each service of each stack, the services its Compose files say it
/// cannot start without.
#[must_use]
pub fn compose_dependencies(document: &ConfigDocument) -> ComposeDependencies {
    let mut dependencies = ComposeDependencies::new();
    for (stack_name, stack) in &document.config.stacks {
        for file in &stack.files {
            let Ok(Some(bytes)) =
                read_regular_file(&document.root.join(file), MAX_COMPOSE_FILE_BYTES)
            else {
                continue;
            };
            let Ok(mut model) = serde_yaml_ng::from_slice::<Value>(&bytes) else {
                continue;
            };
            if model.apply_merge().is_err() {
                continue;
            }
            let Some(services) = model.get("services").and_then(Value::as_mapping) else {
                continue;
            };
            for (name, definition) in services {
                if let Some(name) = name.as_str() {
                    dependencies
                        .entry((stack_name.clone(), name.to_owned()))
                        .or_default()
                        .extend(required_services(definition));
                }
            }
        }
    }
    dependencies
}

/// The services a Compose service names in `depends_on` (unless
/// `required: false`), `links`, `volumes_from`, or a `service:` mode.
#[must_use]
pub fn required_services(definition: &Value) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    match definition.get("depends_on") {
        Some(Value::Mapping(map)) => names.extend(
            map.iter()
                .filter(|(_, options)| {
                    options.get("required").and_then(Value::as_bool) != Some(false)
                })
                .filter_map(|(name, _)| name.as_str().map(str::to_owned)),
        ),
        Some(Value::Sequence(items)) => {
            names.extend(items.iter().filter_map(Value::as_str).map(str::to_owned));
        }
        _ => {}
    }
    for field in ["links", "volumes_from"] {
        if let Some(Value::Sequence(items)) = definition.get(field) {
            names.extend(
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|item| item.split(':').next().unwrap_or(item).to_owned()),
            );
        }
    }
    for field in ["network_mode", "ipc", "pid"] {
        if let Some(service) = definition
            .get(field)
            .and_then(Value::as_str)
            .and_then(|value| value.strip_prefix("service:"))
        {
            names.insert(service.to_owned());
        }
    }
    names
}

/// The optional units, when `environment` is the development environment.
#[must_use]
pub fn optional_in(config: &MmConfig, environment: &str) -> Vec<OptionalUnit> {
    if config.project.development_environment.as_deref() == Some(environment) {
        optional_units(config)
    } else {
        Vec::new()
    }
}

/// Lists, turns on and turns off the optional units of a project. The
/// command line, the terminal UI and the MCP server share it.
#[derive(Clone)]
pub struct DevelopmentUnitService {
    documents: Arc<dyn ProjectDocumentStore>,
}

impl DevelopmentUnitService {
    #[must_use]
    pub fn new(documents: Arc<dyn ProjectDocumentStore>) -> Self {
        Self { documents }
    }

    pub fn list(&self, start: &Path) -> Result<DevelopmentUnits, MmError> {
        let document = self.documents.load_from(start)?;
        Ok(DevelopmentUnits {
            environment: document.config.project.development_environment.clone(),
            units: optional_units(&document.config),
        })
    }

    /// Turns `names` on (`enable`) or off for this machine.
    pub fn set(
        &self,
        start: &Path,
        names: &[String],
        enable: bool,
    ) -> Result<DevelopmentChange, MmError> {
        let before = self.documents.load_from(start)?;
        let environment = development_environment(&before.config)?;
        let mut notes = Vec::new();
        let mut chosen = Vec::new();
        for name in names {
            match check_unit(&before.config, name) {
                Ok(_) => chosen.push(name.clone()),
                Err(UnitRefusal::Unknown)
                    if !enable && before.config.development.enabled.contains(name) =>
                {
                    chosen.push(name.clone());
                }
                Err(UnitRefusal::AlwaysOn) if enable => {
                    notes.push(format!("{name} always runs in {environment}"));
                }
                Err(refusal) => return Err(refused(&before.config, name, &refusal)),
            }
        }
        let enabled = if enable {
            enabled_with(&before.config.development.enabled, &chosen)
        } else {
            enabled_without(&before.config, &chosen, &mut notes)
        };
        let after = if enabled == before.config.development.enabled {
            before.clone()
        } else {
            self.documents.set_development_enabled(start, &enabled)?
        };
        let was_off = off_locally(&before.config);
        let now_off = off_locally(&after.config);
        let started = was_off.difference(&now_off).cloned().collect::<Vec<_>>();
        let stopped = now_off.difference(&was_off).cloned().collect::<Vec<_>>();
        if !enable {
            notes.extend(still_on(&after.config, &chosen));
        }
        Ok(DevelopmentChange {
            environment,
            enable,
            units: names.to_vec(),
            started,
            stopped,
            notes,
            optional: optional_units(&after.config),
        })
    }

    /// Refuses a `mm dev up --with` name that turns nothing on.
    pub fn check_with(&self, start: &Path, names: &[String]) -> Result<(), MmError> {
        if names.is_empty() {
            return Ok(());
        }
        let document = self.documents.load_from(start)?;
        development_environment(&document.config)?;
        for name in names {
            if let Err(refusal) = check_unit(&document.config, name)
                && refusal != UnitRefusal::AlwaysOn
            {
                return Err(refused(&document.config, name, &refusal));
            }
        }
        Ok(())
    }
}

fn development_environment(config: &MmConfig) -> Result<String, MmError> {
    config
        .project
        .development_environment
        .clone()
        .ok_or_else(|| MmError::Environment {
            message: "this project names no development environment, where `development:` applies; set `project.development-environment` in .config/mm.yaml".to_owned(),
        })
}

fn refused(config: &MmConfig, name: &str, refusal: &UnitRefusal) -> MmError {
    let message = match refusal {
        UnitRefusal::Unknown => {
            let units = optional_units(config)
                .into_iter()
                .map(|unit| unit.name)
                .collect::<Vec<_>>();
            if units.is_empty() {
                format!(
                    "no module, service or job is named '{name}', and none is `development: optional`"
                )
            } else {
                format!(
                    "no module, service or job is named '{name}'; the optional ones are {}",
                    units.join(", ")
                )
            }
        }
        UnitRefusal::Off => format!(
            "{name} is `development: off`, so it never runs in development; set `development: optional` on it in .config/mm.yaml to let developers turn it on"
        ),
        UnitRefusal::AlwaysOn => format!(
            "{name} always runs in development; set `development: optional` on it in .config/mm.yaml to make it optional"
        ),
    };
    MmError::Environment { message }
}

fn enabled_with(enabled: &[String], names: &[String]) -> Vec<String> {
    let mut list = enabled.to_vec();
    for name in names {
        if !list.contains(name) {
            list.push(name.clone());
        }
    }
    list
}

/// The enabled names left once every one that `names` covers is gone.
fn enabled_without(config: &MmConfig, names: &[String], notes: &mut Vec<String>) -> Vec<String> {
    let mut services = BTreeSet::new();
    let mut jobs = BTreeSet::new();
    for name in names {
        let (covered_services, covered_jobs) = select_units(config, name).covered(config);
        services.extend(covered_services);
        jobs.extend(covered_jobs);
    }
    let mut kept = Vec::new();
    for entry in &config.development.enabled {
        let (entry_services, entry_jobs) = select_units(config, entry).covered(config);
        let gone = names.contains(entry)
            || (entry_services.is_subset(&services) && entry_jobs.is_subset(&jobs));
        if !gone {
            if !entry_services.is_disjoint(&services) || !entry_jobs.is_disjoint(&jobs) {
                notes.push(format!(
                    "{entry} stays enabled and covers part of it; run `mm dev disable {entry}` to turn it off"
                ));
            }
            kept.push(entry.clone());
        }
    }
    kept
}

/// Notes for what `names` covers that still runs: another enabled unit needs it.
fn still_on(config: &MmConfig, names: &[String]) -> Vec<String> {
    let off = off_locally(config);
    let mut notes = Vec::new();
    for name in names {
        let (services, jobs) = select_units(config, name).covered(config);
        let running = services
            .iter()
            .filter(|(module, service)| {
                crate::domain::development_units::service_development(config, module, service)
                    == crate::domain::development_units::Development::Optional
            })
            .map(|(module, service)| format!("{module}/{service}"))
            .chain(jobs.into_iter().filter(|job| {
                config.jobs.get(job).and_then(|job| job.development)
                    == Some(crate::domain::development_units::Development::Optional)
            }))
            .filter(|unit| !off.contains(unit))
            .collect::<Vec<_>>();
        if !running.is_empty() {
            notes.push(format!(
                "{} still runs: an enabled unit needs it",
                running.join(", ")
            ));
        }
    }
    notes
}

/// A document store that turns `with` on for one run, as `mm dev up --with`.
pub struct DevelopmentDocuments {
    inner: Arc<dyn ProjectDocumentStore>,
    with: Vec<String>,
}

impl DevelopmentDocuments {
    #[must_use]
    pub fn new(inner: Arc<dyn ProjectDocumentStore>, with: Vec<String>) -> Self {
        Self { inner, with }
    }

    fn with_units(&self, mut document: ConfigDocument) -> ConfigDocument {
        document.config.development.with.clone_from(&self.with);
        mark(&mut document);
        document
    }
}

impl ProjectDocumentStore for DevelopmentDocuments {
    fn discover(&self, start: &Path) -> Result<PathBuf, MmError> {
        self.inner.discover(start)
    }

    fn load_from(&self, start: &Path) -> Result<ConfigDocument, MmError> {
        self.inner
            .load_from(start)
            .map(|document| self.with_units(document))
    }

    fn load(&self, path: &Path) -> Result<ConfigDocument, MmError> {
        self.inner
            .load(path)
            .map(|document| self.with_units(document))
    }

    fn initialize(
        &self,
        root: &Path,
        requested_name: Option<&str>,
    ) -> Result<Box<dyn ProjectInitialization>, MmError> {
        self.inner.initialize(root, requested_name)
    }

    fn schema(&self) -> schemars::Schema {
        self.inner.schema()
    }

    fn set_default_environment(
        &self,
        start: &Path,
        environment: Option<&str>,
    ) -> Result<ConfigDocument, MmError> {
        self.inner
            .set_default_environment(start, environment)
            .map(|document| self.with_units(document))
    }

    fn set_environment_delivery(
        &self,
        start: &Path,
        environment: &str,
        mode: crate::domain::environment_delivery::EnvironmentDeliveryMode,
    ) -> Result<ConfigDocument, MmError> {
        self.inner
            .set_environment_delivery(start, environment, mode)
            .map(|document| self.with_units(document))
    }

    fn set_development_mount(
        &self,
        start: &Path,
        module: &str,
        mount: Option<&crate::domain::development_mount::DevelopmentMountConfig>,
        expected_source_hash: &str,
    ) -> Result<ConfigDocument, MmError> {
        self.inner
            .set_development_mount(start, module, mount, expected_source_hash)
            .map(|document| self.with_units(document))
    }

    fn set_development_enabled(
        &self,
        start: &Path,
        enabled: &[String],
    ) -> Result<ConfigDocument, MmError> {
        self.inner
            .set_development_enabled(start, enabled)
            .map(|document| self.with_units(document))
    }

    fn ignored_sources(&self, document: &ConfigDocument) -> Vec<PathBuf> {
        self.inner.ignored_sources(document)
    }
}
