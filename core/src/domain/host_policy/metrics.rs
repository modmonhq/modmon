//! What a host's admin says about the metrics `mmd` serves
//! (`domain::node_metrics`).

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct HostMetrics {
    /// `false` serves no metrics on this host, whatever a project's
    /// `networks.<name>.metrics` asks for.
    #[serde(default = "serves", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// A root-owned file whose content every scrape on a project's
    /// network has to send as `Authorization: Bearer <content>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_file: Option<PathBuf>,
    /// Also serve every project's metrics on the node's mesh address, to
    /// the other nodes of the cluster. Needs `mesh-token-file`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mesh: bool,
    /// A root-owned file whose content every scrape on the mesh address
    /// has to send: a token of its own, which no project holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh_token_file: Option<PathBuf>,
}

const fn serves() -> bool {
    true
}

const fn is_true(value: &bool) -> bool {
    *value
}

impl Default for HostMetrics {
    fn default() -> Self {
        Self {
            enabled: true,
            token_file: None,
            mesh: false,
            mesh_token_file: None,
        }
    }
}

impl HostMetrics {
    pub fn validate(&self) -> Result<(), String> {
        for (key, path) in [
            ("token-file", &self.token_file),
            ("mesh-token-file", &self.mesh_token_file),
        ] {
            if let Some(path) = path
                && !path.is_absolute()
            {
                return Err(format!(
                    "metrics.{key} '{}' must be an absolute path, such as /etc/mm/metrics-token",
                    path.display()
                ));
            }
        }
        if self.mesh && self.mesh_token_file.is_none() {
            return Err("metrics.mesh serves every project's metrics to the other nodes and needs a token of its own: set metrics.mesh-token-file to a root-owned file that holds it".to_owned());
        }
        if self.mesh_token_file.is_some() && self.mesh_token_file == self.token_file {
            return Err("metrics.mesh-token-file must be another file than metrics.token-file: the projects hold that token, and the mesh serves every project's metrics".to_owned());
        }
        Ok(())
    }
}
