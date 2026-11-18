use super::*;

/// A status check may take one SSH connection and a full inspect on the host.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(25);

/// Every configured environment for the Environments view; best effort, so
/// the TUI still opens when the list cannot be read.
pub(super) fn entries(
    invocation: &super::super::dispatch::Invocation,
) -> Vec<tui::EnvironmentEntry> {
    match composition::environment_placements(&invocation.start, invocation.home()) {
        Ok(placements) => placements
            .into_iter()
            .map(|placement| {
                let mut entry = tui::EnvironmentEntry {
                    name: placement.name,
                    default: placement.default,
                    deploy_from: placement.deploy_from,
                    ..tui::EnvironmentEntry::default()
                };
                match placement.resolved {
                    Ok(resolved) => {
                        entry.target = resolved.target;
                        entry.remote = resolved.remote;
                        entry.runtime = Some(resolved.runtime);
                    }
                    Err(error) => entry.problem = Some(error.to_string()),
                }
                entry
            })
            .collect(),
        Err(error) => {
            tracing::debug!(%error, "could not list environments for the TUI");
            Vec::new()
        }
    }
}

/// Checks and opens other environments of the project the TUI was started in.
pub(super) struct EnvironmentDirectory {
    /// The invocation the TUI was started with, including its flags.
    pub(super) base: super::super::dispatch::Invocation,
    /// The environment those flags apply to.
    pub(super) launched: String,
}

impl EnvironmentDirectory {
    /// The base invocation pointed at `environment`, before its runtime is
    /// resolved. Target and runtime flags apply only where the TUI started.
    fn selected(&self, environment: &str) -> super::super::dispatch::Invocation {
        let mut invocation = self.base.clone();
        invocation.selection.environment = Some(environment.to_owned());
        if environment != self.launched {
            invocation.selection.target = None;
            invocation.selection.runtime = None;
        }
        invocation
    }

    /// The invocation for `environment`: the original flags for the
    /// environment the TUI was started in, and that environment's own target
    /// and runtime for any other.
    fn invocation(&self, environment: &str) -> Result<super::super::dispatch::Invocation, MmError> {
        let mut invocation = self.selected(environment);
        // The base invocation was prepared before anything was resolved, so
        // the runtime is resolved here for the launched environment too.
        invocation.runtime_kind = composition::resolve_runtime(
            &invocation.start,
            invocation.home(),
            invocation.selection.runtime,
            invocation.selection.target.as_deref(),
            Some(environment),
            false,
            true,
        )?;
        invocation.runtime = invocation.services.driver(invocation.runtime_kind);
        Ok(invocation)
    }
}

#[async_trait::async_trait]
impl tui::EnvironmentDirectory for EnvironmentDirectory {
    fn environments(&self) -> Vec<tui::EnvironmentEntry> {
        entries(&self.base)
    }

    fn team(&self, environment: &str) -> (Option<String>, Vec<tui::TeamMemberView>) {
        match composition::project_documents().load_from(&self.base.start) {
            Ok(document) => team_view(&document, environment, &self.base),
            Err(error) => {
                tracing::debug!(%error, "could not read the users file for the TUI");
                (None, Vec::new())
            }
        }
    }

    fn device_key(&self) -> Result<String, MmError> {
        let home = self.base.home();
        Ok(match composition::device_key(home)? {
            Some(key) => key.public_key,
            None => composition::create_device_key(home, false)?.key.public_key,
        })
    }

    async fn probe(&self, environment: &str) -> Result<tui::EnvironmentProbe, MmError> {
        let invocation = self.invocation(environment)?;
        let check = async {
            // Who you are there is asked beside the rest, not after it.
            let identify = async {
                match composition::remote_transport_context(
                    &invocation.selection,
                    invocation.home(),
                    invocation.runtime_kind,
                ) {
                    Ok(Some((transport, scope))) => Ok(transport.whoami(scope).await.ok()),
                    Ok(None) => Ok(None),
                    Err(error) => Err(error),
                }
            };
            let (snapshot, nodes, identity) = tokio::join!(
                workspace_snapshot(&invocation),
                checked_nodes(&invocation),
                identify
            );
            Ok::<_, MmError>((snapshot?, identity?, nodes))
        };
        let (snapshot, identity, nodes) = tokio::time::timeout(PROBE_TIMEOUT, check)
            .await
            .map_err(|_| did_not_answer())??;
        Ok(tui::EnvironmentProbe {
            services: snapshot
                .stacks
                .iter()
                .flat_map(|stack| &stack.services)
                .map(tui::ServiceLine::from)
                .collect(),
            routes: snapshot
                .ingress
                .iter()
                .filter(|status| status.url.is_some())
                .count(),
            issues: snapshot
                .validation
                .diagnostics
                .iter()
                .chain(snapshot.runtime.diagnostics.iter())
                .filter(|diagnostic| diagnostic.severity != Severity::Info)
                .count(),
            principal: identity.as_ref().map(|identity| identity.principal.clone()),
            access: identity
                .as_ref()
                .and_then(|identity| identity.access.as_ref())
                .map(|access| access.summary()),
            deployed_commit: identity.and_then(|identity| identity.deployed_commit),
            nodes,
        })
    }

    async fn probe_nodes(&self, environment: &str) -> Result<Vec<tui::NodeLine>, MmError> {
        let invocation = self.invocation(environment)?;
        tokio::time::timeout(PROBE_TIMEOUT, checked_nodes(&invocation))
            .await
            .map_err(|_| did_not_answer())
    }

    async fn launch(&self, environment: &str) -> Result<tui::LaunchRequest, MmError> {
        // The runtime comes from the readiness report, so an environment
        // with none chosen yet still opens, on its setup steps.
        open(
            &self.selected(environment),
            self,
            false,
            &tui::LoadProgress::silent(),
        )
        .await
    }

    async fn load(
        &self,
        environment: &str,
        progress: tui::LoadProgress,
    ) -> Result<tui::LaunchRequest, MmError> {
        open(&self.selected(environment), self, true, &progress).await
    }
}

fn did_not_answer() -> MmError {
    MmError::RuntimeResponse {
        message: "the environment did not answer in time".to_owned(),
    }
}

/// The nodes of a cluster target as `mm target nodes`, `mm target mesh
/// status`, and `mm target storage status` read them, all at once. A local
/// environment and a single host have no node list to show.
async fn checked_nodes(invocation: &super::super::dispatch::Invocation) -> Vec<tui::NodeLine> {
    let (nodes, mesh, storage) = tokio::join!(
        composition::target_nodes(
            &invocation.selection,
            invocation.home(),
            composition::RemoteIdentity::User,
        ),
        composition::target_mesh(
            &invocation.selection,
            invocation.home(),
            composition::RemoteIdentity::User,
        ),
        // Without sizes: counting files is for the command.
        composition::target_storage(
            &invocation.selection,
            invocation.home(),
            composition::RemoteIdentity::User,
            false,
        ),
    );
    let mesh = mesh.map(|report| report.nodes).unwrap_or_default();
    let storage = storage.ok();
    match nodes {
        Ok((_, nodes)) if nodes.len() > 1 => nodes
            .iter()
            .map(|node| {
                tui::NodeLine::from(node)
                    .with_mesh(mesh.iter().find(|mesh| mesh.name == node.name))
                    .with_storage(storage.as_ref())
            })
            .collect(),
        _ => Vec::new(),
    }
}
