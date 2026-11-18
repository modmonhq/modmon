use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;

use crate::app::ports::{
    DashboardServiceInstaller, DashboardServiceStatus, IngressRouteReader, IngressVerifier,
    LocalCaStore, LocalDeploymentRuntime, PrivilegedIngressInstaller, RootlessIngressInstaller,
    RuntimeDriver, ServiceExecutorStatus, ServiceStackExecutor, TargetStore,
    TraefikDynamicConfigStore,
};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::error::MmError;
use crate::domain::ownership::ResourceKind;
use crate::domain::runtime::{
    ConvergenceCancellation, ConvergencePolicy, ConvergenceState, ObservedProject, RuntimeResource,
    RuntimeTarget, ServiceState,
};
use crate::domain::service_stack::{
    CAP_HOST, CAP_IMAGE, CAP_VERSION, DASHBOARD_HOST, MAILPIT_HOST, MAILPIT_IMAGE, MAILPIT_VERSION,
    ServiceComponentKind, ServiceComponentStatus, ServiceStackPlan, VALKEY_IMAGE, VALKEY_VERSION,
};
use crate::domain::target_ingress::ingress_verify_port;
use crate::domain::workspace::ProjectId;

const INGRESS_NETWORK_RESOURCE: &str = "ingress-network";
const INGRESS_ACME_RESOURCE: &str = "ingress-acme";
const INGRESS_CONFIG_RESOURCE: &str = "ingress-config";
use crate::domain::service_stack::DEVELOPMENT_NETWORK_RESOURCE;
const MAILPIT_DATA_RESOURCE: &str = "mailpit-data";
const CAP_VALKEY_DATA_RESOURCE: &str = "cap-valkey-data";

#[derive(Clone)]
pub struct LocalServiceStackExecutor {
    routes: Arc<dyn IngressRouteReader>,
    dynamic_configs: Arc<dyn TraefikDynamicConfigStore>,
    targets: Arc<dyn TargetStore>,
    runtime: Arc<dyn LocalDeploymentRuntime>,
    inspector: Arc<dyn RuntimeDriver>,
    ca: Arc<dyn LocalCaStore>,
    verifier: Arc<dyn IngressVerifier>,
    rootless: Arc<dyn RootlessIngressInstaller>,
    privileged: Arc<dyn PrivilegedIngressInstaller>,
    dashboard: Arc<dyn DashboardServiceInstaller>,
}

impl LocalServiceStackExecutor {
    async fn mailpit_component(
        &self,
        plan: &ServiceStackPlan,
        observed: &ObservedProject,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<ServiceComponentStatus, MmError> {
        let service = observed
            .services
            .iter()
            .find(|service| service.identity.service == "mailpit");
        let image = observed
            .containers
            .iter()
            .find(|container| container.service == "mailpit")
            .map(|container| container.image.clone());
        if image.as_deref() != Some(MAILPIT_IMAGE) {
            diagnostics.push(Diagnostic::error(
                "MM_SERVICES_IMAGE_DRIFT",
                format!("Mailpit image differs from pinned digest {MAILPIT_IMAGE}"),
            ));
        }
        if let Err(error) = self
            .verifier
            .verify_route(ingress_verify_port(plan.target_id), MAILPIT_HOST, "/")
            .await
        {
            diagnostics.push(Diagnostic::error(
                "MM_SERVICES_ROUTE_UNHEALTHY",
                error.to_string(),
            ));
        }
        let ca = self.ca.status()?;
        match ca
            .leaves
            .iter()
            .find(|leaf| leaf.hosts.iter().any(|host| host == MAILPIT_HOST))
        {
            None => diagnostics.push(Diagnostic::error(
                "MM_SERVICES_CERT_MISSING",
                "the Mailpit local TLS certificate is missing",
            )),
            Some(leaf) if leaf.stale => diagnostics.push(Diagnostic::error(
                "MM_SERVICES_CERT_STALE",
                "the Mailpit local TLS certificate is stale",
            )),
            Some(_) => {}
        }
        Ok(ServiceComponentStatus {
            kind: ServiceComponentKind::Mailpit,
            version: MAILPIT_VERSION.to_owned(),
            running: service.is_some_and(|service| service.state == ServiceState::Running),
            healthy: service.is_some_and(|service| {
                service.running_count > 0 && service.healthy_count == service.running_count
            }),
            image,
        })
    }

    async fn cap_components(
        &self,
        plan: &ServiceStackPlan,
        observed: &ObservedProject,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<ServiceComponentStatus> {
        if plan.cap_valkey_data_volume.is_none() {
            return Vec::new();
        }
        let cap = observed
            .services
            .iter()
            .find(|service| service.identity.service == "cap");
        let valkey = observed
            .services
            .iter()
            .find(|service| service.identity.service == "valkey");
        let cap_container = observed
            .containers
            .iter()
            .find(|container| container.service == "cap");
        let valkey_container = observed
            .containers
            .iter()
            .find(|container| container.service == "valkey");
        if cap_container.map(|container| container.image.as_str()) != Some(CAP_IMAGE)
            || valkey_container.map(|container| container.image.as_str()) != Some(VALKEY_IMAGE)
        {
            diagnostics.push(Diagnostic::error(
                "MM_GATE_IMAGE_DRIFT",
                "Cap or Valkey differs from its pinned image digest",
            ));
        }
        if let Err(error) = self
            .verifier
            .verify_route(ingress_verify_port(plan.target_id), CAP_HOST, "/")
            .await
        {
            diagnostics.push(Diagnostic::error(
                "MM_GATE_ROUTE_UNHEALTHY",
                error.to_string(),
            ));
        }
        vec![
            service_component(
                ServiceComponentKind::CapValkey,
                VALKEY_VERSION,
                valkey,
                valkey_container,
            ),
            service_component(ServiceComponentKind::Cap, CAP_VERSION, cap, cap_container),
        ]
    }

    async fn dashboard_component(
        &self,
        plan: &ServiceStackPlan,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> ServiceComponentStatus {
        let dashboard = match self.dashboard.status(plan).await {
            Ok(status) => status,
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    "MM_DASHBOARD_SERVICE_DRIFT",
                    error.to_string(),
                ));
                DashboardServiceStatus {
                    installed: false,
                    running: false,
                }
            }
        };
        let mut healthy = dashboard.running;
        if dashboard.running
            && let Err(error) = self
                .verifier
                .verify_route(ingress_verify_port(plan.target_id), DASHBOARD_HOST, "/")
                .await
        {
            healthy = false;
            diagnostics.push(Diagnostic::error(
                "MM_DASHBOARD_ROUTE_UNHEALTHY",
                error.to_string(),
            ));
        }
        if plan.dashboard_service.is_some() && (!dashboard.installed || !dashboard.running) {
            diagnostics.push(
                Diagnostic::error(
                    "MM_DASHBOARD_SERVICE_STOPPED",
                    "the local dashboard control service is not running",
                )
                .with_help("run a reviewed `mm dev services repair` plan"),
            );
        }
        ServiceComponentStatus {
            kind: ServiceComponentKind::MmControlApi,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            running: dashboard.running,
            healthy,
            image: None,
        }
    }
}

fn service_component(
    kind: ServiceComponentKind,
    version: &str,
    service: Option<&crate::domain::runtime::ServiceObservation>,
    container: Option<&crate::domain::runtime::ContainerObservation>,
) -> ServiceComponentStatus {
    ServiceComponentStatus {
        kind,
        version: version.to_owned(),
        running: service.is_some_and(|service| service.state == ServiceState::Running),
        healthy: service.is_some_and(|service| {
            service.running_count > 0 && service.healthy_count == service.running_count
        }),
        image: container.map(|container| container.image.clone()),
    }
}

#[derive(Clone)]
pub struct LocalServiceStackExecutorConfig {
    pub routes: Arc<dyn IngressRouteReader>,
    pub dynamic_configs: Arc<dyn TraefikDynamicConfigStore>,
    pub targets: Arc<dyn TargetStore>,
    pub runtime: Arc<dyn LocalDeploymentRuntime>,
    pub inspector: Arc<dyn RuntimeDriver>,
    pub ca: Arc<dyn LocalCaStore>,
    pub verifier: Arc<dyn IngressVerifier>,
    pub rootless: Arc<dyn RootlessIngressInstaller>,
    pub privileged: Arc<dyn PrivilegedIngressInstaller>,
    pub dashboard: Arc<dyn DashboardServiceInstaller>,
}

impl std::fmt::Debug for LocalServiceStackExecutor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalServiceStackExecutor")
            .finish_non_exhaustive()
    }
}

impl LocalServiceStackExecutor {
    #[must_use]
    pub fn new(config: LocalServiceStackExecutorConfig) -> Self {
        Self {
            routes: config.routes,
            dynamic_configs: config.dynamic_configs,
            targets: config.targets,
            runtime: config.runtime,
            inspector: config.inspector,
            ca: config.ca,
            verifier: config.verifier,
            rootless: config.rootless,
            privileged: config.privileged,
            dashboard: config.dashboard,
        }
    }

    fn target(
        &self,
        plan: &ServiceStackPlan,
    ) -> Result<crate::domain::target::NamedTarget, MmError> {
        let target = self.targets.resolve_target("local")?;
        if target.definition.id != plan.target_id
            || target.definition.canonical_fingerprint() != plan.target_fingerprint
        {
            return Err(MmError::Security {
                message: "the local target changed after the reviewed service plan".to_owned(),
            });
        }
        Ok(target)
    }

    fn ingress_target(
        plan: &ServiceStackPlan,
        target: &crate::domain::target::NamedTarget,
    ) -> RuntimeTarget {
        RuntimeTarget {
            root: plan.ingress.root.clone(),
            project_directory: plan.ingress.root.clone(),
            target: Some(target.clone()),
            project_id: ProjectId(uuid::Uuid::nil()),
            environment: "services".to_owned(),
            stack_name: "ingress".to_owned(),
            project_name: plan.ingress.manifest.compose_project_name.clone(),
            artifact_hash: plan.ingress.manifest.artifact_hash.clone(),
            compose_files: vec![plan.ingress.compose_path.clone()],
            user_compose_file_count: 1,
            interpolation_environment: Default::default(),
            installation_scoped: true,
            registry_credentials: Vec::new(),
        }
    }

    fn services_target(
        plan: &ServiceStackPlan,
        target: &crate::domain::target::NamedTarget,
    ) -> RuntimeTarget {
        RuntimeTarget {
            root: plan.artifact.root.clone(),
            project_directory: plan.artifact.root.clone(),
            target: Some(target.clone()),
            project_id: ProjectId(uuid::Uuid::nil()),
            environment: "services".to_owned(),
            stack_name: "services".to_owned(),
            project_name: crate::domain::service_stack::services_project_name(plan.target_id),
            artifact_hash: plan.plan_hash.clone(),
            compose_files: vec![plan.artifact.compose_path.clone()],
            user_compose_file_count: 1,
            interpolation_environment: Default::default(),
            installation_scoped: true,
            registry_credentials: Vec::new(),
        }
    }

    fn resources(plan: &ServiceStackPlan) -> Vec<RuntimeResource> {
        let mut resources = vec![
            RuntimeResource {
                kind: ResourceKind::Network,
                name: plan.ingress.manifest.network_name.clone(),
                resource: INGRESS_NETWORK_RESOURCE.to_owned(),
            },
            RuntimeResource {
                kind: ResourceKind::Volume,
                name: plan.ingress.manifest.acme_volume_name.clone(),
                resource: INGRESS_ACME_RESOURCE.to_owned(),
            },
            RuntimeResource {
                kind: ResourceKind::Volume,
                name: plan.ingress.manifest.dynamic_config_volume_name.clone(),
                resource: INGRESS_CONFIG_RESOURCE.to_owned(),
            },
            RuntimeResource {
                kind: ResourceKind::Network,
                name: plan.development_network.clone(),
                resource: DEVELOPMENT_NETWORK_RESOURCE.to_owned(),
            },
            RuntimeResource {
                kind: ResourceKind::Volume,
                name: plan.mailpit_data_volume.clone(),
                resource: MAILPIT_DATA_RESOURCE.to_owned(),
            },
        ];
        if let Some(volume) = &plan.cap_valkey_data_volume {
            resources.push(RuntimeResource {
                kind: ResourceKind::Volume,
                name: volume.clone(),
                resource: CAP_VALKEY_DATA_RESOURCE.to_owned(),
            });
        }
        resources
    }

    async fn ensure_resources(
        &self,
        plan: &ServiceStackPlan,
        target: &crate::domain::target::NamedTarget,
    ) -> Result<(), MmError> {
        crate::app::ensure_ingress_network(
            self.runtime.as_ref(),
            target,
            &plan.ingress.manifest,
            INGRESS_NETWORK_RESOURCE,
        )
        .await?;
        self.runtime
            .ensure_volume(
                target,
                &plan.ingress.manifest.acme_volume_name,
                INGRESS_ACME_RESOURCE,
            )
            .await?;
        self.runtime
            .create_volume_file_if_absent(
                target,
                &plan.ingress.manifest.acme_volume_name,
                INGRESS_ACME_RESOURCE,
                "acme.json",
                0o600,
            )
            .await?;
        self.runtime
            .ensure_volume(
                target,
                &plan.ingress.manifest.dynamic_config_volume_name,
                INGRESS_CONFIG_RESOURCE,
            )
            .await?;
        self.runtime
            .ensure_network(
                target,
                &plan.development_network,
                DEVELOPMENT_NETWORK_RESOURCE,
            )
            .await?;
        self.runtime
            .ensure_volume(target, &plan.mailpit_data_volume, MAILPIT_DATA_RESOURCE)
            .await?;
        if let Some(volume) = &plan.cap_valkey_data_volume {
            self.runtime
                .ensure_volume(target, volume, CAP_VALKEY_DATA_RESOURCE)
                .await?;
        }
        Ok(())
    }

    async fn install_route(
        &self,
        plan: &ServiceStackPlan,
        target: &crate::domain::target::NamedTarget,
    ) -> Result<(), MmError> {
        let mut certificate_hosts = plan
            .dashboard_hosts
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        certificate_hosts.insert(MAILPIT_HOST.to_owned());
        if plan.cap_valkey_data_volume.is_some() {
            certificate_hosts.insert(CAP_HOST.to_owned());
        }
        self.publish_certificates(plan, target, &certificate_hosts)
            .await?;
        self.runtime
            .replace_volume_file(
                target,
                &plan.ingress.manifest.dynamic_config_volume_name,
                INGRESS_CONFIG_RESOURCE,
                "mailpit.dynamic.yaml",
                &plan.artifact.route_path,
                0o600,
            )
            .await
    }

    async fn publish_certificates(
        &self,
        plan: &ServiceStackPlan,
        target: &crate::domain::target::NamedTarget,
        hosts: &BTreeSet<String>,
    ) -> Result<(), MmError> {
        if hosts.is_empty() {
            return Ok(());
        }
        let issued = self.ca.issue(hosts)?;
        if issued.len() != hosts.len() {
            return Err(MmError::Security {
                message: "local CA did not issue the complete managed-service certificate set"
                    .to_owned(),
            });
        }
        let volume = &plan.ingress.manifest.dynamic_config_volume_name;
        for host in hosts {
            self.runtime
                .replace_volume_file(
                    target,
                    volume,
                    INGRESS_CONFIG_RESOURCE,
                    &format!("{host}.crt"),
                    &self.ca.certificate_path(host)?,
                    0o644,
                )
                .await?;
            self.runtime
                .replace_volume_file(
                    target,
                    volume,
                    INGRESS_CONFIG_RESOURCE,
                    &format!("{host}.key"),
                    &self.ca.key_path(host)?,
                    0o600,
                )
                .await?;
        }
        Ok(())
    }

    async fn reconcile_project_routes(
        &self,
        plan: &ServiceStackPlan,
        target: &crate::domain::target::NamedTarget,
    ) -> Result<(), MmError> {
        let routes = self.routes.read_routes(plan.target_id)?;
        let hosts = routes
            .claims
            .iter()
            .filter(|claim| claim.tls && crate::domain::ca::is_local_ca_host(&claim.host))
            .map(|claim| claim.host.clone())
            .collect::<BTreeSet<_>>();
        let rendered = crate::app::render_traefik_dynamic_routes(
            plan.target_id,
            &routes.claims,
            &routes.middlewares,
            &hosts,
        )
        .map_err(|error| MmError::Target {
            message: format!("could not render registered project ingress: {error}"),
        })?;
        let config = self.dynamic_configs.publish(plan.target_id, &rendered)?;
        self.publish_certificates(plan, target, &hosts).await?;
        self.runtime
            .replace_volume_file(
                target,
                &plan.ingress.manifest.dynamic_config_volume_name,
                INGRESS_CONFIG_RESOURCE,
                "dynamic.yaml",
                &config.path,
                0o644,
            )
            .await
    }
}

#[async_trait]
impl ServiceStackExecutor for LocalServiceStackExecutor {
    async fn preflight(&self, plan: &ServiceStackPlan) -> Result<(), MmError> {
        plan.validate()
            .map_err(|message| MmError::Security { message })?;
        let target = self.target(plan)?;
        self.runtime
            .preflight(
                &target,
                &Self::resources(plan),
                &[
                    Self::ingress_target(plan, &target),
                    Self::services_target(plan, &target),
                ],
            )
            .await?;
        self.dashboard.preflight(plan).await?;
        if let Some(port) = plan
            .host_smtp_port
            .filter(|port| Some(*port) != plan.previous_host_smtp_port)
        {
            std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).map_err(
                |source| MmError::Security {
                    message: format!(
                        "host SMTP loopback port {port} is unavailable before mutation: {source}"
                    ),
                },
            )?;
        }
        Ok(())
    }

    async fn apply(&self, plan: &ServiceStackPlan) -> Result<(), MmError> {
        let target = self.target(plan)?;
        self.ensure_resources(plan, &target).await?;
        self.reconcile_project_routes(plan, &target).await?;
        let ingress = Self::ingress_target(plan, &target);
        self.runtime.apply(&ingress).await?;
        self.verifier
            .verify_listener(plan.ingress.manifest.proxy_verify_port)
            .await?;
        self.install_route(plan, &target).await?;
        let services = Self::services_target(plan, &target);
        self.runtime.apply(&services).await?;
        let convergence = self
            .inspector
            .wait_for_convergence(
                &services,
                ConvergencePolicy::default(),
                ConvergenceCancellation::default(),
            )
            .await?;
        if convergence.state != ConvergenceState::Succeeded {
            return Err(MmError::RuntimeResponse {
                message: format!(
                    "Mailpit did not converge ({:?}, diagnostic {:?})",
                    convergence.state, convergence.diagnostic_code
                ),
            });
        }
        if plan.dashboard_service.is_some() {
            self.dashboard.install(plan).await?;
            self.verifier
                .verify_route(ingress_verify_port(plan.target_id), DASHBOARD_HOST, "/")
                .await?;
        } else {
            self.dashboard.uninstall(plan).await?;
        }
        self.verifier
            .verify_route(ingress_verify_port(plan.target_id), MAILPIT_HOST, "/")
            .await?;
        if plan.cap_valkey_data_volume.is_some() {
            self.verifier
                .verify_route(ingress_verify_port(plan.target_id), CAP_HOST, "/")
                .await?;
        }
        self.rootless
            .install(&crate::domain::target_ingress::TargetIngressPlan {
                target_name: "local".to_owned(),
                target_id: plan.target_id,
                plan_hash: plan.ingress.manifest.artifact_hash.clone(),
                artifact: plan.ingress.clone(),
            })
            .await?;
        self.privileged
            .install(&crate::domain::target_ingress::TargetIngressPlan {
                target_name: "local".to_owned(),
                target_id: plan.target_id,
                plan_hash: plan.ingress.manifest.artifact_hash.clone(),
                artifact: plan.ingress.clone(),
            })
            .await
    }

    async fn status(&self, plan: &ServiceStackPlan) -> Result<ServiceExecutorStatus, MmError> {
        let target = self.target(plan)?;
        let mut diagnostics = Vec::new();
        if let Err(error) = self.preflight(plan).await {
            diagnostics.push(Diagnostic::error(
                "MM_SERVICES_OWNERSHIP_OR_RESOURCE",
                error.to_string(),
            ));
            return Ok(ServiceExecutorStatus {
                components: Vec::new(),
                diagnostics,
            });
        }
        let ingress_observed = self
            .inspector
            .observe(&Self::ingress_target(plan, &target))
            .await?;
        let services_observed = self
            .inspector
            .observe(&Self::services_target(plan, &target))
            .await?;
        let ingress_running = ingress_observed
            .services
            .iter()
            .any(|service| service.state == ServiceState::Running);
        let mailpit = self
            .mailpit_component(plan, &services_observed, &mut diagnostics)
            .await?;
        let cap = self
            .cap_components(plan, &services_observed, &mut diagnostics)
            .await;
        let dashboard = self.dashboard_component(plan, &mut diagnostics).await;
        let mut components = vec![
            ServiceComponentStatus {
                kind: ServiceComponentKind::LocalIngress,
                version: "3".to_owned(),
                running: ingress_running,
                healthy: ingress_running,
                image: ingress_observed
                    .containers
                    .first()
                    .map(|container| container.image.clone()),
            },
            dashboard,
            mailpit,
        ];
        components.extend(cap);
        Ok(ServiceExecutorStatus {
            components,
            diagnostics,
        })
    }

    async fn remove(&self, plan: &ServiceStackPlan) -> Result<(), MmError> {
        let target = self.target(plan)?;
        self.dashboard.uninstall(plan).await?;
        self.runtime
            .remove(&Self::services_target(plan, &target))
            .await?;
        let empty = plan.artifact.root.join("empty.dynamic.yaml");
        if !empty.is_file() {
            std::fs::write(&empty, crate::domain::traefik_dynamic::EMPTY_DYNAMIC_CONFIG).map_err(
                |source| MmError::Io {
                    context: format!("writing route removal artifact {}", empty.display()),
                    source,
                },
            )?;
        }
        self.runtime
            .replace_volume_file(
                &target,
                &plan.ingress.manifest.dynamic_config_volume_name,
                INGRESS_CONFIG_RESOURCE,
                "mailpit.dynamic.yaml",
                &empty,
                0o644,
            )
            .await?;
        let ingress_plan = crate::domain::target_ingress::TargetIngressPlan {
            target_name: "local".to_owned(),
            target_id: plan.target_id,
            plan_hash: plan.ingress.manifest.artifact_hash.clone(),
            artifact: plan.ingress.clone(),
        };
        self.privileged.uninstall(&ingress_plan).await?;
        self.rootless.uninstall(&ingress_plan).await?;
        self.runtime
            .remove(&Self::ingress_target(plan, &target))
            .await?;
        self.runtime
            .remove_network(
                &target,
                &plan.development_network,
                DEVELOPMENT_NETWORK_RESOURCE,
            )
            .await?;
        self.runtime
            .remove_network(
                &target,
                &plan.ingress.manifest.network_name,
                INGRESS_NETWORK_RESOURCE,
            )
            .await?;
        if plan.purge_data {
            self.runtime
                .remove_volume(&target, &plan.mailpit_data_volume, MAILPIT_DATA_RESOURCE)
                .await?;
            if let Some(volume) = &plan.cap_valkey_data_volume {
                self.runtime
                    .remove_volume(&target, volume, CAP_VALKEY_DATA_RESOURCE)
                    .await?;
            }
            self.runtime
                .remove_volume(
                    &target,
                    &plan.ingress.manifest.dynamic_config_volume_name,
                    INGRESS_CONFIG_RESOURCE,
                )
                .await?;
            self.runtime
                .remove_volume(
                    &target,
                    &plan.ingress.manifest.acme_volume_name,
                    INGRESS_ACME_RESOURCE,
                )
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "service_stack_runtime_tests.rs"]
mod tests;
