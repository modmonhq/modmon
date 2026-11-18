use std::path::PathBuf;

use async_trait::async_trait;
use zeroize::Zeroizing;

use crate::domain::error::MmError;

/// How setup reaches one node: as its setup account, with this device's mm
/// key, trusting exactly the reviewed host key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterNodeConnection {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// `<type> <base64>`.
    pub host_key: String,
    pub identity_file: PathBuf,
}

/// mm-owned scripts setup runs on a node as root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterScript {
    /// Hostname, `/etc/mm/node.env`, and the tools the mesh needs.
    Prepare,
    /// Has `mmd` bring the node's interface to the state on stdin and
    /// restore it at boot.
    Mesh,
    /// Prints the node's mesh facts as JSON; changes nothing.
    MeshFacts,
    /// Ping every peer's private address, and once at the mesh MTU.
    Reach,
    /// The host NFS server for mm's shared storage.
    Storage,
    /// Prints whether the node serves shared storage and keeps files
    /// there; changes nothing.
    StorageFacts,
    /// Stops serving shared storage from the node; its files stay.
    StorageWithdraw,
    /// Mounts the storage node's export once, read-only, and takes it off
    /// again.
    StorageReach,
    /// Prints the member record of an environment's managed secret store,
    /// `{}` when the node has none; changes nothing.
    StoreFacts,
    /// Installs a pinned OpenBao release after checking its archive's
    /// digest, with the account and directories its member runs with.
    StoreInstall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClusterNodeCommand {
    Script {
        script: ClusterScript,
        arguments: Vec<String>,
    },
    /// A fixed program and arguments, quoted for the remote shell.
    Program(Vec<String>),
}

#[async_trait]
pub trait ClusterNodeShell: Send + Sync {
    /// The node's ed25519 host key, `<type> <base64>`, as it presents it now.
    async fn scan_host_key(&self, host: &str, port: u16) -> Result<String, MmError>;

    /// Runs `command` on the node and returns its standard output. Standard
    /// input carries `stdin` when given; it is never logged.
    async fn run(
        &self,
        node: &ClusterNodeConnection,
        command: &ClusterNodeCommand,
        stdin: Option<Zeroizing<String>>,
    ) -> Result<Zeroizing<String>, MmError>;

    /// `<type> <base64> <comment>` of an unencrypted OpenSSH private key.
    async fn public_key_of(&self, private_key: &[u8]) -> Result<String, MmError>;

    /// The bundle encrypted to a node's age recipient, ASCII armored.
    fn seal_secret_bundle(
        &self,
        recipient: &str,
        bundle: &crate::domain::secret::SecretBundle,
    ) -> Result<Zeroizing<String>, MmError>;

    /// A new ed25519 key pair: the OpenSSH private key and its public line.
    async fn generate_deploy_key(
        &self,
        comment: &str,
    ) -> Result<(Zeroizing<Vec<u8>>, String), MmError>;
}
