//! Service names across cluster nodes: the DNS messages a node's resolver
//! reads and writes, which containers a name resolves to for which asker,
//! what a node takes from another node's records, how the engine's
//! containers are read, and what a status says a name resolves to.

use std::collections::VecDeque;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

use mm::adapters::runtime::DockerComposeRuntime;
use mm::app::ports::{CommandOutput, CommandSpec, ProcessRunner, RuntimeDriver};
use mm::domain::cluster_network::{NetworkNeed, NetworkTable, NodeNetworks};
use mm::domain::discovery::wire::{self, Failure, Query};
use mm::domain::discovery::{
    Attachment, ContainerRecord, Endpoint, Health, NameAddress, NameReport, NodeLookup,
    NodeRecords, ObservedContainer, Reach, Resolution, ResolvedName, ServiceMap, resolver_file,
    resolver_file_name,
};
use mm::domain::error::MmError;
use mm::domain::repo_target::Ipv4Cidr;
use zeroize::Zeroizing;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const BACKEND: &str = "019c527b6242-production-backend";
const FRONTEND: &str = "019c527b6242-production-frontend";

fn ip(text: &str) -> Result<Ipv4Addr, std::net::AddrParseError> {
    text.parse()
}

/// A query for `name` with one question, as a client sends it.
fn query(id: u16, name: &str, kind: u16) -> Vec<u8> {
    let mut message = id.to_be_bytes().to_vec();
    message.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.split('.').filter(|label| !label.is_empty()) {
        message.push(u8::try_from(label.len()).unwrap_or(u8::MAX));
        message.extend_from_slice(label.as_bytes());
    }
    message.push(0);
    message.extend_from_slice(&kind.to_be_bytes());
    message.extend_from_slice(&wire::CLASS_IN.to_be_bytes());
    message
}

fn question(message: &[u8]) -> Result<wire::Question, String> {
    match wire::parse(message) {
        Query::Question(question) => Ok(question),
        other => Err(format!("not a question: {other:?}")),
    }
}

/// The addresses of the answer records of a reply, and its header counts.
fn addresses(reply: &[u8], question_length: usize) -> Vec<Ipv4Addr> {
    reply[12 + question_length..]
        .chunks(16)
        .filter(|record| record.len() == 16)
        .map(|record| Ipv4Addr::new(record[12], record[13], record[14], record[15]))
        .collect()
}

#[test]
fn a_question_is_read_and_answered_with_address_records() -> TestResult {
    let message = query(0xbeef, "WhoAmI", wire::TYPE_A);
    let question = question(&message)?;
    assert_eq!(question.id, 0xbeef);
    assert_eq!(question.name, "whoami", "names compare without case");
    assert_eq!(
        (question.kind, question.class),
        (wire::TYPE_A, wire::CLASS_IN)
    );

    let reply = question.answer(
        &[ip("10.64.0.2")?, ip("10.64.1.2")?],
        5,
        wire::DATAGRAM_LIMIT,
    );
    assert_eq!(&reply[..2], &[0xbe, 0xef], "the client's identifier");
    assert_eq!(
        reply[2], 0x85,
        "a response, authoritative, recursion desired"
    );
    assert_eq!(reply[3], 0x80, "recursion available, no error");
    assert_eq!(&reply[4..8], &[0, 1, 0, 2], "one question, two answers");
    let section = message.len() - 12;
    assert_eq!(
        &reply[12..12 + section],
        &message[12..],
        "the question as sent"
    );
    let record = &reply[12 + section..12 + section + 16];
    assert_eq!(
        record,
        &[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 5, 0, 4, 10, 64, 0, 2],
        "name pointer, type A, class IN, five seconds, four bytes"
    );
    assert_eq!(
        addresses(&reply, section),
        vec![ip("10.64.0.2")?, ip("10.64.1.2")?]
    );
    Ok(())
}

#[test]
fn another_record_type_of_a_known_name_is_an_empty_answer() -> TestResult {
    let message = query(7, "whoami", 28);
    let question = question(&message)?;
    assert!(!question.wants_addresses());
    let reply = question.answer(&[ip("10.64.0.2")?], 5, wire::DATAGRAM_LIMIT);
    assert_eq!(reply[3] & 0x0f, 0, "the name exists");
    assert_eq!(&reply[6..8], &[0, 0], "and has no such record");
    assert_eq!(reply.len(), message.len());
    Ok(())
}

#[test]
fn an_answer_that_does_not_fit_a_datagram_is_marked_truncated() -> TestResult {
    let message = query(1, "whoami", wire::TYPE_A);
    let question = question(&message)?;
    let many = (0..60_u8)
        .map(|host| Ipv4Addr::new(10, 64, 0, host))
        .collect::<Vec<_>>();
    let datagram = question.answer(&many, 5, wire::DATAGRAM_LIMIT);
    assert!(datagram.len() <= wire::DATAGRAM_LIMIT);
    assert_eq!(
        datagram[2] & 0x02,
        0x02,
        "truncated: ask again over a stream"
    );
    let fitted = usize::from(u16::from_be_bytes([datagram[6], datagram[7]]));
    assert_eq!(fitted, (wire::DATAGRAM_LIMIT - message.len()) / 16);
    assert_eq!(addresses(&datagram, message.len() - 12).len(), fitted);

    let stream = question.answer(&many, 5, wire::STREAM_LIMIT);
    assert_eq!(stream[2] & 0x02, 0);
    assert_eq!(addresses(&stream, message.len() - 12), many);
    Ok(())
}

#[test]
fn what_is_not_a_plain_question_is_refused_or_ignored() -> TestResult {
    assert_eq!(wire::parse(&[1, 2, 3]), Query::Noise, "too short");
    let mut response = query(1, "whoami", wire::TYPE_A);
    response[2] |= 0x80;
    assert_eq!(
        wire::parse(&response),
        Query::Noise,
        "an answer, not a query"
    );

    let rcode = |message: &[u8]| match wire::parse(message) {
        Query::Refused { reply } => Ok((reply[0], reply[1], reply[2] & 0x80, reply[3] & 0x0f)),
        other => Err(format!("not refused: {other:?}")),
    };
    let mut update = query(0x0102, "whoami", wire::TYPE_A);
    update[2] |= 5 << 3;
    assert_eq!(rcode(&update)?, (1, 2, 0x80, 4), "not implemented");
    let mut two = query(3, "whoami", wire::TYPE_A);
    two[5] = 2;
    assert_eq!(rcode(&two)?.3, 1, "one question per message");
    let mut pointer = query(3, "whoami", wire::TYPE_A);
    pointer[12] = 0xc0;
    assert_eq!(rcode(&pointer)?.3, 1, "a question holds its name in full");
    let cut = &query(3, "whoami", wire::TYPE_A)[..20];
    assert_eq!(rcode(cut)?.3, 1, "the question ends early");

    let odd = question(&{
        let mut message = query(3, "a", wire::TYPE_A);
        message[13] = b'.';
        message
    })?;
    assert_ne!(odd.name, ".", "a byte no host name has matches no name");

    let failure = question(&query(9, "example.com", wire::TYPE_A))?.failure(Failure::Server);
    assert_eq!(failure[3] & 0x0f, 2);
    assert_eq!(&failure[4..8], &[0, 1, 0, 0]);
    Ok(())
}

#[test]
fn a_forwarded_message_gets_another_identifier_and_its_own_back() {
    let message = query(0x1234, "example.com", wire::TYPE_A);
    let sent = wire::with_message_id(&message, 0xabcd);
    assert_eq!(wire::message_id(&sent), Some(0xabcd));
    assert_eq!(&sent[2..], &message[2..]);
    assert!(!wire::is_response(&sent));
    let mut reply = sent;
    reply[2] |= 0x80;
    assert!(wire::is_response(&reply));
    assert_eq!(
        wire::message_id(&wire::with_message_id(&reply, 0x1234)),
        Some(0x1234)
    );
    assert_eq!(wire::message_id(&[1]), None);
}

/// `backend` on three nodes and `frontend` on two, as node `node` holds it.
fn networks(node: &str) -> Result<NodeNetworks, String> {
    let space: Ipv4Cidr = "10.64.0.0/12".parse()?;
    let mut table = NetworkTable::default();
    for (key, holder) in [
        (BACKEND, "app-1"),
        (BACKEND, "app-2"),
        (BACKEND, "db-1"),
        (FRONTEND, "app-1"),
        (FRONTEND, "db-1"),
    ] {
        table.assign(
            space,
            &NetworkNeed {
                key,
                node: holder,
                network: &format!("mm-{holder}-{}", key.rsplit('-').next().unwrap_or(key)),
                internal: false,
            },
        )?;
    }
    Ok(NodeNetworks {
        node: node.to_owned(),
        space,
        mesh_interface: "mm0".to_owned(),
        mtu: 1420,
        table,
    })
}

fn container(
    name: &str,
    service: Option<&str>,
    health: Health,
    endpoints: &[(&str, &str, &[&str])],
) -> Result<ContainerRecord, std::net::AddrParseError> {
    Ok(ContainerRecord {
        name: name.to_owned(),
        id: format!("{name:0<12}").chars().take(12).collect(),
        service: service.map(str::to_owned),
        health,
        endpoints: endpoints
            .iter()
            .map(|(network, address, aliases)| {
                Ok(Endpoint {
                    network: (*network).to_owned(),
                    address: ip(address)?,
                    aliases: aliases.iter().map(|alias| (*alias).to_owned()).collect(),
                })
            })
            .collect::<Result<_, std::net::AddrParseError>>()?,
        restart_limit: false,
    })
}

struct Cluster {
    networks: NodeNetworks,
    local: NodeRecords,
    peers: Vec<NodeRecords>,
}

impl Cluster {
    fn map(&self) -> ServiceMap<'_> {
        ServiceMap {
            networks: &self.networks,
            local: &self.local,
            peers: &self.peers,
        }
    }

    fn lookup(&self, source: &str, name: &str) -> Result<Resolution, std::net::AddrParseError> {
        Ok(self.map().resolve(ip(source)?, name))
    }
}

/// The cluster as `app-1` knows it. `web` is on both networks, `whoami`
/// runs on every node, `worker` fails its health check on `app-2`, and on
/// `db-1` `whoami` is still starting.
fn cluster() -> Result<Cluster, Box<dyn std::error::Error>> {
    use Health::{Healthy, Starting, Unhealthy};
    Ok(Cluster {
        networks: networks("app-1")?,
        local: NodeRecords {
            node: "app-1".to_owned(),
            containers: vec![
                container(
                    "shop-web-1",
                    Some("web"),
                    Healthy,
                    &[(BACKEND, "10.64.0.2", &[]), (FRONTEND, "10.64.16.2", &[])],
                )?,
                container(
                    "shop-whoami-1",
                    Some("whoami"),
                    Healthy,
                    &[(BACKEND, "10.64.0.3", &[])],
                )?,
                container(
                    "cache",
                    None,
                    Healthy,
                    &[("host/legacy", "172.18.0.2", &["redis"])],
                )?,
                container("job", None, Healthy, &[("host/legacy", "172.18.0.3", &[])])?,
            ],
            ..NodeRecords::default()
        },
        peers: vec![
            NodeRecords {
                node: "app-2".to_owned(),
                containers: vec![
                    container(
                        "shop-whoami-1",
                        Some("whoami"),
                        Healthy,
                        &[(BACKEND, "10.64.1.2", &[])],
                    )?,
                    container(
                        "shop-worker-1",
                        Some("worker"),
                        Unhealthy,
                        &[(BACKEND, "10.64.1.3", &[])],
                    )?,
                ],
                ..NodeRecords::default()
            },
            NodeRecords {
                node: "db-1".to_owned(),
                containers: vec![
                    container(
                        "shop-api-1",
                        Some("api"),
                        Healthy,
                        &[(FRONTEND, "10.64.17.2", &[])],
                    )?,
                    container(
                        "shop-whoami-1",
                        Some("whoami"),
                        Starting,
                        &[(BACKEND, "10.64.2.2", &[])],
                    )?,
                    container(
                        "shop-db-1",
                        Some("db"),
                        Healthy,
                        &[(BACKEND, "10.64.2.3", &["database"])],
                    )?,
                ],
                ..NodeRecords::default()
            },
        ],
    })
}

fn answer(addresses: &[&str]) -> Result<Resolution, std::net::AddrParseError> {
    Ok(Resolution::Addresses(
        addresses
            .iter()
            .map(|address| ip(address))
            .collect::<Result<_, _>>()?,
    ))
}

#[test]
fn a_service_name_is_the_nodes_own_healthy_containers_first() -> TestResult {
    let cluster = cluster()?;
    // app-1 runs whoami, so its containers get that one and stay on the node.
    let own = answer(&["10.64.0.3"])?;
    assert_eq!(cluster.lookup("10.64.0.2", "whoami")?, own);
    assert_eq!(cluster.lookup("10.64.0.2", "WHOAMI.")?, own);
    // A container asking for its own service gets itself.
    assert_eq!(cluster.lookup("10.64.0.3", "whoami")?, own);
    // `tasks.` is every healthy container on every node.
    let healthy = answer(&["10.64.0.3", "10.64.1.2"])?;
    assert_eq!(cluster.lookup("10.64.0.2", "tasks.whoami")?, healthy);
    assert_eq!(cluster.lookup("10.64.0.3", "TASKS.whoami.")?, healthy);
    assert_eq!(
        cluster.lookup("10.64.0.2", "whoami.app-2")?,
        answer(&["10.64.1.2"])?
    );
    assert_eq!(
        cluster.lookup("10.64.0.2", "whoami.app-1")?,
        answer(&["10.64.0.3"])?
    );
    // Known and not healthy while the service has healthy containers: the
    // name exists and has no address.
    assert_eq!(cluster.lookup("10.64.0.2", "whoami.db-1")?, answer(&[])?);
    // No worker is healthy anywhere: its running one is answered all the
    // same (fail open).
    assert_eq!(
        cluster.lookup("10.64.0.2", "worker")?,
        answer(&["10.64.1.3"])?
    );
    assert_eq!(
        cluster.lookup("10.64.0.2", "database")?,
        answer(&["10.64.2.3"])?
    );
    // Not a name of the cluster.
    for name in [
        "example.com",
        "whoami.example.com",
        "tasks.database",
        "nosuch",
        "whoami.nosuch",
    ] {
        assert_eq!(
            cluster.lookup("10.64.0.2", name)?,
            Resolution::Elsewhere,
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn a_node_without_a_healthy_container_answers_with_the_other_nodes() -> TestResult {
    let mut cluster = cluster()?;
    // db runs on db-1 only: app-1 has none of its own.
    assert_eq!(cluster.lookup("10.64.0.2", "db")?, answer(&["10.64.2.3"])?);
    // The last local whoami turns unhealthy: the very next lookup gets the
    // other node's, never an empty answer in between.
    assert_eq!(
        cluster.lookup("10.64.0.2", "whoami")?,
        answer(&["10.64.0.3"])?
    );
    cluster.local.containers[1].health = Health::Unhealthy;
    assert_eq!(
        cluster.lookup("10.64.0.2", "whoami")?,
        answer(&["10.64.1.2"])?
    );
    // And comes back the same way.
    cluster.local.containers[1].health = Health::Healthy;
    assert_eq!(
        cluster.lookup("10.64.0.2", "whoami")?,
        answer(&["10.64.0.3"])?
    );
    // Two of its own: both, and still nobody else's.
    cluster.local.containers.push(container(
        "shop-whoami-2",
        Some("whoami"),
        Health::Healthy,
        &[(BACKEND, "10.64.0.4", &[])],
    )?);
    assert_eq!(
        cluster.lookup("10.64.0.2", "whoami")?,
        answer(&["10.64.0.3", "10.64.0.4"])?
    );
    assert_eq!(
        cluster.lookup("10.64.0.2", "tasks.whoami")?,
        answer(&["10.64.0.3", "10.64.0.4", "10.64.1.2"])?
    );
    // No healthy container anywhere: the running ones are answered (fail
    // open), the stopped one never.
    cluster.local.containers.pop();
    cluster.local.containers[1].health = Health::Stopped;
    cluster.peers[0].containers[0].health = Health::Unhealthy;
    assert_eq!(
        cluster.lookup("10.64.0.2", "whoami")?,
        answer(&["10.64.1.2", "10.64.2.2"])?
    );
    // One turns healthy: only it again.
    cluster.peers[0].containers[0].health = Health::Healthy;
    assert_eq!(
        cluster.lookup("10.64.0.2", "whoami")?,
        answer(&["10.64.1.2"])?
    );
    // Nothing runs: no address.
    for peer in &mut cluster.peers {
        for container in &mut peer.containers {
            container.health = Health::Stopped;
        }
    }
    assert_eq!(cluster.lookup("10.64.0.2", "whoami")?, answer(&[])?);
    Ok(())
}

#[test]
fn a_network_alias_stays_on_the_node_like_its_service() -> TestResult {
    let mut cluster = cluster()?;
    cluster.local.containers[1].endpoints[0].aliases = vec!["who".to_owned()];
    cluster.peers[0].containers[0].endpoints[0].aliases = vec!["who".to_owned()];
    assert_eq!(cluster.lookup("10.64.0.2", "who")?, answer(&["10.64.0.3"])?);
    cluster.local.containers[1].health = Health::Unhealthy;
    assert_eq!(cluster.lookup("10.64.0.2", "who")?, answer(&["10.64.1.2"])?);
    // An alias only another node's container carries.
    assert_eq!(
        cluster.lookup("10.64.0.2", "database")?,
        answer(&["10.64.2.3"])?
    );
    Ok(())
}

#[test]
fn a_proxy_can_ask_for_the_backends_next_to_it_or_for_all() -> TestResult {
    let cluster = cluster()?;
    let map = cluster.map();
    assert_eq!(
        map.service_addresses(BACKEND, "whoami", Reach::LocalFirst),
        (vec![ip("10.64.0.3")?], true)
    );
    assert_eq!(
        map.service_addresses(BACKEND, "whoami", Reach::EveryNode),
        (vec![ip("10.64.0.3")?, ip("10.64.1.2")?], false)
    );
    // Nothing of its own: the other nodes', and it says so.
    assert_eq!(
        map.service_addresses(BACKEND, "db", Reach::LocalFirst),
        (vec![ip("10.64.2.3")?], false)
    );
    // web is on both networks; each network has its own address.
    assert_eq!(
        map.service_addresses(FRONTEND, "web", Reach::LocalFirst),
        (vec![ip("10.64.16.2")?], true)
    );
    assert_eq!(
        map.service_addresses(FRONTEND, "whoami", Reach::EveryNode),
        (Vec::new(), false)
    );
    assert_eq!(
        map.service_addresses(BACKEND, "worker", Reach::LocalFirst),
        (vec![ip("10.64.1.3")?], false),
        "the only worker is unhealthy, and an ingress proxy routes to it all the same"
    );
    Ok(())
}

#[test]
fn a_container_that_is_not_healthy_still_finds_itself() -> TestResult {
    let mut cluster = cluster()?;
    // whoami on app-1 is starting: its own health check may ask for it,
    // and must not be sent to another node's container.
    cluster.local.containers[1].health = Health::Starting;
    assert_eq!(
        cluster.lookup("10.64.0.3", "whoami")?,
        answer(&["10.64.0.3"])?
    );
    assert_eq!(
        cluster.lookup("10.64.0.3", "tasks.whoami")?,
        answer(&["10.64.0.3", "10.64.1.2"])?
    );
    assert_eq!(
        cluster.lookup("10.64.0.3", "whoami.app-1")?,
        answer(&["10.64.0.3"])?
    );
    assert_eq!(
        cluster.lookup("10.64.0.3", "shop-whoami-1")?,
        answer(&["10.64.0.3"])?
    );
    // Nobody else gets it.
    assert_eq!(
        cluster.lookup("10.64.0.2", "whoami")?,
        answer(&["10.64.1.2"])?
    );
    assert_eq!(cluster.lookup("10.64.0.2", "whoami.app-1")?, answer(&[])?);
    Ok(())
}

#[test]
fn a_container_gets_answers_for_its_own_networks_only() -> TestResult {
    let cluster = cluster()?;
    // web is on both networks, whichever of its addresses it asks from.
    assert_eq!(
        cluster.lookup("10.64.0.2", "api")?,
        answer(&["10.64.17.2"])?
    );
    assert_eq!(
        cluster.lookup("10.64.16.2", "whoami")?,
        answer(&["10.64.0.3"])?
    );
    assert_eq!(
        cluster.lookup("10.64.16.2", "tasks.whoami")?,
        answer(&["10.64.0.3", "10.64.1.2"])?
    );
    // whoami is on backend only: api is not its to see, and web is the
    // backend address, never the frontend one.
    assert_eq!(cluster.lookup("10.64.0.3", "api")?, Resolution::Elsewhere);
    assert_eq!(cluster.lookup("10.64.0.3", "web")?, answer(&["10.64.0.2"])?);
    // A container the node has not seen yet is on the network of its slice.
    assert_eq!(
        cluster.lookup("10.64.0.9", "whoami")?,
        answer(&["10.64.0.3"])?
    );
    assert_eq!(
        cluster.lookup("10.64.0.9", "tasks.whoami")?,
        answer(&["10.64.0.3", "10.64.1.2"])?
    );
    assert_eq!(cluster.lookup("10.64.0.9", "api")?, Resolution::Elsewhere);
    assert_eq!(
        cluster.lookup("10.64.16.9", "api")?,
        answer(&["10.64.17.2"])?
    );
    Ok(())
}

#[test]
fn only_this_nodes_containers_are_answered() -> TestResult {
    let cluster = cluster()?;
    for source in [
        "10.64.1.2",
        "10.64.2.2",
        "10.90.0.2",
        "192.0.2.7",
        "172.18.0.9",
    ] {
        assert_eq!(
            cluster.lookup(source, "whoami")?,
            Resolution::NotAsked,
            "{source}"
        );
        assert_eq!(
            cluster.lookup(source, "example.com")?,
            Resolution::NotAsked,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn a_containers_own_name_and_a_host_network_stay_on_the_node() -> TestResult {
    let cluster = cluster()?;
    assert_eq!(
        cluster.lookup("10.64.0.3", "shop-web-1")?,
        answer(&["10.64.0.2"])?
    );
    assert_eq!(
        cluster.lookup("10.64.0.3", "shop-web-100")?,
        answer(&["10.64.0.2"])?,
        "its short id"
    );
    // The same container name on another node is that node's business.
    assert_eq!(
        cluster.lookup("10.64.0.2", "shop-worker-1")?,
        Resolution::Elsewhere
    );
    assert_eq!(
        cluster.lookup("10.64.0.2", "tasks.shop-web-1")?,
        Resolution::Elsewhere
    );
    // A network that does not span the nodes: its containers see each
    // other by name and alias, and nobody else sees them.
    assert_eq!(
        cluster.lookup("172.18.0.3", "cache")?,
        answer(&["172.18.0.2"])?
    );
    assert_eq!(
        cluster.lookup("172.18.0.3", "redis")?,
        answer(&["172.18.0.2"])?
    );
    assert_eq!(
        cluster.lookup("172.18.0.3", "whoami")?,
        Resolution::Elsewhere
    );
    assert_eq!(cluster.lookup("10.64.0.2", "cache")?, Resolution::Elsewhere);
    let sent = cluster.local.for_peers();
    assert_eq!(
        sent.containers
            .iter()
            .map(|container| container.name.as_str())
            .collect::<Vec<_>>(),
        ["shop-web-1", "shop-whoami-1"],
        "what stays on the node is not sent"
    );
    Ok(())
}

#[test]
fn a_node_takes_only_addresses_of_the_senders_own_slices() -> TestResult {
    let networks = networks("app-1")?;
    let sent = NodeRecords {
        node: "app-2".to_owned(),
        containers: vec![
            container(
                "honest",
                Some("whoami"),
                Health::Healthy,
                &[(BACKEND, "10.64.1.2", &[])],
            )?,
            // app-1's own slice, db-1's slice, a network app-2 is not on, a
            // host network, an address outside the cluster.
            container(
                "thief",
                Some("web"),
                Health::Healthy,
                &[(BACKEND, "10.64.0.2", &[])],
            )?,
            container(
                "other",
                Some("db"),
                Health::Healthy,
                &[(BACKEND, "10.64.2.3", &[])],
            )?,
            container(
                "absent",
                Some("api"),
                Health::Healthy,
                &[(FRONTEND, "10.64.17.6", &[])],
            )?,
            container(
                "local",
                Some("cache"),
                Health::Healthy,
                &[("host/legacy", "172.18.0.2", &[])],
            )?,
            container(
                "outside",
                Some("whoami"),
                Health::Healthy,
                &[(BACKEND, "192.0.2.1", &[])],
            )?,
        ],
        ..NodeRecords::default()
    };
    let accepted = sent
        .clone()
        .accepted_from("app-2", &networks)
        .ok_or("the records of app-2 were refused")?;
    assert_eq!(
        accepted
            .containers
            .iter()
            .map(|container| container.name.as_str())
            .collect::<Vec<_>>(),
        ["honest"]
    );
    assert!(
        sent.accepted_from("db-1", &networks).is_none(),
        "records under another node's name"
    );
    Ok(())
}

#[test]
fn records_survive_a_newer_node_that_sends_more() -> TestResult {
    let sent = r#"{"node":"app-2","generation":7,"containers":[{"name":"c","service":"whoami",
        "health":"healthy","since":1,"endpoints":[{"network":"n","address":"10.64.1.2","zone":"a"}]}]}"#;
    let records: NodeRecords = serde_json::from_str(sent)?;
    assert_eq!(records.containers[0].endpoints[0].address, ip("10.64.1.2")?);
    let again: NodeRecords = serde_json::from_str(&serde_json::to_string(&records)?)?;
    assert_eq!(again, records);
    Ok(())
}

#[test]
fn health_is_the_health_check_or_running_without_one() {
    assert_eq!(Health::from_engine("running", None), Health::Healthy);
    assert_eq!(
        Health::from_engine("running", Some("healthy")),
        Health::Healthy
    );
    assert_eq!(
        Health::from_engine("running", Some("starting")),
        Health::Starting
    );
    assert_eq!(
        Health::from_engine("running", Some("unhealthy")),
        Health::Unhealthy
    );
    assert_eq!(
        Health::from_engine("exited", Some("healthy")),
        Health::Stopped
    );
    assert_eq!(Health::from_engine("paused", None), Health::Stopped);
    assert!(Health::Healthy.answers());
    assert!(
        ![Health::Starting, Health::Unhealthy, Health::Stopped]
            .iter()
            .any(|health| health.answers())
    );
}

#[test]
fn what_the_engine_reports_becomes_the_nodes_records() -> TestResult {
    let networks = networks("app-1")?;
    let observed = vec![
        ObservedContainer {
            id: "0123456789abcdef0123".to_owned(),
            name: "shop-web-1".to_owned(),
            service: Some("web".to_owned()),
            health: Health::Healthy,
            attachments: vec![
                Attachment {
                    network: "mm-app-1-backend".to_owned(),
                    address: ip("10.64.0.2")?,
                    names: ["shop-web-1", "web", "0123456789ab", "www"]
                        .map(str::to_owned)
                        .to_vec(),
                },
                Attachment {
                    network: "legacy_default".to_owned(),
                    address: ip("172.18.0.5")?,
                    names: vec!["web".to_owned()],
                },
            ],
        },
        ObservedContainer {
            id: "ffff".to_owned(),
            name: "stopped".to_owned(),
            service: Some("db".to_owned()),
            health: Health::Stopped,
            attachments: Vec::new(),
        },
    ];
    let records = NodeRecords::observed(&networks, observed);
    assert_eq!(records.node, "app-1");
    assert_eq!(
        records.containers.len(),
        1,
        "a container without an address has no name"
    );
    let web = &records.containers[0];
    assert_eq!(web.id, "0123456789ab");
    assert_eq!(
        web.endpoints,
        vec![
            Endpoint {
                network: BACKEND.to_owned(),
                address: ip("10.64.0.2")?,
                aliases: vec!["www".to_owned()],
            },
            Endpoint {
                network: "host/legacy_default".to_owned(),
                address: ip("172.18.0.5")?,
                aliases: Vec::new(),
            },
        ]
    );
    Ok(())
}

#[test]
fn the_resolver_file_names_the_gateway_and_keeps_the_hosts_search_domains() -> TestResult {
    assert_eq!(resolver_file_name(BACKEND), format!("{BACKEND}.conf"));
    let plain = resolver_file(
        ip("10.64.0.1")?,
        "nameserver 127.0.0.53\noptions edns0 trust-ad\nsearch .\n",
    );
    assert!(
        plain.ends_with("nameserver 10.64.0.1\noptions ndots:0\n"),
        "{plain}"
    );
    assert!(!plain.contains("127.0.0.53") && !plain.contains("search"));
    let searching = resolver_file(
        ip("10.64.17.1")?,
        "# comment\nsearch corp.example  lan.example\nnameserver 192.0.2.53\nsearchlight x\n",
    );
    assert!(
        searching
            .ends_with("nameserver 10.64.17.1\nsearch corp.example lan.example\noptions ndots:0\n"),
        "{searching}"
    );
    Ok(())
}

fn listed(
    address: &str,
    node: &str,
    health: Health,
) -> Result<NameAddress, std::net::AddrParseError> {
    Ok(NameAddress {
        address: ip(address)?,
        node: node.to_owned(),
        health,
        unknown_to: Vec::new(),
        restart_limit: false,
    })
}

#[test]
fn a_node_reports_the_names_of_one_environment() -> TestResult {
    let cluster = cluster()?;
    let report = cluster.map().report("019c527b6242-production-");
    assert_eq!(report.networks, ["backend", "frontend"]);
    let names = report
        .names
        .iter()
        .map(|name| {
            (
                name.name.as_str(),
                name.network.as_str(),
                name.addresses.len(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            ("api", "frontend", 1),
            ("db", "backend", 1),
            ("web", "backend", 1),
            ("web", "frontend", 1),
            ("whoami", "backend", 3),
            ("worker", "backend", 1)
        ]
    );
    let whoami = report
        .names
        .iter()
        .find(|name| name.name == "whoami")
        .ok_or("no whoami")?;
    assert_eq!(
        whoami
            .answers()
            .map(|answer| answer.address.to_string())
            .collect::<Vec<_>>(),
        ["10.64.0.3", "10.64.1.2"]
    );
    // What app-1's own containers get for each name: its own whoami and
    // web, and the other nodes' where it has none.
    let lookups = report
        .names
        .iter()
        .map(|name| {
            let [lookup] = name.lookups.as_slice() else {
                return Err(format!("{} has {} lookups", name.name, name.lookups.len()));
            };
            Ok((
                name.name.as_str(),
                name.network.as_str(),
                lookup.node.as_str(),
                lookup
                    .addresses
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
                lookup.local,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    assert_eq!(
        lookups,
        [
            ("api", "frontend", "app-1", "10.64.17.2".to_owned(), false),
            ("db", "backend", "app-1", "10.64.2.3".to_owned(), false),
            ("web", "backend", "app-1", "10.64.0.2".to_owned(), true),
            ("web", "frontend", "app-1", "10.64.16.2".to_owned(), true),
            ("whoami", "backend", "app-1", "10.64.0.3".to_owned(), true),
            ("worker", "backend", "app-1", "10.64.1.3".to_owned(), false),
        ]
    );
    // worker is answered while unhealthy, and the report says so: in the
    // status, in `mm deploy status`, and as a doctor finding.
    let open = report
        .names
        .iter()
        .filter(|name| name.fail_open)
        .map(|name| name.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(open, ["worker"]);
    assert_eq!(report.answered_unhealthy(), ["worker on backend"]);
    let findings = mm::domain::discovery::fail_open_diagnostics(&report);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "MM_SERVICE_ANSWERED_UNHEALTHY");
    assert!(
        findings[0].message.contains("service 'worker'")
            && findings[0]
                .message
                .contains("`healthcheck:` of service 'worker'"),
        "{}",
        findings[0].message
    );
    assert!(cluster.map().report("another-project-").is_empty());
    Ok(())
}

#[test]
fn the_nodes_views_become_one_report_that_names_who_misses_what() -> TestResult {
    let lookup =
        |node: &str, address: &str, local: bool| -> Result<NodeLookup, std::net::AddrParseError> {
            Ok(NodeLookup {
                node: node.to_owned(),
                addresses: vec![ip(address)?],
                local,
            })
        };
    let name = |addresses: Vec<NameAddress>, lookups: Vec<NodeLookup>| ResolvedName {
        name: "whoami".to_owned(),
        network: "backend".to_owned(),
        addresses,
        lookups,
        fail_open: false,
    };
    let view = |networks: &[&str], names: Vec<ResolvedName>| NameReport {
        networks: networks
            .iter()
            .map(|network| (*network).to_owned())
            .collect(),
        names,
        problems: Vec::new(),
    };
    let merged = NameReport::merged(vec![
        (
            "app-1".to_owned(),
            view(
                &["backend"],
                vec![name(
                    vec![
                        listed("10.64.0.3", "app-1", Health::Healthy)?,
                        // app-1 last heard that app-2's container was healthy.
                        listed("10.64.1.2", "app-2", Health::Healthy)?,
                    ],
                    vec![
                        lookup("app-1", "10.64.0.3", true)?,
                        // Not app-1's to say.
                        lookup("app-2", "10.64.1.2", true)?,
                    ],
                )],
            ),
        ),
        (
            "app-2".to_owned(),
            view(
                &["backend"],
                // Its own container is unhealthy, and it has not heard of
                // app-1's: its containers get nothing.
                vec![name(
                    vec![listed("10.64.1.2", "app-2", Health::Unhealthy)?],
                    vec![NodeLookup {
                        node: "app-2".to_owned(),
                        addresses: Vec::new(),
                        local: false,
                    }],
                )],
            ),
        ),
        // db-1 holds no part of backend: it is not expected to know.
        ("db-1".to_owned(), view(&["frontend"], Vec::new())),
    ]);
    assert_eq!(merged.networks, ["backend", "frontend"]);
    assert_eq!(merged.names.len(), 1);
    let whoami = &merged.names[0];
    assert_eq!(
        whoami.addresses,
        vec![
            NameAddress {
                unknown_to: vec!["app-2".to_owned()],
                ..listed("10.64.0.3", "app-1", Health::Healthy)?
            },
            // A node knows its own containers best.
            listed("10.64.1.2", "app-2", Health::Unhealthy)?,
        ]
    );
    assert_eq!(whoami.answers().count(), 1);
    // Each node's own word on what its containers get, by node.
    assert_eq!(
        whoami.lookups,
        vec![
            lookup("app-1", "10.64.0.3", true)?,
            NodeLookup {
                node: "app-2".to_owned(),
                addresses: Vec::new(),
                local: false,
            },
        ]
    );
    assert!(merged.clone().only("web").names.is_empty());
    assert_eq!(merged.only("whoami").names.len(), 1);
    Ok(())
}

struct Engine {
    outputs: Mutex<VecDeque<&'static str>>,
    commands: Mutex<Vec<Vec<String>>>,
}

#[async_trait::async_trait]
impl ProcessRunner for Engine {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let poisoned = || MmError::State {
            message: "the test engine's lock was poisoned".to_owned(),
        };
        self.commands
            .lock()
            .map_err(|_| poisoned())?
            .push(specification.redacted_record().arguments);
        let stdout = self
            .outputs
            .lock()
            .map_err(|_| poisoned())?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "the test engine has no more output".to_owned(),
            })?;
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(stdout.to_owned()),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

const INSPECTED: &str = r#"[
 {"Id": "f1f7342c31e5cbdf734c92ffef38e5ee07dd6a564854c0ff016d2e9015999f8d",
  "Name": "/shop-api-1",
  "State": {"Status": "running", "Health": {"Status": "unhealthy"}},
  "Config": {"Labels": {"com.docker.compose.service": "api", "com.docker.compose.oneoff": "False"}},
  "NetworkSettings": {"Networks": {
    "mm-17b2-frontend": {"Aliases": ["shop-api-1", "api", "backend-api"], "IPAddress": "10.64.17.4",
      "DNSNames": ["shop-api-1", "api", "f1f7342c31e5"]},
    "mm-17b2-backend": {"Aliases": null, "IPAddress": "", "DNSNames": null}}}},
 {"Id": "aaaa", "Name": "/shop-migrate-run-1",
  "State": {"Status": "running"},
  "Config": {"Labels": {"com.docker.compose.service": "api", "com.docker.compose.oneoff": "True"}},
  "NetworkSettings": {"Networks": {"mm-17b2-frontend": {"IPAddress": "10.64.17.6", "DNSNames": ["shop-migrate-run-1"]}}}},
 {"Id": "bbbb", "Name": "/exited",
  "State": {"Status": "exited"},
  "Config": {"Labels": {}},
  "NetworkSettings": {"Networks": {"mm-17b2-frontend": {"IPAddress": ""}}}}
]"#;

#[tokio::test]
async fn the_engines_containers_are_read_with_two_commands() -> TestResult {
    let engine = Arc::new(Engine {
        outputs: Mutex::new(VecDeque::from(["f1f7342c31e5\naaaa\nbbbb\n", INSPECTED])),
        commands: Mutex::new(Vec::new()),
    });
    let runtime = DockerComposeRuntime::new(engine.clone());
    let networks = ["mm-17b2-frontend".to_owned(), "mm-17b2-backend".to_owned()];
    let containers = runtime.network_containers(&networks).await?;
    let commands = engine.commands.lock().map_err(|_| "poisoned")?.clone();
    assert_eq!(
        commands[0],
        [
            "ps",
            "--all",
            "--quiet",
            "--no-trunc",
            "--filter",
            "network=mm-17b2-frontend",
            "--filter",
            "network=mm-17b2-backend"
        ]
    );
    assert_eq!(
        commands[1],
        [
            "container",
            "inspect",
            "--format",
            "json",
            "f1f7342c31e5",
            "aaaa",
            "bbbb"
        ]
    );
    assert_eq!(containers.len(), 3);
    let api = &containers[0];
    assert_eq!(
        (api.name.as_str(), api.service.as_deref()),
        ("shop-api-1", Some("api"))
    );
    assert_eq!(api.health, Health::Unhealthy);
    assert_eq!(
        api.attachments,
        vec![Attachment {
            network: "mm-17b2-frontend".to_owned(),
            address: ip("10.64.17.4")?,
            names: ["shop-api-1", "api", "f1f7342c31e5", "backend-api"]
                .map(str::to_owned)
                .to_vec(),
        }],
        "a network the container holds no address on is left out"
    );
    // A job is no member of its service.
    assert_eq!(containers[1].service, None);
    assert_eq!(containers[1].health, Health::Healthy);
    assert_eq!(containers[2].health, Health::Stopped);
    assert!(containers[2].attachments.is_empty());

    // Without networks there is nothing to ask the engine.
    assert!(runtime.network_containers(&[]).await?.is_empty());
    assert_eq!(engine.commands.lock().map_err(|_| "poisoned")?.len(), 2);
    Ok(())
}

const NODE_VIEW_CONFIG: &str = "schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1df0
  name: shop
target: local
networks:
  backend: {}
  sealed: {internal: true}
stacks:
  app:
    files: [compose.yaml]
modules:
  shop:
    path: .
    services:
      web: {stack: app, networks: [backend]}
environments:
  production: {target: production}
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11, private-address: 10.0.0.1}
      - {name: app-2, host: 203.0.113.12, private-address: 10.0.0.2}
";

#[test]
fn a_node_names_a_resolver_file_for_each_network_that_spans_the_nodes() -> TestResult {
    use mm::domain::config::MmConfig;
    use mm::domain::placement::{NodeScope, view_from_node};

    let scope = |resolver_files: Option<&str>| NodeScope {
        target: "production".to_owned(),
        node: "app-2".to_owned(),
        environment: "production".to_owned(),
        resolver_files: resolver_files.map(std::path::PathBuf::from),
        mesh_addresses: Default::default(),
    };
    let files = |config: &MmConfig| {
        config
            .networks
            .iter()
            .map(|(name, network)| (name.clone(), network.resolver.clone()))
            .collect::<Vec<_>>()
    };
    let named = |name: &str| {
        Some(std::path::PathBuf::from(format!(
            "/var/lib/mm/networks/resolv/019c527b6242-production-{name}.conf"
        )))
    };

    let mut config: MmConfig = serde_yaml_ng::from_str(NODE_VIEW_CONFIG)?;
    assert!(
        files(&config).iter().all(|(_, file)| file.is_none()),
        "never read from a file"
    );
    view_from_node(&mut config, &scope(Some("/var/lib/mm/networks/resolv")))?;
    assert_eq!(
        files(&config),
        [
            ("backend".to_owned(), named("backend")),
            ("sealed".to_owned(), named("sealed"))
        ]
    );
    // The field is mmd's own: it is not written back.
    assert!(!serde_yaml_ng::to_string(&config)?.contains("resolv"));

    // A host whose mmd answers no names.
    let mut config: MmConfig = serde_yaml_ng::from_str(NODE_VIEW_CONFIG)?;
    view_from_node(&mut config, &scope(None))?;
    assert!(files(&config).iter().all(|(_, file)| file.is_none()));

    // An existing private interface: the networks stay on each host, and so
    // does the engine's own resolver.
    let own_interface = NODE_VIEW_CONFIG.replace(
        "    nodes:\n",
        "    cluster: {mesh: {interface: eth1}}\n    nodes:\n",
    );
    let mut config: MmConfig = serde_yaml_ng::from_str(&own_interface)?;
    view_from_node(&mut config, &scope(Some("/var/lib/mm/networks/resolv")))?;
    assert!(files(&config).iter().all(|(_, file)| file.is_none()));
    Ok(())
}

#[tokio::test]
async fn neighbours_on_a_network_of_the_host_are_read_too() -> TestResult {
    // web is on a cluster network and on a network of its own host, where
    // redis is its only neighbour.
    let web = r#"[{"Id": "aaaa", "Name": "/shop-web-1", "State": {"Status": "running"},
      "Config": {"Labels": {"com.docker.compose.service": "web"}},
      "NetworkSettings": {"Networks": {
        "mm-17b2-backend": {"IPAddress": "10.64.0.2", "DNSNames": ["shop-web-1", "web"]},
        "shop_cache": {"IPAddress": "172.18.0.2", "DNSNames": ["shop-web-1", "web"]}}}}]"#;
    let both = r#"[{"Id": "aaaa", "Name": "/shop-web-1", "State": {"Status": "running"},
      "Config": {"Labels": {"com.docker.compose.service": "web"}},
      "NetworkSettings": {"Networks": {"shop_cache": {"IPAddress": "172.18.0.2"}}}},
     {"Id": "bbbb", "Name": "/shop-redis-1", "State": {"Status": "running"},
      "Config": {"Labels": {"com.docker.compose.service": "redis"}},
      "NetworkSettings": {"Networks": {"shop_cache": {"IPAddress": "172.18.0.3", "DNSNames": ["shop-redis-1", "redis"]}}}}]"#;
    let engine = Arc::new(Engine {
        outputs: Mutex::new(VecDeque::from(["aaaa\n", web, "aaaa\nbbbb\n", both])),
        commands: Mutex::new(Vec::new()),
    });
    let runtime = DockerComposeRuntime::new(engine.clone());
    let containers = runtime
        .network_containers(&["mm-17b2-backend".to_owned()])
        .await?;
    let commands = engine.commands.lock().map_err(|_| "poisoned")?.clone();
    assert_eq!(commands.len(), 4);
    assert_eq!(
        commands[2],
        [
            "ps",
            "--all",
            "--quiet",
            "--no-trunc",
            "--filter",
            "network=shop_cache"
        ]
    );
    assert_eq!(
        containers
            .iter()
            .map(|container| container.name.as_str())
            .collect::<Vec<_>>(),
        ["shop-web-1", "shop-redis-1"]
    );

    // web finds redis by name; redis is never sent to another node.
    let mut networks = networks("app-1")?;
    if let Some(held) = networks
        .table
        .networks
        .get_mut(BACKEND)
        .and_then(|network| network.nodes.get_mut("app-1"))
    {
        held.network = "mm-17b2-backend".to_owned();
    }
    let local = NodeRecords::observed(&networks, containers);
    let map = ServiceMap {
        networks: &networks,
        local: &local,
        peers: &[],
    };
    assert_eq!(
        map.resolve(ip("10.64.0.2")?, "redis"),
        answer(&["172.18.0.3"])?
    );
    assert_eq!(
        map.resolve(ip("172.18.0.2")?, "redis"),
        answer(&["172.18.0.3"])?
    );
    assert_eq!(local.for_peers().containers.len(), 1);
    Ok(())
}
