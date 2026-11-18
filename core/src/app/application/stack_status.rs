//! The stacks of a status snapshot: what the engine renders and runs for
//! each, or what the configuration declares when the engine is not asked.

use std::collections::BTreeMap;

use futures_util::future::join_all;

use super::Application;
use super::snapshot::{
    configured_service_names, job_only_service_names, require_capabilities, service_snapshots,
    skipped_target_runtime, target_blocks_runtime, user_runtime_target,
};
use crate::app::config_validation::runtime_targets;
use crate::domain::config::ConfigDocument;
use crate::domain::diagnostic::{Diagnostic, ValidationReport};
use crate::domain::error::MmError;
use crate::domain::runtime::{
    ObservedProject, RenderedProject, RuntimeCapability, RuntimeProbe, RuntimeTarget,
};
use crate::domain::target::NamedTarget;
use crate::domain::workspace::StackSnapshot;

/// How much a status asks of the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusRead {
    /// The stacks are rendered: the status also says what the Compose
    /// files declare beyond the configuration (resources, a service the
    /// configuration names and the files lack).
    Rendered,
    /// Only what runs is read: the containers of each stack's project. A
    /// host answers this many times a minute and during a deployment, so
    /// it renders nothing and waits for nothing a deployment holds.
    Observed,
}

/// How many stacks are read from the engine at once. Each takes three
/// engine commands; more at a time would only queue on a small host.
const STACKS_AT_ONCE: usize = 4;

impl Application {
    /// The engine's own report and the stacks of a status. The engine is
    /// probed while the stacks are read from it: each is several engine
    /// commands, and neither waits for the other. What was read is used
    /// only when the probe says the engine can be asked; otherwise the
    /// stacks are the ones the configuration declares.
    pub(super) async fn runtime_and_stacks(
        &self,
        document: &ConfigDocument,
        environment: &str,
        target: &NamedTarget,
        (validation, read): (&mut ValidationReport, StatusRead),
    ) -> Result<(RuntimeProbe, Vec<StackSnapshot>), MmError> {
        let blocked = target_blocks_runtime(validation);
        let probe = async {
            if blocked {
                skipped_target_runtime(self.runtime.kind())
            } else if read == StatusRead::Observed {
                self.runtime.status_probe().await
            } else {
                self.runtime.probe().await
            }
        };
        let inspect = async {
            if blocked || !validation.is_valid() {
                return None;
            }
            Some(
                self.inspected_stacks(document, environment, target, read)
                    .await,
            )
        };
        let (mut runtime, inspected) = tokio::join!(probe, inspect);
        let required = vec![
            RuntimeCapability::RenderCompose,
            RuntimeCapability::ObserveServices,
        ];
        let runtime_ready = runtime.available && require_capabilities(&mut runtime, &required);
        let stacks = match inspected {
            Some(inspected) if runtime_ready => {
                let (stacks, findings) = inspected?;
                validation.diagnostics.extend(findings);
                stacks
            }
            _ => declared_stacks(
                document,
                runtime_targets(document, environment, Some(target)),
            ),
        };
        Ok((runtime, stacks))
    }

    /// Every stack as the engine renders and runs it, in configuration
    /// order, with what the rendering says about the service mapping.
    async fn inspected_stacks(
        &self,
        document: &ConfigDocument,
        environment: &str,
        target: &NamedTarget,
        read: StatusRead,
    ) -> Result<(Vec<StackSnapshot>, Vec<Diagnostic>), MmError> {
        let targets = self
            .stack_manager()?
            .runtime_targets(document, environment, target)?;
        let mut stacks = Vec::with_capacity(targets.len());
        let mut findings = Vec::new();
        let mut targets = targets.into_iter().peekable();
        while targets.peek().is_some() {
            let few = targets.by_ref().take(STACKS_AT_ONCE);
            let inspected =
                join_all(few.map(|target| self.inspected_stack(document, target, read))).await;
            for stack in inspected {
                let (stack, found) = stack?;
                stacks.push(stack);
                findings.extend(found);
            }
        }
        Ok((stacks, findings))
    }

    /// One stack: the user's own files are rendered for the service
    /// mapping while the stack as mm deploys it is rendered and observed,
    /// all at the same time.
    async fn inspected_stack(
        &self,
        document: &ConfigDocument,
        target: RuntimeTarget,
        read: StatusRead,
    ) -> Result<(StackSnapshot, Vec<Diagnostic>), MmError> {
        if read == StatusRead::Observed {
            // The containers by the project's name, and the services the
            // configuration declares: nothing is rendered.
            let declared = declared_project(document, &target);
            if let Some(observed) = self
                .runtime
                .observe_unrendered(&target, &declared.service_names)
                .await
            {
                return Ok((
                    stack_snapshot(document, target, &declared, observed?),
                    Vec::new(),
                ));
            }
        }
        let mapping = async {
            let mut report = ValidationReport::default();
            if let Ok(user_target) = user_runtime_target(&target)
                && let Ok(user_rendered) = self.runtime.render(&user_target).await
            {
                self.validate_service_mapping(
                    document,
                    &target.environment,
                    &target.stack_name,
                    &user_rendered,
                    &mut report,
                );
            }
            report.diagnostics
        };
        let (findings, deployed) = tokio::join!(mapping, self.runtime.render_and_observe(&target));
        let (rendered, observed) = deployed?;
        Ok((
            stack_snapshot(document, target, &rendered, observed),
            findings,
        ))
    }
}

/// The stacks as the configuration declares them, with nothing observed.
fn declared_stacks(document: &ConfigDocument, targets: Vec<RuntimeTarget>) -> Vec<StackSnapshot> {
    targets
        .into_iter()
        .map(|target| {
            let rendered = declared_project(document, &target);
            let observed = ObservedProject {
                stack: target.stack_name.clone(),
                services: Vec::new(),
                containers: Vec::new(),
            };
            stack_snapshot(document, target, &rendered, observed)
        })
        .collect()
}

/// A stack's project as the configuration declares it: its name and the
/// services placed in it, with nothing the engine would add by rendering.
fn declared_project(document: &ConfigDocument, target: &RuntimeTarget) -> RenderedProject {
    RenderedProject {
        stack: target.stack_name.clone(),
        project_name: target.project_name.clone(),
        service_names: configured_service_names(document, &target.stack_name, &target.environment),
        services: BTreeMap::new(),
        resources: Vec::new(),
        config_hash: String::new(),
    }
}

fn stack_snapshot(
    document: &ConfigDocument,
    target: RuntimeTarget,
    rendered: &RenderedProject,
    observed: ObservedProject,
) -> StackSnapshot {
    let job_only = job_only_service_names(document, &target.stack_name, &target.environment);
    StackSnapshot {
        name: target.stack_name,
        compose_project_name: rendered.project_name.clone(),
        artifact_hash: target.artifact_hash,
        rendered_config_hash: rendered.config_hash.clone(),
        services: service_snapshots(document, rendered, observed, &job_only),
        owned_resources: rendered
            .resources
            .iter()
            .filter(|resource| {
                !resource.external
                    && resource.ownership_labels.get("io.mm.project-id")
                        == Some(&document.config.project.id.0.to_string())
            })
            .map(|resource| crate::domain::workspace::OwnedResourceSnapshot {
                kind: resource.kind,
                logical_name: resource.logical_name.clone(),
            })
            .collect(),
    }
}

/// What runs on this host that differs from the configuration its last
/// deployment applied: a service placed here that has no container (unless
/// `replicas` or `autoscale` spreads it over its nodes, or it has `role:
/// job`), and a
/// service that runs here although the configuration places it on other
/// nodes (one that moved away and was not stopped, or was started by hand).
/// A Compose service the configuration does not declare is not drift: on
/// one host the Compose files may run more than `modules` names.
pub(super) fn service_drift(
    document: &ConfigDocument,
    environment: &str,
    stacks: &[StackSnapshot],
) -> Vec<String> {
    let mut drift = Vec::new();
    for stack in stacks {
        let placed = configured_service_names(document, &stack.name, environment);
        let placed_elsewhere = |name: &str| {
            document.config.modules.values().any(|module| {
                module.services.get(name).is_some_and(|service| {
                    service.stack == stack.name
                        && service.is_in_environment(environment)
                        && !service.is_active(environment)
                })
            })
        };
        // A service whose count is spread over its nodes may have none here.
        let scaled = |name: &str| {
            document.config.modules.values().any(|module| {
                module.services.get(name).is_some_and(|service| {
                    service.stack == stack.name
                        && (service.replicas.is_some() || service.autoscale.is_some())
                })
            })
        };
        for service in &stack.services {
            let runs = service
                .containers
                .iter()
                .any(|container| !container.history);
            let job_only = service.state == crate::domain::runtime::ServiceState::JobOnly;
            if placed.contains(&service.name) && !runs && !scaled(&service.name) && !job_only {
                drift.push(format!(
                    "service '{}' is deployed here and has no container",
                    service.name
                ));
            } else if runs && placed_elsewhere(&service.name) {
                drift.push(format!(
                    "service '{}' runs here, and the deployed configuration places it on other nodes",
                    service.name
                ));
            }
        }
    }
    drift
}
