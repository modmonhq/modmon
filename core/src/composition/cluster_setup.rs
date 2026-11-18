//! Wiring for `mm target setup` on a cluster target, and `record`, which
//! writes what setup learned back into the repo's targets file.

use super::*;
use crate::domain::cluster_setup::ClusterSetupResult;
use crate::domain::repo_target::RepoTargetConfig;

/// The resolved environment, and whether its repo target lists `nodes:`.
pub(crate) fn cluster_environment(
    start: &Path,
    environment: Option<&str>,
) -> Result<(String, bool), MmError> {
    let document = ConfigLoader.load_from(start)?;
    let environment = resolve_environment(&document, environment)?;
    let cluster = crate::app::cluster_target(&document, &environment).is_some();
    Ok((environment, cluster))
}

/// The commit the first access sync uses: it has to be on a remote branch,
/// because each node fetches it from the origin. On a target with
/// `delivery: push` setup sends the nodes the commit's files itself, so
/// any commit of this checkout will do.
pub(crate) fn pushed_commit(
    start: &Path,
    environment: &str,
    explicit: Option<&str>,
) -> Result<String, MmError> {
    let commit = resolve_commit(start, explicit)?;
    let document = ConfigLoader.load_from(start)?;
    if crate::app::cluster_target(&document, environment)
        .is_some_and(|(_, target)| target.delivery.is_push())
    {
        return Ok(commit);
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(start)
        .args(["branch", "--remotes", "--contains", &commit])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|source| MmError::ProcessSpawn {
            program: "git".to_owned(),
            source,
        })?;
    if !output.status.success() || output.stdout.iter().all(u8::is_ascii_whitespace) {
        return Err(MmError::Environment {
            message: format!(
                "commit {commit} is on no remote branch, and every node fetches it from the origin; push it first or pass --commit"
            ),
        });
    }
    Ok(commit)
}

pub(crate) struct ClusterSetupServices {
    pub(crate) service: ClusterSetupService,
    pub(crate) secrets: SecretManager,
    home: MmHome,
}

pub(crate) fn cluster_setup_services(
    start: &Path,
    explicit_home: Option<&Path>,
    process: Arc<dyn ProcessRunner>,
    clock: Arc<dyn Clock>,
) -> Result<ClusterSetupServices, MmError> {
    cluster_setup_services_for(start, explicit_home, process, clock, true)
}

/// The same, for a command that runs setup's steps (`steps`), which need
/// Ansible on this machine, or one that does not: `deploy-key` and
/// `record` run on a machine without it.
pub(crate) fn cluster_setup_services_for(
    start: &Path,
    explicit_home: Option<&Path>,
    process: Arc<dyn ProcessRunner>,
    clock: Arc<dyn Clock>,
    steps: bool,
) -> Result<ClusterSetupServices, MmError> {
    let document = ConfigLoader.load_from(start)?;
    let home = MmHome::resolve(explicit_home)?;
    let age = Arc::new(AgeSecretStore::new(
        identity_path(&home),
        home.secrets_directory
            .join(format!("{}.age", document.config.project.id.0)),
    ));
    let revision_key = crate::adapters::secrets::load_or_create_environment_revision_key(
        &home.identities_directory.join("environment-revisions.key"),
    )?;
    let registry = secret_provider_registry(
        age,
        revision_key,
        &home,
        document.config.project.id,
        &document.config,
    )?;
    let inspector: Arc<dyn SecretMetadataInspector> = registry.clone();
    let resolver: Arc<dyn SecretMaterialResolver> = registry;
    // Setup connects to the nodes with this device's key, which mm opens
    // with the passphrase it keeps.
    let keys = super::keys::managed_keys(&home, crate::domain::key::ManagedKeyKind::Device)?;
    let bootstrap: Arc<dyn crate::app::ports::TargetBootstrapExecutor> = if steps {
        Arc::new(
            crate::adapters::target::AnsibleTargetBootstrapExecutor::new(
                Arc::clone(&process),
                home.run_directory.join("bootstrap"),
            )?
            .with_managed_keys(keys.clone()),
        )
    } else {
        Arc::new(NoSetupSteps)
    };
    let service = ClusterSetupService::new(ClusterSetupDependencies {
        documents: project_documents(),
        inspector,
        resolver,
        shell: Arc::new(crate::adapters::target::SshClusterNodeShell::new(
            process, keys,
        )),
        bootstrap,
        identity_file: super::device_key_path(&home),
    })
    .with_pushed_commits(Arc::new(crate::adapters::remote::GitCommitBundles::new(
        document.root.clone(),
    )))
    .with_secret_store(Arc::new(store_keeper(&home)));
    let secrets = environment_services(start, explicit_home, clock)?.secrets;
    Ok(ClusterSetupServices {
        service,
        secrets,
        home,
    })
}

/// What keeps the managed secret stores' authorities and recovery copies
/// on this machine.
pub(crate) fn store_keeper(home: &MmHome) -> crate::adapters::secrets::FileStoreKeeper {
    crate::adapters::secrets::FileStoreKeeper::new(
        identity_path(home),
        home.secrets_directory.clone(),
    )
}

/// `mm secret provider recovery export`: the environment's recovery copy,
/// sealed to `recipient` or to this machine's operator.
pub(crate) fn export_store_recovery(
    explicit_home: Option<&Path>,
    document: &ConfigDocument,
    (environment, provider): (&str, &str),
    recipient: Option<&str>,
) -> Result<String, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    store_keeper(&home).export(
        crate::app::ports::StoreScope {
            project: document.config.project.id,
            environment,
            provider,
        },
        recipient,
    )
}

/// `mm secret provider recovery import`.
pub(crate) fn import_store_recovery(
    explicit_home: Option<&Path>,
    document: &ConfigDocument,
    (environment, provider): (&str, &str),
    sealed: &str,
) -> Result<(), MmError> {
    let home = MmHome::resolve(explicit_home)?;
    store_keeper(&home).import(
        crate::app::ports::StoreScope {
            project: document.config.project.id,
            environment,
            provider,
        },
        sealed,
    )
}

/// The executor of a command that runs none of setup's steps.
struct NoSetupSteps;

#[async_trait::async_trait]
impl crate::app::ports::TargetBootstrapExecutor for NoSetupSteps {
    async fn execute(
        &self,
        _project_root: &Path,
        _plan: &crate::domain::target::TargetBootstrapPlan,
    ) -> Result<crate::domain::target::BootstrapExecution, MmError> {
        Err(MmError::State {
            message: "this command runs no setup step".to_owned(),
        })
    }
}

impl ClusterSetupServices {
    fn result_path(&self, project: &str, environment: &str) -> PathBuf {
        self.home
            .root
            .join("cluster-setup")
            .join(format!("{project}-{environment}.json"))
    }

    /// Keeps the last apply's result so `record` can write it to the repo.
    pub(crate) fn save_result(&self, result: &ClusterSetupResult) -> Result<(), MmError> {
        // Setup replaced the relay, `mmd`, and the keys the hosts accept:
        // connections made before it are not used after it.
        close_kept_connections(&self.home);
        let path = self.result_path(&result.project_id.0.to_string(), &result.environment);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| MmError::Io {
                context: format!("creating {}", parent.display()),
                source,
            })?;
        }
        let bytes = serde_json::to_vec_pretty(result).map_err(|error| MmError::State {
            message: format!("serializing the cluster setup result: {error}"),
        })?;
        std::fs::write(&path, bytes).map_err(|source| MmError::Io {
            context: format!("writing {}", path.display()),
            source,
        })
    }

    /// Writes each node's host key, daemon identity, and private address
    /// from the last apply into `.config/mm/targets.yaml`. Returns the nodes
    /// that changed.
    pub(crate) fn record(
        &self,
        start: &Path,
        environment: Option<&str>,
    ) -> Result<(PathBuf, Vec<String>), MmError> {
        let document = ConfigLoader.load_from(start)?;
        let environment = resolve_environment(&document, environment)?;
        let (target_name, _) =
            crate::app::cluster_target(&document, &environment).ok_or_else(|| MmError::Target {
                message: format!("environment '{environment}' has no repo target with nodes:"),
            })?;
        let target_name = target_name.to_owned();
        let path = self.result_path(&document.config.project.id.0.to_string(), &environment);
        let result: ClusterSetupResult = std::fs::read(&path)
            .map_err(|source| MmError::Io {
                context: format!(
                    "reading {}; run `mm -e {environment} target setup apply` first",
                    path.display()
                ),
                source,
            })
            .and_then(|bytes| {
                serde_json::from_slice(&bytes).map_err(|error| MmError::State {
                    message: format!("invalid cluster setup result {}: {error}", path.display()),
                })
            })?;
        let targets_file = document.root.join(".config/mm/targets.yaml");
        if !targets_file.is_file() {
            return Err(MmError::Target {
                message: "record rewrites .config/mm/targets.yaml; move targets there first with `mm config split targets`".to_owned(),
            });
        }
        let text = std::fs::read_to_string(&targets_file).map_err(|source| MmError::Io {
            context: format!("reading {}", targets_file.display()),
            source,
        })?;
        let mut targets: std::collections::BTreeMap<String, RepoTargetConfig> =
            serde_yaml_ng::from_str(&text).map_err(|error| MmError::Target {
                message: format!("{}: {error}", targets_file.display()),
            })?;
        let target = targets
            .get_mut(&target_name)
            .ok_or_else(|| MmError::Target {
                message: format!("{} has no target '{target_name}'", targets_file.display()),
            })?;
        let mut changed = Vec::new();
        for record in &result.nodes {
            let node = target
                .nodes
                .iter_mut()
                .find(|node| node.name == record.name)
                .ok_or_else(|| MmError::Target {
                    message: format!(
                        "target '{target_name}' no longer lists node {}",
                        record.name
                    ),
                })?;
            let recorded = (
                Some(record.private_address),
                Some(record.host_key.clone()),
                Some(record.daemon_fingerprint.clone()),
            );
            if (
                node.private_address,
                node.host_key.clone(),
                node.daemon_fingerprint.clone(),
            ) != recorded
            {
                (node.private_address, node.host_key, node.daemon_fingerprint) = recorded;
                changed.push(record.name.clone());
            }
        }
        target
            .validate(&target_name)
            .map_err(|message| MmError::Target { message })?;
        if !changed.is_empty() {
            let header = text
                .lines()
                .take_while(|line| line.starts_with('#'))
                .map(|line| format!("{line}\n"))
                .collect::<String>();
            let body = serde_yaml_ng::to_string(&targets).map_err(|error| MmError::State {
                message: format!("serializing targets: {error}"),
            })?;
            std::fs::write(&targets_file, format!("{header}{body}")).map_err(|source| {
                MmError::Io {
                    context: format!("writing {}", targets_file.display()),
                    source,
                }
            })?;
        }
        Ok((targets_file, changed))
    }
}
