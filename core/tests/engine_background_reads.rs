//! What a node's daemon reads from the engine on its own clock, outside
//! requests: the containers on its networks for the name resolver, and the
//! containers that wait for shared storage. Where the engine's API is
//! read, both come from its socket and no engine process is started.
//!
//! The engine here is a small HTTP server on a Unix socket that lists
//! containers by the filters Docker Engine knows (`network`, `label`,
//! `status`) and answers an inspect request with the container's document.

#![cfg(unix)]

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mm::adapters::engine_api::EngineApi;
use mm::adapters::runtime::DockerComposeRuntime;
use mm::app::ports::{CommandOutput, CommandSpec, ProcessRunner, RuntimeDriver};
use mm::domain::discovery::{Attachment, Health};
use mm::domain::error::MmError;
use mm::domain::shared_storage::{STORAGE_LABEL, WaitingContainer};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixListener;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A process runner that proves nothing was started.
struct NoProcesses;

#[async_trait::async_trait]
impl ProcessRunner for NoProcesses {
    async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
        Err(MmError::State {
            message: "a read from the engine's API started a process".to_owned(),
        })
    }
}

struct Engine {
    socket: PathBuf,
    requests: Arc<Mutex<Vec<String>>>,
    _directory: tempfile::TempDir,
}

impl Engine {
    /// A Docker runtime that reads this engine's API and can start nothing.
    fn runtime(&self) -> DockerComposeRuntime {
        DockerComposeRuntime::new(Arc::new(NoProcesses))
            .reading_engine_api(Some(EngineApi::at(&self.socket)))
    }

    fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .map(|requests| requests.clone())
            .unwrap_or_default()
    }
}

/// An engine that holds `containers`, each an inspect document.
fn start_engine(containers: Vec<Value>) -> Result<Engine, Box<dyn std::error::Error>> {
    let directory = temp_dirs::tempdir()?;
    let socket = directory.path().join("engine.sock");
    let listener = UnixListener::bind(&socket)?;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&requests);
    let containers = Arc::new(containers);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let containers = Arc::clone(&containers);
            let seen = Arc::clone(&seen);
            tokio::spawn(async move {
                if let Err(error) = serve(stream, &containers, &seen).await {
                    eprintln!("the fake engine failed: {error}");
                }
            });
        }
    });
    Ok(Engine {
        socket,
        requests,
        _directory: directory,
    })
}

fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| text.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// Whether the engine would list `container` under `filters`: every kind
/// of filter must match, and one value of a kind is enough.
fn listed(container: &Value, filters: &Value) -> bool {
    let any = |kind: &str, matches: &dyn Fn(&str) -> bool| {
        filters[kind]
            .as_array()
            .is_none_or(|values| values.iter().filter_map(Value::as_str).any(matches))
    };
    any("network", &|network| {
        container["NetworkSettings"]["Networks"]
            .get(network)
            .is_some()
    }) && any("label", &|label| {
        container["Config"]["Labels"].get(label).is_some()
    }) && any("status", &|status| container["State"]["Status"] == status)
}

async fn serve(
    stream: tokio::net::UnixStream,
    containers: &[Value],
    seen: &Mutex<Vec<String>>,
) -> std::io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let Some(request) = lines.next_line().await? else {
        return Ok(());
    };
    while let Some(header) = lines.next_line().await? {
        if header.is_empty() {
            break;
        }
    }
    let path = percent_decoded(request.split(' ').nth(1).unwrap_or_default());
    if let Ok(mut seen) = seen.lock() {
        seen.push(path.clone());
    }
    let (route, query) = path.split_once('?').unwrap_or((&path, ""));
    let parts = route.trim_matches('/').split('/').collect::<Vec<_>>();
    let answer = match parts.as_slice() {
        ["containers", "json"] => {
            let filters = query
                .split('&')
                .find_map(|pair| pair.strip_prefix("filters="))
                .and_then(|filters| serde_json::from_str::<Value>(filters).ok())
                .unwrap_or_default();
            Some(Value::Array(
                containers
                    .iter()
                    .filter(|container| listed(container, &filters))
                    .map(|container| json!({"Id": container["Id"]}))
                    .collect(),
            ))
        }
        ["containers", id, "json"] => containers
            .iter()
            .find(|container| container["Id"] == *id)
            .cloned(),
        _ => None,
    };
    let (status, body) = match answer {
        Some(document) => ("200 OK", document.to_string()),
        None => ("404 Not Found", "{\"message\":\"not found\"}".to_owned()),
    };
    writer
        .write_all(
            format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
}

/// A container's inspect document. `networks` are the networks it is on
/// with its address there; a stopped container holds none.
fn container(id: &str, service: &str, state: &str, networks: &[(&str, &str)]) -> Value {
    let networks = networks
        .iter()
        .map(|(network, address)| {
            (
                (*network).to_owned(),
                json!({
                    "IPAddress": address,
                    "DNSNames": [format!("shop-{service}-1"), service],
                    "Aliases": [service, id],
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    json!({
        "Id": id,
        "Name": format!("/shop-{service}-1"),
        "Config": {"Labels": {"com.docker.compose.service": service}},
        "State": {
            "Status": state,
            "Error": "",
            "Health": {"Status": if state == "running" { "starting" } else { "" }},
        },
        "HostConfig": {"RestartPolicy": {"Name": "unless-stopped"}},
        "NetworkSettings": {"Networks": networks},
    })
}

#[tokio::test]
async fn the_resolvers_look_at_a_networks_containers_starts_no_process() -> TestResult {
    let engine = start_engine(vec![
        // On the cluster network and on a network of its own host.
        container(
            "aaa",
            "web",
            "running",
            &[("mm-backend", "10.64.1.2"), ("shop_default", "172.18.0.2")],
        ),
        // Its neighbour on the host's network only.
        container("bbb", "cache", "running", &[("shop_default", "172.18.0.3")]),
        // On neither.
        container("ccc", "other", "running", &[("elsewhere", "172.19.0.2")]),
        // Stopped: on the cluster network, without an address.
        container("ddd", "worker", "exited", &[("mm-backend", "")]),
    ])?;
    let containers = engine
        .runtime()
        .network_containers(&["mm-backend".to_owned()])
        .await?;
    assert_eq!(
        containers
            .iter()
            .map(|container| (
                container.id.as_str(),
                container.service.as_deref(),
                container.health
            ))
            .collect::<Vec<_>>(),
        [
            ("aaa", Some("web"), Health::Starting),
            ("ddd", Some("worker"), Health::Stopped),
            ("bbb", Some("cache"), Health::Starting),
        ],
        "the network's containers, then the neighbours on its containers' other networks"
    );
    assert_eq!(
        containers[0].attachments,
        [
            Attachment {
                network: "mm-backend".to_owned(),
                address: Ipv4Addr::new(10, 64, 1, 2),
                names: vec!["shop-web-1".to_owned(), "web".to_owned(), "aaa".to_owned()],
            },
            Attachment {
                network: "shop_default".to_owned(),
                address: Ipv4Addr::new(172, 18, 0, 2),
                names: vec!["shop-web-1".to_owned(), "web".to_owned(), "aaa".to_owned()],
            },
        ]
    );
    assert!(containers[1].attachments.is_empty());
    // One list and one inspect per container, twice: the network, then
    // the neighbours. Containers are inspected at once, in no fixed order.
    let requests = engine.requests();
    let (lists, mut inspects): (Vec<_>, Vec<_>) = requests
        .iter()
        .map(String::as_str)
        .partition(|request| request.starts_with("/containers/json?"));
    inspects.sort_unstable();
    assert_eq!(
        lists,
        [
            "/containers/json?all=true&filters={\"network\":[\"mm-backend\"]}",
            "/containers/json?all=true&filters={\"network\":[\"shop_default\"]}",
        ]
    );
    assert_eq!(
        inspects,
        [
            "/containers/aaa/json",
            "/containers/aaa/json",
            "/containers/bbb/json",
            "/containers/ddd/json",
        ]
    );

    // No network to look at: nothing is asked.
    let engine = start_engine(Vec::new())?;
    assert!(engine.runtime().network_containers(&[]).await?.is_empty());
    assert!(engine.requests().is_empty());
    Ok(())
}

#[tokio::test]
async fn the_storage_keepers_look_for_waiting_containers_starts_no_process() -> TestResult {
    let mut waiting = container("aaa", "web", "created", &[]);
    waiting["Config"]["Labels"][STORAGE_LABEL] = json!("10.90.0.2");
    waiting["State"]["Error"] = json!("error while mounting volume: connection timed out");
    let mut running = container("bbb", "web", "running", &[]);
    running["Config"]["Labels"][STORAGE_LABEL] = json!("10.90.0.2");
    // Exited, and nothing of shared storage.
    let unlabelled = container("ccc", "job", "exited", &[]);
    let engine = start_engine(vec![waiting, running, unlabelled])?;

    assert_eq!(
        engine.runtime().waiting_containers(STORAGE_LABEL).await?,
        [WaitingContainer {
            id: "aaa".to_owned(),
            name: "shop-web-1".to_owned(),
            storage: "10.90.0.2".to_owned(),
            state: "created".to_owned(),
            error: "error while mounting volume: connection timed out".to_owned(),
            restart_policy: "unless-stopped".to_owned(),
        }]
    );
    assert_eq!(
        engine.requests(),
        [
            format!(
                "/containers/json?all=true&filters={{\"label\":[\"{STORAGE_LABEL}\"],\"status\":[\"created\",\"exited\"]}}"
            ),
            "/containers/aaa/json".to_owned(),
        ]
    );

    // Nothing waits: the list is the only request.
    let engine = start_engine(vec![container("bbb", "web", "running", &[])])?;
    assert!(
        engine
            .runtime()
            .waiting_containers(STORAGE_LABEL)
            .await?
            .is_empty()
    );
    assert_eq!(engine.requests().len(), 1);
    Ok(())
}
