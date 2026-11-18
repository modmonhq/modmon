//! The managed secret store of cluster environments: `mode: managed` with
//! `scope: environment` under `providers:`.

use crate::domain::config::ConfigDocument;
use crate::domain::diagnostic::{Diagnostic, ValidationReport};
use crate::domain::secret_backend::{ProviderConfiguration, ProviderScope};
use crate::domain::secret_store::{
    STORE_DRIVER, is_cluster_store, pinned_lines, pinned_release, store_members,
};

const CODE: &str = "MM_SECRET_STORE_INVALID";

pub(super) fn validate_secret_store(document: &ConfigDocument, report: &mut ValidationReport) {
    let config = &document.config;
    let mut stores = Vec::new();
    for (name, instance) in &config.providers {
        if instance.scope != ProviderScope::Environment {
            continue;
        }
        let mut problems = Vec::new();
        if !is_cluster_store(instance) || instance.driver.as_str() != STORE_DRIVER {
            problems.push(format!(
                "providers.{name} has `scope: environment`, which is for the store mm runs on a cluster's nodes: `driver: openbao` with `mode: managed`"
            ));
        }
        if let ProviderConfiguration::Managed { release, nodes, .. } = &instance.configuration {
            if pinned_release(release).is_none() {
                problems.push(format!(
                    "providers.{name}.release is '{release}'; the store mm runs on a cluster's nodes is OpenBao {}",
                    pinned_lines().join(" or ")
                ));
            }
            for (environment, target) in config.environments.keys().filter_map(|environment| {
                crate::app::cluster_target(document, environment)
                    .map(|(_, target)| (environment, target))
            }) {
                let names = target
                    .nodes
                    .iter()
                    .map(|node| node.name.as_str())
                    .collect::<Vec<_>>();
                if let Err(problem) = store_members(nodes.as_ref(), &names, name.as_str()) {
                    problems.push(format!("environment '{environment}': {problem}"));
                }
            }
        }
        stores.push(name.as_str());
        for problem in problems {
            report.push(Diagnostic::error(CODE, problem).at_path(document.source("providers")));
        }
    }
    if stores.len() > 1 {
        report.push(
            Diagnostic::error(
                CODE,
                format!(
                    "providers {} each declare `scope: environment`; an environment has one managed store. Keep one and point the secrets of the others at it",
                    stores.join(" and ")
                ),
            )
            .at_path(document.source("providers")),
        );
    }
}
