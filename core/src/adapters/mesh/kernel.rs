//! What the host's kernel holds for a WireGuard interface, read and changed
//! through `ip` and `wg`. Nothing here ever asks for or prints a private
//! key: the key is handed over as a file path, and state is read with the
//! per-field `wg show` forms, never with `dump`, which includes it.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use zeroize::Zeroizing;

use crate::app::ports::{CommandOutput, CommandSpec, ProcessRunner};
use crate::domain::cluster_network::{BRIDGE_PREFIX, HostRange, HostRangeKind};
use crate::domain::error::MmError;

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Link {
    pub mtu: u16,
    pub up: bool,
}

/// One peer as the interface holds it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct KernelPeer {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    /// Unix seconds of the last handshake; zero when there was none.
    pub latest_handshake: u64,
    pub received_bytes: u64,
    pub sent_bytes: u64,
    pub keepalive: Option<u16>,
}

#[derive(Clone)]
pub(super) struct Kernel {
    process: Arc<dyn ProcessRunner>,
}

fn unexpected(program: &str, what: &str) -> MmError {
    MmError::RuntimeResponse {
        message: format!("`{program}` printed {what} mmd does not understand"),
    }
}

fn failed(program: &str, arguments: &[&str], output: &CommandOutput) -> MmError {
    MmError::ProcessFailed {
        program: format!("{program} {}", arguments.join(" ")),
        code: output.code,
        stderr: output.stderr.trim().to_owned(),
    }
}

impl Kernel {
    pub(super) fn new(process: Arc<dyn ProcessRunner>) -> Self {
        Self { process }
    }

    async fn run(
        &self,
        program: &str,
        arguments: &[&str],
        stdin: Option<Zeroizing<String>>,
    ) -> Result<CommandOutput, MmError> {
        let mut specification = CommandSpec::new(program)
            .args(arguments.iter().copied())
            .environment("LC_ALL", "C")
            .timeout(TIMEOUT);
        if let Some(stdin) = stdin {
            specification = specification.sensitive_stdin(stdin);
        }
        self.process
            .run(specification)
            .await
            .map_err(|error| match error {
                MmError::ProcessSpawn { program, .. } => MmError::Target {
                    message: format!(
                        "`{program}` is not installed on this host; the mesh needs the iproute2 and wireguard-tools packages, which `mm target setup` installs"
                    ),
                },
                other => other,
            })
    }

    /// Runs a command that must succeed and returns what it printed.
    async fn output(&self, program: &str, arguments: &[&str]) -> Result<String, MmError> {
        let output = self.run(program, arguments, None).await?;
        if output.success() {
            Ok(output.stdout.to_string())
        } else {
            Err(failed(program, arguments, &output))
        }
    }

    pub(super) async fn ip(&self, arguments: &[&str]) -> Result<(), MmError> {
        self.output("ip", arguments).await.map(drop)
    }

    pub(super) async fn wg(&self, arguments: &[&str]) -> Result<(), MmError> {
        self.output("wg", arguments).await.map(drop)
    }

    pub(super) async fn set_private_key(
        &self,
        key_directory: &Path,
        interface: &str,
        private_key: &Zeroizing<String>,
    ) -> Result<(), MmError> {
        let path = key_directory.join(format!("{interface}.key"));
        write_key_file(key_directory, &path, private_key)?;
        let path_text = path.to_string_lossy().into_owned();
        let result = self
            .output("wg", &["set", interface, "private-key", &path_text])
            .await
            .map(drop);
        let removed = std::fs::remove_file(&path);
        result?;
        removed.map_err(|source| MmError::Io {
            context: format!("removing {path_text}"),
            source,
        })
    }

    /// A new private key, as `wg genkey` makes it. It is never logged: the
    /// process runner keeps command output out of its records.
    pub(super) async fn generate_key(&self) -> Result<Zeroizing<String>, MmError> {
        let output = self.run("wg", &["genkey"], None).await?;
        if !output.success() || output.stdout.trim().len() != 44 {
            return Err(MmError::Security {
                message: "`wg genkey` did not produce a key".to_owned(),
            });
        }
        Ok(Zeroizing::new(output.stdout.trim().to_owned()))
    }

    /// The public half of `private_key`, which travels on standard input.
    pub(super) async fn derive_public_key(
        &self,
        private_key: &Zeroizing<String>,
    ) -> Result<String, MmError> {
        let stdin = Zeroizing::new(format!("{}\n", private_key.trim()));
        let output = self.run("wg", &["pubkey"], Some(stdin)).await?;
        let public_key = output.stdout.trim().to_owned();
        if !output.success() || crate::domain::mesh::validate_public_key(&public_key).is_err() {
            // Never the command's own output: it could quote its input.
            return Err(MmError::Security {
                message: "the node's WireGuard private key file does not hold a valid key"
                    .to_owned(),
            });
        }
        Ok(public_key)
    }

    /// The interface's MTU and whether it is up; `None` when it does not exist.
    pub(super) async fn link(&self, interface: &str) -> Result<Option<Link>, MmError> {
        let arguments = ["-j", "link", "show", "dev", interface];
        let output = self.run("ip", &arguments, None).await?;
        if !output.success() {
            if output.stderr.contains("does not exist") {
                return Ok(None);
            }
            return Err(failed("ip", &arguments, &output));
        }
        let links: Vec<serde_json::Value> = serde_json::from_str(&output.stdout)
            .map_err(|_| unexpected("ip -j link show", "a link list"))?;
        let Some(link) = links.first() else {
            return Ok(None);
        };
        let mtu = link["mtu"]
            .as_u64()
            .and_then(|mtu| u16::try_from(mtu).ok())
            .ok_or_else(|| unexpected("ip -j link show", "an MTU"))?;
        let up = link["flags"]
            .as_array()
            .is_some_and(|flags| flags.iter().any(|flag| flag.as_str() == Some("UP")));
        Ok(Some(Link { mtu, up }))
    }

    /// The interface's IPv4 addresses with their prefix lengths.
    pub(super) async fn addresses(&self, interface: &str) -> Result<Vec<(Ipv4Addr, u8)>, MmError> {
        let text = self
            .output("ip", &["-j", "-4", "address", "show", "dev", interface])
            .await?;
        let links: Vec<serde_json::Value> = serde_json::from_str(&text)
            .map_err(|_| unexpected("ip -j address show", "an address list"))?;
        Ok(links
            .iter()
            .filter_map(|link| link["addr_info"].as_array())
            .flatten()
            .filter_map(|address| {
                Some((
                    address["local"].as_str()?.parse().ok()?,
                    u8::try_from(address["prefixlen"].as_u64()?).ok()?,
                ))
            })
            .collect())
    }

    /// The prefixes `mmd` routed through the interface.
    pub(super) async fn routes(&self, interface: &str) -> Result<Vec<String>, MmError> {
        let text = self
            .output(
                "ip",
                &[
                    "-j", "-4", "route", "show", "dev", interface, "proto", "static",
                ],
            )
            .await?;
        let routes: Vec<serde_json::Value> = serde_json::from_str(&text)
            .map_err(|_| unexpected("ip -j route show", "a route list"))?;
        Ok(routes
            .iter()
            .filter_map(|route| route["dst"].as_str().map(str::to_owned))
            .collect())
    }

    /// Every IPv4 address the host holds and every route it has, except
    /// on the interfaces in `own`, on the bridges of cluster networks, and
    /// on the loopback interface. The default route is not a range.
    pub(super) async fn host_ranges(&self, own: &[String]) -> Result<Vec<HostRange>, MmError> {
        let text = self
            .output("ip", &["-j", "-4", "route", "show", "table", "all"])
            .await?;
        let routes: Vec<serde_json::Value> = serde_json::from_str(&text)
            .map_err(|_| unexpected("ip -j route show table all", "a route list"))?;
        let mut ranges = Vec::<HostRange>::new();
        for route in &routes {
            let interface = route["dev"].as_str();
            if interface.is_some_and(|interface| {
                interface == "lo"
                    || interface.starts_with(BRIDGE_PREFIX)
                    || own.iter().any(|own| own == interface)
            }) {
                continue;
            }
            let kind = match route["type"].as_str() {
                Some("local") => HostRangeKind::Address,
                Some("broadcast" | "multicast" | "anycast") => continue,
                _ => HostRangeKind::Route,
            };
            let Some(range) = route["dst"].as_str().and_then(|destination| {
                if destination.contains('/') {
                    destination.parse().ok()
                } else {
                    format!("{destination}/32").parse().ok()
                }
            }) else {
                // `default`, or a destination that is not IPv4.
                continue;
            };
            let range = HostRange {
                range,
                kind,
                interface: interface.map(str::to_owned),
            };
            if !ranges.contains(&range) {
                ranges.push(range);
            }
        }
        Ok(ranges)
    }

    async fn field(&self, interface: &str, field: &str) -> Result<String, MmError> {
        self.output("wg", &["show", interface, field]).await
    }

    /// The interface's own public key, when it has a private key.
    pub(super) async fn public_key(&self, interface: &str) -> Result<Option<String>, MmError> {
        let key = self.field(interface, "public-key").await?;
        let key = key.trim();
        Ok((!key.is_empty() && key != "(none)").then(|| key.to_owned()))
    }

    pub(super) async fn listen_port(&self, interface: &str) -> Result<u16, MmError> {
        self.field(interface, "listen-port")
            .await?
            .trim()
            .parse()
            .map_err(|_| unexpected("wg show listen-port", "a port"))
    }

    /// Every peer of the interface, in the order WireGuard lists them.
    pub(super) async fn peers(&self, interface: &str) -> Result<Vec<KernelPeer>, MmError> {
        let mut order = Vec::new();
        let mut peers = BTreeMap::<String, KernelPeer>::new();
        for field in [
            "endpoints",
            "allowed-ips",
            "latest-handshakes",
            "transfer",
            "persistent-keepalive",
        ] {
            let text = self.field(interface, field).await?;
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                let mut columns = line.split('\t');
                let key = columns.next().unwrap_or_default().to_owned();
                let values = columns.collect::<Vec<_>>();
                if !peers.contains_key(&key) {
                    order.push(key.clone());
                }
                let peer = peers.entry(key.clone()).or_insert_with(|| KernelPeer {
                    public_key: key,
                    ..KernelPeer::default()
                });
                read_field(peer, field, &values);
            }
        }
        Ok(order
            .into_iter()
            .filter_map(|key| peers.remove(&key))
            .collect())
    }
}

/// One `wg show <interface> <field>` line into `peer`. A value mmd cannot
/// read counts as absent.
fn read_field(peer: &mut KernelPeer, field: &str, values: &[&str]) {
    let first = values.first().copied().unwrap_or_default();
    let number = |text: &str| text.trim().parse::<u64>().unwrap_or(0);
    match field {
        "endpoints" => {
            peer.endpoint = (first != "(none)" && !first.is_empty()).then(|| first.to_owned())
        }
        "allowed-ips" => {
            peer.allowed_ips = first
                .split_whitespace()
                .filter(|prefix| *prefix != "(none)")
                .map(str::to_owned)
                .collect();
        }
        "latest-handshakes" => peer.latest_handshake = number(first),
        "transfer" => {
            peer.received_bytes = number(first);
            peer.sent_bytes = number(values.get(1).copied().unwrap_or_default());
        }
        "persistent-keepalive" => peer.keepalive = first.trim().parse().ok(),
        _ => {}
    }
}

fn write_key_file(
    directory: &Path,
    path: &Path,
    private_key: &Zeroizing<String>,
) -> Result<(), MmError> {
    use std::io::Write as _;
    let io = |context: String| move |source| MmError::Io { context, source };
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder
        .create(directory)
        .map_err(io(format!("creating {}", directory.display())))?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(path)
        .map_err(io(format!("writing {}", path.display())))?;
    let text = Zeroizing::new(format!("{}\n", private_key.trim()));
    file.write_all(text.as_bytes())
        .map_err(io(format!("writing {}", path.display())))
}
