//! The nodes of a cluster target. Each node is one host running `mmd`. mm
//! addresses every request to one node by name and never sends it to
//! another: there is no failover between nodes.

use std::net::Ipv4Addr;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    TargetTransport, default_ssh_port, hash_field, hash_optional_path, validate_ssh_host,
    validate_ssh_host_key, validate_ssh_host_key_fingerprint, validate_ssh_user,
};

/// One node of a cluster target, in the order the target declares them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct TargetNode {
    pub name: String,
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub user: String,
    /// The node's address on the cluster's private network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_address: Option<Ipv4Addr>,
    /// The node's SSH public key, `<type> <base64>`. `None` until setup
    /// recorded it; mm cannot connect to the node before that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_key: Option<String>,
    /// OpenSSH `SHA256:` fingerprint of `host_key`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_key_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<PathBuf>,
}

impl TargetNode {
    /// `host:port`, as error messages and `mm target nodes` show it.
    #[must_use]
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Whether setup recorded the keys mm needs to connect.
    #[must_use]
    pub const fn is_recorded(&self) -> bool {
        self.host_key.is_some()
            && self.host_key_fingerprint.is_some()
            && self.daemon_fingerprint.is_some()
    }

    /// The node as an SSH transport, or `None` before its keys are recorded.
    #[must_use]
    pub fn transport(&self) -> Option<TargetTransport> {
        Some(TargetTransport::Ssh {
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            host_key_fingerprint: self.host_key_fingerprint.clone()?,
            host_key: Some(self.host_key.clone()?),
            identity_file: self.identity_file.clone(),
            remote_root: None,
            daemon_fingerprint: Some(self.daemon_fingerprint.clone()?),
        })
    }
}

pub(super) fn validate_nodes(
    transport: &TargetTransport,
    nodes: &[TargetNode],
) -> Result<(), String> {
    if nodes.is_empty() {
        return Ok(());
    }
    if !matches!(transport, TargetTransport::Ssh { .. }) {
        return Err("only SSH targets can list nodes".to_owned());
    }
    let mut names = std::collections::BTreeSet::new();
    for node in nodes {
        super::validate_target_name(&node.name)
            .map_err(|message| format!("node '{}': {message}", node.name))?;
        if !names.insert(node.name.as_str()) {
            return Err(format!("node '{}' is listed twice", node.name));
        }
        let context = |message: String| format!("node '{}': {message}", node.name);
        if node.port == 0 {
            return Err(context("SSH port must be greater than zero".to_owned()));
        }
        validate_ssh_host(&node.host).map_err(context)?;
        validate_ssh_user(&node.user).map_err(context)?;
        match (&node.host_key, &node.host_key_fingerprint) {
            (Some(key), Some(fingerprint)) => {
                validate_ssh_host_key_fingerprint(fingerprint).map_err(context)?;
                validate_ssh_host_key(key, fingerprint).map_err(context)?;
            }
            (None, None) => {}
            _ => {
                return Err(context(
                    "host key and its fingerprint must be recorded together".to_owned(),
                ));
            }
        }
        if node.daemon_fingerprint.as_ref().is_some_and(|fingerprint| {
            fingerprint.len() != 64
                || !fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        }) {
            return Err(context(
                "daemon fingerprint must be the 64-character value from `mmd --print-identity`"
                    .to_owned(),
            ));
        }
        if node
            .identity_file
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            return Err(context("identity file must be an absolute path".to_owned()));
        }
    }
    Ok(())
}

/// The node list is part of the target's identity: a plan reviewed before a
/// node was added, removed, renamed, moved, or re-keyed is rejected. No nodes
/// hashes nothing, so single-host targets keep their fingerprints.
pub(super) fn hash_nodes(hasher: &mut blake3::Hasher, nodes: &[TargetNode]) {
    if nodes.is_empty() {
        return;
    }
    hash_field(hasher, "nodes-v1");
    hasher.update(&(nodes.len() as u64).to_le_bytes());
    for node in nodes {
        hash_field(hasher, &node.name);
        hash_field(hasher, &node.host);
        hash_field(hasher, &node.port.to_string());
        hash_field(hasher, &node.user);
        hash_optional_path(
            hasher,
            "private-address",
            node.private_address
                .map(|address| address.to_string())
                .as_deref(),
        );
        hash_optional_path(hasher, "host-key", node.host_key.as_deref());
        hash_optional_path(
            hasher,
            "host-key-fingerprint",
            node.host_key_fingerprint.as_deref(),
        );
        hash_optional_path(
            hasher,
            "identity-file",
            node.identity_file.as_ref().and_then(|path| path.to_str()),
        );
        hash_optional_path(
            hasher,
            "daemon-fingerprint",
            node.daemon_fingerprint.as_deref(),
        );
    }
}
