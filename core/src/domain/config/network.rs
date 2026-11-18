//! A network of `mm.yaml`.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NetworkConfig {
    /// Containers on the network reach its members and nothing else: no
    /// outbound access.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub internal: bool,
    /// Never read from a file. `mmd` on a cluster node sets it to the file
    /// on the node that names the node's resolver for this network; the
    /// containers of the network's services get it as `/etc/resolv.conf`
    /// (see `domain::placement::view_from_node`).
    #[serde(skip)]
    #[schemars(skip)]
    pub resolver: Option<PathBuf>,
    /// Never read from a file. `mmd` on a cluster node sets it on the
    /// network the ingress proxy and the routed services share: `true` on
    /// a node that runs the proxy, which makes the proxy one of the
    /// network's members there.
    #[serde(skip)]
    #[schemars(skip)]
    pub cluster_ingress: Option<bool>,
}
