//! The update order of a service that sets none.
//!
//! A service that runs on more than one node starts its new container next
//! to the old one on a node, as with `update.order: start-first`, unless
//! something of the host keeps two of its containers from running side by
//! side there. Its old container then drains while the deployment goes on.

use serde_yaml_ng::Value;

/// What a Compose service definition takes of its host, so that a second
/// container of it cannot start next to the first on the same node.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostTies {
    /// `ports` publishes a port on the host.
    pub host_ports: bool,
    /// `container_name` fixes the container's name.
    pub container_name: bool,
    /// `network_mode`, `pid` or `ipc` is `host`.
    pub host_namespaces: bool,
    /// A volume or bind mount that is not read-only.
    pub writable_mounts: bool,
    /// `devices` hands it a device of the host.
    pub devices: bool,
    /// `scale` or `deploy.replicas` runs other than one container.
    pub scaled: bool,
}

impl HostTies {
    /// The ties of one service of a rendered or source Compose file.
    #[must_use]
    pub fn of(service: &Value) -> Self {
        let present = |key: &str| {
            service.get(key).is_some_and(|value| match value {
                Value::Null => false,
                Value::Sequence(items) => !items.is_empty(),
                Value::Mapping(entries) => !entries.is_empty(),
                Value::String(text) => !text.is_empty(),
                _ => true,
            })
        };
        let host = |key: &str| service.get(key).and_then(Value::as_str) == Some("host");
        Self {
            host_ports: present("ports"),
            container_name: present("container_name"),
            host_namespaces: host("network_mode") || host("pid") || host("ipc"),
            writable_mounts: service
                .get("volumes")
                .and_then(Value::as_sequence)
                .is_some_and(|mounts| mounts.iter().any(writable)),
            devices: present("devices"),
            scaled: service
                .get("scale")
                .or_else(|| {
                    service
                        .get("deploy")
                        .and_then(|deploy| deploy.get("replicas"))
                })
                .and_then(Value::as_u64)
                .is_some_and(|scale| scale != 1),
        }
    }

    #[must_use]
    pub const fn any(&self) -> bool {
        self.host_ports
            || self.container_name
            || self.host_namespaces
            || self.writable_mounts
            || self.devices
            || self.scaled
    }

    /// Both: what either of two definitions of the service takes.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self {
            host_ports: self.host_ports || other.host_ports,
            container_name: self.container_name || other.container_name,
            host_namespaces: self.host_namespaces || other.host_namespaces,
            writable_mounts: self.writable_mounts || other.writable_mounts,
            devices: self.devices || other.devices,
            scaled: self.scaled || other.scaled,
        }
    }
}

/// Whether a service that sets no `update.order`, placed on `nodes` nodes,
/// starts its new container before the old one stops.
#[must_use]
pub const fn starts_first_by_default(nodes: usize, ties: &HostTies) -> bool {
    nodes > 1 && !ties.any()
}

fn writable(mount: &Value) -> bool {
    match mount {
        Value::String(short) => {
            let mut parts = short.split(':');
            let target_only = parts.clone().count() == 1;
            !target_only
                && parts
                    .nth(2)
                    .is_none_or(|options| !options.split(',').any(|option| option == "ro"))
        }
        Value::Mapping(_) => {
            let read_only = mount.get("read_only").and_then(Value::as_bool) == Some(true);
            let kind = mount
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("volume");
            let shared = mount.get("source").is_some_and(|source| !source.is_null());
            !read_only && kind != "tmpfs" && shared
        }
        _ => true,
    }
}
