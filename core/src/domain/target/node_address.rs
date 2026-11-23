//! Where this device connects for a cluster node when the address the repo
//! records does not answer from here. Only the TCP destination changes: the
//! node's pinned host key and daemon identity stay as recorded.

use std::fmt;
use std::net::Ipv6Addr;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{TargetNode, validate_ssh_host};

/// `host` or `host:port`, IPv6 in brackets when it has a port. Without a
/// port, the node's recorded SSH port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeAddress {
    pub host: String,
    pub port: Option<u16>,
}

impl NodeAddress {
    /// The host and port to connect to for `node`.
    #[must_use]
    pub fn dial(&self, node: &TargetNode) -> (String, u16) {
        (self.host.clone(), self.port.unwrap_or(node.port))
    }
}

/// `host:port`, with an IPv6 host in brackets.
#[must_use]
pub fn host_and_port(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

impl FromStr for NodeAddress {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || {
            format!(
                "'{text}' is not an address; give a hostname, an IPv4 address, or an IPv6 address, with an optional port: `10.0.0.3`, `node-1.internal:2222`, `[fd00::3]:22`"
            )
        };
        let (host, port) = if let Some(rest) = text.strip_prefix('[') {
            let (host, after) = rest.split_once(']').ok_or_else(invalid)?;
            host.parse::<Ipv6Addr>().map_err(|_| invalid())?;
            match after {
                "" => (host, None),
                _ => (host, Some(after.strip_prefix(':').ok_or_else(invalid)?)),
            }
        } else if text.matches(':').count() > 1 {
            text.parse::<Ipv6Addr>().map_err(|_| invalid())?;
            (text, None)
        } else {
            match text.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (text, None),
            }
        };
        validate_ssh_host(host).map_err(|_| invalid())?;
        let port = port
            .map(|port| match port.parse::<u16>() {
                Ok(number) if number > 0 && port.bytes().all(|byte| byte.is_ascii_digit()) => {
                    Ok(number)
                }
                _ => Err(format!(
                    "'{text}' has no valid port; a port is a number from 1 to 65535"
                )),
            })
            .transpose()?;
        Ok(Self {
            host: host.to_owned(),
            port,
        })
    }
}

impl fmt::Display for NodeAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.port {
            Some(port) => formatter.write_str(&host_and_port(&self.host, port)),
            None => formatter.write_str(&self.host),
        }
    }
}

impl Serialize for NodeAddress {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for NodeAddress {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// One node's address on this device, with the target it was set for.
/// Keys this release does not know are kept aside for a warning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceNodeAddress {
    pub address: NodeAddress,
    pub target: String,
    #[serde(flatten, skip_serializing)]
    pub unknown: std::collections::BTreeMap<String, serde_json::Value>,
}

impl DeviceNodeAddress {
    #[must_use]
    pub fn new(address: NodeAddress, target: String) -> Self {
        Self {
            address,
            target,
            unknown: std::collections::BTreeMap::new(),
        }
    }
}
