//! `development: on | optional | off` on modules, services and jobs, and the
//! optional units one machine runs in the development environment.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::config::MmConfig;

/// Whether a module, service or job runs in the development environment.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Development {
    #[default]
    On,
    Optional,
    Off,
}

impl Development {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Optional => "optional",
            Self::Off => "off",
        }
    }
}

/// `development:` of `.config/mm.overrides.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct LocalDevelopment {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enabled: Vec<String>,
}

/// The optional units this machine turns on: `enabled` from the local
/// overrides, `with` for one `mm dev up --with`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DevelopmentChoices {
    pub enabled: Vec<String>,
    pub with: Vec<String>,
}

/// Why a service or job does not run in the development environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OffLocally {
    pub environment: String,
    pub development: Development,
    /// The unit whose `development:` says so, as `mm dev enable` names it.
    pub unit: String,
}

impl OffLocally {
    #[must_use]
    pub fn is_optional(&self) -> bool {
        self.development == Development::Optional
    }

    /// The command that turns the unit on, for an optional one.
    #[must_use]
    pub fn enable_command(&self) -> Option<String> {
        self.is_optional()
            .then(|| format!("mm dev enable {}", self.unit))
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum UnitKind {
    Module,
    Service,
    Job,
}

/// What one name of `mm dev enable` stands for: every module, service and
/// job of that name, or the one service `MODULE/SERVICE` names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitSelection {
    pub modules: BTreeSet<String>,
    pub services: BTreeSet<(String, String)>,
    pub jobs: BTreeSet<String>,
}

impl UnitSelection {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty() && self.services.is_empty() && self.jobs.is_empty()
    }

    /// The services and jobs it turns on: its own, and those of its modules
    /// that take the module's `development:`.
    #[must_use]
    pub fn covered(&self, config: &MmConfig) -> (BTreeSet<(String, String)>, BTreeSet<String>) {
        let mut services = self.services.clone();
        let mut jobs = self.jobs.clone();
        for module_name in &self.modules {
            if let Some(module) = config.modules.get(module_name) {
                services.extend(
                    module
                        .services
                        .keys()
                        .map(|service| (module_name.clone(), service.clone())),
                );
            }
            jobs.extend(
                config
                    .jobs
                    .iter()
                    .filter(|(_, job)| &job.module == module_name)
                    .map(|(name, _)| name.clone()),
            );
        }
        (services, jobs)
    }
}

#[must_use]
pub fn select_units(config: &MmConfig, name: &str) -> UnitSelection {
    let mut selection = UnitSelection::default();
    if let Some((module, service)) = name.split_once('/') {
        if config
            .modules
            .get(module)
            .is_some_and(|config| config.services.contains_key(service))
        {
            selection
                .services
                .insert((module.to_owned(), service.to_owned()));
        }
        return selection;
    }
    if config.modules.contains_key(name) {
        selection.modules.insert(name.to_owned());
    }
    for (module_name, module) in &config.modules {
        if module.services.contains_key(name) {
            selection
                .services
                .insert((module_name.clone(), name.to_owned()));
        }
    }
    if config.jobs.contains_key(name) {
        selection.jobs.insert(name.to_owned());
    }
    selection
}

/// The service's `development:`, or its module's.
#[must_use]
pub fn service_development(config: &MmConfig, module: &str, service: &str) -> Development {
    config
        .modules
        .get(module)
        .map(|module_config| {
            module_config
                .services
                .get(service)
                .and_then(|service| service.development)
                .or(module_config.development)
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// The shortest name `mm dev enable` takes for exactly this service.
#[must_use]
pub fn service_unit_name(config: &MmConfig, module: &str, service: &str) -> String {
    let only = UnitSelection {
        services: BTreeSet::from([(module.to_owned(), service.to_owned())]),
        ..UnitSelection::default()
    };
    if select_units(config, service) == only {
        service.to_owned()
    } else {
        format!("{module}/{service}")
    }
}

/// The unit whose `development:` decides about a service.
#[must_use]
pub fn deciding_unit(config: &MmConfig, module: &str, service: &str) -> String {
    let own = config
        .modules
        .get(module)
        .and_then(|module| module.services.get(service))
        .is_some_and(|service| service.development.is_some());
    if own {
        service_unit_name(config, module, service)
    } else {
        module.to_owned()
    }
}

/// For each `(stack, service)`, the services of the same stack its Compose
/// `depends_on` requires.
pub type ComposeDependencies = BTreeMap<(String, String), BTreeSet<String>>;

/// The services and jobs the choices turn on, with every service a turned
/// on service or job needs.
#[must_use]
pub fn turned_on(
    config: &MmConfig,
    dependencies: &ComposeDependencies,
) -> (BTreeSet<(String, String)>, BTreeSet<String>) {
    let mut services = BTreeSet::new();
    let mut jobs = BTreeSet::new();
    for name in config
        .development
        .enabled
        .iter()
        .chain(&config.development.with)
    {
        let (covered_services, covered_jobs) = select_units(config, name).covered(config);
        services.extend(covered_services);
        jobs.extend(covered_jobs);
    }
    for job in jobs.iter().filter_map(|name| config.jobs.get(name)) {
        services.insert((job.module.clone(), job.service.clone()));
    }
    let owners = config
        .modules
        .iter()
        .flat_map(|(module_name, module)| {
            module.services.iter().map(move |(name, service)| {
                ((service.stack.clone(), name.clone()), module_name.clone())
            })
        })
        .collect::<BTreeMap<_, _>>();
    let mut pending = services.iter().cloned().collect::<Vec<_>>();
    while let Some((module, service)) = pending.pop() {
        let Some(stack) = config
            .modules
            .get(&module)
            .and_then(|config| config.services.get(&service))
            .map(|config| config.stack.clone())
        else {
            continue;
        };
        for dependency in dependencies
            .get(&(stack.clone(), service))
            .into_iter()
            .flatten()
        {
            if let Some(owner) = owners.get(&(stack.clone(), dependency.clone()))
                && services.insert((owner.clone(), dependency.clone()))
            {
                pending.push((owner.clone(), dependency.clone()));
            }
        }
    }
    (services, jobs)
}

/// Marks every service and job that does not run in the development
/// environment on this machine, and why. Other environments are unchanged.
pub fn mark_development(config: &mut MmConfig, dependencies: &ComposeDependencies) {
    let Some(environment) = config.project.development_environment.clone() else {
        clear_marks(config);
        return;
    };
    let (services_on, jobs_on) = turned_on(config, dependencies);
    let mut service_marks = ServiceMarks::new();
    for (module_name, module) in &config.modules {
        for name in module.services.keys() {
            let development = service_development(config, module_name, name);
            let on = match development {
                Development::On => true,
                Development::Optional => services_on.contains(&(module_name.clone(), name.clone())),
                Development::Off => false,
            };
            let mark = (!on).then(|| OffLocally {
                environment: environment.clone(),
                development,
                unit: deciding_unit(config, module_name, name),
            });
            service_marks.insert((module_name.clone(), name.clone()), mark);
        }
    }
    for (name, job) in &mut config.jobs {
        let development = job.development.unwrap_or_default();
        let on = match development {
            Development::On => true,
            Development::Optional => jobs_on.contains(name),
            Development::Off => false,
        };
        let service = service_marks
            .get(&(job.module.clone(), job.service.clone()))
            .cloned()
            .flatten();
        job.off_locally = (!on)
            .then(|| OffLocally {
                environment: environment.clone(),
                development,
                unit: name.clone(),
            })
            .or(service);
    }
    let routes = route_marks(config, &service_marks, &environment);
    for ((module_name, name), mark) in service_marks {
        if let Some(service) = config
            .modules
            .get_mut(&module_name)
            .and_then(|module| module.services.get_mut(&name))
        {
            service.off_locally = mark;
        }
    }
    for (stack_name, stack) in &mut config.stacks {
        for (index, route) in stack.ingress.routes_mut().into_iter().enumerate() {
            route.off_locally = routes
                .contains(&(stack_name.clone(), index))
                .then(|| environment.clone());
        }
    }
}

type ServiceMarks = BTreeMap<(String, String), Option<OffLocally>>;

/// The routes, by stack and position, that go to a service that is off: the
/// one they name, or the stack's every service when they name none.
fn route_marks(
    config: &MmConfig,
    service_marks: &ServiceMarks,
    environment: &str,
) -> BTreeSet<(String, usize)> {
    let mut marked = BTreeSet::new();
    for (stack_name, stack) in &config.stacks {
        let services = config
            .modules
            .iter()
            .flat_map(|(module_name, module)| {
                module
                    .services
                    .iter()
                    .filter(|(_, service)| &service.stack == stack_name)
                    .map(move |(name, service)| (module_name, name, service))
            })
            .filter(|(_, _, service)| {
                service.environments.is_empty()
                    || service.environments.iter().any(|name| name == environment)
            })
            .map(|(module_name, name, _)| {
                let off = service_marks
                    .get(&(module_name.clone(), name.clone()))
                    .is_some_and(Option::is_some);
                (name.clone(), off)
            })
            .collect::<Vec<_>>();
        for (index, route) in stack.ingress.routes().into_iter().enumerate() {
            let off = match &route.service {
                Some(service) => services.iter().any(|(name, off)| name == service && *off),
                None => !services.is_empty() && services.iter().all(|(_, off)| *off),
            };
            if off {
                marked.insert((stack_name.clone(), index));
            }
        }
    }
    marked
}

fn clear_marks(config: &mut MmConfig) {
    for service in config
        .modules
        .values_mut()
        .flat_map(|module| module.services.values_mut())
    {
        service.off_locally = None;
    }
    for job in config.jobs.values_mut() {
        job.off_locally = None;
    }
    for route in config
        .stacks
        .values_mut()
        .flat_map(|stack| stack.ingress.routes_mut())
    {
        route.off_locally = None;
    }
}

/// A module, service or job that is `development: optional`, and whether it
/// runs on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OptionalUnit {
    pub kind: UnitKind,
    /// The name `mm dev enable` and `mm dev disable` take.
    pub name: String,
    /// Whether it runs in the development environment on this machine.
    pub on: bool,
    /// Whether `.config/mm.overrides.yaml` turns it on.
    pub enabled: bool,
    /// Whether `mm dev up --with` turns it on for this run only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub this_run: bool,
    /// Its services as `module/service`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<String>,
}

/// Every optional unit of the configuration, as marked by
/// [`mark_development`].
#[must_use]
pub fn optional_units(config: &MmConfig) -> Vec<OptionalUnit> {
    let mut units = Vec::new();
    for (module_name, module) in &config.modules {
        if module.development == Some(Development::Optional) {
            let services = module
                .services
                .iter()
                .filter(|(_, service)| service.development.is_none())
                .map(|(name, service)| {
                    (
                        format!("{module_name}/{name}"),
                        service.off_locally.is_none(),
                    )
                })
                .collect::<Vec<_>>();
            units.push(unit(
                config,
                (UnitKind::Module, module_name.clone()),
                &services,
                &[],
            ));
        }
        for (name, service) in &module.services {
            if service.development == Some(Development::Optional) {
                units.push(unit(
                    config,
                    (
                        UnitKind::Service,
                        service_unit_name(config, module_name, name),
                    ),
                    &[(
                        format!("{module_name}/{name}"),
                        service.off_locally.is_none(),
                    )],
                    &[],
                ));
            }
        }
    }
    for (name, job) in &config.jobs {
        if job.development == Some(Development::Optional) {
            units.push(unit(
                config,
                (UnitKind::Job, name.clone()),
                &[],
                &[(name.clone(), job.off_locally.is_none())],
            ));
        }
    }
    units
}

fn unit(
    config: &MmConfig,
    (kind, name): (UnitKind, String),
    services: &[(String, bool)],
    jobs: &[(String, bool)],
) -> OptionalUnit {
    let target = select_units(config, &name);
    let names = |list: &[String]| {
        list.iter().any(|chosen| {
            let chosen = select_units(config, chosen);
            match kind {
                UnitKind::Module => target.modules.is_subset(&chosen.modules),
                UnitKind::Service | UnitKind::Job => {
                    let (chosen_services, chosen_jobs) = chosen.covered(config);
                    let (services, jobs) = target.covered(config);
                    services.is_subset(&chosen_services) && jobs.is_subset(&chosen_jobs)
                }
            }
        })
    };
    OptionalUnit {
        kind,
        on: services.iter().chain(jobs).any(|(_, on)| *on),
        enabled: names(&config.development.enabled),
        this_run: names(&config.development.with),
        services: services.iter().map(|(name, _)| name.clone()).collect(),
        jobs: jobs.iter().map(|(name, _)| name.clone()).collect(),
        name,
    }
}

/// The optional units of a project and which of them run on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DevelopmentUnits {
    /// The development environment, where `development:` applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    pub units: Vec<OptionalUnit>,
}

/// What `mm dev enable` or `mm dev disable` changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DevelopmentChange {
    pub environment: String,
    /// `true` for `mm dev enable`.
    pub enable: bool,
    pub units: Vec<String>,
    /// Services and jobs that run now and did not before, as
    /// `module/service` and job names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub started: Vec<String>,
    /// Services and jobs that ran before and do not now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stopped: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    pub optional: Vec<OptionalUnit>,
}

/// Why a name cannot be turned on or off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitRefusal {
    Unknown,
    AlwaysOn,
    Off,
}

/// What `name` covers, when some of it is optional.
pub fn check_unit(config: &MmConfig, name: &str) -> Result<UnitSelection, UnitRefusal> {
    let selection = select_units(config, name);
    if selection.is_empty() {
        return Err(UnitRefusal::Unknown);
    }
    let (services, jobs) = selection.covered(config);
    let settings = services
        .iter()
        .map(|(module, service)| service_development(config, module, service))
        .chain(jobs.iter().filter_map(|job| {
            config
                .jobs
                .get(job)
                .map(|job| job.development.unwrap_or_default())
        }))
        .collect::<BTreeSet<_>>();
    if settings.contains(&Development::Optional) {
        Ok(selection)
    } else if settings.contains(&Development::Off) {
        Err(UnitRefusal::Off)
    } else {
        Err(UnitRefusal::AlwaysOn)
    }
}

/// The services and jobs that do not run in the development environment, as
/// `module/service` and job names.
#[must_use]
pub fn off_locally(config: &MmConfig) -> BTreeSet<String> {
    config
        .modules
        .iter()
        .flat_map(|(module_name, module)| {
            module
                .services
                .iter()
                .filter(|(_, service)| service.off_locally.is_some())
                .map(move |(name, _)| format!("{module_name}/{name}"))
        })
        .chain(
            config
                .jobs
                .iter()
                .filter(|(_, job)| job.off_locally.is_some())
                .map(|(name, _)| name.clone()),
        )
        .collect()
}
