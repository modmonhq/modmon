//! Which user namespace each service of a stack runs in.
//!
//! On a remote host that runs Podman every service gets user ids of its
//! own that mm chooses and the host keeps (`domain::user_namespace`):
//! its containers' ids 0 to 65535 are 65536 host ids no other service of
//! the host has, the same in every container of the service and from one
//! deployment to the next. A service that has to keep the host's ids says
//! why (`host-user-namespace`), and so does a Compose file that sets a
//! `userns_mode` other than a plain `auto`. A service on the host's
//! network keeps them too.
//! Elsewhere containers keep the host's ids, which `userns_mode: host`
//! says on any engine.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_yaml_ng::Value;

use crate::app::ports::ServiceUserIds;
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::stack::{CompiledComposeSource, CompiledStackArtifact, StackServiceManifest};
use crate::domain::user_namespace::{
    ServiceIds, ServiceIdsChoice, ServiceIdsOwner, holds_in_any_namespace,
};

/// The services of `stack` in `environment` that say why they keep the
/// host's user ids.
pub(super) fn host_user_namespaces(
    document: &ConfigDocument,
    environment: &str,
    stack: &str,
) -> BTreeSet<String> {
    document
        .config
        .modules
        .values()
        .flat_map(|module| &module.services)
        .filter(|(_, service)| {
            service.stack == stack
                && service.is_active(environment)
                && service
                    .host_user_namespace
                    .as_deref()
                    .is_some_and(|reason| !reason.trim().is_empty())
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// The user ids of a stack's services that have ids of their own here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct StackUserIds(BTreeMap<String, ServiceIds>);

impl StackUserIds {
    pub(super) fn get(&self, service: &str) -> Option<ServiceIds> {
        self.0.get(service).copied()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&String, &ServiceIds)> {
        self.0.iter()
    }
}

/// What a stack's services get from the host they run on: their user ids,
/// their config files given to those ids, and on rootful Podman the ports
/// below 1024 for any user.
pub(super) struct StackHost {
    pub user_ids: StackUserIds,
    pub config_files: super::config_files::StackConfigFiles,
    pub low_ports: BTreeSet<String>,
    pub shared_pid: BTreeSet<String>,
}

pub(super) struct ServiceHost<'a> {
    user_ids: Option<&'a dyn ServiceUserIds>,
    document: &'a ConfigDocument,
    environment: &'a str,
    config_root: &'a Path,
}

impl<'a> ServiceHost<'a> {
    pub(super) fn of(input: &super::compilation::DocumentCompilation<'a>) -> Self {
        Self {
            user_ids: input.user_ids,
            document: input.document,
            environment: input.environment,
            config_root: input.config_files,
        }
    }

    pub(super) fn stack(
        &self,
        stack: &str,
        services: &[StackServiceManifest],
        sources: &[CompiledComposeSource],
        shared: &super::shared_volumes::NodeVolumes,
    ) -> Result<StackHost, MmError> {
        let models = match self.user_ids {
            Some(_) => compose_models(sources)?,
            None => Vec::new(),
        };
        let user_ids = self.user_ids(stack, &models, shared)?;
        if self.user_ids.is_some() {
            let found = files_in_chosen_namespaces(
                self.document,
                self.environment,
                stack,
                &chosen_user_namespaces(&models),
            );
            if !found.is_empty() {
                return Err(MmError::Remote {
                    code: "MM_USER_NAMESPACE",
                    message: found.join("; "),
                });
            }
        }
        let config_files = super::config_files::stack_config_files(
            self.document,
            self.environment,
            stack,
            services,
            self.config_root,
            &user_ids,
        )?;
        let low_ports = match self.user_ids {
            Some(_) => super::low_ports::services_opening_low_ports(sources)?,
            None => BTreeSet::new(),
        };
        let shared_pid = match self.user_ids {
            Some(ids) => {
                super::shared_pid::services_sharing_pid(sources, ids.shared_pid_apparmor())?
            }
            None => BTreeSet::new(),
        };
        Ok(StackHost {
            user_ids,
            config_files,
            low_ports,
            shared_pid,
        })
    }

    fn user_ids(
        &self,
        stack: &str,
        models: &[Value],
        shared: &super::shared_volumes::NodeVolumes,
    ) -> Result<StackUserIds, MmError> {
        let Some(user_ids) = self.user_ids else {
            return Ok(StackUserIds::default());
        };
        let keeps_host_ids = host_user_namespaces(self.document, self.environment, stack);
        let wanted = services_without_own_userns(models)
            .into_iter()
            .filter(|service| !keeps_host_ids.contains(service))
            .map(|service| ServiceIdsOwner {
                project: self.document.config.project.id,
                environment: self.environment.to_owned(),
                stack: stack.to_owned(),
                service,
            })
            .collect::<BTreeSet<_>>();
        let chosen = user_ids.reserve(&wanted)?;
        for volume in &shared.volumes {
            for service in &volume.services {
                if let Some((owner, choice)) =
                    chosen.iter().find(|(owner, _)| &owner.service == service)
                    && !choice.is_first_choice()
                {
                    // Not kept: the next plan tries the first choice again.
                    user_ids.release(&BTreeSet::from([owner.clone()]))?;
                    return Err(not_first_choice(self.document, owner, choice, &volume.key));
                }
            }
        }
        Ok(StackUserIds(
            chosen
                .into_iter()
                .map(|(owner, choice)| (owner.service, choice.ids))
                .collect(),
        ))
    }
}

fn compose_models(sources: &[CompiledComposeSource]) -> Result<Vec<Value>, MmError> {
    sources
        .iter()
        .map(|source| {
            let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
                message: format!("Compose file '{}': {cause}", source.manifest.path.display()),
            };
            let mut model: Value = serde_yaml_ng::from_slice(&source.bytes).map_err(invalid)?;
            model.apply_merge().map_err(invalid)?;
            Ok(model)
        })
        .collect()
}

fn compose_services(models: &[Value]) -> impl Iterator<Item = (&str, &Value)> {
    models
        .iter()
        .filter_map(|model| model.get("services").and_then(Value::as_mapping))
        .flatten()
        .filter_map(|(name, service)| name.as_str().map(|name| (name, service)))
}

/// What one Compose file says about a service's `userns_mode`.
pub(super) enum UsernsSetting {
    /// Nothing, `~` or `""`: an earlier file's value stands.
    Unset,
    /// `!reset`: no file sets it.
    Reset,
    Mode(String),
}

pub(super) fn userns_setting(service: &Value) -> UsernsSetting {
    setting_of(service.get("userns_mode"))
}

fn setting_of(value: Option<&Value>) -> UsernsSetting {
    match value {
        None | Some(Value::Null) => UsernsSetting::Unset,
        Some(Value::String(mode)) if mode.trim().is_empty() => UsernsSetting::Unset,
        Some(Value::String(mode)) => UsernsSetting::Mode(mode.trim().to_owned()),
        Some(Value::Tagged(tagged)) if tagged.tag == "reset" => UsernsSetting::Reset,
        Some(Value::Tagged(tagged)) => setting_of(Some(&tagged.value)),
        Some(other) => UsernsSetting::Mode(format!("{other:?}")),
    }
}

/// A Compose service's user namespace after every file of its stack.
#[derive(Default)]
struct ComposeUserns {
    mode: Option<String>,
    host_network: bool,
}

impl ComposeUserns {
    /// mm gives the service ids of its own: its files set no `userns_mode`,
    /// or a plain `auto`, which mm's own block of ids is, and it is not on
    /// the host's network.
    fn gets_own_ids(&self) -> bool {
        !self.host_network && self.mode.as_deref().is_none_or(|mode| mode == "auto")
    }
}

/// Each service of the stack's Compose files, with the services it
/// `extends` in its own file merged under it, the last file that sets
/// `userns_mode` winning, as Compose merges them.
fn compose_userns(models: &[Value]) -> BTreeMap<String, ComposeUserns> {
    let models = models
        .iter()
        .map(|model| {
            let mut model = model.clone();
            if let Some(mapping) = model.as_mapping_mut() {
                crate::domain::compose_extends::resolve_extends(mapping);
            }
            model
        })
        .collect::<Vec<_>>();
    let mut services = BTreeMap::<String, ComposeUserns>::new();
    for (name, service) in compose_services(&models) {
        let entry = services.entry(name.to_owned()).or_default();
        entry.host_network |= super::host_network::on_host_network(service);
        match userns_setting(service) {
            UsernsSetting::Unset => {}
            UsernsSetting::Reset => entry.mode = None,
            UsernsSetting::Mode(mode) => entry.mode = Some(mode),
        }
    }
    services
}

/// The services of the stack's Compose files that mm gives ids of their
/// own: those that set no `userns_mode` or a plain `auto`, and are not on
/// the host's network.
pub(crate) fn services_without_own_userns(models: &[Value]) -> BTreeSet<String> {
    compose_userns(models)
        .into_iter()
        .filter(|(_, userns)| userns.gets_own_ids())
        .map(|(name, _)| name)
        .collect()
}

/// A `userns_mode` a Compose file sets that mm does not replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChosenUserns {
    pub mode: String,
    pub host_network: bool,
}

/// The services whose Compose files set a `userns_mode` mm does not
/// replace, other than `host`: Podman chooses their user ids.
pub(crate) fn chosen_user_namespaces(models: &[Value]) -> BTreeMap<String, ChosenUserns> {
    compose_userns(models)
        .into_iter()
        .filter(|(_, userns)| !userns.gets_own_ids())
        .filter_map(|(name, userns)| {
            let host_network = userns.host_network;
            userns
                .mode
                .filter(|mode| mode != "host")
                .map(|mode| (name, ChosenUserns { mode, host_network }))
        })
        .collect()
}

/// The secret files and config files of `stack`'s services in
/// `environment` whose owner or mode would not hold in the user namespace
/// `chosen` says Podman picks for their service, each as what to change.
pub(crate) fn files_in_chosen_namespaces(
    document: &ConfigDocument,
    environment: &str,
    stack: &str,
    chosen: &BTreeMap<String, ChosenUserns>,
) -> Vec<String> {
    let mut found = Vec::new();
    for (module, config) in &document.config.modules {
        for (service, config) in &config.services {
            let Some(chosen) = chosen.get(service) else {
                continue;
            };
            let mode = &chosen.mode;
            let instead = if chosen.host_network {
                "(on the host's network it then keeps the host's ids, and the file its uid and gid)"
            } else {
                "or set it to plain `auto` (mm then gives it fixed ids of its own and the file to its uid and gid)"
            };
            if config.stack != stack
                || config.host_user_namespace.is_some()
                || !config.is_in_environment(environment)
            {
                continue;
            }
            for (key, path) in owned_files(config, environment) {
                found.push(format!(
                    "modules.{module}.services.{service}.{key}.\"{path}\" names an owner or a mode that holds only where mm knows the service's user ids, and its Compose file sets `userns_mode: {mode}`, so Podman picks them when it creates a container, after mm wrote the file. Remove `userns_mode` from service '{service}' {instead}, or give the file uid \"0\", gid \"0\" and a mode every user can read, such as 0o444"
                ));
            }
        }
    }
    found
}

/// The secret files and config files of a service that name an owner
/// other than root or a mode not every user can read, by key.
fn owned_files<'a>(
    config: &'a crate::domain::config::ServiceConfig,
    environment: &'a str,
) -> impl Iterator<Item = (&'static str, &'a String)> + 'a {
    let secrets = config.secret_files_in(environment).map(|(path, file)| {
        let holds = holds_in_any_namespace(Some(&file.uid), Some(&file.gid), file.mode);
        ("secret-files", path, holds)
    });
    let configs = config.config_files_in(environment).map(|(path, file)| {
        let holds = holds_in_any_namespace(file.uid.as_deref(), file.gid.as_deref(), file.mode);
        ("config-files", path, holds)
    });
    secrets
        .chain(configs)
        .filter(|(_, _, holds)| !holds)
        .map(|(key, path, _)| (key, path))
}

/// The owned files (see [`owned_files`]) of the services of `stacks` in
/// `environment` that do not run in the host's user namespace in the
/// Compose files the engine gets, mm's overlay last, each as its key.
pub(super) fn files_outside_host_namespace(
    document: &ConfigDocument,
    environment: &str,
    stacks: &[CompiledStackArtifact],
) -> Result<Vec<String>, MmError> {
    let mut found = Vec::new();
    for stack in stacks {
        let mut models = compose_models(&stack.user_files)?;
        let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
            message: format!(
                "the generated overlay of stack '{}': {cause}",
                stack.manifest.stack
            ),
        };
        models.push(serde_yaml_ng::from_slice(&stack.generated_overlay).map_err(invalid)?);
        let services = compose_userns(&models);
        for (module, config) in &document.config.modules {
            for (service, config) in &config.services {
                let outside = services
                    .get(service)
                    .is_some_and(|userns| userns.mode.as_deref() != Some("host"));
                if config.stack != stack.manifest.stack
                    || !config.is_in_environment(environment)
                    || !outside
                {
                    continue;
                }
                found.extend(owned_files(config, environment).map(|(key, path)| {
                    format!("modules.{module}.services.{service}.{key}.\"{path}\"")
                }));
            }
        }
    }
    Ok(found)
}

/// A service on a shared volume reaches its files from every node with
/// the same ids, its first choice. On this node another service has them.
fn not_first_choice(
    document: &ConfigDocument,
    owner: &ServiceIdsOwner,
    choice: &ServiceIdsChoice,
    volume: &str,
) -> MmError {
    let first = choice.first_choice;
    let holder = choice.first_choice_held_by.as_ref().map_or_else(
        || "no service has them, but this host gave the service other ids earlier (was mm's range in /etc/subuid changed?)".to_owned(),
        |holder| {
            let project = if holder.project == owner.project {
                "this project".to_owned()
            } else {
                format!("project {}", holder.project.0)
            };
            format!(
                "they belong to service '{}' of stack '{}' in environment '{}' of {project}",
                holder.service, holder.stack, holder.environment
            )
        },
    );
    let module = document
        .config
        .modules
        .iter()
        .find(|(_, module)| module.services.contains_key(&owner.service))
        .map_or("<module>", |(name, _)| name.as_str());
    MmError::Remote {
        code: "MM_USER_NAMESPACE",
        message: format!(
            "service '{service}' mounts shared volume '{volume}', which it reads and writes from every node with the same user ids, host ids {} to {}. On this node {holder}, so its files there would belong to other ids. Keep the host's ids for it with `host-user-namespace: <why>` under modules.{module}.services.{service}",
            first.first,
            first.last(),
            service = owner.service,
        ),
    }
}
