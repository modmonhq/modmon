//! What `mmd state export` keeps of a host and `mmd state import` puts
//! back: the host's own records, which neither the repository nor the other
//! nodes can restore. That is the host policy with its hand edits, the
//! node's name, each environment's synced access, emergency blocks, and
//! deployment history, the addresses of the cluster networks the node
//! holds, and the audit log.
//!
//! Never a key, a secret value, or a passphrase: the daemon identity, the
//! mesh and deploy keys, secret stores, and host provider credentials stay
//! out, because only the files named here are written or read. A host
//! restored from an export gets new keys from `mm target setup`.

use serde::{Deserialize, Serialize};

/// The archive's layout version.
pub const STATE_FORMAT: u32 = 1;
/// The first entry of an archive: what it is and where it came from.
pub const MANIFEST: &str = "mm-state.json";
/// The host policy, from `host_policy_path` in `mmd.json`.
pub const HOST_POLICY: &str = "host.yaml";
/// The node's name and mesh address, from `/etc/mm/node.env`.
pub const NODE_ENV: &str = "node.env";
/// Files under the daemon's state directory, `/var/lib/mm`.
pub const STATE_PREFIX: &str = "state/";

/// The archive's first entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StateManifest {
    pub format: u32,
    /// The node's name, when the host is a cluster node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    pub mmd_version: String,
    pub exported_at_unix_ms: i64,
    /// Every other entry, in the archive's order.
    pub files: Vec<String>,
}

/// Whether a file under the daemon's state directory, by its path from
/// there with `/` between parts, belongs in an export.
#[must_use]
pub fn is_exported_state(relative: &str) -> bool {
    let parts = relative.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["audit.jsonl"] | ["networks", "networks.json"] => true,
        [rotated] => rotated.strip_prefix("audit.jsonl.").is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        }),
        ["projects", project, rest @ ..] => {
            uuid::Uuid::parse_str(project).is_ok() && is_project_record(rest)
        }
        _ => false,
    }
}

/// A project's records: blocks, and per environment its access, what it
/// runs, and its deployment history.
fn is_project_record(rest: &[&str]) -> bool {
    match rest {
        ["blocks.json"] => true,
        ["access" | "deployed" | "deployments", file] => {
            file.strip_suffix(".json").is_some_and(|name| {
                !name.is_empty()
                    && !name.starts_with('.')
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            })
        }
        _ => false,
    }
}

/// Whether an archive entry other than the manifest is one an import
/// takes.
#[must_use]
pub fn is_archive_entry(name: &str) -> bool {
    name == HOST_POLICY
        || name == NODE_ENV
        || name
            .strip_prefix(STATE_PREFIX)
            .is_some_and(is_exported_state)
}
