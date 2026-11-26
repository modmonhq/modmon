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
    /// A root-owned file whose content every scrape has to send as
    /// `Authorization: Bearer <content>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_file: Option<PathBuf>,
    /// Also serve every project's metrics on the node's mesh address, to
    /// the other nodes of the cluster. Needs `token-file`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mesh: bool,
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
        }
    }
}

impl HostMetrics {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(path) = &self.token_file
            && !path.is_absolute()
        {
            return Err(format!(
                "metrics.token-file '{}' must be an absolute path, such as /etc/mm/metrics-token",
                path.display()
            ));
        }
        if self.mesh && self.token_file.is_none() {
            return Err("metrics.mesh serves every project's metrics to the other nodes and needs a token: set metrics.token-file to a root-owned file that holds it".to_owned());
        }
        Ok(())
    }
}
