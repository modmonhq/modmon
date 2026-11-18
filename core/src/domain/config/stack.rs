//! A stack of `mm.yaml`.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::IngressConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StackConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    pub files: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<String>,
    #[serde(default, skip_serializing_if = "IngressConfig::is_empty")]
    pub ingress: IngressConfig,
    /// Top-level Compose volumes of this stack that are one directory on
    /// every node of a cluster target: the target's storage node keeps the
    /// files and the other nodes mount them over the mesh. For files that
    /// replicas share, such as uploads. Not for databases: a shared volume
    /// is slower than local disk and does not give the locking guarantees a
    /// database needs. On a target that names one host they are ordinary
    /// volumes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_volumes: Vec<String>,
    /// Stacks a deployment rolls out before this one. Without it, stacks
    /// roll out in name order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    /// Never read from a file. `mmd` on a cluster node sets it to where
    /// that node finds the cluster's shared volumes (see
    /// `domain::placement::view_from_node`).
    #[serde(skip)]
    #[schemars(skip)]
    pub storage: Option<crate::domain::shared_storage::StorageAccess>,
}
