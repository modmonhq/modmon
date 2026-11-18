use super::*;

pub(crate) fn inspect_local_state(home: Option<&Path>) -> Result<(), MmError> {
    let home = MmHome::inspect(home)?;
    SqliteStateStore::inspect(&home.state_database).map(|_| ())
}

pub(crate) fn state_recovery(
    home: Option<&Path>,
) -> Result<Box<dyn crate::app::ports::StateRecoveryStore>, MmError> {
    let home = MmHome::inspect(home)?;
    Ok(Box::new(crate::adapters::state::SqliteStateRecovery {
        path: home.state_database,
    }))
}

pub(crate) fn readiness_service(
    home: Option<&Path>,
    runtime: &RuntimeComposition,
) -> Result<crate::app::ReadinessService, MmError> {
    let home = MmHome::inspect(home)?;
    let services_check = development_services_check(&home, runtime)?;
    let tls_check = Arc::new(crate::app::LocalTlsCheck::new(
        project_documents(),
        Arc::new(crate::adapters::state::SqliteStateInspector {
            path: home.state_database.clone(),
        }),
        Arc::new(FilesystemTargetStore::new(home.targets_file.clone())),
        local_ca_manager(Some(&home.root), false, runtime.process())?,
    ));
    Ok(crate::app::ReadinessService::new(
        project_documents(),
        Arc::new(crate::adapters::state::SqliteStateInspector {
            path: home.state_database,
        }),
        Arc::new(FilesystemTargetStore::new(home.targets_file)),
        RuntimeKind::ALL
            .into_iter()
            .map(|kind| runtime.driver(kind))
            .collect(),
    )
    .with_probe(tls_check)
    .with_probe(services_check))
}

pub(crate) fn development_services_check(
    home: &MmHome,
    runtime: &RuntimeComposition,
) -> Result<Arc<dyn crate::app::ports::ReadinessProbe>, MmError> {
    let runtimes = RuntimeKind::ALL
        .into_iter()
        .map(|kind| {
            runtime
                .factory
                .deployment(kind)
                .map(|driver| (kind, driver))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Arc::new(crate::app::DevelopmentServicesCheck::new(
        project_documents(),
        Arc::new(crate::adapters::state::SqliteStateInspector {
            path: home.state_database.clone(),
        }),
        Arc::new(FilesystemTargetStore::new(home.targets_file.clone())),
        runtimes,
    )))
}

/// The readiness checks plus the ones that contact remote hosts: `mm doctor`
/// asks every node of a cluster target whether it is ready.
pub(crate) fn doctor_service(
    home: Option<&Path>,
    runtime: &RuntimeComposition,
) -> Result<crate::app::ReadinessService, MmError> {
    Ok(readiness_service(home, runtime)?
        .with_probe(Arc::new(ClusterNodesCheck {
            home: home.map(Path::to_path_buf),
        }))
        .with_probe(Arc::new(DeviceKeysCheck {
            home: home.map(Path::to_path_buf),
        })))
}

/// How this device's SSH keys are protected: a plain key, a key whose
/// passphrase the vault no longer has, a passphrase kept in a file. It
/// reads the key files and asks the vault whether it has an entry; no
/// passphrase is read.
struct DeviceKeysCheck {
    home: Option<PathBuf>,
}

#[async_trait::async_trait]
impl crate::app::ports::ReadinessProbe for DeviceKeysCheck {
    fn name(&self) -> &'static str {
        "device keys"
    }

    async fn inspect(
        &self,
        _selection: &WorkspaceSelection,
    ) -> Result<Vec<crate::domain::diagnostic::Diagnostic>, MmError> {
        let home = MmHome::inspect(self.home.as_deref())?;
        // Asking a keychain starts a process and waits for it.
        tokio::task::spawn_blocking(move || super::keys::key_service_at(&home)?.diagnostics())
            .await
            .map_err(|error| MmError::State {
                message: format!("checking this device's keys did not finish: {error}"),
            })?
    }
}

/// Per-node problems of a cluster target: a node that does not answer, is
/// not set up, answers as another node, or disagrees with the others on the
/// `mmd` version or the deployed commit. On every remote target: a host or
/// this `mm` outside the environment's pinned release, and hosts that run
/// another release than this `mm`.
struct ClusterNodesCheck {
    home: Option<PathBuf>,
}

#[async_trait::async_trait]
impl crate::app::ports::ReadinessProbe for ClusterNodesCheck {
    fn name(&self) -> &'static str {
        "cluster nodes"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["selection"]
    }

    async fn inspect(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<Vec<crate::domain::diagnostic::Diagnostic>, MmError> {
        let home = self.home.as_deref();
        let runtime = resolve_runtime(
            &selection.start,
            home,
            selection.runtime,
            selection.target.as_deref(),
            selection.environment.as_deref(),
            false,
            true,
        )?;
        match remote_environment(selection, home, runtime)? {
            Some(environment) if environment.is_cluster() => {
                let (nodes, mesh, networks, storage, runtimes, status) = tokio::join!(
                    environment.node_reports(),
                    environment.mesh_report(),
                    environment.network_report(),
                    super::target_storage(selection, home, RemoteIdentity::User, false),
                    environment.runtime_reports(),
                    environment.status()
                );
                let mut diagnostics =
                    crate::domain::remote::node_diagnostics(environment.target(), &nodes);
                diagnostics.extend(daemon_and_service_findings(&nodes, status.as_ref().ok()));
                diagnostics.extend(mesh.problems);
                diagnostics.extend(networks.problems);
                // A node that does not answer is reported once, above; a
                // target on an interface of its own has no shared storage.
                if let Ok(storage) = storage {
                    diagnostics.extend(
                        storage
                            .problems
                            .into_iter()
                            .filter(|problem| problem.code != "MM_STORAGE_NODE_SILENT"),
                    );
                }
                diagnostics.extend(crate::domain::hardening::runtime_diagnostics(&runtimes));
                diagnostics.extend(environment.version_diagnostics(&nodes));
                // A service answered while none of its containers is healthy.
                if let Some(names) = status
                    .as_ref()
                    .ok()
                    .and_then(|status| status.names.as_ref())
                {
                    diagnostics.extend(crate::domain::discovery::fail_open_diagnostics(names));
                }
                if let Ok(status) = &status {
                    diagnostics.extend(crate::domain::cluster_ingress::tunnel_diagnostics(
                        &status.nodes,
                    ));
                    let restarted = status
                        .stacks
                        .iter()
                        .flat_map(|stack| &stack.services)
                        .filter_map(|service| {
                            service
                                .restarts
                                .as_ref()
                                .map(|restarts| (service.name.as_str(), restarts))
                        })
                        .collect::<Vec<_>>();
                    diagnostics.extend(crate::domain::healing::restart_limit_diagnostics(
                        &restarted,
                    ));
                }
                Ok(diagnostics)
            }
            // A target that names one host: whether its engine can be used,
            // whether it has the runtimes its policy offers, which release
            // it runs, how its daemon fares and what has not converged. A
            // host that does not answer says nothing more here; `mm target
            // nodes` and every command name it.
            Some(environment) => {
                let (nodes, runtimes, status) = tokio::join!(
                    environment.node_reports(),
                    environment.runtime_reports(),
                    environment.status()
                );
                let mut diagnostics = crate::domain::remote::engine_diagnostics(&nodes);
                diagnostics.extend(crate::domain::hardening::runtime_diagnostics(&runtimes));
                diagnostics.extend(environment.version_diagnostics(&nodes));
                diagnostics.extend(daemon_and_service_findings(&nodes, status.ok().as_ref()));
                Ok(diagnostics)
            }
            None => Ok(Vec::new()),
        }
    }
}

/// Per node: how its daemon fares (`daemon_diagnostics`), and the services
/// whose share there has not converged, from the environment's status.
fn daemon_and_service_findings(
    nodes: &[crate::domain::remote::NodeReport],
    status: Option<&crate::domain::workspace::WorkspaceSnapshot>,
) -> Vec<crate::domain::diagnostic::Diagnostic> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut found = nodes
        .iter()
        .filter_map(|node| node.daemon.as_ref().map(|daemon| (node, daemon)))
        .flat_map(|(node, daemon)| {
            crate::domain::rollout::daemon_diagnostics(&node.name, daemon, now)
        })
        .collect::<Vec<_>>();
    let Some(status) = status else {
        return found;
    };
    for service in status.stacks.iter().flat_map(|stack| &stack.services) {
        let shares = if service.nodes.is_empty() {
            vec![(
                String::new(),
                service.state,
                service.healthy_count,
                service.desired_count,
                service.starting,
            )]
        } else {
            service
                .nodes
                .iter()
                .map(|share| {
                    (
                        format!(" on node '{}'", share.node),
                        share.state,
                        share.healthy_count,
                        share.desired_count,
                        share.starting,
                    )
                })
                .collect()
        };
        for (node, state, healthy, desired, starting) in shares {
            // A node that did not answer is reported once, as a node.
            if state == crate::domain::runtime::ServiceState::Unknown
                || state == crate::domain::runtime::ServiceState::JobOnly
                || (healthy >= desired && state == crate::domain::runtime::ServiceState::Running)
            {
                continue;
            }
            found.push(
                crate::domain::diagnostic::Diagnostic::warning(
                    "MM_SERVICE_NOT_CONVERGED",
                    format!(
                        "service '{}' of stack '{}'{node} is {}: {healthy} of {desired} container(s) ready{}",
                        service.name,
                        service.stack,
                        format!("{state:?}").to_ascii_lowercase(),
                        if starting { ", still starting" } else { "" }
                    ),
                )
                .with_help(format!(
                    "`mm service status` shows its containers and `mm service logs {}` what they logged",
                    service.name
                )),
            );
        }
    }
    found
}

/// The selected environment's remote target, whose hosts `mm target nodes`
/// lists.
fn nodes_environment(
    selection: &WorkspaceSelection,
    home: Option<&Path>,
    identity: RemoteIdentity,
) -> Result<crate::app::RemoteEnvironment, MmError> {
    let runtime = resolve_runtime(
        &selection.start,
        home,
        selection.runtime,
        selection.target.as_deref(),
        selection.environment.as_deref(),
        false,
        true,
    )?;
    remote_environment_as(selection, home, runtime, identity)?.ok_or_else(|| {
        MmError::Environment {
            message: "the selected environment runs on this machine; `mm target nodes` lists the hosts of a remote target, so pass --environment with a remote environment"
                .to_owned(),
        }
    })
}

/// Every node of the selected environment's remote target, for the
/// terminal UI's node list.
pub(crate) async fn target_nodes(
    selection: &WorkspaceSelection,
    home: Option<&Path>,
    identity: RemoteIdentity,
) -> Result<(String, Vec<crate::domain::remote::NodeReport>), MmError> {
    let environment = nodes_environment(selection, home, identity)?;
    Ok((
        environment.target().to_owned(),
        environment.node_reports().await,
    ))
}

/// What `mm target nodes` prints: every node of the selected environment's
/// remote target and the addresses of the networks that span them. The two
/// are read at the same time.
pub(crate) async fn target_nodes_and_addresses(
    selection: &WorkspaceSelection,
    home: Option<&Path>,
    identity: RemoteIdentity,
) -> Result<
    (
        String,
        Vec<crate::domain::remote::NodeReport>,
        crate::domain::cluster_network::AddressTable,
    ),
    MmError,
> {
    let environment = nodes_environment(selection, home, identity)?;
    let (nodes, addresses) = tokio::join!(environment.node_reports(), environment.address_table());
    Ok((environment.target().to_owned(), nodes, addresses))
}

/// The mesh between the nodes of the selected environment's cluster target,
/// for `mm target mesh status`, the terminal UI, and the MCP server alike.
pub(crate) async fn target_mesh(
    selection: &WorkspaceSelection,
    home: Option<&Path>,
    identity: RemoteIdentity,
) -> Result<crate::domain::mesh::MeshReport, MmError> {
    let runtime = resolve_runtime(
        &selection.start,
        home,
        selection.runtime,
        selection.target.as_deref(),
        selection.environment.as_deref(),
        false,
        true,
    )?;
    match remote_environment_as(selection, home, runtime, identity)? {
        Some(environment) if environment.is_cluster() => Ok(environment.mesh_report().await),
        Some(environment) => Err(MmError::Environment {
            message: format!(
                "target '{}' names one host, so it has no mesh; a mesh connects the hosts a target lists under nodes:",
                environment.target()
            ),
        }),
        None => Err(MmError::Environment {
            message: "the selected environment runs on this machine; a mesh connects the nodes of a remote target, so pass --environment with an environment whose target lists nodes:"
                .to_owned(),
        }),
    }
}
