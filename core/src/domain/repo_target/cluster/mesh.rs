//! The private network between the nodes of a cluster target.

use std::net::Ipv4Addr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Ipv4Cidr, is_private};

pub use crate::domain::mesh::MESH_INTERFACE;

const DEFAULT_SUBNET: Ipv4Cidr = Ipv4Cidr {
    address: Ipv4Addr::new(10, 90, 0, 0),
    prefix: 24,
};
const DEFAULT_PORT: u16 = 51820;
const DEFAULT_MTU: u16 = 1420;
/// IPv6 needs 1280; jumbo frames end at 9000.
const MTU_RANGE: std::ops::RangeInclusive<u16> = 1280..=9000;

/// In YAML this is a map with either `interface:` or any of `subnet:`,
/// `port:`, and `mtu:`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "MeshFields", into = "MeshFields")]
pub enum MeshConfig {
    /// A WireGuard mesh setup creates between every pair of nodes.
    Wireguard(WireguardMesh),
    /// An existing private interface, such as a provider's VLAN NIC.
    Interface(String),
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self::Wireguard(WireguardMesh::default())
    }
}

impl MeshConfig {
    /// The interface the nodes reach each other on.
    #[must_use]
    pub fn interface(&self) -> &str {
        match self {
            Self::Wireguard(_) => MESH_INTERFACE,
            Self::Interface(interface) => interface,
        }
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireguardMesh {
    /// Holds every node's private address.
    pub subnet: Ipv4Cidr,
    /// The UDP port every node listens on.
    pub port: u16,
    pub mtu: u16,
}

impl Default for WireguardMesh {
    fn default() -> Self {
        Self {
            subnet: DEFAULT_SUBNET,
            port: DEFAULT_PORT,
            mtu: DEFAULT_MTU,
        }
    }
}

/// The YAML shape of [`MeshConfig`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MeshFields {
    /// The WireGuard mesh's address range. Default `10.90.0.0/24`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subnet: Option<Ipv4Cidr>,
    /// The UDP port WireGuard listens on, on every node. It must be reachable
    /// between the nodes. Default `51820`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    /// The mesh interface's MTU: the underlay's MTU minus 80 bytes of
    /// WireGuard overhead. Default `1420`, which fits a 1500-byte underlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mtu: Option<u16>,
    /// Use this existing private interface (`eth1`) instead of creating a
    /// WireGuard mesh. Every node then declares its `private-address`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    interface: Option<String>,
}

impl TryFrom<MeshFields> for MeshConfig {
    type Error = String;

    fn try_from(fields: MeshFields) -> Result<Self, Self::Error> {
        let defaults = WireguardMesh::default();
        match fields {
            MeshFields {
                interface: Some(interface),
                subnet: None,
                port: None,
                mtu: None,
            } => Ok(Self::Interface(interface)),
            MeshFields {
                interface: Some(_), ..
            } => Err(
                "mesh takes either interface: (an existing private network) or subnet:, port:, and mtu: (a WireGuard mesh mm creates), not both"
                    .to_owned(),
            ),
            MeshFields {
                subnet, port, mtu, ..
            } => Ok(Self::Wireguard(WireguardMesh {
                subnet: subnet.unwrap_or(defaults.subnet),
                port: port.unwrap_or(defaults.port),
                mtu: mtu.unwrap_or(defaults.mtu),
            })),
        }
    }
}

impl From<MeshConfig> for MeshFields {
    fn from(config: MeshConfig) -> Self {
        let defaults = WireguardMesh::default();
        match config {
            MeshConfig::Wireguard(wireguard) => Self {
                subnet: (wireguard.subnet != defaults.subnet).then_some(wireguard.subnet),
                port: (wireguard.port != defaults.port).then_some(wireguard.port),
                mtu: (wireguard.mtu != defaults.mtu).then_some(wireguard.mtu),
                interface: None,
            },
            MeshConfig::Interface(interface) => Self {
                interface: Some(interface),
                ..Self::default()
            },
        }
    }
}

impl JsonSchema for MeshConfig {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        MeshFields::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        MeshFields::json_schema(generator)
    }
}

pub(super) fn mesh_problem(mesh: &MeshConfig) -> Result<(), String> {
    match mesh {
        MeshConfig::Wireguard(wireguard) => {
            if !is_private(wireguard.subnet.address) {
                return Err(format!(
                    "cluster.mesh.subnet {} is not a private range",
                    wireguard.subnet
                ));
            }
            if wireguard.subnet.prefix > 30 {
                return Err(format!(
                    "cluster.mesh.subnet {} is too small; use a prefix of /30 or shorter",
                    wireguard.subnet
                ));
            }
            if wireguard.port == 0 {
                return Err("cluster.mesh.port must be between 1 and 65535".to_owned());
            }
            if !MTU_RANGE.contains(&wireguard.mtu) {
                return Err(format!(
                    "cluster.mesh.mtu {} must be between {} and {}",
                    wireguard.mtu,
                    MTU_RANGE.start(),
                    MTU_RANGE.end()
                ));
            }
            Ok(())
        }
        MeshConfig::Interface(interface) => {
            if interface.is_empty()
                || interface.len() > 15
                || !interface
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            {
                return Err(format!(
                    "cluster.mesh.interface '{interface}' is not a valid interface name"
                ));
            }
            Ok(())
        }
    }
}

/// A node's `endpoint:` is where its WireGuard peers send to, so it only
/// means something on a mesh mm creates.
pub(super) fn endpoint_problem(mesh: &MeshConfig, endpoint: Option<&str>) -> Result<(), String> {
    let Some(endpoint) = endpoint else {
        return Ok(());
    };
    if let MeshConfig::Interface(interface) = mesh {
        return Err(format!(
            "sets endpoint, which is where WireGuard peers send to; with cluster.mesh.interface {interface} mm creates no WireGuard mesh, so remove endpoint"
        ));
    }
    // A colon belongs to an IPv6 address; `host:port` is not an endpoint.
    let valid = if endpoint.contains(':') {
        endpoint.parse::<std::net::Ipv6Addr>().is_ok()
    } else {
        !endpoint.is_empty()
            && endpoint.len() <= 253
            && !endpoint.starts_with('-')
            && endpoint
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "endpoint '{endpoint}' must be a host name or an address, without a port; the port is cluster.mesh.port"
        ))
    }
}
