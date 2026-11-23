// A cluster network on one node: an ordinary bridge network whose subnet is
// the network's block and whose containers get addresses from the node's
// slice.
//
// Docker Engine drops whatever another host routes to a container address,
// and rewrites the source of everything a container sends off its bridge.
// Three driver options make the network work with mm's rules instead:
// masquerading is off, so traffic between nodes keeps the container's
// address and mm's own nat chain covers the way out; the mesh interface is
// trusted, which lifts the engine's drop for packets that arrive through it
// (mm's filter chain then accepts only the same network's); and the bridge
// has a fixed name the rules can refer to before the engine creates it.
//
// The engine hands out the first free address of the range, which without
// more is the slice's own first address (`10.64.1.0`). That address and the
// slice's last are named as auxiliary addresses, so no container gets one.

const BRIDGE_NAME_OPTION: &str = "com.docker.network.bridge.name";
const BRIDGE_MTU_OPTION: &str = "com.docker.network.driver.mtu";
const BRIDGE_MASQUERADE_OPTION: &str = "com.docker.network.bridge.enable_ip_masquerade";
const BRIDGE_TRUSTED_OPTION: &str = "com.docker.network.bridge.trusted_host_interfaces";
const AUXILIARY_ADDRESSES: &str = "AuxiliaryAddresses";

/// The driver options of a cluster network, in the order they are passed.
fn cluster_network_options(
    network: &crate::domain::cluster_network::LocalNetwork,
) -> [(&'static str, String); 4] {
    [
        (BRIDGE_NAME_OPTION, network.bridge.clone()),
        (BRIDGE_MTU_OPTION, network.mtu.to_string()),
        (BRIDGE_MASQUERADE_OPTION, "false".to_owned()),
        (BRIDGE_TRUSTED_OPTION, network.mesh_interface.clone()),
    ]
}

/// The addressing of an inspected network: `172.18.0.0/16`, or
/// `10.64.0.0/20, range 10.64.1.0/24`.
fn cluster_network_addresses(inspected: &serde_json::Value) -> String {
    let addresses = inspected["IPAM"]["Config"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|config| {
            let subnet = config["Subnet"].as_str()?;
            Some(match config["IPRange"].as_str().filter(|range| !range.is_empty()) {
                Some(range) => format!("{subnet}, range {range}"),
                None => subnet.to_owned(),
            })
        })
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        "no IPv4 addresses".to_owned()
    } else {
        addresses.join(" and ")
    }
}

fn is_docker_internal(inspected: &serde_json::Value) -> bool {
    inspected["Internal"].as_bool() == Some(true)
}

/// The addressing of an inspected network, and its default route when
/// `wanted` is internal and should have none.
fn observed_cluster_addresses(
    inspected: &serde_json::Value,
    wanted: Option<&crate::domain::cluster_network::LocalNetwork>,
) -> String {
    let addresses = cluster_network_addresses(inspected);
    if wanted.is_some_and(|wanted| wanted.internal) && !is_docker_internal(inspected) {
        format!("{addresses}, with a default route")
    } else {
        addresses
    }
}

/// Whether an inspected network is the bridge network `wanted` calls for.
fn is_cluster_network(
    inspected: &serde_json::Value,
    wanted: &crate::domain::cluster_network::LocalNetwork,
) -> bool {
    let configs = inspected["IPAM"]["Config"].as_array();
    let reserved = wanted.reserved();
    let addressing = configs.is_some_and(|configs| {
        configs.len() == 1
            && configs.first().is_some_and(|config| {
                let kept_back = &config[AUXILIARY_ADDRESSES];
                config["Subnet"].as_str() == Some(wanted.block.to_string().as_str())
                    && config["IPRange"].as_str() == Some(wanted.slice.to_string().as_str())
                    && config["Gateway"].as_str() == Some(wanted.gateway.to_string().as_str())
                    && kept_back.as_object().map_or(0, serde_json::Map::len) == reserved.len()
                    && reserved.iter().all(|(name, address)| {
                        kept_back[*name].as_str() == Some(address.to_string().as_str())
                    })
            })
    });
    addressing
        && inspected["Driver"].as_str() == Some("bridge")
        && is_docker_internal(inspected) == wanted.internal
        && cluster_network_options(wanted)
            .iter()
            .all(|(option, value)| inspected["Options"][option].as_str() == Some(value.as_str()))
}

impl DockerComposeRuntime {
    /// mm's rules enter the engine's packet filter through `DOCKER-USER`,
    /// which only the iptables backend has.
    async fn require_iptables_firewall(&self) -> Result<(), MmError> {
        let backend = match &self.engine {
            Some(engine) => engine.json("/info").await?["FirewallBackend"]["Driver"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            None => {
                let output = self
                    .command(vec![
                        "info".to_owned(),
                        "--format".to_owned(),
                        "{{.FirewallBackend.Driver}}".to_owned(),
                    ])
                    .await?;
                self.ensure_success(output)?.stdout.trim().to_owned()
            }
        };
        let backend = backend.as_str();
        // An engine too old to report a backend keeps its rules in iptables.
        if backend.is_empty() || backend == "<no value>" || backend.starts_with("iptables") {
            return Ok(());
        }
        Err(MmError::Target {
            message: format!(
                "Docker on this node keeps its firewall in {backend}; networks that span cluster nodes need its iptables backend, where mm's rules enter through DOCKER-USER. Remove \"firewall-backend\" from /etc/docker/daemon.json, run `systemctl restart docker`, and plan again"
            ),
        })
    }

    async fn inspect_network_by_name(&self, name: &str) -> Result<Option<serde_json::Value>, MmError> {
        validate_resource_name(name)?;
        if let Some(engine) = &self.engine {
            return engine.json_if_found(&format!("/networks/{name}")).await;
        }
        let output = self
            .command(vec![
                "network".to_owned(),
                "inspect".to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
                name.to_owned(),
            ])
            .await?;
        if !output.success() {
            if is_missing_resource(&output) {
                return Ok(None);
            }
            self.ensure_success(output)?;
            return Ok(None);
        }
        let parsed: serde_json::Value =
            serde_json::from_str(output.stdout.trim()).map_err(|_| MmError::RuntimeResponse {
                message: "Docker network inspect returned invalid JSON".to_owned(),
            })?;
        Ok(match parsed {
            serde_json::Value::Array(mut networks) if !networks.is_empty() => {
                Some(networks.swap_remove(0))
            }
            serde_json::Value::Array(_) => None,
            other => Some(other),
        })
    }

    async fn observed_cluster_network(
        &self,
        name: &str,
        wanted: Option<&crate::domain::cluster_network::LocalNetwork>,
    ) -> Result<Option<crate::domain::cluster_network::ObservedNetwork>, MmError> {
        self.require_iptables_firewall().await?;
        self.observed_network(name, wanted).await
    }

    async fn observed_network(
        &self,
        name: &str,
        wanted: Option<&crate::domain::cluster_network::LocalNetwork>,
    ) -> Result<Option<crate::domain::cluster_network::ObservedNetwork>, MmError> {
        Ok(self.inspect_network_by_name(name).await?.map(|inspected| {
            crate::domain::cluster_network::ObservedNetwork {
                addresses: observed_cluster_addresses(&inspected, wanted),
                current: wanted.is_some_and(|wanted| is_cluster_network(&inspected, wanted)),
            }
        }))
    }

    /// Every network of a node: the firewall is checked once, and the
    /// networks are looked at a few at a time.
    async fn observed_cluster_networks(
        &self,
        wanted: &[crate::domain::cluster_network::LocalNetwork],
    ) -> Result<Vec<Option<crate::domain::cluster_network::ObservedNetwork>>, MmError> {
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        self.require_iptables_firewall().await?;
        let mut observed = Vec::with_capacity(wanted.len());
        for few in wanted.chunks(4) {
            let looked = futures_util::future::join_all(
                few.iter()
                    .map(|network| self.observed_network(&network.network, Some(network))),
            )
            .await;
            for network in looked {
                observed.push(network?);
            }
        }
        Ok(observed)
    }

    /// Removes the network and every container on it, running or not: a
    /// container keeps the identity of the network it was created on, so it
    /// cannot move to the network's replacement.
    async fn remove_network_with_containers(&self, name: &str) -> Result<(), MmError> {
        let listed = self
            .command(vec![
                "ps".to_owned(),
                "--all".to_owned(),
                "--quiet".to_owned(),
                "--filter".to_owned(),
                format!("network={name}"),
            ])
            .await?;
        let listed = self.ensure_success(listed)?;
        let containers = listed
            .stdout
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if !containers.is_empty() {
            let mut remove = vec!["rm".to_owned(), "--force".to_owned()];
            remove.extend(containers);
            let output = self
                .process
                .run(self.command_spec(remove).timeout(Duration::from_secs(120)))
                .await?;
            self.ensure_success(output)?;
        }
        let output = self
            .command(vec!["network".to_owned(), "rm".to_owned(), name.to_owned()])
            .await?;
        self.ensure_success(output)?;
        Ok(())
    }

    async fn ensured_cluster_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
        network: &crate::domain::cluster_network::LocalNetwork,
    ) -> Result<ResourceEnsureResult, MmError> {
        let ownership = Self::ownership(target, ResourceKind::Network, name, resource)?;
        self.require_iptables_firewall().await?;
        if let Some(existing) = self.inspect_owned(&ownership).await? {
            if is_cluster_network(&existing, network) {
                return Ok(ResourceEnsureResult::Unchanged);
            }
            self.remove_network_with_containers(name).await?;
        }
        let mut args = vec![
            "network".to_owned(),
            "create".to_owned(),
            "--driver".to_owned(),
            "bridge".to_owned(),
            "--subnet".to_owned(),
            network.block.to_string(),
            "--ip-range".to_owned(),
            network.slice.to_string(),
            "--gateway".to_owned(),
            network.gateway.to_string(),
        ];
        if network.internal {
            args.push("--internal".to_owned());
        }
        for (name, address) in network.reserved() {
            args.extend(["--aux-address".to_owned(), format!("{name}={address}")]);
        }
        for (option, value) in cluster_network_options(network) {
            args.extend(["--opt".to_owned(), format!("{option}={value}")]);
        }
        args.extend(ownership_label_args(&ownership));
        args.push(name.to_owned());
        let output = self.command(args).await?;
        self.ensure_success(output)?;
        Ok(ResourceEnsureResult::Created)
    }

    async fn ensured_internal_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        let ownership = Self::ownership(target, ResourceKind::Network, name, resource)?;
        if let Some(existing) = self.inspect_owned(&ownership).await? {
            if existing["Internal"].as_bool() == Some(true) {
                return Ok(ResourceEnsureResult::Unchanged);
            }
            return Err(MmError::Target {
                message: format!(
                    "network '{name}' exists and is not internal; `internal: true` takes effect when the network is created. Stop the services on it, remove it with `docker network rm {name}`, then retry"
                ),
            });
        }
        let mut args = vec![
            "network".to_owned(),
            "create".to_owned(),
            "--internal".to_owned(),
        ];
        args.extend(ownership_label_args(&ownership));
        args.push(name.to_owned());
        let output = self.command(args).await?;
        self.ensure_success(output)?;
        Ok(ResourceEnsureResult::Created)
    }
}
