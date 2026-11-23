//! On a remote host that runs Podman every service gets user ids of its
//! own that mm chooses and keeps (`domain::user_namespace`): root in a
//! container is an unprivileged user of the host, no two services share
//! ids, and a service has the same ids in every container, on every node
//! and from one deployment to the next. A service keeps the host's ids
//! with `host-user-namespace`, whose value says why.
//!
//! Files mm hands over (config files, secret files) are given to the host
//! ids that their `uid` and `gid` are in the service's namespace, so those
//! have to be among its 65536 ids. And a volume that two services mount
//! works only when all of them keep the host's ids: Podman gives a new
//! volume to the ids of the first container that mounts it, and each
//! service's files belong to ids the others do not have.

use super::*;
use crate::domain::runtime::RuntimeKind;
use crate::domain::user_namespace::SERVICE_IDS;

pub(super) fn validate_user_namespaces(
    context: &ValidationContext<'_>,
    report: &mut ValidationReport,
) {
    let document = context.document;
    let environment = context.environment;
    for (module, config) in &document.config.modules {
        for (service, config) in &config.services {
            if config
                .host_user_namespace
                .as_deref()
                .is_some_and(|reason| reason.trim().is_empty())
            {
                report.push(Diagnostic::error(
                    "MM_USER_NAMESPACE",
                    format!(
                        "modules.{module}.services.{service}.host-user-namespace has no reason; write why the service keeps the host's user ids, or remove the key"
                    ),
                ));
            }
        }
    }
    let podman = document
        .config
        .environments
        .get(environment)
        .and_then(|environment| environment.runtime.as_ref())
        .and_then(|runtime| runtime.driver)
        .or(document.config.runtime.driver)
        == Some(RuntimeKind::PodmanCompose);
    // A target of the repository is a host mm set up for Podman.
    if !podman
        || !crate::domain::user_namespace::on_prepared_hosts(&document.config, environment, None)
    {
        return;
    }
    validate_owner_ids(context, report);
    for (stack_name, stack) in &document.config.stacks {
        // Missing and malformed files are reported by the stack checks.
        let Some(models) = super::shared_volumes::compose_models(document, stack) else {
            continue;
        };
        let own_ids = crate::app::stacks::services_without_own_userns(&models);
        for finding in crate::app::stacks::files_in_chosen_namespaces(
            document,
            environment,
            stack_name,
            &crate::app::stacks::chosen_user_namespaces(&models),
        ) {
            report.push(Diagnostic::error("MM_USER_NAMESPACE", finding));
        }
        for (volume, users) in crate::app::stacks::volume_users(&models) {
            let users = users
                .into_iter()
                .filter_map(|service| {
                    let ids = Ids::of(document, environment, stack_name, &service)?;
                    Some((service, ids))
                })
                .collect::<Vec<_>>();
            let own = users
                .iter()
                .filter(|(service, _)| own_ids.contains(service))
                .filter_map(|(service, ids)| match ids {
                    Ids::Own(module) => Some((service, module)),
                    Ids::Host => None,
                })
                .collect::<Vec<_>>();
            if users.len() < 2 || own.is_empty() {
                continue;
            }
            let names = users
                .iter()
                .map(|(service, _)| format!("'{service}'"))
                .collect::<Vec<_>>()
                .join(", ");
            let keys = own
                .iter()
                .map(|(service, module)| format!("modules.{module}.services.{service}"))
                .collect::<Vec<_>>()
                .join(" and ");
            report.push(Diagnostic::error(
                "MM_USER_NAMESPACE",
                format!(
                    "services {names} of stack '{stack_name}' all mount volume '{volume}', and on Podman each service has user ids of its own: the volume belongs to the one that starts first, and none can change what another wrote. Use the volume in one service, or keep the host's ids for every service that mounts it with `host-user-namespace: <why>` under {keys}"
                ),
            ));
        }
    }
}

/// The ids a service of the environment runs with.
enum Ids {
    /// Its own, and the module mm.yaml lists it under.
    Own(String),
    Host,
}

impl Ids {
    /// `None` when `service` of `stack` is not part of `environment`. A
    /// Compose service mm.yaml does not list is part of every environment.
    fn of(
        document: &ConfigDocument,
        environment: &str,
        stack: &str,
        service: &str,
    ) -> Option<Self> {
        let listed = document.config.modules.iter().find_map(|(module, config)| {
            config
                .services
                .get(service)
                .filter(|config| config.stack == stack)
                .map(|config| (module, config))
        });
        match listed {
            Some((_, config)) if !config.is_in_environment(environment) => None,
            Some((_, config)) if config.host_user_namespace.is_some() => Some(Self::Host),
            Some((module, _)) => Some(Self::Own(module.clone())),
            None => Some(Self::Own("<module>".to_owned())),
        }
    }
}

/// The owners a service's config files and secret files name are among
/// the ids its containers have.
fn validate_owner_ids(context: &ValidationContext<'_>, report: &mut ValidationReport) {
    // A value that is not a number is the config checks' finding.
    let outside = |value: Option<&String>| match value.map(|value| value.parse::<u64>()) {
        Some(Ok(id)) if id >= u64::from(SERVICE_IDS) => Some(id),
        Some(Ok(_) | Err(_)) | None => None,
    };
    for (module, config) in &context.document.config.modules {
        for (service, config) in &config.services {
            if config.host_user_namespace.is_some()
                || !config.is_in_environment(context.environment)
            {
                continue;
            }
            let owners = config
                .config_files_in(context.environment)
                .map(|(path, file)| ("config-files", path, file.uid.as_ref(), file.gid.as_ref()))
                .chain(
                    config
                        .secret_files_in(context.environment)
                        .map(|(path, file)| {
                            ("secret-files", path, Some(&file.uid), Some(&file.gid))
                        }),
                );
            for (key, path, uid, gid) in owners {
                if let Some(id) = outside(uid).or_else(|| outside(gid)) {
                    report.push(Diagnostic::error(
                        "MM_USER_NAMESPACE",
                        format!(
                            "modules.{module}.services.{service}.{key}.\"{path}\" names id {id}, and on Podman the service's containers have the ids 0 to {}: name one of them, or keep the host's ids with `host-user-namespace: <why>`",
                            SERVICE_IDS - 1
                        ),
                    ));
                }
            }
        }
    }
}
