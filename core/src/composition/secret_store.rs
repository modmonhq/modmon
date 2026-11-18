//! `mmd openbao`: the managed secret store's member on this node, as
//! `mm target setup` runs it as root, and the daemon's unsealer.

use std::sync::Arc;

use crate::adapters::process::TokioProcessRunner;
use crate::adapters::remote::DaemonConfig;
use crate::adapters::secrets::{
    NodeStore, OpenBaoConnectionStore, ReqwestOpenBaoTransport, StoreJoinAnswer, StoreLayout,
    StoreUnsealer,
};
use crate::adapters::state::MmHome;
use crate::domain::error::MmError;
use crate::domain::secret_store::{
    StoreInitAnswer, StoreInitRequest, StoreMemberAnswer, StoreMemberSpec, StoreStatus,
};
use crate::domain::workspace::ProjectId;

/// How long `mm target nodes` waits for this host's member.
const STATUS_DEADLINE: std::time::Duration = std::time::Duration::from_secs(15);
/// The largest document setup hands a member on stdin.
const MAX_INPUT_BYTES: usize = 256 * 1024;

fn connections(home: &MmHome) -> Arc<OpenBaoConnectionStore> {
    Arc::new(OpenBaoConnectionStore::new(
        home.identities_directory.join("openbao-connections.key"),
        home.secrets_directory.join("openbao-connections.age"),
    ))
}

fn node_store(
    config: &DaemonConfig,
    project: ProjectId,
    environment: &str,
) -> Result<NodeStore, MmError> {
    node_store_at(&config.mm_home, project, environment)
}

fn node_store_at(
    mm_home: &std::path::Path,
    project: ProjectId,
    environment: &str,
) -> Result<NodeStore, MmError> {
    crate::domain::host_policy::validate_environment_name(environment)
        .map_err(|message| MmError::Environment { message })?;
    let home = MmHome::resolve(Some(mm_home))?;
    Ok(NodeStore {
        layout: StoreLayout::host(),
        connections: connections(&home),
        transport: Arc::new(ReqwestOpenBaoTransport),
        process: Arc::new(TokioProcessRunner),
        host_store: super::host_secrets::host_store(mm_home, project)?,
        project,
        environment: environment.to_owned(),
    })
}

/// This host's member of the environment's store, as `mm target nodes` and
/// `mm doctor` show it: `None` when it runs none.
pub(super) async fn store_here(
    mm_home: &std::path::Path,
    project: ProjectId,
    environment: &str,
) -> Option<StoreStatus> {
    let store = node_store_at(mm_home, project, environment).ok()?;
    let status = tokio::time::timeout(STATUS_DEADLINE, store.status())
        .await
        .ok()?
        .ok()?;
    (!status.node.is_empty()).then_some(status)
}

/// A snapshot of this host's member, for `mm secret provider snapshot`.
pub(super) async fn snapshot_here(
    mm_home: &std::path::Path,
    scope: &crate::domain::remote::RemoteScope,
    recipient: &str,
) -> Result<String, MmError> {
    use crate::app::ports::Clock as _;
    node_store_at(mm_home, scope.project_id, &scope.environment)?
        .snapshot(recipient, crate::adapters::clock::SystemClock.now_unix_ms())
        .await
}

/// One step of `mm secret provider restore` on this host.
pub(super) async fn restore_here(
    mm_home: &std::path::Path,
    scope: &crate::domain::remote::RemoteScope,
    step: crate::domain::secret_store::StoreRestoreStep,
) -> Result<crate::domain::secret_store::StoreRestored, MmError> {
    node_store_at(mm_home, scope.project_id, &scope.environment)?
        .restore(step)
        .await
}

/// `mmd openbao audit`: the newest records of the member's audit device.
pub fn store_audit(
    config: &DaemonConfig,
    (project, environment): (ProjectId, &str),
    limit: usize,
) -> Result<Vec<serde_json::Value>, MmError> {
    node_store(config, project, environment)?.audit(limit)
}

fn parse<T: serde::de::DeserializeOwned>(input: &str, what: &str) -> Result<T, MmError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(MmError::Target {
            message: format!("{what} on stdin is larger than {MAX_INPUT_BYTES} bytes"),
        });
    }
    serde_json::from_str(input).map_err(|error| MmError::Target {
        message: format!("{what} on stdin is not valid: {error}"),
    })
}

/// `mmd openbao member`: the member described on stdin.
pub async fn store_member(
    config: &DaemonConfig,
    (project, environment): (ProjectId, &str),
    input: &str,
) -> Result<StoreMemberAnswer, MmError> {
    let spec = parse::<StoreMemberSpec>(input, "the member")?;
    node_store(config, project, environment)?.member(spec).await
}

/// `mmd openbao init`: the first initialisation, sealed to the recipients
/// on stdin.
pub async fn store_init(
    config: &DaemonConfig,
    (project, environment): (ProjectId, &str),
    input: &str,
) -> Result<StoreInitAnswer, MmError> {
    let request = parse::<StoreInitRequest>(input, "the initialisation")?;
    node_store(config, project, environment)?
        .init(request)
        .await
}

/// `mmd openbao join`: the recovery document sealed to this node, on stdin.
pub async fn store_join(
    config: &DaemonConfig,
    (project, environment): (ProjectId, &str),
    input: &str,
) -> Result<StoreJoinAnswer, MmError> {
    node_store(config, project, environment)?
        .join(input.as_bytes())
        .await
}

/// `mmd openbao status`.
pub async fn store_status(
    config: &DaemonConfig,
    (project, environment): (ProjectId, &str),
) -> Result<StoreStatus, MmError> {
    node_store(config, project, environment)?.status().await
}

/// `mmd openbao remove-peer`: `peer`'s old Raft entry, removed through this
/// node's member.
pub async fn store_remove_peer(
    config: &DaemonConfig,
    (project, environment): (ProjectId, &str),
    peer: &str,
) -> Result<crate::domain::secret_store::StorePeerRemoval, MmError> {
    node_store(config, project, environment)?
        .remove_peer(peer)
        .await
}

/// `mmd openbao versions`.
pub async fn store_versions(
    config: &DaemonConfig,
    (project, environment): (ProjectId, &str),
) -> Result<Vec<crate::domain::secret_store::StoreSecretVersions>, MmError> {
    node_store(config, project, environment)?.versions().await
}

/// What unseals this node's members whenever they are sealed.
pub fn store_unsealer(config: &DaemonConfig) -> Result<StoreUnsealer, MmError> {
    let home = MmHome::resolve(Some(&config.mm_home))?;
    Ok(StoreUnsealer::new(
        StoreLayout::host(),
        connections(&home),
        Arc::new(ReqwestOpenBaoTransport),
    )
    .rotating(Arc::new(TokioProcessRunner)))
}

/// The store a cluster environment declares, refused with the reason
/// otherwise.
fn cluster_store_of<'a>(
    document: &'a crate::domain::config::ConfigDocument,
    environment: &str,
    provider: &str,
) -> Result<
    (
        &'a crate::domain::repo_target::RepoTargetConfig,
        crate::domain::secret_store::DeclaredStore<'a>,
    ),
    MmError,
> {
    let declared = crate::domain::secret_store::declared_store(&document.config)
        .filter(|declared| declared.provider.as_str() == provider);
    match (declared, crate::app::cluster_target(document, environment)) {
        (Some(declared), Some((_, target))) => Ok((target, declared)),
        _ => Err(MmError::Secret {
            message: format!(
                "'{provider}' is not the managed store of a cluster environment; snapshots are of the store `mm target setup` runs on '{environment}''s nodes (`mode: managed`, `scope: environment`)"
            ),
        }),
    }
}

/// The managed store of the cluster environment `environment` as its
/// members report it: `mm secret provider status` of that store, and MCP's
/// read `secret_store`.
pub(crate) async fn cluster_store_report(
    document: &crate::domain::config::ConfigDocument,
    environment: &str,
    remote: &crate::app::RemoteEnvironment,
) -> Result<crate::domain::secret_store::ClusterStoreReport, MmError> {
    use crate::domain::secret_store::{
        ClusterStoreReport, declared_store, members_on, pinned_release,
    };
    let (Some(declared), Some((_, target))) = (
        declared_store(&document.config),
        crate::app::cluster_target(document, environment),
    ) else {
        return Err(MmError::Secret {
            message: format!(
                "environment '{environment}' keeps no managed secret store on its nodes: a cluster environment does when a provider has `mode: managed` and `scope: environment`"
            ),
        });
    };
    let provider = declared.provider.as_str();
    let members = members_on(target, declared.nodes, provider)
        .map_err(|message| MmError::Target { message })?
        .into_iter()
        .map(|member| member.name)
        .collect::<Vec<_>>();
    let version = pinned_release(declared.release)
        .map_or(declared.release, |pinned| pinned.version)
        .to_owned();
    let reports = remote.node_reports().await;
    Ok(ClusterStoreReport::of(
        (provider, environment, remote.target()),
        &version,
        &members,
        &reports,
    ))
}

/// `mm secret provider snapshot`.
pub(crate) async fn take_store_snapshot(
    explicit_home: Option<&std::path::Path>,
    document: &crate::domain::config::ConfigDocument,
    (environment, provider): (&str, &str),
    remote: &crate::app::RemoteEnvironment,
) -> Result<(String, String), MmError> {
    use crate::app::ports::ClusterStoreKeeper as _;
    cluster_store_of(document, environment, provider)?;
    let home = MmHome::resolve(explicit_home)?;
    let recipient =
        super::cluster_setup::store_keeper(&home).operator_recipient(document.config.project.id)?;
    remote.store_snapshot(&recipient).await
}

/// What `mm secret provider restore` would put back from the snapshot
/// sealed in `file`.
pub(crate) fn store_restore_plan(
    explicit_home: Option<&std::path::Path>,
    document: &crate::domain::config::ConfigDocument,
    (environment, provider): (&str, &str),
    file: &str,
) -> Result<
    (
        crate::domain::secret_store::StoreRestorePlan,
        crate::domain::secret_store::StoreSnapshot,
    ),
    MmError,
> {
    use crate::app::ports::ClusterStoreKeeper as _;
    let (target, declared) = cluster_store_of(document, environment, provider)?;
    let home = MmHome::resolve(explicit_home)?;
    let opened =
        super::cluster_setup::store_keeper(&home).open(document.config.project.id, file)?;
    let snapshot: crate::domain::secret_store::StoreSnapshot = serde_json::from_slice(&opened)
        .map_err(|_| MmError::Secret {
            message: "that file is not a snapshot of a managed secret store".to_owned(),
        })?;
    if snapshot.project_id != document.config.project.id
        || snapshot.environment != environment
        || snapshot.provider != provider
    {
        return Err(MmError::Secret {
            message: format!(
                "that snapshot is of the store '{}' of environment '{}', not of '{provider}' of '{environment}'",
                snapshot.provider, snapshot.environment
            ),
        });
    }
    let members = crate::domain::secret_store::members_on(target, declared.nodes, provider)
        .map_err(|message| MmError::Target { message })?;
    Ok((
        crate::domain::secret_store::StoreRestorePlan::of(&snapshot, members, file.as_bytes()),
        snapshot,
    ))
}

/// `mm secret provider restore --plan HASH --yes`.
pub(crate) async fn restore_store(
    explicit_home: Option<&std::path::Path>,
    document: &crate::domain::config::ConfigDocument,
    (environment, provider): (&str, &str),
    (file, reviewed): (&str, &str),
    remote: &crate::app::RemoteEnvironment,
) -> Result<Vec<crate::domain::secret_store::StoreRestored>, MmError> {
    let (plan, snapshot) =
        store_restore_plan(explicit_home, document, (environment, provider), file)?;
    if plan.plan_hash != reviewed {
        return Err(MmError::State {
            message: format!(
                "the restore plan changed (expected {reviewed}, current {}); review the new plan",
                plan.plan_hash
            ),
        });
    }
    let home = MmHome::resolve(explicit_home)?;
    let keeper = super::cluster_setup::store_keeper(&home);
    remote
        .store_restore(&snapshot, &plan.members, &keeper)
        .await
}
