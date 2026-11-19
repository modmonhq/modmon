//! What a setup plan reads before it is made.

use std::path::Path;

use futures_util::future::join_all;

use super::{ClusterSetupRequest, ClusterSetupService, same_key, subset};
use crate::app::HostCpu;
use crate::app::ports::ProgressReporter;
use crate::domain::cluster_setup::{
    ClusterDeployKeyPlan, ClusterNodePlan, ClusterSecretPlan, ClusterSetupStep, ClusterSkippedNode,
    HostKeyOrigin,
};
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::mesh::MeshPlan;
use crate::domain::progress::{NodeState, ProgressEvent, StepOutcome};
use crate::domain::repo_target::{RepoDeployKey, RepoTargetConfig, ssh_fingerprint};
use crate::domain::secret::{SecretScope, SecretStatus};
use crate::domain::target::{BootstrapInputFile, HostBinaries};

/// The step that reads the nodes; the others name their nodes under it.
pub(super) const READ_NODES: &str = "read-nodes";
const READ_SECRETS: &str = "read-secrets";
const READ_BINARIES: &str = "read-binaries";

/// What the nodes said.
pub(super) struct NodeReading {
    pub nodes: Vec<ClusterNodePlan>,
    pub skipped: Vec<ClusterSkippedNode>,
    pub mesh: Option<MeshPlan>,
    pub steps: Vec<ClusterSetupStep>,
    /// Notices of the steps a left-out node limits.
    pub step_notices: Vec<String>,
    /// Notices of a node that stops serving shared storage.
    pub handover: Vec<String>,
}

/// The environment's secrets as the plan binds them.
pub(super) struct SecretReading {
    pub secrets: Vec<ClusterSecretPlan>,
    pub unset: Vec<String>,
    pub deploy_key: ClusterDeployKeyPlan,
}

/// The target as a plan reads it.
pub(super) struct PlannedTarget<'a> {
    pub name: &'a str,
    pub config: &'a RepoTargetConfig,
    pub user: &'a str,
    pub storage: &'a str,
}

impl ClusterSetupService {
    /// Host keys, then the mesh, then shared storage, from every node at once.
    pub(super) async fn read_nodes(
        &self,
        target: &PlannedTarget<'_>,
        request: &ClusterSetupRequest,
        progress: &dyn ProgressReporter,
    ) -> Result<NodeReading, MmError> {
        let selected = if request.nodes.is_empty() {
            target.config.nodes.len()
        } else {
            request.nodes.len()
        };
        progress.report(ProgressEvent::started(
            READ_NODES,
            format!("Reading {}", nodes_text(selected)),
        ));
        let read = self.read_nodes_unreported(target, request, progress).await;
        progress.report(match &read {
            Ok(reading) => ProgressEvent::finished(
                READ_NODES,
                StepOutcome::Done,
                Some(format!("Read {}", nodes_text(reading.nodes.len()))),
            ),
            Err(_) => ProgressEvent::finished(READ_NODES, StepOutcome::Failed, None),
        });
        read
    }

    async fn read_nodes_unreported(
        &self,
        target: &PlannedTarget<'_>,
        request: &ClusterSetupRequest,
        progress: &dyn ProgressReporter,
    ) -> Result<NodeReading, MmError> {
        let (mut nodes, skipped) = self
            .scan_nodes_reporting(
                target.config,
                (request.trust_new_host_keys, &request.nodes),
                progress,
            )
            .await?;
        let (mesh, cpus) = self
            .plan_mesh_and_cpus(
                target.name,
                target.config,
                target.user,
                (&nodes, &skipped),
                progress,
            )
            .await?;
        for node in &mut nodes {
            node.cpu = cpus.get(&node.name).cloned();
        }
        let mut step_notices = Vec::new();
        let steps = subset::steps(target.config, target.storage, &skipped, &mut step_notices);
        for node in nodes.iter().filter(|node| {
            node.name != target.storage && node.host_key_origin == HostKeyOrigin::Recorded
        }) {
            if steps.contains(&ClusterSetupStep::ServeStorage) {
                progress.report(ProgressEvent::node(
                    READ_NODES,
                    &node.name,
                    NodeState::Working,
                    Some("asking whether it serves shared volumes"),
                ));
            }
        }
        let handover = self
            .planned_handover(&steps, (target.name, target.user, target.storage), &nodes)
            .await?;
        for node in &nodes {
            progress.report(ProgressEvent::node(
                READ_NODES,
                &node.name,
                NodeState::Done,
                Some(match node.host_key_origin {
                    HostKeyOrigin::Recorded => "host key as recorded",
                    HostKeyOrigin::FirstUse => "host key seen for the first time",
                    HostKeyOrigin::Replaced => "host key replaced",
                }),
            ));
        }
        Ok(NodeReading {
            nodes,
            skipped,
            mesh,
            steps,
            step_notices,
            handover,
        })
    }

    /// Scans the host key of every selected node; `only` empty selects all.
    #[cfg(test)]
    pub(super) async fn scan_nodes(
        &self,
        target: &RepoTargetConfig,
        trust_new_host_keys: bool,
        only: &[String],
    ) -> Result<(Vec<ClusterNodePlan>, Vec<ClusterSkippedNode>), MmError> {
        self.scan_nodes_reporting(
            target,
            (trust_new_host_keys, only),
            &crate::app::ports::NoProgress,
        )
        .await
    }

    async fn scan_nodes_reporting(
        &self,
        target: &RepoTargetConfig,
        (trust_new_host_keys, only): (bool, &[String]),
        progress: &dyn ProgressReporter,
    ) -> Result<(Vec<ClusterNodePlan>, Vec<ClusterSkippedNode>), MmError> {
        let addresses = target
            .mesh_addresses()
            .map_err(|message| MmError::Target { message })?;
        subset::check_names(target, only)?;
        let mut selected = Vec::with_capacity(target.nodes.len());
        let mut skipped = Vec::new();
        for (position, (node, private_address)) in target.nodes.iter().zip(addresses).enumerate() {
            if !only.is_empty() && !only.contains(&node.name) {
                progress.report(ProgressEvent::node(
                    READ_NODES,
                    &node.name,
                    NodeState::Skipped,
                    Some("left out"),
                ));
                skipped.push(ClusterSkippedNode {
                    name: node.name.clone(),
                    position,
                    private_address,
                });
                continue;
            }
            selected.push((node, private_address));
        }
        let scans = join_all(selected.iter().map(|(node, _)| async move {
            progress.report(ProgressEvent::node(
                READ_NODES,
                &node.name,
                NodeState::Working,
                Some("scanning its host key"),
            ));
            let scanned = self
                .dependencies
                .shell
                .scan_host_key(&node.host, node.port)
                .await;
            if scanned.is_err() {
                progress.report(ProgressEvent::node(
                    READ_NODES,
                    &node.name,
                    NodeState::Failed,
                    Some("unreachable"),
                ));
            }
            scanned
        }))
        .await;
        let mut nodes = Vec::with_capacity(selected.len());
        for ((node, private_address), scanned) in selected.into_iter().zip(scans) {
            let scanned = scanned.map_err(|error| subset::unreachable(target, node, &error))?;
            let host_key_origin = match node.host_key.as_deref().map(str::trim) {
                None => HostKeyOrigin::FirstUse,
                Some(recorded) if same_key(recorded, &scanned) => HostKeyOrigin::Recorded,
                Some(_) if trust_new_host_keys => HostKeyOrigin::Replaced,
                Some(_) => {
                    progress.report(ProgressEvent::node(
                        READ_NODES,
                        &node.name,
                        NodeState::Failed,
                        Some("presents another host key"),
                    ));
                    return Err(subset::foreign_host_key(target, node));
                }
            };
            nodes.push(ClusterNodePlan {
                name: node.name.clone(),
                host: node.host.clone(),
                port: node.port,
                private_address,
                host_key_fingerprint: ssh_fingerprint(&scanned)
                    .map_err(|message| MmError::Security { message })?,
                host_key: scanned,
                host_key_origin,
                cpu: None,
            });
        }
        Ok((nodes, skipped))
    }

    /// The environment's age secrets and the deploy key the nodes get.
    pub(super) async fn read_secrets(
        &self,
        document: &ConfigDocument,
        environment: &str,
        deploy_key: Option<RepoDeployKey>,
        progress: &dyn ProgressReporter,
    ) -> Result<SecretReading, MmError> {
        let declared = document
            .config
            .secrets
            .values()
            .filter(|declaration| declaration.storage.provider.as_str() == "age")
            .count();
        progress.report(ProgressEvent::started(
            READ_SECRETS,
            format!("Reading {}", secrets_text(declared)),
        ));
        let read = async {
            let key_secret = deploy_key.as_ref().map(|key| key.secret.as_str());
            let (secrets, unset, revision) = self
                .secret_revisions(document, environment, key_secret)
                .await?;
            let deploy_key = match deploy_key {
                Some(key) => {
                    self.planned_deploy_key(document, environment, key, revision)
                        .await?
                }
                None => ClusterDeployKeyPlan::default(),
            };
            Ok(SecretReading {
                secrets,
                unset,
                deploy_key,
            })
        }
        .await;
        progress.report(match &read {
            Ok(reading) => ProgressEvent::finished(
                READ_SECRETS,
                StepOutcome::Done,
                Some(if reading.unset.is_empty() {
                    format!("Read {}", secrets_text(reading.secrets.len()))
                } else {
                    format!(
                        "Read {}; {} unset here",
                        secrets_text(reading.secrets.len()),
                        reading.unset.len()
                    )
                }),
            ),
            Err(_) => ProgressEvent::finished(READ_SECRETS, StepOutcome::Failed, None),
        });
        read
    }

    /// Revisions of set secrets, names of unset ones, and the deploy key's revision.
    async fn secret_revisions(
        &self,
        document: &ConfigDocument,
        environment: &str,
        deploy_key_secret: Option<&str>,
    ) -> Result<(Vec<ClusterSecretPlan>, Vec<String>, Option<String>), MmError> {
        let scope = SecretScope::new(environment.to_owned());
        let mut declared = Vec::new();
        for (name, declaration) in &document.config.secrets {
            if declaration.storage.provider.as_str() != "age" {
                continue;
            }
            let reference = declaration
                .storage
                .reference()
                .map_err(|message| MmError::Secret { message })?;
            declared.push((name, reference));
        }
        let references = declared
            .iter()
            .map(|(_, reference)| reference.clone())
            .collect::<Vec<_>>();
        let metadata = self
            .dependencies
            .inspector
            .inspect_many(&scope, &references)
            .await?;
        let mut secrets = Vec::new();
        let mut unset_secrets = Vec::new();
        let mut deploy_key_revision = None;
        for ((name, reference), metadata) in declared.into_iter().zip(metadata) {
            match (metadata.status, metadata.revision) {
                (SecretStatus::Set, Some(revision)) if Some(name.as_str()) == deploy_key_secret => {
                    deploy_key_revision = Some(revision);
                }
                (SecretStatus::Set, Some(revision)) => secrets.push(ClusterSecretPlan {
                    name: name.clone(),
                    reference: super::reference_text(&reference),
                    revision,
                }),
                _ => unset_secrets.push(name.clone()),
            }
        }
        Ok((secrets, unset_secrets, deploy_key_revision))
    }
}

/// The project's bootstrap inputs.
pub(super) async fn read_inputs(root: &Path) -> Result<Vec<BootstrapInputFile>, MmError> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || crate::app::targets::collect_bootstrap_inputs(&root))
        .await
        .unwrap_or_else(|error| {
            Err(MmError::State {
                message: format!("reading the bootstrap inputs stopped: {error}"),
            })
        })
}

impl ClusterSetupService {
    /// The `mm` and `mmd` each node gets: `--host-binaries`, or for each
    /// CPU the nodes have, this mm's directory or this mm's release.
    pub(super) async fn host_binaries(
        &self,
        request: &ClusterSetupRequest,
        nodes: &[ClusterNodePlan],
        progress: &dyn ProgressReporter,
    ) -> Result<Vec<HostBinaries>, MmError> {
        progress.report(ProgressEvent::started(
            READ_BINARIES,
            "Choosing the host binaries",
        ));
        let hosts = nodes
            .iter()
            .map(|node| HostCpu {
                host: node.name.clone(),
                cpu: node.cpu.clone(),
            })
            .collect::<Vec<_>>();
        let read = match (&self.binaries, request.binaries.as_deref()) {
            (Some(source), given) => source.resolve(given, &hosts, progress).await,
            (None, given) => {
                crate::app::host_binaries::given_host_binaries(given, &hosts).map(|set| vec![set])
            }
        };
        progress.report(match &read {
            Ok(sets) => ProgressEvent::finished(
                READ_BINARIES,
                StepOutcome::Done,
                Some(format!(
                    "mm and mmd for {}",
                    sets.iter()
                        .map(|set| set.architecture.as_str())
                        .collect::<Vec<_>>()
                        .join(" and ")
                )),
            ),
            Err(_) => ProgressEvent::finished(READ_BINARIES, StepOutcome::Failed, None),
        });
        read
    }
}

fn nodes_text(count: usize) -> String {
    format!("{count} node{}", if count == 1 { "" } else { "s" })
}

fn secrets_text(count: usize) -> String {
    format!("{count} secret{}", if count == 1 { "" } else { "s" })
}
