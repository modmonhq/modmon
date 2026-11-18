//! A cluster node's part in the ingress of a reviewed plan: an ingress node
//! serves the plan's routes through its proxy, every other node takes away
//! what it served of the environment before.

use super::*;
use crate::domain::cluster_ingress::{IngressTable, SealedIngress, TableTunnel, table_name};

impl DeploymentService {
    /// Serves `ingress` on `node` when the plan names it, and takes the
    /// environment's routes off it otherwise. The proxy picks the routes up
    /// without a restart; this returns once it runs them.
    pub async fn apply_cluster_ingress(
        &self,
        selection: &WorkspaceSelection,
        node: &str,
        ingress: &SealedIngress,
    ) -> Result<(), MmError> {
        let host = self
            .planner
            .ingress_host()
            .ok_or_else(|| MmError::Environment {
                message: "this daemon does not run an ingress proxy".to_owned(),
            })?;
        let document = self.ports.documents.load_from(&selection.start)?;
        let environment = crate::domain::config::resolve_environment(
            &document,
            selection.environment.as_deref(),
        )?;
        let table = table_name(document.config.project.id, &environment);
        // The engine's network the proxy meets the routed services on.
        let stored = match self.planner.cluster_networks() {
            Some(networks) => networks.stored()?,
            None => None,
        };
        let network = stored
            .as_ref()
            .and_then(|stored| stored.local_network(&ingress.network))
            .map(|local| local.network);
        if !ingress.serves(node) {
            return host.withdraw(&table, network.as_deref()).await;
        }
        let network = network.ok_or_else(|| MmError::Placement {
            message: format!(
                "node '{node}' runs the ingress proxy of '{environment}' but holds no addresses of its network; run `mm deploy plan` again"
            ),
        })?;
        let certificate = match &ingress.certificate {
            Some(tls) => {
                let provided = self
                    .ports
                    .environments
                    .provided_certificate(selection, tls)
                    .await?;
                Some(host.keep_certificate(&table, &provided.certificate, &provided.key)?)
            }
            None => None,
        };
        let token = match &ingress.tunnel {
            Some(tunnel) => Some(
                self.ports
                    .environments
                    .tunnel_token(selection, &tunnel.token)
                    .await?,
            ),
            None => None,
        };
        let routes = IngressTable {
            network: ingress.network.clone(),
            routes: ingress.routes.clone(),
            certificate,
            engine_network: Some(network),
            tunnel: ingress.tunnel.as_ref().map(|tunnel| TableTunnel {
                token: tunnel.token.clone(),
                image: tunnel.image.clone(),
                metrics_port: None,
                generation: 0,
            }),
        };
        host.ensure_proxy(&table, &routes).await?;
        host.publish(
            &table,
            &routes,
            token.as_ref().map(|held| held.token.as_slice()),
        )
        .await
    }
}
