//! What a host answers to a status read: what runs, with this node's
//! cluster networks, names and tunnel connector.

use super::DaemonUseCaseAdapter;
use crate::composition::{RuntimeComposition, runtime_operations_service_on};
use crate::domain::error::MmError;

impl DaemonUseCaseAdapter {
    pub(super) async fn status_here(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
    ) -> Result<crate::domain::workspace::WorkspaceSnapshot, MmError> {
        let selection = self.selection(context)?;
        let services = RuntimeComposition::new();
        let manager = runtime_operations_service_on(
            selection.document_scope(),
            &selection.start,
            Some(&self.mm_home),
            context.scope.runtime,
            &services,
        )?;
        // A host answers a status many times a minute and during a
        // deployment: it reads what runs and renders nothing. A node's
        // cluster networks are looked at meanwhile.
        let approval = crate::domain::observability::ApprovalContext {
            principal: context.principal.clone(),
            source: "authenticated-ssh-relay".to_owned(),
            approved_high_risk: false,
        };
        let networks = async {
            match (&self.networks, &selection.node) {
                (Some(host), Some(_)) => {
                    let runtime = services.driver(context.scope.runtime);
                    Some(
                        crate::composition::cluster_networks::status(
                            host.as_ref(),
                            runtime.as_ref(),
                        )
                        .await,
                    )
                }
                _ => None,
            }
        };
        let (snapshot, networks) = tokio::join!(
            manager.status_reading(&selection, &approval, crate::app::StatusRead::Observed),
            networks
        );
        let mut snapshot = snapshot?;
        snapshot.network_drift = match networks {
            Some(Ok(status)) => status.drift.into_iter().map(|drift| drift.detail).collect(),
            // The services are still worth showing.
            Some(Err(error)) => vec![format!(
                "the node's cluster networks could not be read: {error}"
            )],
            None => Vec::new(),
        };
        snapshot.target = context.scope.target.clone();
        snapshot.target_source = Some("authenticated-remote-scope".to_owned());
        if let (Some(names), Some(_)) = (&self.names, &selection.node) {
            // The names of this environment's networks, as this node's
            // resolver answers them.
            let report = names
                .report(&crate::domain::cluster_network::network_key(
                    context.scope.project_id,
                    &context.scope.environment,
                    "",
                ))
                .only_networks(crate::composition::cluster_networks::environment_networks(
                    context,
                    &selection.start,
                ));
            snapshot.names = (!report.is_empty()).then_some(report);
        }
        if let (Some(host), Some(_)) = (&self.node_host, &selection.node) {
            snapshot.tunnel = host
                .ingress
                .tunnel_status(&crate::domain::cluster_ingress::table_name(
                    context.scope.project_id,
                    &context.scope.environment,
                ))
                .await;
        }
        Ok(snapshot)
    }
}
