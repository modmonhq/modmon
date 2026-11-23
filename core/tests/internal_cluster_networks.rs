//! An internal network that spans cluster nodes never carries a
//! container's default route, on rootful Podman and on Docker Engine, one
//! an earlier mm created with a default route is created again, and its
//! containers on different nodes answer each other.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use mm::adapters::runtime::{DockerComposeRuntime, PodmanComposeRuntime};
use mm::app::ports::{
    CommandOutput, CommandSpec, LocalDeploymentRuntime, ProcessRunner, RuntimeDriver,
};
use mm::domain::cluster_network::{LocalNetwork, NetworkNeed, NetworkTable, SealedNetworks};
use mm::domain::error::MmError;
use mm::domain::ownership::{ResourceKind, ResourceOwnership};
use mm::domain::repo_target::Ipv4Cidr;
use mm::domain::runtime::ResourceEnsureResult;
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId};
use zeroize::Zeroizing;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Default)]
struct Engine {
    answers: Mutex<VecDeque<(i32, String)>>,
    commands: Mutex<Vec<String>>,
}

impl Engine {
    fn answering(answers: Vec<(i32, String)>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.into()),
            commands: Mutex::default(),
        })
    }

    fn commands(&self) -> Result<Vec<String>, String> {
        Ok(self.commands.lock().map_err(|_| "poisoned")?.clone())
    }
}

#[async_trait::async_trait]
impl ProcessRunner for Engine {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let poisoned = || MmError::State {
            message: "the test engine's lock was poisoned".to_owned(),
        };
        let record = specification.redacted_record();
        self.commands.lock().map_err(|_| poisoned())?.push(format!(
            "{} {}",
            record.program,
            record.arguments.join(" ")
        ));
        let (code, stdout) = self
            .answers
            .lock()
            .map_err(|_| poisoned())?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: format!("the test engine has no answer for `{}`", record.program),
            })?;
        Ok(CommandOutput {
            code: Some(code),
            stdout: Zeroizing::new(stdout),
            stderr: Zeroizing::new(if code == 0 {
                String::new()
            } else {
                "Error: No such network".to_owned()
            }),
            truncated: false,
        })
    }
}

fn local_target() -> NamedTarget {
    NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(uuid::Uuid::from_u128(7))),
    )
}

fn vault() -> Result<LocalNetwork, String> {
    Ok(LocalNetwork {
        key: "0123456789ab-production-vault".to_owned(),
        network: "vault".to_owned(),
        bridge: "mmn-0123456789a".to_owned(),
        block: "10.64.16.0/20".parse()?,
        slice: "10.64.17.0/24".parse()?,
        gateway: std::net::Ipv4Addr::new(10, 64, 17, 1),
        internal: true,
        mtu: 1420,
        mesh_interface: "mm0".to_owned(),
    })
}

fn labels() -> std::collections::BTreeMap<String, String> {
    ResourceOwnership::for_named_target(&local_target(), ResourceKind::Network, "vault", "network")
        .labels()
}

fn podman_vault(options: serde_json::Value) -> String {
    serde_json::json!([{
        "name": "vault",
        "driver": "bridge",
        "network_interface": "mmn-0123456789a",
        "subnets": [{
            "subnet": "10.64.16.0/20",
            "gateway": "10.64.17.1",
            "lease_range": {"start_ip": "10.64.17.2", "end_ip": "10.64.17.254"},
        }],
        "internal": false,
        "dns_enabled": false,
        "labels": labels(),
        "options": options,
    }])
    .to_string()
}

fn docker_vault(internal: bool) -> String {
    serde_json::json!([{
        "Name": "vault",
        "Driver": "bridge",
        "Internal": internal,
        "Labels": labels(),
        "IPAM": {"Config": [{
            "Subnet": "10.64.16.0/20", "IPRange": "10.64.17.0/24", "Gateway": "10.64.17.1",
            "AuxiliaryAddresses": {"first": "10.64.17.0", "last": "10.64.17.255"},
        }]},
        "Options": {
            "com.docker.network.bridge.name": "mmn-0123456789a",
            "com.docker.network.driver.mtu": "1420",
            "com.docker.network.bridge.enable_ip_masquerade": "false",
            "com.docker.network.bridge.trusted_host_interfaces": "mm0",
        },
    }])
    .to_string()
}

#[tokio::test]
async fn an_internal_podman_network_is_created_without_a_default_route() -> TestResult {
    let routed = podman_vault(serde_json::json!({"mtu": "1420"}));
    let unrouted = podman_vault(serde_json::json!({"mtu": "1420", "no_default_route": "true"}));
    let podman = Engine::answering(vec![
        (0, String::new()),
        (0, routed.clone()),
        (0, String::new()),
        (0, unrouted.clone()),
        (0, String::new()),
        (0, routed.clone()),
        (0, routed),
        (0, "aaa111\n".to_owned()),
        (0, String::new()),
        (0, String::new()),
        (0, String::new()),
        (0, String::new()),
        (0, unrouted.clone()),
        (0, unrouted),
    ]);
    let runtime = PodmanComposeRuntime::new(podman.clone());
    let network = vault()?;
    let observed = runtime
        .observe_cluster_networks(&[network.clone(), network.clone()])
        .await?
        .into_iter()
        .map(|seen| seen.map(|seen| (seen.current, seen.addresses)))
        .collect::<Vec<_>>();
    let range = "10.64.16.0/20, range 10.64.17.2-10.64.17.254";
    assert_eq!(
        observed,
        [
            Some((false, format!("{range}, with a default route"))),
            Some((true, range.to_owned())),
        ]
    );
    let target = local_target();
    for expected in [
        ResourceEnsureResult::Created,
        ResourceEnsureResult::Unchanged,
    ] {
        assert_eq!(
            runtime
                .ensure_cluster_network(&target, "vault", "network", &network)
                .await?,
            expected
        );
    }
    let commands = podman.commands()?;
    assert!(
        commands[7].ends_with("ps --all --quiet --filter network=vault")
            && commands[8].ends_with("rm --force aaa111")
            && commands[9].ends_with("network rm vault"),
        "{commands:#?}"
    );
    assert!(
        commands[10].contains(" network create ")
            && commands[10].contains("--opt mtu=1420 --opt no_default_route=true"),
        "{commands:#?}"
    );
    assert_eq!(commands.len(), 14, "an unchanged network gets no command");
    Ok(())
}

#[tokio::test]
async fn an_internal_docker_network_is_internal_to_docker_and_carries_no_default_route()
-> TestResult {
    let docker = Engine::answering(vec![
        (0, "iptables\n".to_owned()),
        (0, docker_vault(false)),
        (0, "iptables\n".to_owned()),
        (0, docker_vault(false)),
        (0, String::new()),
        (0, String::new()),
        (0, String::new()),
        (0, "iptables\n".to_owned()),
        (0, docker_vault(true)),
    ]);
    let runtime = DockerComposeRuntime::new(docker.clone());
    let network = vault()?;
    let observed = runtime
        .observe_cluster_network("vault", Some(&network))
        .await?
        .ok_or("the network was not observed")?;
    assert_eq!(
        (observed.current, observed.addresses.as_str()),
        (
            false,
            "10.64.16.0/20, range 10.64.17.0/24, with a default route"
        )
    );
    let target = local_target();
    for expected in [
        ResourceEnsureResult::Created,
        ResourceEnsureResult::Unchanged,
    ] {
        assert_eq!(
            runtime
                .ensure_cluster_network(&target, "vault", "network", &network)
                .await?,
            expected
        );
    }
    let commands = docker.commands()?;
    assert!(
        commands[4].ends_with("ps --all --quiet --filter network=vault")
            && commands[5].ends_with("network rm vault"),
        "{commands:#?}"
    );
    assert!(
        commands[6].contains("network create --driver bridge --subnet 10.64.16.0/20 --ip-range 10.64.17.0/24 --gateway 10.64.17.1 --internal "),
        "{commands:#?}"
    );
    assert_eq!(commands.len(), 9, "an unchanged network gets no command");
    Ok(())
}

#[tokio::test]
async fn a_network_no_longer_internal_is_planned_again_without_a_default_route() -> TestResult {
    let ordinary = LocalNetwork {
        internal: false,
        ..vault()?
    };
    let podman = Engine::answering(vec![
        (0, String::new()),
        (
            0,
            podman_vault(serde_json::json!({"mtu": "1420", "no_default_route": "true"})),
        ),
    ]);
    let observed = PodmanComposeRuntime::new(podman)
        .observe_cluster_networks(std::slice::from_ref(&ordinary))
        .await?
        .into_iter()
        .map(|seen| seen.map(|seen| (seen.current, seen.addresses)))
        .collect::<Vec<_>>();
    assert_eq!(
        observed,
        [Some((
            false,
            "10.64.16.0/20, range 10.64.17.2-10.64.17.254, without a default route".to_owned()
        ))]
    );
    let docker = Engine::answering(vec![(0, "iptables\n".to_owned()), (0, docker_vault(true))]);
    let observed = DockerComposeRuntime::new(docker)
        .observe_cluster_network("vault", Some(&ordinary))
        .await?
        .ok_or("the network was not observed")?;
    assert_eq!(
        (observed.current, observed.addresses.as_str()),
        (
            false,
            "10.64.16.0/20, range 10.64.17.0/24, without a default route"
        )
    );
    Ok(())
}

struct Packet<'a> {
    from: &'a str,
    to: &'a str,
    input: &'a str,
    output: &'a str,
    state: &'a str,
}

fn matches(rule: &str, packet: &Packet) -> Result<bool, String> {
    let rule = rule.split(" -m comment ").next().unwrap_or(rule);
    let mut words = rule.split_whitespace().skip(2);
    while let Some(word) = words.next() {
        let value = words.next().unwrap_or_default();
        let holds = match word {
            "-s" => value
                .parse::<Ipv4Cidr>()?
                .contains(packet.from.parse().map_err(|_| "bad address")?),
            "-d" => value
                .parse::<Ipv4Cidr>()?
                .contains(packet.to.parse().map_err(|_| "bad address")?),
            "-i" => value == packet.input,
            "-o" => value == packet.output,
            "-m" => {
                let _ = words.next();
                words
                    .next()
                    .unwrap_or_default()
                    .split(',')
                    .any(|state| state == packet.state)
            }
            _ => return Err(format!("the test cannot read `{word}` in `{rule}`")),
        };
        if !holds {
            return Ok(false);
        }
    }
    Ok(true)
}

fn verdict(rules: &[String], packet: &Packet) -> Result<String, String> {
    for rule in rules {
        if matches(rule, packet)? {
            return Ok(rule.rsplit(" -j ").next().unwrap_or_default().to_owned());
        }
    }
    Ok("RETURN".to_owned())
}

#[test]
fn an_internal_network_accepts_replies_between_its_nodes_ahead_of_docker() -> TestResult {
    let space: Ipv4Cidr = "10.64.0.0/12".parse()?;
    let mut table = NetworkTable::default();
    for (key, node, internal) in [
        ("mm-backend", "app-1", false),
        ("mm-backend", "app-2", false),
        ("mm-vault", "app-1", true),
        ("mm-vault", "app-2", true),
    ] {
        table.assign(
            space,
            &NetworkNeed {
                key,
                node,
                network: "mm-engine-name",
                internal,
            },
        )?;
    }
    let node = SealedNetworks {
        space,
        mesh_interface: "mm0".to_owned(),
        mtu: 1420,
        table,
        retired: Vec::new(),
    }
    .on_node("app-2");
    let filter = node.firewall().filter;
    let vault = node.local_network("mm-vault").ok_or("no vault")?;
    let backend = node.local_network("mm-backend").ok_or("no backend")?;
    let (here, there) = ("10.64.17.2", "10.64.16.2");
    assert_eq!(
        (vault.block.to_string(), vault.slice.to_string()),
        ("10.64.16.0/20".to_owned(), "10.64.17.0/24".to_owned())
    );
    for state in ["NEW", "ESTABLISHED", "RELATED"] {
        for (from, to, input, output) in [
            (there, here, "mm0", vault.bridge.as_str()),
            (here, there, vault.bridge.as_str(), "mm0"),
        ] {
            let packet = Packet {
                from,
                to,
                input,
                output,
                state,
            };
            assert_eq!(
                verdict(&filter, &packet)?,
                "ACCEPT",
                "{state} {from} to {to}\n{filter:#?}"
            );
        }
    }
    let reply_from_outside = Packet {
        from: "203.0.113.7",
        to: here,
        input: "eth0",
        output: vault.bridge.as_str(),
        state: "ESTABLISHED",
    };
    assert_eq!(verdict(&filter, &reply_from_outside)?, "RETURN");
    let way_out = Packet {
        from: here,
        to: "203.0.113.7",
        input: vault.bridge.as_str(),
        output: "eth0",
        state: "NEW",
    };
    assert_eq!(verdict(&filter, &way_out)?, "DROP");
    let ordinary_reply = Packet {
        from: "10.64.0.2",
        to: "10.64.1.2",
        input: "mm0",
        output: backend.bridge.as_str(),
        state: "ESTABLISHED",
    };
    assert_eq!(verdict(&filter, &ordinary_reply)?, "RETURN");
    Ok(())
}
