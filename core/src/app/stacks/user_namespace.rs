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
use crate::domain::user_namespace::{ServiceIds, ServiceIdsChoice, ServiceIdsOwner};

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
        let user_ids = self.user_ids(stack, sources, shared)?;
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
        Ok(StackHost {
            user_ids,
            config_files,
            low_ports,
        })
    }

    fn user_ids(
        &self,
        stack: &str,
        sources: &[CompiledComposeSource],
        shared: &super::shared_volumes::NodeVolumes,
    ) -> Result<StackUserIds, MmError> {
        let Some(user_ids) = self.user_ids else {
            return Ok(StackUserIds::default());
        };
        let keeps_host_ids = host_user_namespaces(self.document, self.environment, stack);
        let wanted = services_without_own_userns(sources)?
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

/// The services of the stack's Compose files that do not set
/// `userns_mode` themselves and are not on the host's network.
fn services_without_own_userns(sources: &[CompiledComposeSource]) -> Result<Vec<String>, MmError> {
    let mut services = BTreeMap::<String, bool>::new();
    for source in sources {
        let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
            message: format!("Compose file '{}': {cause}", source.manifest.path.display()),
        };
        let mut model: Value = serde_yaml_ng::from_slice(&source.bytes).map_err(invalid)?;
        model.apply_merge().map_err(invalid)?;
        let Some(listed) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, service) in listed {
            let Some(name) = name.as_str() else {
                continue;
            };
            let own = service.get("userns_mode").is_some()
                || super::host_network::on_host_network(service);
            *services.entry(name.to_owned()).or_default() |= own;
        }
    }
    Ok(services
        .into_iter()
        .filter(|(_, own)| !own)
        .map(|(name, _)| name)
        .collect())
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
