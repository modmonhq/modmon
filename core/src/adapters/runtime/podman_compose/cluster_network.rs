// A cluster network on a node that runs rootful Podman: a netavark bridge
// network whose subnet is the network's block and whose containers get
// addresses from the node's slice, as on Docker (see
// docker_compose/cluster_network.rs). What differs, all of it here:
//
// - netavark takes the range as first and last address, so the slice's own
//   first and last address and the gateway are left out by the range
//   itself; it has no auxiliary addresses.
// - Its DNS server (aardvark-dns) is off: the node's mmd answers names on
//   the gateway's port 53.
// - netavark masquerades only what leaves the network's subnet, which is
//   the whole block, so traffic between nodes keeps the container's
//   address with no option; mm's nat chain covers the way out as on
//   Docker. netavark accepts everything from inside the subnet, so there is
//   no interface to trust.
// - netavark creates the bridge when the network's first container starts
//   and removes it with the last one; the node's mmd gives a bridge that
//   came back its settings (`ClusterNetworkHost::settle_bridges`).

const NO_DEFAULT_ROUTE_OPTION: &str = "no_default_route";

/// The first and last address containers on `network` get: the slice
/// without its first address, the gateway after it, and its last.
fn podman_lease_range(
    network: &crate::domain::cluster_network::LocalNetwork,
) -> (std::net::Ipv4Addr, std::net::Ipv4Addr) {
    let first = u32::from(network.slice.address);
    let last = u32::from(network.slice.last_address());
    (
        std::net::Ipv4Addr::from(first.saturating_add(2)),
        std::net::Ipv4Addr::from(last.saturating_sub(1)),
    )
}

/// The addressing of a network as `podman network inspect` prints it:
/// `10.88.0.0/16`, or `10.64.0.0/20, range 10.64.1.2-10.64.1.254`.
fn podman_network_addresses(inspected: &Value) -> String {
    let addresses = inspected["subnets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|subnet| {
            let block = subnet["subnet"].as_str()?;
            let range = &subnet["lease_range"];
            Some(
                match (range["start_ip"].as_str(), range["end_ip"].as_str()) {
                    (Some(start), Some(end)) => format!("{block}, range {start}-{end}"),
                    _ => block.to_owned(),
                },
            )
        })
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        "no IPv4 addresses".to_owned()
    } else {
        addresses.join(" and ")
    }
}

fn has_podman_default_route(inspected: &Value) -> bool {
    inspected["options"][NO_DEFAULT_ROUTE_OPTION].as_str() != Some("true")
}

/// The addressing of an inspected network, and whether it carries a
/// default route when `wanted` calls for the other.
fn observed_podman_addresses(
    inspected: &Value,
    wanted: Option<&crate::domain::cluster_network::LocalNetwork>,
) -> String {
    let addresses = podman_network_addresses(inspected);
    let routed = has_podman_default_route(inspected);
    match wanted {
        Some(wanted) if wanted.internal == routed => {
            let route = if routed { "with" } else { "without" };
            format!("{addresses}, {route} a default route")
        }
        _ => addresses,
    }
}

/// Whether an inspected network is the bridge network `wanted` calls for.
fn is_podman_cluster_network(
    inspected: &Value,
    wanted: &crate::domain::cluster_network::LocalNetwork,
) -> bool {
    let (start, end) = podman_lease_range(wanted);
    let addressing = inspected["subnets"].as_array().is_some_and(|subnets| {
        subnets.len() == 1
            && subnets.first().is_some_and(|subnet| {
                subnet["subnet"].as_str() == Some(wanted.block.to_string().as_str())
                    && subnet["gateway"].as_str() == Some(wanted.gateway.to_string().as_str())
                    && subnet["lease_range"]["start_ip"].as_str() == Some(start.to_string().as_str())
                    && subnet["lease_range"]["end_ip"].as_str() == Some(end.to_string().as_str())
            })
    });
    addressing
        && inspected["driver"].as_str() == Some("bridge")
        && inspected["network_interface"].as_str() == Some(wanted.bridge.as_str())
        && inspected["dns_enabled"].as_bool() == Some(false)
        && inspected["internal"].as_bool() != Some(true)
        && has_podman_default_route(inspected) != wanted.internal
        && inspected["options"]["mtu"].as_str() == Some(wanted.mtu.to_string().as_str())
}

impl PodmanComposeRuntime {
    /// The network `name` as `podman network inspect` prints it, or `None`
    /// when Podman has no network of that name.
    async fn inspected_network(&self, name: &str) -> Result<Option<Value>, MmError> {
        validate_resource_name(name)?;
        let exists = self
            .local_command(
                vec!["network".to_owned(), "exists".to_owned(), name.to_owned()],
                Duration::from_secs(30),
            )
            .await?;
        match exists.code {
            Some(0) => {}
            Some(1) => return Ok(None),
            _ => {
                self.ensure_success(exists)?;
                return Ok(None);
            }
        }
        let output = self
            .local_command(
                vec![
                    "network".to_owned(),
                    "inspect".to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                    name.to_owned(),
                ],
                Duration::from_secs(30),
            )
            .await?;
        let output = self.ensure_success(output)?;
        let parsed: Value =
            serde_json::from_str(output.stdout.trim()).map_err(|_| MmError::RuntimeResponse {
                message: "Podman network inspect returned invalid JSON".to_owned(),
            })?;
        Ok(match parsed {
            Value::Array(mut networks) if !networks.is_empty() => Some(networks.swap_remove(0)),
            Value::Array(_) => None,
            other => Some(other),
        })
    }

    async fn observed_podman_network(
        &self,
        name: &str,
        wanted: Option<&crate::domain::cluster_network::LocalNetwork>,
    ) -> Result<Option<crate::domain::cluster_network::ObservedNetwork>, MmError> {
        Ok(self.inspected_network(name).await?.map(|inspected| {
            crate::domain::cluster_network::ObservedNetwork {
                addresses: observed_podman_addresses(&inspected, wanted),
                current: wanted.is_some_and(|wanted| is_podman_cluster_network(&inspected, wanted)),
            }
        }))
    }

    /// Every network of a node, a few at a time.
    async fn observed_podman_networks(
        &self,
        wanted: &[crate::domain::cluster_network::LocalNetwork],
    ) -> Result<Vec<Option<crate::domain::cluster_network::ObservedNetwork>>, MmError> {
        let mut observed = Vec::with_capacity(wanted.len());
        for few in wanted.chunks(4) {
            let looked = futures_util::future::join_all(
                few.iter()
                    .map(|network| self.observed_podman_network(&network.network, Some(network))),
            )
            .await;
            for network in looked {
                observed.push(network?);
            }
        }
        Ok(observed)
    }

    /// Removes the network and every container on it, running or not: a
    /// container keeps the network it was created on.
    async fn remove_podman_network_with_containers(&self, name: &str) -> Result<(), MmError> {
        let listed = self
            .local_command(
                vec![
                    "ps".to_owned(),
                    "--all".to_owned(),
                    "--quiet".to_owned(),
                    "--filter".to_owned(),
                    format!("network={name}"),
                ],
                Duration::from_secs(30),
            )
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
            let output = self.local_command(remove, Duration::from_secs(120)).await?;
            self.ensure_success(output)?;
        }
        let output = self
            .local_command(
                vec!["network".to_owned(), "rm".to_owned(), name.to_owned()],
                Duration::from_secs(30),
            )
            .await?;
        self.ensure_success(output)?;
        Ok(())
    }

    async fn ensured_podman_cluster_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
        network: &crate::domain::cluster_network::LocalNetwork,
    ) -> Result<ResourceEnsureResult, MmError> {
        let target_id = require_local_named_target(target)?;
        self.require_local_engine_environment()?;
        validate_resource_label(resource)?;
        if let Some(existing) = self.inspected_network(name).await? {
            self.inspect_owned_resource(PodmanResourceKind::Network, name, target_id, resource)
                .await?;
            if is_podman_cluster_network(&existing, network) {
                return Ok(ResourceEnsureResult::Unchanged);
            }
            self.remove_podman_network_with_containers(name).await?;
        }
        let ownership = ResourceOwnership {
            project_id: None,
            environment: None,
            target_id,
            kind: ResourceKind::Network,
            logical_name: name.to_owned(),
            resource: resource.to_owned(),
            generation: crate::domain::ownership::OWNERSHIP_GENERATION,
        };
        let (start, end) = podman_lease_range(network);
        let mut args = vec![
            "network".to_owned(),
            "create".to_owned(),
            "--driver".to_owned(),
            "bridge".to_owned(),
            "--subnet".to_owned(),
            network.block.to_string(),
            "--ip-range".to_owned(),
            format!("{start}-{end}"),
            "--gateway".to_owned(),
            network.gateway.to_string(),
            "--interface-name".to_owned(),
            network.bridge.clone(),
            "--disable-dns".to_owned(),
            "--opt".to_owned(),
            format!("mtu={}", network.mtu),
        ];
        if network.internal {
            args.extend(["--opt".to_owned(), format!("{NO_DEFAULT_ROUTE_OPTION}=true")]);
        }
        args.extend(ownership_label_args(&ownership));
        args.push(name.to_owned());
        let output = self.local_command(args, Duration::from_secs(30)).await?;
        self.ensure_success(output)?;
        Ok(ResourceEnsureResult::Created)
    }
}
