//! The steps of a cluster setup apply. Each one is idempotent.

use std::path::Path;

use super::mesh::MMD;
use super::{ClusterSetupService, same_key};
use crate::app::ports::{ClusterNodeCommand, ClusterScript, ProgressReporter, progress_on_node};
use crate::domain::cluster_setup::{
    ClusterNodePlan, ClusterNodeRecord, ClusterSetupPlan, ClusterSetupStep,
};
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::repo_target::MeshConfig;
use crate::domain::secret::{BundledDeployKey, BundledSecret, SecretBundle, SecretScope};
use crate::domain::target::{
    BootstrapAccess, NamedTarget, TargetBootstrap, TargetBootstrapPlan, TargetDefinition, TargetId,
    TargetTransport, bootstrap_requirements,
};

fn program(words: &[&str]) -> ClusterNodeCommand {
    ClusterNodeCommand::Program(words.iter().map(|word| (*word).to_owned()).collect())
}

pub(super) fn mmd(plan: &ClusterSetupPlan, words: &[&str]) -> ClusterNodeCommand {
    mmd_of((plan.project_id, &plan.environment), words)
}

pub(super) fn mmd_of(
    (project, environment): (crate::domain::workspace::ProjectId, &str),
    words: &[&str],
) -> ClusterNodeCommand {
    let mut argv = vec![MMD.to_owned()];
    argv.extend(words.iter().map(|word| (*word).to_owned()));
    argv.extend([
        "--project".to_owned(),
        project.0.to_string(),
        "--environment".to_owned(),
        environment.to_owned(),
    ]);
    ClusterNodeCommand::Program(argv)
}

fn json(output: &str, context: &str) -> Result<serde_json::Value, MmError> {
    serde_json::from_str(output).map_err(|error| MmError::Target {
        message: format!("{context} printed unexpected output: {error}"),
    })
}

impl ClusterSetupService {
    /// Hostnames, node facts, and the tools the mesh needs. The mesh itself
    /// comes after provisioning, which installs the `mmd` that owns it.
    pub(super) async fn prepare_nodes(
        &self,
        plan: &ClusterSetupPlan,
        progress: &dyn ProgressReporter,
    ) -> Result<(), MmError> {
        let wireguard = matches!(plan.cluster.mesh, MeshConfig::Wireguard(_));
        let interface = plan.cluster.mesh.interface().to_owned();
        let step = ClusterSetupStep::PrepareNodes;
        super::apply::nodes_wait(progress, step, plan);
        for node in &plan.nodes {
            let connection = self.connection(plan, node);
            let command = ClusterNodeCommand::Script {
                script: ClusterScript::Prepare,
                arguments: vec![
                    node.name.clone(),
                    interface.clone(),
                    node.private_address.to_string(),
                    if wireguard { "yes" } else { "no" }.to_owned(),
                    plan.runtime.name().to_owned(),
                ],
            };
            let preparing = self.dependencies.shell.run(&connection, &command, None);
            progress_on_node(
                progress,
                (step.label(), &node.name, "name, node facts, mesh tools"),
                preparing,
            )
            .await?;
        }
        Ok(())
    }

    /// The node's own bootstrap plan: the project's playbook and the remote
    /// access layer, exactly as single-host setup runs them.
    fn node_bootstrap_plan(
        &self,
        plan: &ClusterSetupPlan,
        node: &ClusterNodePlan,
    ) -> Result<TargetBootstrapPlan, MmError> {
        let transport = TargetTransport::Ssh {
            host: node.host.clone(),
            port: node.port,
            user: plan.setup.user.clone(),
            host_key_fingerprint: node.host_key_fingerprint.clone(),
            host_key: Some(node.host_key.clone()),
            identity_file: Some(self.dependencies.identity_file.clone()),
            remote_root: None,
            daemon_fingerprint: None,
        };
        let requirements = bootstrap_requirements(&transport, plan.runtime, false, true)
            .map_err(|message| MmError::Target { message })?;
        let id = crate::domain::host_policy::derived_id(&[
            "cluster-node",
            &plan.project_id.0.to_string(),
            &plan.environment,
            &node.name,
        ]);
        let target = NamedTarget::new(
            node.name.clone(),
            TargetDefinition {
                id: TargetId(id),
                transport,
                bootstrap: TargetBootstrap::Ansible,
                ingress: None,
                default_runtime: Some(plan.runtime),
                available_runtimes: vec![plan.runtime],
                runtime: None,
                nodes: Vec::new(),
            },
        );
        let access = BootstrapAccess {
            origin: plan.setup.origin.clone(),
            origin_host_key: plan.setup.origin_host_key.clone(),
            break_glass: plan
                .setup
                .break_glass
                .iter()
                .map(|entry| crate::domain::host_policy::BreakGlassKey {
                    name: entry.name.clone(),
                    key: entry.key.clone(),
                })
                .collect(),
            binaries: plan.binaries.clone(),
            mm_digest: plan.mm_digest.clone(),
            mmd_digest: plan.mmd_digest.clone(),
            upgrade: false,
            downgrade: false,
            notices: Vec::new(),
        };
        let mut bootstrap = TargetBootstrapPlan {
            project_id: plan.project_id,
            project_name: plan.project_name.clone(),
            config_hash: plan.config_hash.clone(),
            environment: plan.environment.clone(),
            target,
            selected_runtime: plan.runtime,
            rootless: requirements.rootless,
            input_files: plan.input_files.clone(),
            required_capabilities: requirements.capabilities,
            steps: requirements.steps,
            access: Some(access),
            plan_hash: String::new(),
        };
        bootstrap.plan_hash = bootstrap
            .canonical_hash()
            .map_err(|message| MmError::Target { message })?;
        Ok(bootstrap)
    }

    pub(super) async fn provision_nodes(
        &self,
        plan: &ClusterSetupPlan,
        root: &Path,
        progress: &dyn ProgressReporter,
    ) -> Result<Vec<ClusterNodeRecord>, MmError> {
        let mut records = Vec::with_capacity(plan.nodes.len());
        let step = ClusterSetupStep::ProvisionNodes;
        super::apply::nodes_wait(progress, step, plan);
        for node in &plan.nodes {
            let bootstrap = self.node_bootstrap_plan(plan, node)?;
            let execution = progress_on_node(
                progress,
                (
                    step.label(),
                    &node.name,
                    &format!("installing {} and mmd", plan.runtime.name()),
                ),
                self.dependencies.bootstrap.execute(root, &bootstrap),
            )
            .await?;
            let report = execution.access.ok_or_else(|| MmError::Target {
                message: format!("node {} returned no access report", node.name),
            })?;
            if !same_key(&report.host_key, &node.host_key) {
                return Err(MmError::Security {
                    message: format!(
                        "node {} reported a host key that differs from the reviewed plan",
                        node.name
                    ),
                });
            }
            records.push(ClusterNodeRecord {
                name: node.name.clone(),
                private_address: node.private_address,
                host_key: node.host_key.clone(),
                daemon_fingerprint: report.daemon_fingerprint,
                version: report.version,
            });
        }
        Ok(records)
    }

    /// Seals the environment's secrets and deploy key to each node's own
    /// store and imports them there. Returns how many entries changed.
    pub(super) async fn push_secrets(
        &self,
        plan: &ClusterSetupPlan,
        document: &ConfigDocument,
        progress: &dyn ProgressReporter,
    ) -> Result<usize, MmError> {
        let step = ClusterSetupStep::PushSecrets;
        let scope = SecretScope::new(plan.environment.clone());
        let mut secrets = Vec::with_capacity(plan.secrets.len());
        for planned in &plan.secrets {
            let declaration =
                document
                    .config
                    .secrets
                    .get(&planned.name)
                    .ok_or_else(|| MmError::State {
                        message: format!("secret '{}' left the configuration", planned.name),
                    })?;
            let reference = declaration
                .storage
                .reference()
                .map_err(|message| MmError::Secret { message })?;
            let resolution = self
                .dependencies
                .resolver
                .resolve(&scope, &reference)
                .await?;
            if resolution.metadata.revision.as_deref() != Some(planned.revision.as_str()) {
                return Err(MmError::State {
                    message: format!(
                        "secret '{}' changed after the plan was reviewed",
                        planned.name
                    ),
                });
            }
            let material = resolution.material.ok_or_else(|| MmError::Secret {
                message: format!("secret '{}' has no value", planned.name),
            })?;
            secrets.push(BundledSecret {
                reference,
                revision: planned.revision.clone(),
                material,
            });
        }
        let deploy_key = if plan.deploy_key.is_none() {
            None
        } else {
            Some(BundledDeployKey {
                private_key: self
                    .deploy_key(document, &plan.environment, &plan.deploy_key.secret)
                    .await?,
                public_key: plan.deploy_key.public_key.clone(),
            })
        };
        let bundle = SecretBundle {
            project_id: plan.project_id,
            environment: plan.environment.clone(),
            secrets,
            deploy_key,
        };
        let mut changed = 0;
        super::apply::nodes_wait(progress, step, plan);
        for node in &plan.nodes {
            changed += progress_on_node(
                progress,
                (step.label(), &node.name, "sealing and importing"),
                self.push_secrets_to(plan, node, &bundle),
            )
            .await?;
        }
        Ok(changed)
    }

    /// Seals `bundle` to one node and imports it; returns how many entries changed.
    async fn push_secrets_to(
        &self,
        plan: &ClusterSetupPlan,
        node: &ClusterNodePlan,
        bundle: &SecretBundle,
    ) -> Result<usize, MmError> {
        {
            let connection = self.connection(plan, node);
            let shell = &self.dependencies.shell;
            let output = shell
                .run(&connection, &mmd(plan, &["secrets", "recipient"]), None)
                .await?;
            let recipient = json(&output, "mmd secrets recipient")?["recipient"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| MmError::Target {
                    message: format!("node {} reported no age recipient", node.name),
                })?;
            let sealed = shell.seal_secret_bundle(&recipient, bundle)?;
            let output = shell
                .run(
                    &connection,
                    &mmd(plan, &["secrets", "import"]),
                    Some(sealed),
                )
                .await?;
            let summary = json(&output, "mmd secrets import")?;
            Ok(summary["secrets"].as_object().map_or(0, |entries| {
                entries
                    .values()
                    .filter(|entry| entry["changed"].as_bool() == Some(true))
                    .count()
            }))
        }
    }

    /// Every node runs the reviewed `mm` and `mmd` and keeps the identity
    /// setup recorded for it.
    pub(super) async fn verify(
        &self,
        plan: &ClusterSetupPlan,
        records: &[ClusterNodeRecord],
    ) -> Result<(), MmError> {
        let shell = &self.dependencies.shell;
        let mut versions = std::collections::BTreeSet::new();
        for (node, record) in plan.nodes.iter().zip(records) {
            let connection = self.connection(plan, node);
            let identity = shell
                .run(&connection, &program(&[MMD, "--print-identity"]), None)
                .await?;
            if identity.trim() != record.daemon_fingerprint {
                return Err(MmError::Security {
                    message: format!(
                        "node {} changed its daemon identity during setup",
                        node.name
                    ),
                });
            }
            let scanned = shell.scan_host_key(&node.host, node.port).await?;
            if !same_key(&scanned, &node.host_key) {
                return Err(MmError::Security {
                    message: format!("node {} changed its SSH host key during setup", node.name),
                });
            }
            versions.insert(record.version.clone());
        }
        if versions.len() > 1 {
            return Err(MmError::Target {
                message: format!(
                    "nodes run different mm versions: {}",
                    versions.into_iter().collect::<Vec<_>>().join(", ")
                ),
            });
        }
        Ok(())
    }
}
