//! `mm target setup` for a repo target declared by `nodes:`.
//!
//! Planning reads the repo, scans each node's host key, and binds the
//! environment's secret revisions and deploy key into one hash. Applying
//! re-plans, refuses any difference, and runs the steps in order. Every step
//! is idempotent, so a failed apply resumes by applying a fresh plan.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;

use crate::app::ports::{
    ClusterNodeConnection, ClusterNodeShell, ProgressReporter, ProjectDocumentStore,
    SecretMaterialResolver, SecretMetadataInspector, TargetBootstrapExecutor,
};
use crate::domain::cluster_setup::{
    ClusterDeployKeyPlan, ClusterNodePlan, ClusterSetupPlan, HostKeyOrigin,
};
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::repo_target::RepoTargetConfig;
use crate::domain::runtime::RuntimeKind;
use crate::domain::secret::{SecretReference, SecretScope, SecretStatus};

#[path = "cluster_setup/apply.rs"]
mod apply;
pub use apply::step_text;
#[path = "cluster_setup/mesh.rs"]
mod mesh;
#[path = "cluster_setup/reading.rs"]
mod reading;
#[path = "cluster_setup/rotation.rs"]
mod rotation;
#[path = "cluster_setup/secret_store.rs"]
mod secret_store;
pub use rotation::MeshRotationRequest;
#[path = "cluster_setup/steps.rs"]
mod steps;
#[path = "cluster_setup/storage.rs"]
mod storage;
#[path = "cluster_setup/subset.rs"]
mod subset;

/// Where the deploy key `mm target setup deploy-key` prints comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeployKeyOrigin {
    /// The environment's own store held it already.
    Existing,
    /// Copied into the environment's store from the store of this other
    /// environment of the same target.
    Copied(String),
    /// Made now: no environment of the target held one.
    Created,
}

/// What the operator passes to `plan` and `apply`.
#[derive(Debug, Clone)]
pub struct ClusterSetupRequest {
    pub start: PathBuf,
    pub environment: Option<String>,
    /// Accept scanned host keys that differ from the recorded ones.
    pub trust_new_host_keys: bool,
    /// `--host-binaries`: a directory with `mm` and `mmd` built for the
    /// hosts. Without it, this mm's directory or this mm's release, per CPU.
    pub binaries: Option<PathBuf>,
    /// The commit every node syncs the users file from.
    pub access_commit: String,
    /// `--commit` named `access_commit`, rather than `HEAD`.
    pub explicit_commit: bool,
    /// `--nodes`: set up only these nodes; empty means every node. The
    /// others are not contacted and keep the version they run.
    pub nodes: Vec<String>,
    /// `--downgrade`: install this release on nodes that run a newer one.
    pub downgrade: bool,
    /// `--upgrade`: install this mm and mmd on nodes that run another build
    /// or an older release.
    pub upgrade: bool,
}

impl ClusterSetupRequest {
    fn flags(&self) -> crate::domain::cluster_setup::ClusterSetupFlags {
        crate::domain::cluster_setup::ClusterSetupFlags {
            release: crate::domain::cluster_setup::ReleaseConsent {
                downgrade: self.downgrade,
                upgrade: self.upgrade,
            },
            commit: self.explicit_commit.then(|| self.access_commit.clone()),
        }
    }
}

pub struct ClusterSetupDependencies {
    pub documents: Arc<dyn ProjectDocumentStore>,
    pub inspector: Arc<dyn SecretMetadataInspector>,
    pub resolver: Arc<dyn SecretMaterialResolver>,
    pub shell: Arc<dyn ClusterNodeShell>,
    pub bootstrap: Arc<dyn TargetBootstrapExecutor>,
    /// This device's mm key, authorized for each node's setup account.
    pub identity_file: PathBuf,
}

pub struct ClusterSetupService {
    dependencies: ClusterSetupDependencies,
    /// Builds the bundle the first access sync sends to nodes that fetch
    /// nothing.
    bundles: Option<Arc<dyn crate::app::ports::CommitBundles>>,
    /// Keeps the managed secret store's authority and recovery copy.
    keeper: Option<Arc<dyn crate::app::ports::ClusterStoreKeeper>>,
    /// Where the host binaries come from without `--host-binaries`.
    binaries: Option<crate::app::HostBinarySource>,
}

impl std::fmt::Debug for ClusterSetupService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClusterSetupService")
            .field("identity_file", &self.dependencies.identity_file)
            .finish_non_exhaustive()
    }
}

/// The environment's repo target, when it lists `nodes:`.
pub fn cluster_target<'a>(
    document: &'a ConfigDocument,
    environment: &str,
) -> Option<(&'a str, &'a RepoTargetConfig)> {
    let name = document
        .config
        .environments
        .get(environment)
        .and_then(|overlay| overlay.target.as_deref())
        .unwrap_or(document.config.target.as_str());
    document
        .config
        .targets
        .get_key_value(name)
        .filter(|(_, target)| target.is_cluster())
        .map(|(name, target)| (name.as_str(), target))
}

/// What setup replaces on each node it read, refusing what `consent` does
/// not allow. A node's build is compared with the `mmd` for its CPU.
fn release_notices(
    mesh: Option<&crate::domain::mesh::MeshPlan>,
    nodes: &[ClusterNodePlan],
    (binaries, consent): (
        &[crate::domain::target::HostBinaries],
        crate::domain::cluster_setup::ReleaseConsent,
    ),
) -> Result<Vec<String>, MmError> {
    let read = mesh
        .iter()
        .flat_map(|mesh| &mesh.nodes)
        .filter_map(|node| {
            nodes
                .iter()
                .find(|planned| {
                    planned.name == node.node && planned.host_key_origin == HostKeyOrigin::Recorded
                })
                .map(|planned| {
                    (
                        planned.cpu.as_deref(),
                        node.node.as_str(),
                        node.release.as_ref(),
                    )
                })
        })
        .collect::<Vec<_>>();
    let mut notices = Vec::new();
    let mut builds = read
        .iter()
        .map(
            |(cpu, _, _)| match crate::domain::target::binaries_for(binaries, *cpu).as_slice() {
                [set] => Some(set.mmd_digest.clone()),
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    builds.sort();
    builds.dedup();
    for build in builds {
        let this = crate::domain::cluster_setup::HostRelease {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            protocol: crate::domain::remote::PROTOCOL_VERSION,
            build: build.clone(),
        };
        let group = read
            .iter()
            .filter(|(cpu, _, _)| {
                match crate::domain::target::binaries_for(binaries, *cpu).as_slice() {
                    [set] => Some(&set.mmd_digest) == build.as_ref(),
                    _ => build.is_none(),
                }
            })
            .map(|(_, node, release)| (*node, *release))
            .collect::<Vec<_>>();
        notices.extend(
            crate::domain::cluster_setup::release_notices(&group, &this, consent)
                .map_err(|message| MmError::Target { message })?,
        );
    }
    Ok(notices)
}

impl ClusterSetupService {
    #[must_use]
    pub fn new(dependencies: ClusterSetupDependencies) -> Self {
        Self {
            dependencies,
            bundles: None,
            keeper: None,
            binaries: None,
        }
    }

    /// Where the host binaries come from without `--host-binaries`.
    #[must_use]
    pub fn with_host_binaries(mut self, source: crate::app::HostBinarySource) -> Self {
        self.binaries = Some(source);
        self
    }

    /// For targets with `delivery: push`: what builds a commit's bundle
    /// from the checkout setup runs in.
    #[must_use]
    pub fn with_pushed_commits(
        mut self,
        bundles: Arc<dyn crate::app::ports::CommitBundles>,
    ) -> Self {
        self.bundles = Some(bundles);
        self
    }

    pub fn load(&self, start: &Path) -> Result<ConfigDocument, MmError> {
        self.dependencies.documents.load_from(start)
    }

    pub async fn plan(&self, request: &ClusterSetupRequest) -> Result<ClusterSetupPlan, MmError> {
        self.plan_reporting(request, &crate::app::ports::NoProgress)
            .await
    }

    /// The plan, reporting what it reads.
    pub async fn plan_reporting(
        &self,
        request: &ClusterSetupRequest,
        progress: &dyn ProgressReporter,
    ) -> Result<ClusterSetupPlan, MmError> {
        let document = self.load(&request.start)?;
        let environment =
            crate::domain::config::resolve_environment(&document, request.environment.as_deref())?;
        require_valid(&document, &environment)?;
        // Setup installs this mm's release on the nodes unless told
        // otherwise, so a pinned environment takes only that release.
        let pin = crate::domain::version_pin::VersionPin::of(&document.config, &environment);
        if let Some(pin) = &pin {
            pin.check_client(env!("CARGO_PKG_VERSION"))
                .map_err(|message| MmError::Environment { message })?;
        }
        let (target_name, target) =
            cluster_target(&document, &environment).ok_or_else(|| MmError::Target {
                message: format!(
                    "environment '{environment}' has no repo target with nodes:; cluster setup needs one"
                ),
            })?;
        let (setup, deploy_key) = declared_setup(target_name, target)?;
        let runtime = node_runtime(&document, &environment);
        let storage = target
            .storage_node()
            .map(str::to_owned)
            .ok_or_else(|| setup_error(target_name, "at least one node"))?;
        let planned = reading::PlannedTarget {
            name: target_name,
            config: target,
            user: &setup.user,
            storage: &storage,
        };
        let (mut reading, secrets, input_files) = tokio::try_join!(
            self.read_nodes(&planned, request, progress),
            self.read_secrets(&document, &environment, deploy_key, progress),
            reading::read_inputs(&document.root),
        )?;
        let binaries = self
            .host_binaries(request, &reading.nodes, progress)
            .await?;
        let secret_store = self
            .plan_store(&document, (&environment, target), &mut reading, &setup.user)
            .await?;
        let mut notices = registry_notices(&document, &environment, &secrets.unset);
        let consent = crate::domain::cluster_setup::ReleaseConsent {
            downgrade: request.downgrade,
            upgrade: request.upgrade || pin.is_some(),
        };
        notices.extend(release_notices(
            reading.mesh.as_ref(),
            &reading.nodes,
            (&binaries, consent),
        )?);
        notices.extend(reading.step_notices);
        notices.extend(reading.handover);
        notices.extend(
            reading
                .mesh
                .as_ref()
                .map(mesh::departed_notices)
                .unwrap_or_default(),
        );
        let mut plan = ClusterSetupPlan {
            project_id: document.config.project.id,
            project_name: document.config.project.name.clone(),
            config_hash: document.source_hash.clone(),
            environment,
            target: target_name.to_owned(),
            nodes: reading.nodes,
            skipped: reading.skipped,
            cluster: target.cluster.clone(),
            mesh: reading.mesh,
            storage,
            runtime,
            setup,
            input_files,
            host_binaries: match binaries.as_slice() {
                [set] if request.binaries.is_some() => Some(set.directory.clone()),
                _ => None,
            },
            binaries,
            secrets: secrets.secrets,
            secret_store,
            notices,
            unset_secrets: secrets.unset,
            deploy_key: secrets.deploy_key,
            access_commit: request.access_commit.clone(),
            steps: reading.steps,
            flags: request.flags(),
            plan_hash: String::new(),
        };
        plan.plan_hash = plan
            .canonical_hash()
            .map_err(|message| MmError::Target { message })?;
        Ok(plan)
    }

    /// The deploy key a fetch target's nodes get: its secret has a value
    /// and is the private half of the public key the target declares.
    async fn planned_deploy_key(
        &self,
        document: &ConfigDocument,
        environment: &str,
        deploy_key: crate::domain::repo_target::RepoDeployKey,
        revision: Option<String>,
    ) -> Result<ClusterDeployKeyPlan, MmError> {
        let revision = revision.ok_or_else(|| MmError::Secret {
            message: format!(
                "deploy key secret '{}' has no value in {environment}; run `mm -e {environment} target setup deploy-key`",
                deploy_key.secret
            ),
        })?;
        let private_key = self
            .deploy_key(document, environment, &deploy_key.secret)
            .await?;
        let derived = self.dependencies.shell.public_key_of(&private_key).await?;
        if !same_key(&derived, &deploy_key.public_key) {
            let secret = &deploy_key.secret;
            return Err(MmError::Security {
                message: format!(
                    "secret '{secret}' holds another deploy key in {environment} than setup.deploy-key.public-key; a target has one deploy key for all its environments. If {environment}'s is the wrong one, run `mm -e {environment} secret unset {secret}` and `mm -e {environment} target setup deploy-key`, which copies the target's key from an environment that holds it; if the target's line is wrong, set setup.deploy-key.public-key to the key `mm -e {environment} target setup deploy-key` prints"
                ),
            });
        }
        Ok(ClusterDeployKeyPlan {
            secret: deploy_key.secret,
            revision,
            public_key: deploy_key.public_key,
        })
    }

    /// The deploy key's OpenSSH private bytes, decoded from the one-line
    /// base64 the secret store holds.
    async fn deploy_key(
        &self,
        document: &ConfigDocument,
        environment: &str,
        secret: &str,
    ) -> Result<zeroize::Zeroizing<Vec<u8>>, MmError> {
        let declaration = document
            .config
            .secrets
            .get(secret)
            .ok_or_else(|| undeclared_deploy_key(secret))?;
        let reference = declaration
            .storage
            .reference()
            .map_err(|message| MmError::Secret { message })?;
        let material = self
            .dependencies
            .resolver
            .resolve(&SecretScope::new(environment.to_owned()), &reference)
            .await?
            .material
            .ok_or_else(|| MmError::Secret {
                message: format!("deploy key secret '{secret}' has no value"),
            })?;
        material.expose(|value| {
            base64::engine::general_purpose::STANDARD
                .decode(value.trim())
                .map(zeroize::Zeroizing::new)
                .map_err(|_| MmError::Secret {
                    message: format!(
                        "deploy key secret '{secret}' is not base64; recreate it with `target setup deploy-key`"
                    ),
                })
        })
    }

    /// Gives the environment the deploy key of its target when its secret
    /// is unset, and returns the public line either way. A target has one
    /// deploy key, as each node keeps one per project: the key comes from
    /// another environment of the target whose store holds it, and is made
    /// only when none does. Never replaces an existing key.
    pub async fn ensure_deploy_key(
        &self,
        start: &Path,
        environment: Option<&str>,
        secrets: &crate::app::secrets::SecretManager,
    ) -> Result<(String, DeployKeyOrigin), MmError> {
        let document = self.load(start)?;
        let environment = crate::domain::config::resolve_environment(&document, environment)?;
        let (target_name, target) =
            cluster_target(&document, &environment).ok_or_else(|| MmError::Target {
                message: format!("environment '{environment}' has no repo target with nodes:"),
            })?;
        if target.delivery.is_push() {
            return Err(MmError::Target {
                message: format!(
                    "target '{target_name}' has `delivery: push`: mm sends its nodes each commit's files, so they fetch nothing and need no deploy key"
                ),
            });
        }
        let declared = target
            .setup
            .as_ref()
            .and_then(|setup| setup.deploy_key.as_ref());
        let secret = declared.map(|key| key.secret.clone()).ok_or_else(|| {
            setup_error(
                target_name,
                "setup.deploy-key: with the secret: that holds the nodes' read-only git key",
            )
        })?;
        let declaration = document
            .config
            .secrets
            .get(&secret)
            .ok_or_else(|| undeclared_deploy_key(&secret))?;
        let reference = declaration
            .storage
            .reference()
            .map_err(|message| MmError::Secret { message })?;
        let metadata = self
            .dependencies
            .inspector
            .inspect(&SecretScope::new(environment.clone()), &reference)
            .await?;
        if metadata.status == SecretStatus::Set {
            let private_key = self.deploy_key(&document, &environment, &secret).await?;
            return Ok((
                self.dependencies.shell.public_key_of(&private_key).await?,
                DeployKeyOrigin::Existing,
            ));
        }
        let declared = declared.map(|key| key.public_key.as_str());
        let (private_key, public_key, origin) = match self
            .target_key(&document, (target_name, &environment), &secret, declared)
            .await?
        {
            Some((from, private_key, public_key)) => {
                (private_key, public_key, DeployKeyOrigin::Copied(from))
            }
            None => {
                let comment = format!("mm-deploy-{}-{target_name}", document.config.project.name);
                let (private_key, public_key) = self
                    .dependencies
                    .shell
                    .generate_deploy_key(&comment)
                    .await?;
                (private_key, public_key, DeployKeyOrigin::Created)
            }
        };
        let encoded = zeroize::Zeroizing::new(
            base64::engine::general_purpose::STANDARD.encode(private_key.as_slice()),
        );
        let material = crate::domain::secret::SecretMaterial::from_zeroizing(encoded)
            .map_err(|message| MmError::Secret { message })?;
        secrets
            .set(start, Some(&environment), &secret, material)
            .await?;
        Ok((public_key, origin))
    }

    /// The deploy key another environment of the target holds in its own
    /// store on this machine: the one the target declares, else the first
    /// one found. Returns that environment, the private bytes and the
    /// public line.
    async fn target_key(
        &self,
        document: &ConfigDocument,
        (target_name, environment): (&str, &str),
        secret: &str,
        declared: Option<&str>,
    ) -> Result<Option<(String, zeroize::Zeroizing<Vec<u8>>, String)>, MmError> {
        let reference = document
            .config
            .secrets
            .get(secret)
            .ok_or_else(|| undeclared_deploy_key(secret))?
            .storage
            .reference()
            .map_err(|message| MmError::Secret { message })?;
        let mut found = None;
        for other in document.config.environments.keys() {
            if other == environment
                || cluster_target(document, other).map(|(name, _)| name) != Some(target_name)
            {
                continue;
            }
            let metadata = self
                .dependencies
                .inspector
                .inspect(&SecretScope::new(other.clone()), &reference)
                .await?;
            if metadata.status != SecretStatus::Set {
                continue;
            }
            let private_key = self.deploy_key(document, other, secret).await?;
            let public_key = self.dependencies.shell.public_key_of(&private_key).await?;
            if declared.is_some_and(|declared| same_key(declared, &public_key)) {
                return Ok(Some((other.clone(), private_key, public_key)));
            }
            found.get_or_insert((other.clone(), private_key, public_key));
        }
        Ok(found)
    }

    fn connection(&self, plan: &ClusterSetupPlan, node: &ClusterNodePlan) -> ClusterNodeConnection {
        self.connection_as(&plan.setup.user, node)
    }
}

/// Setup never runs from a configuration that does not validate.
fn require_valid(document: &ConfigDocument, environment: &str) -> Result<(), MmError> {
    let report = crate::app::config_validation::validate(document, environment);
    if report.is_valid() {
        Ok(())
    } else {
        Err(MmError::invalid(&report.diagnostics))
    }
}

/// One notice per registry whose login secret has no value in `environment`.
fn registry_notices(
    document: &ConfigDocument,
    environment: &str,
    unset_secrets: &[String],
) -> Vec<String> {
    document
        .config
        .registries
        .iter()
        .filter(|(_, registry)| unset_secrets.contains(&registry.password.secret))
        .map(|(host, registry)| {
            crate::app::environment::unset_registry_secret(
                document,
                environment,
                host,
                &registry.password.secret,
            )
        })
        .collect()
}

/// The target's `setup:` block and its deploy key, which setup cannot run
/// without. A target mm pushes the project's files to has no deploy key:
/// its nodes fetch nothing.
fn declared_setup(
    target_name: &str,
    target: &RepoTargetConfig,
) -> Result<
    (
        crate::domain::repo_target::RepoTargetSetup,
        Option<crate::domain::repo_target::RepoDeployKey>,
    ),
    MmError,
> {
    let pushed = target.delivery.is_push();
    let setup = target.setup.clone().ok_or_else(|| {
        setup_error(
            target_name,
            if pushed {
                "setup: with a break-glass: recovery key"
            } else {
                "setup: with the origin: git URL the nodes fetch from"
            },
        )
    })?;
    if pushed {
        return Ok((setup, None));
    }
    let deploy_key = setup.deploy_key.clone().ok_or_else(|| {
        setup_error(
            target_name,
            "setup.deploy-key: with the secret: that holds the nodes' read-only git key",
        )
    })?;
    Ok((setup, Some(deploy_key)))
}

fn setup_error(target: &str, needed: &str) -> MmError {
    MmError::Target {
        message: format!("`mm target setup` needs targets.{target} to declare {needed}"),
    }
}

fn undeclared_deploy_key(secret: &str) -> MmError {
    MmError::Secret {
        message: format!(
            "setup.deploy-key names secret '{secret}', which secrets: does not declare; declare it with the age provider"
        ),
    }
}

/// The container runtime setup installs on every node: the environment's
/// driver, else the project's, else Docker Compose. The access layer runs
/// `mmd` as root, so either engine is installed rootful.
fn node_runtime(document: &ConfigDocument, environment: &str) -> RuntimeKind {
    document
        .config
        .environments
        .get(environment)
        .and_then(|overlay| overlay.runtime.as_ref())
        .and_then(|runtime| runtime.driver)
        .or(document.config.runtime.driver)
        .unwrap_or(RuntimeKind::DockerCompose)
}

/// Compares the type and key material of two OpenSSH public key lines.
fn same_key(left: &str, right: &str) -> bool {
    let fields = |line: &str| {
        line.split_whitespace()
            .take(2)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let left = fields(left);
    left.len() == 2 && left == fields(right)
}

fn reference_text(reference: &SecretReference) -> String {
    format!("{}://{}", reference.scheme(), reference.locator())
}

#[cfg(test)]
#[path = "cluster_setup/tests.rs"]
mod tests;
