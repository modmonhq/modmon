use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;

use crate::adapters::runtime::RuntimeFactory;
use crate::app::ports::{GateIngressPublisher, ServiceStackStore, TraefikDynamicConfigStore};
use crate::domain::error::MmError;
use crate::domain::ingress::{CanonicalMiddleware, IngressRouteClaim};
use crate::domain::service_stack::ServiceStackJournalState;
use crate::domain::target::{NamedTarget, TargetId};
use crate::domain::target_ingress::ingress_dynamic_config_volume_name;

const INGRESS_CONFIG_RESOURCE: &str = "ingress-config";

pub struct RuntimeGateIngressPublisher {
    target: NamedTarget,
    services: Arc<dyn ServiceStackStore>,
    runtimes: RuntimeFactory,
    artifacts: Arc<dyn TraefikDynamicConfigStore>,
}

impl RuntimeGateIngressPublisher {
    #[must_use]
    pub fn new(
        target: NamedTarget,
        services: Arc<dyn ServiceStackStore>,
        runtimes: RuntimeFactory,
        artifacts: Arc<dyn TraefikDynamicConfigStore>,
    ) -> Self {
        Self {
            target,
            services,
            runtimes,
            artifacts,
        }
    }
}

#[async_trait]
impl GateIngressPublisher for RuntimeGateIngressPublisher {
    async fn publish(
        &self,
        target_id: TargetId,
        claims: &[IngressRouteClaim],
        middlewares: &BTreeMap<String, CanonicalMiddleware>,
    ) -> Result<String, MmError> {
        if self.target.definition.id != target_id {
            return Err(MmError::Security {
                message: "gate ingress target does not match the installed local target".to_owned(),
            });
        }
        let service_state = self.services.load_state()?.ok_or_else(|| MmError::State {
            message: "the local service stack is not installed; run `mm services install` before changing access-gate routes"
                .to_owned(),
        })?;
        if service_state.target_id != target_id
            || service_state.journal_state != ServiceStackJournalState::Applied
        {
            return Err(MmError::Security {
                message: "access-gate routes require an applied local service stack on the selected target"
                    .to_owned(),
            });
        }
        let runtime = self.runtimes.deployment(service_state.runtime)?;
        let local_hosts = claims
            .iter()
            .filter(|claim| claim.tls && crate::domain::ca::is_local_ca_host(&claim.host))
            .map(|claim| claim.host.clone())
            .collect::<BTreeSet<_>>();
        let rendered =
            crate::app::render_traefik_dynamic_routes(target_id, claims, middlewares, &local_hosts)
                .map_err(|error| MmError::Target {
                    message: format!("could not render access-gate routes: {error}"),
                })?;
        let artifact = self.artifacts.publish(target_id, &rendered)?;
        let volume = ingress_dynamic_config_volume_name(target_id);
        runtime
            .ensure_volume(&self.target, &volume, INGRESS_CONFIG_RESOURCE)
            .await?;
        runtime
            .replace_volume_file(
                &self.target,
                &volume,
                INGRESS_CONFIG_RESOURCE,
                "dynamic.yaml",
                &artifact.path,
                0o644,
            )
            .await?;
        Ok(rendered.manifest.digest)
    }
}
