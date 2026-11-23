//! Which user namespace each service of a stack runs in.
//!
//! On a remote host that runs Podman every service gets user ids of its
//! own that mm chooses and the host keeps (`domain::user_namespace`):
//! its containers' ids 0 to 65535 are 65536 host ids no other service of
//! the host has, the same in every container of the service and from one
//! deployment to the next. A service that has to keep the host's ids says
//! why (`host-user-namespace`), and so does a Compose file that sets
//! `userns_mode` itself. A service on the host's network keeps them too.
//! Elsewhere containers keep the host's ids, which `userns_mode: host`
//! says on any engine.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_yaml_ng::Value;

use crate::app::ports::ServiceUserIds;
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::stack::{CompiledComposeSource, StackServiceManifest};
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
            Some(ids) if ids.confines_shared_pid() => {
                super::shared_pid::services_sharing_pid(sources)?
            }
            _ => BTreeSet::new(),
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

/// The services of the stack's Compose files that do not set
/// `userns_mode` themselves and are not on the host's network.
fn services_without_own_userns(models: &[Value]) -> Vec<String> {
    let mut services = BTreeMap::<String, bool>::new();
    for (name, service) in compose_services(models) {
        let own =
            service.get("userns_mode").is_some() || super::host_network::on_host_network(service);
        *services.entry(name.to_owned()).or_default() |= own;
    }
    services
        .into_iter()
        .filter(|(_, own)| !own)
        .map(|(name, _)| name)
        .collect()
}

/// The services whose Compose files set a `userns_mode` other than
/// `host`, with that mode: Podman chooses their user ids.
pub(crate) fn chosen_user_namespaces(models: &[Value]) -> BTreeMap<String, String> {
    compose_services(models)
        .filter_map(|(name, service)| {
            let mode = service.get("userns_mode")?;
            let mode = mode
                .as_str()
                .map_or_else(|| format!("{mode:?}"), str::to_owned);
            (mode != "host").then(|| (name.to_owned(), mode))
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
    chosen: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut found = Vec::new();
    for (module, config) in &document.config.modules {
        for (service, config) in &config.services {
            let Some(mode) = chosen.get(service) else {
                continue;
            };
            if config.stack != stack
                || config.host_user_namespace.is_some()
                || !config.is_in_environment(environment)
            {
                continue;
            }
            let secrets = config.secret_files_in(environment).map(|(path, file)| {
                let holds = holds_in_any_namespace(Some(&file.uid), Some(&file.gid), file.mode);
                ("secret-files", path, holds)
            });
            let configs = config.config_files_in(environment).map(|(path, file)| {
                let holds =
                    holds_in_any_namespace(file.uid.as_deref(), file.gid.as_deref(), file.mode);
                ("config-files", path, holds)
            });
            for (key, path, _) in secrets.chain(configs).filter(|(_, _, holds)| !holds) {
                found.push(format!(
                    "modules.{module}.services.{service}.{key}.\"{path}\" names an owner or a mode that holds only where mm knows the service's user ids, and its Compose file sets `userns_mode: {mode}`, so Podman picks them when it creates a container, after mm wrote the file. Remove `userns_mode` from service '{service}' (mm then gives it fixed ids of its own and the file to its uid and gid), or give the file uid \"0\", gid \"0\" and a mode every user can read, such as 0o444"
                ));
            }
        }
    }
    found
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
