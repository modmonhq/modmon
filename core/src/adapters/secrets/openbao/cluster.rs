//! The managed store of a cluster environment, on its nodes and on the operator's machine.

mod audit;
mod authority;
mod bootstrap;
mod member;
mod rejoin;
mod snapshot;
mod unsealer;

use std::sync::Arc;
use std::time::Duration;

use reqwest::Method;
use serde::Deserialize;
use zeroize::Zeroizing;

pub use authority::FileStoreKeeper;
pub use member::{MemberRecord, StoreLayout, render_config, render_unit};
pub use unsealer::StoreUnsealer;

use super::connection_store::{
    AppRoleCredential, OpenBaoConnection, OpenBaoConnectionStore, OpenBaoMode, ca_sha256,
};
use super::http::{OpenBaoHttpResponse, OpenBaoHttpTransport};
use crate::adapters::secrets::AgeSecretStore;
use crate::app::ports::{CommandSpec, ProcessRunner};
use crate::domain::error::MmError;
use crate::domain::secret_store::{
    STORE_API_PORT, STORE_AUTH_MOUNT, STORE_MOUNT, StoreCredential, StoreMemberAnswer,
    StoreMemberSpec, StorePeer, StoreRecovery, StoreStatus, store_id, store_unit,
};
use crate::domain::workspace::ProjectId;

/// How long a member may take to answer, unseal, or join after a start.
const SETTLE: Duration = Duration::from_secs(90);
const POLL: Duration = Duration::from_secs(2);
const SYSTEMCTL_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RECOVERY_BYTES: usize = 64 * 1024;

/// What `mmd openbao join` answers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StoreJoinAnswer {
    pub member: bool,
    pub sealed: bool,
}

/// One environment's store as its node's mmd runs it: the commands setup
/// runs as root, outside mmd's own sandbox.
pub struct NodeStore {
    pub layout: StoreLayout,
    pub connections: Arc<OpenBaoConnectionStore>,
    pub transport: Arc<dyn OpenBaoHttpTransport>,
    pub process: Arc<dyn ProcessRunner>,
    /// The project's store on this host, which recovery documents are
    /// sealed to.
    pub host_store: AgeSecretStore,
    pub project: ProjectId,
    pub environment: String,
}

impl std::fmt::Debug for NodeStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NodeStore")
            .field("project", &self.project)
            .field("environment", &self.environment)
            .finish_non_exhaustive()
    }
}

/// A connection to a member, with no credential.
pub(crate) fn member_connection(
    project: ProjectId,
    id: &str,
    address: String,
    ca_pem: String,
) -> OpenBaoConnection {
    OpenBaoConnection {
        mode: OpenBaoMode::Managed,
        address,
        namespace: None,
        mount: STORE_MOUNT.to_owned(),
        auth_mount: STORE_AUTH_MOUNT.to_owned(),
        ca_sha256: ca_sha256(&ca_pem),
        ca_pem,
        project_id: project,
        credentials: std::collections::BTreeMap::new(),
        allowed_locations: std::collections::BTreeMap::new(),
        managed_installation: Some(id.to_owned()),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct SealStatus {
    pub initialized: bool,
    pub sealed: bool,
    #[serde(default)]
    pub version: String,
}

fn malformed(what: &str) -> MmError {
    MmError::Target {
        message: format!("the secret store answered {what} in a form mm does not understand"),
    }
}

fn failed(response: &OpenBaoHttpResponse, what: &str) -> MmError {
    MmError::Target {
        message: format!("the secret store refused {what} (HTTP {})", response.status),
    }
}

pub(crate) async fn seal_status(
    transport: &dyn OpenBaoHttpTransport,
    connection: &OpenBaoConnection,
) -> Result<SealStatus, MmError> {
    let response = transport
        .request(connection, Method::GET, "sys/seal-status", None, None)
        .await?;
    if !(200..300).contains(&response.status) {
        return Err(failed(&response, "its seal status"));
    }
    serde_json::from_slice(&response.body).map_err(|_| malformed("its seal status"))
}

/// Gives a sealed member the keys. True when it is unsealed afterwards.
pub(crate) async fn unseal(
    transport: &dyn OpenBaoHttpTransport,
    connection: &OpenBaoConnection,
    keys: &[Zeroizing<String>],
) -> Result<bool, MmError> {
    for key in keys {
        let body = serde_json::json!({ "key": key.as_str() });
        let response = transport
            .request(connection, Method::POST, "sys/unseal", None, Some(&body))
            .await?;
        if !(200..300).contains(&response.status) {
            return Ok(false);
        }
        let status: SealStatus =
            serde_json::from_slice(&response.body).map_err(|_| malformed("an unseal"))?;
        if !status.sealed {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A short-lived token of an AppRole login.
pub(crate) async fn login(
    transport: &dyn OpenBaoHttpTransport,
    connection: &OpenBaoConnection,
    credential: &StoreCredential,
) -> Result<Zeroizing<String>, MmError> {
    #[derive(Deserialize)]
    struct Login {
        auth: Auth,
    }
    #[derive(Deserialize)]
    struct Auth {
        client_token: Zeroizing<String>,
    }
    let body = serde_json::json!({
        "role_id": credential.role_id.as_str(),
        "secret_id": credential.secret_id.as_str(),
    });
    let response = transport
        .request(
            connection,
            Method::POST,
            &format!("auth/{STORE_AUTH_MOUNT}/login"),
            None,
            Some(&body),
        )
        .await?;
    if !(200..300).contains(&response.status) {
        return Err(failed(&response, "mmd's login"));
    }
    serde_json::from_slice::<Login>(&response.body)
        .map(|login| login.auth.client_token)
        .map_err(|_| malformed("a login"))
}

/// The Raft members the store knows, by a member that serves.
pub(crate) async fn peers(
    transport: &dyn OpenBaoHttpTransport,
    connection: &OpenBaoConnection,
    token: &str,
) -> Result<Vec<StorePeer>, MmError> {
    #[derive(Deserialize)]
    struct Answer {
        data: Data,
    }
    #[derive(Deserialize)]
    struct Data {
        config: Config,
    }
    #[derive(Deserialize)]
    struct Config {
        servers: Vec<Server>,
    }
    #[derive(Deserialize)]
    struct Server {
        node_id: String,
        address: String,
        leader: bool,
        voter: bool,
    }
    let response = transport
        .request(
            connection,
            Method::GET,
            "sys/storage/raft/configuration",
            Some(token),
            None,
        )
        .await?;
    if !(200..300).contains(&response.status) {
        return Err(failed(&response, "its Raft configuration"));
    }
    let answer: Answer =
        serde_json::from_slice(&response.body).map_err(|_| malformed("its Raft configuration"))?;
    Ok(answer
        .data
        .config
        .servers
        .into_iter()
        .map(|server| StorePeer {
            name: server.node_id,
            address: server.address,
            leader: server.leader,
            voter: server.voter,
        })
        .collect())
}

impl NodeStore {
    fn id(&self) -> String {
        store_id(self.project, &self.environment)
    }

    fn loopback(&self, ca_pem: String) -> OpenBaoConnection {
        member_connection(
            self.project,
            &self.id(),
            format!("https://127.0.0.1:{STORE_API_PORT}"),
            ca_pem,
        )
    }

    async fn systemctl(&self, arguments: &[&str]) -> Result<(), MmError> {
        let output = self
            .process
            .run(
                CommandSpec::new("systemctl")
                    .args(arguments.iter().copied())
                    .environment("LC_ALL", "C")
                    .timeout(SYSTEMCTL_TIMEOUT),
            )
            .await?;
        if output.success() {
            return Ok(());
        }
        Err(MmError::ProcessFailed {
            program: format!("systemctl {}", arguments.join(" ")),
            code: output.code,
            stderr: output.stderr.trim().to_owned(),
        })
    }

    async fn active(&self, unit: &str) -> bool {
        self.process
            .run(
                CommandSpec::new("systemctl")
                    .args(["is-active", "--quiet", unit])
                    .timeout(SYSTEMCTL_TIMEOUT),
            )
            .await
            .is_ok_and(|output| output.success())
    }

    /// The member's seal status once it answers on loopback.
    async fn answering(&self, connection: &OpenBaoConnection) -> Result<SealStatus, MmError> {
        let deadline = tokio::time::Instant::now() + SETTLE;
        loop {
            match seal_status(self.transport.as_ref(), connection).await {
                Ok(status) => return Ok(status),
                Err(error) if tokio::time::Instant::now() >= deadline => return Err(error),
                Err(_) => tokio::time::sleep(POLL).await,
            }
        }
    }

    /// Unseals the member with `keys` until it serves.
    async fn unsealed(
        &self,
        connection: &OpenBaoConnection,
        keys: &[Zeroizing<String>],
    ) -> Result<(), MmError> {
        let deadline = tokio::time::Instant::now() + SETTLE;
        loop {
            let status = seal_status(self.transport.as_ref(), connection).await;
            let ready = match status {
                Ok(status) if !status.sealed => self.healthy(connection).await,
                Ok(_) => {
                    let _unsealed = unseal(self.transport.as_ref(), connection, keys).await;
                    false
                }
                Err(_) => false,
            };
            if ready {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(MmError::Target {
                    message: format!(
                        "the secret store's member here did not come up unsealed within {} s; `journalctl -u {}` says why",
                        SETTLE.as_secs(),
                        store_unit(&self.id())
                    ),
                });
            }
            tokio::time::sleep(POLL).await;
        }
    }

    async fn healthy(&self, connection: &OpenBaoConnection) -> bool {
        self.transport
            .request(
                connection,
                Method::GET,
                "sys/health?standbyok=true&perfstandbyok=true",
                None,
                None,
            )
            .await
            .is_ok_and(|response| response.status == 200)
    }

    /// `mmd openbao member`: the member's files, its unit, and, when they
    /// changed, a restart after which it serves unsealed again.
    pub async fn member(&self, spec: StoreMemberSpec) -> Result<StoreMemberAnswer, MmError> {
        crate::domain::secret_store::pinned_release(
            spec.version.rsplit_once('.').map_or("", |(line, _)| line),
        )
        .filter(|pinned| pinned.version == spec.version)
        .ok_or_else(|| MmError::Target {
            message: format!("OpenBao {} is not a release mm installs", spec.version),
        })?;
        let id = self.id();
        let held = self.connections.cluster_store(&id)?;
        let initialized = self
            .layout
            .record(&id)?
            .is_some_and(|record| record.initialized)
            && self.layout.raft_data(&id).is_file();
        let (written, _) = member::write_member(
            &self.layout,
            (&id, self.project, &self.environment),
            &spec,
            initialized,
        )?;
        if let Some(request) = written.request {
            return Ok(StoreMemberAnswer {
                csr: Some(request),
                has_keys: held.is_some(),
                initialized,
                ..StoreMemberAnswer::default()
            });
        }
        let unit = store_unit(&id);
        if written.unit_changed {
            self.systemctl(&["daemon-reload"]).await?;
        }
        self.systemctl(&["enable", "--quiet", &unit]).await?;
        if written.changed || !self.active(&unit).await {
            self.systemctl(&["restart", &unit]).await?;
        }
        let connection = self.loopback(spec.ca_pem.clone());
        let mut status = self.answering(&connection).await?;
        if let Some(recovery) = &held
            && initialized
            && status.initialized
            && status.sealed
        {
            self.unsealed(&connection, &recovery.unseal_keys).await?;
            status = self.answering(&connection).await?;
        }
        let joined = match (status.initialized, status.sealed) {
            (false, _) => false,
            (true, false) => true,
            (true, true) => initialized,
        };
        if let Some(mut record) = self.layout.record(&id)?
            && record.initialized != joined
        {
            record.initialized = joined;
            self.layout.save_record(&id, &record)?;
        }
        Ok(StoreMemberAnswer {
            changed: written.changed,
            csr: None,
            has_keys: held.is_some(),
            initialized: joined,
            sealed: status.sealed,
        })
    }

    /// Keeps `recovery` here: the unseal keys, both logins, and the
    /// connection mmd reads the environment's values through.
    fn keep(&self, recovery: &StoreRecovery) -> Result<bool, MmError> {
        let id = self.id();
        let member = self.layout.record(&id)?;
        let address = match (&member, recovery.members.first()) {
            (Some(_), _) => format!("https://127.0.0.1:{STORE_API_PORT}"),
            (None, Some(first)) => format!("https://{}:{STORE_API_PORT}", first.address),
            (None, None) => {
                return Err(MmError::Target {
                    message: "the store's recovery document names no member".to_owned(),
                });
            }
        };
        let mut connection = member_connection(self.project, &id, address, recovery.ca_pem.clone());
        connection.credentials.insert(
            self.environment.clone(),
            AppRoleCredential {
                role_id: recovery.deploy.role_id.clone(),
                secret_id: recovery.deploy.secret_id.clone(),
            },
        );
        connection
            .allowed_locations
            .insert(self.environment.clone(), vec!["*".to_owned()]);
        let name =
            crate::adapters::secrets::host_connection_name(&recovery.provider, self.project)?;
        self.connections
            .save_cluster_store(&id, recovery.clone(), (&name, connection))?;
        Ok(member.is_some())
    }

    /// `mmd openbao join`: keeps the recovery document and unseals the member.
    pub async fn join(&self, sealed: &[u8]) -> Result<StoreJoinAnswer, MmError> {
        let recovery = self.opened_recovery(sealed)?;
        self.join_with(recovery).await
    }

    fn opened_recovery(&self, sealed: &[u8]) -> Result<StoreRecovery, MmError> {
        let opened = self.host_store.open_sealed(sealed, MAX_RECOVERY_BYTES)?;
        serde_json::from_slice(&opened).map_err(|_| MmError::Secret {
            message: "the store's recovery document is not valid".to_owned(),
        })
    }

    async fn join_with(&self, mut recovery: StoreRecovery) -> Result<StoreJoinAnswer, MmError> {
        if let Some(record) = self.layout.record(&self.id())? {
            recovery.ca_pem = self.member_ca()?;
            recovery.members = record.members;
        }
        if recovery.project_id != self.project || recovery.environment != self.environment {
            return Err(MmError::Security {
                message: format!(
                    "the recovery document is for another project or environment than {}",
                    self.environment
                ),
            });
        }
        let member = self.keep(&recovery)?;
        if !member {
            return Ok(StoreJoinAnswer {
                member: false,
                sealed: false,
            });
        }
        let id = self.id();
        let connection = self.loopback(recovery.ca_pem.clone());
        self.unsealed(&connection, &recovery.unseal_keys).await?;
        if let Some(mut record) = self.layout.record(&id)?
            && !record.initialized
        {
            record.initialized = true;
            self.layout.save_record(&id, &record)?;
        }
        Ok(StoreJoinAnswer {
            member: true,
            sealed: false,
        })
    }

    /// `mmd openbao audit`: the newest `limit` records of the member's
    /// audit device, oldest first. Values in them are hashed.
    pub fn audit(&self, limit: usize) -> Result<Vec<serde_json::Value>, MmError> {
        audit::newest(&self.layout.audit_log(&self.id()), limit)
    }

    /// `mmd openbao status`: what this node's member says, and the Raft
    /// cluster as it sees it.
    pub async fn status(&self) -> Result<StoreStatus, MmError> {
        let id = self.id();
        let unit = store_unit(&id);
        let record = self.layout.record(&id)?;
        let held = self.connections.cluster_store(&id)?;
        let mut status = StoreStatus {
            provider: held
                .as_ref()
                .map(|held| held.provider.clone())
                .or_else(|| record.as_ref().map(|record| record.provider.clone()))
                .unwrap_or_default(),
            node: record
                .as_ref()
                .map(|record| record.node.clone())
                .unwrap_or_default(),
            version: record.as_ref().map(|record| record.version.clone()),
            has_keys: held.is_some(),
            unit: unit.clone(),
            ..StoreStatus::default()
        };
        let Some(record) = record else {
            return Ok(status);
        };
        status.running = self.active(&unit).await;
        let ca = member::read_text(&self.layout.ca(&id))?.unwrap_or_default();
        let connection = self.loopback(ca);
        match seal_status(self.transport.as_ref(), &connection).await {
            Ok(seal) => {
                status.initialized = seal.initialized;
                status.sealed = seal.sealed;
                if !seal.version.is_empty() {
                    status.version = Some(seal.version);
                }
            }
            Err(error) => {
                status
                    .problems
                    .push(format!("the member does not answer: {error}"));
                return Ok(status);
            }
        }
        if let Some(held) = held
            && !status.sealed
        {
            match login(self.transport.as_ref(), &connection, &held.operations).await {
                Ok(token) => match peers(self.transport.as_ref(), &connection, &token).await {
                    Ok(peers) => {
                        status.leader = peers
                            .iter()
                            .find(|peer| peer.leader)
                            .map(|peer| peer.name.clone());
                        status.peers = peers;
                    }
                    Err(error) => status.problems.push(error.to_string()),
                },
                Err(error) => status.problems.push(error.to_string()),
            }
        }
        if status.sealed {
            status
                .problems
                .push(format!("the member on {} is sealed", record.node));
        }
        Ok(status)
    }
}
