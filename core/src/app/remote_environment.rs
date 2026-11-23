//! A remote environment as the client sees it: one host, or every node of a
//! cluster target.
//!
//! The client coordinates. It asks each node's `mmd` to plan, binds the node
//! plans under one hash, and applies them node by node; a node only ever
//! acts on its own engine. A target that names one host is the same thing
//! with one unnamed node, so the command line, the terminal UI, and the MCP
//! server all deploy, sync access, and read status through this one type.

use std::net::Ipv4Addr;
use std::sync::Arc;

use futures_util::future::join_all;

use crate::app::ports::RemoteTransport;
use crate::domain::error::MmError;
use crate::domain::placement::NodeServices;
use crate::domain::remote::{NodeReport, NodeState, RemoteScope, unreachable_help};

#[path = "remote_environment/cancel.rs"]
mod cancel;
#[path = "remote_environment/delivery.rs"]
mod delivery;
#[path = "remote_environment/ingress.rs"]
mod ingress;
#[path = "remote_environment/mesh.rs"]
mod mesh;
#[path = "remote_environment/networks.rs"]
mod networks;
#[path = "remote_environment/operate.rs"]
mod operate;
#[path = "remote_environment/plans.rs"]
mod plans;
#[path = "remote_environment/progress.rs"]
mod progress;
#[path = "remote_environment/reporting.rs"]
mod reporting;
#[path = "remote_environment/rollback.rs"]
mod rollback;
#[path = "remote_environment/rollout.rs"]
mod rollout;
#[path = "remote_environment/runtimes.rs"]
mod runtimes;
#[path = "remote_environment/services.rs"]
mod services;
#[path = "remote_environment/storage.rs"]
mod storage;
#[path = "remote_environment/store.rs"]
mod store;
#[path = "remote_environment/values.rs"]
mod values;
#[path = "remote_environment/versions.rs"]
mod versions;
pub use services::{SILENT_NODE_PATIENCE, StatusSoFar};
pub use values::{KeyEdit, StageReport, ValueEdit, ValueSealer};

/// Why mm has no connection to a node: what is wrong and what to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeUnavailable {
    pub subject: String,
    pub problem: String,
    pub help: String,
}

impl NodeUnavailable {
    fn error(&self) -> MmError {
        MmError::Unreachable {
            subject: self.subject.clone(),
            problem: self.problem.clone(),
            help: self.help.clone(),
        }
    }
}

/// One host of a remote environment and the connection to its `mmd`.
#[derive(Clone)]
pub struct RemoteNode {
    /// The node's name; `None` for a target that names one host.
    pub name: Option<String>,
    /// Where this device connects to the node.
    pub host: String,
    pub port: u16,
    /// The repo's host and port for the node, when this device connects at
    /// an address of its own instead.
    pub repo_address: Option<(String, u16)>,
    /// The node's address on the cluster's private network.
    pub private_address: Option<Ipv4Addr>,
    /// The connection, or why there is none (a node not set up yet).
    pub transport: Result<Arc<dyn RemoteTransport>, NodeUnavailable>,
    /// The scope of every request to this host; it names the node.
    pub scope: RemoteScope,
    /// The services this checkout's configuration places on the node;
    /// `None` on a single host, which runs them all.
    pub placed: Option<NodeServices>,
}

impl std::fmt::Debug for RemoteNode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteNode")
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl RemoteNode {
    fn connect(&self) -> Result<&Arc<dyn RemoteTransport>, MmError> {
        self.transport.as_ref().map_err(NodeUnavailable::error)
    }

    /// The connection and scope for one request to this host.
    pub fn connection(&self) -> Result<(Arc<dyn RemoteTransport>, RemoteScope), MmError> {
        Ok((Arc::clone(self.connect()?), self.scope.clone()))
    }
}

/// Every host of one remote environment, in the target's declared order.
#[derive(Debug, Clone)]
pub struct RemoteEnvironment {
    target: String,
    nodes: Vec<RemoteNode>,
    /// The private interface the nodes already share, when the target names
    /// one instead of having mm create a WireGuard mesh.
    existing_interface: Option<String>,
    /// The address space, mesh interface, and MTU of the networks that span
    /// the nodes; `None` when the target's networks stay local to each host.
    cluster_networks: Option<crate::domain::cluster_network::SealedNetworks>,
    /// The node that keeps the target's shared volumes.
    storage_node: Option<String>,
    /// Set when the target's hosts take the project's files from mm
    /// instead of fetching them.
    pushed: Option<delivery::PushedCommits>,
    /// What each node said to the last status read of this value: a status
    /// read again shows it for a node until the node answers anew.
    last_status: services::LastStatus,
    /// How long a host may show no progress on a step before the client
    /// stops waiting (`deploy.progress-timeout`).
    progress_timeout: std::time::Duration,
    /// The environment's `deploy.mm-version`.
    version_pin: Option<crate::domain::version_pin::VersionPin>,
    /// Every environment of the project, which tells this environment's
    /// networks from another's (`domain::cluster_network::environment_network`).
    /// Empty: a plan takes no network away.
    environments: Vec<String>,
}

impl RemoteEnvironment {
    pub fn new(target: String, nodes: Vec<RemoteNode>) -> Result<Self, MmError> {
        if nodes.is_empty() {
            return Err(MmError::Target {
                message: format!("target '{target}' names no host"),
            });
        }
        Ok(Self {
            target,
            nodes,
            existing_interface: None,
            cluster_networks: None,
            storage_node: None,
            pushed: None,
            last_status: services::LastStatus::default(),
            progress_timeout: crate::domain::config::DEFAULT_PROGRESS_TIMEOUT,
            version_pin: None,
            environments: Vec::new(),
        })
    }

    /// How long a host may show no progress before the client stops
    /// waiting for it.
    #[must_use]
    pub fn with_progress_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.progress_timeout = timeout;
        self
    }

    /// For a cluster target with `cluster.mesh.interface`: the nodes reach
    /// each other on that interface and mm manages no WireGuard mesh.
    #[must_use]
    pub fn on_existing_interface(mut self, interface: Option<String>) -> Self {
        self.existing_interface = interface;
        self
    }

    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Whether the target lists nodes rather than one host.
    #[must_use]
    pub fn is_cluster(&self) -> bool {
        self.nodes.iter().any(|node| node.name.is_some())
    }

    #[must_use]
    pub fn nodes(&self) -> &[RemoteNode] {
        &self.nodes
    }

    /// The host that answers for the environment as a whole, such as who the
    /// caller is: the single host, or the first node mm can connect to.
    pub fn first_connection(&self) -> Result<(Arc<dyn RemoteTransport>, RemoteScope), MmError> {
        match self.nodes.iter().find(|node| node.transport.is_ok()) {
            Some(node) => node.connection(),
            None => self.first().connection(),
        }
    }

    fn first(&self) -> &RemoteNode {
        &self.nodes[0]
    }

    /// Ends the connections kept open to the hosts. After access changed on
    /// them, the next request authenticates again from nothing.
    pub fn close_kept_connections(&self) {
        for node in &self.nodes {
            if let Ok(transport) = &node.transport {
                transport.close_kept_connection();
            }
        }
    }

    /// The scope shared by every node: the node's own scope without a name.
    fn target_scope(&self) -> RemoteScope {
        RemoteScope {
            node: None,
            ..self.first().scope.clone()
        }
    }

    fn name_of<'a>(&'a self, node: &'a RemoteNode) -> &'a str {
        node.name.as_deref().unwrap_or(&self.target)
    }

    /// The nodes `only` selects, in declared order, and the names it leaves
    /// out. `None` selects every node.
    fn select(&self, only: Option<&[String]>) -> Result<(Vec<&RemoteNode>, Vec<String>), MmError> {
        let Some(only) = only else {
            return Ok((self.nodes.iter().collect(), Vec::new()));
        };
        if !self.is_cluster() {
            return Err(MmError::Target {
                message: format!(
                    "target '{}' names one host; --nodes applies to a target that lists nodes:",
                    self.target
                ),
            });
        }
        let known = self
            .nodes
            .iter()
            .filter_map(|node| node.name.as_deref())
            .collect::<Vec<_>>();
        if let Some(unknown) = only.iter().find(|name| !known.contains(&name.as_str())) {
            return Err(MmError::Target {
                message: format!(
                    "--nodes names '{unknown}', which is not a node of target '{}'; its nodes are {}",
                    self.target,
                    known.join(", ")
                ),
            });
        }
        if only.is_empty() {
            return Err(MmError::Target {
                message: format!("--nodes needs at least one of {}", known.join(", ")),
            });
        }
        let (selected, skipped): (Vec<_>, Vec<_>) = self.nodes.iter().partition(|node| {
            node.name
                .as_ref()
                .is_some_and(|name| only.iter().any(|wanted| wanted == name))
        });
        Ok((
            selected,
            skipped
                .into_iter()
                .filter_map(|node| node.name.clone())
                .collect(),
        ))
    }

    /// Every answer, or one error that names each node that failed. Nodes
    /// that did not answer come with the `--nodes` list that leaves them out.
    fn all_or_explain<T>(
        &self,
        asked: &[&RemoteNode],
        answers: Vec<Result<T, MmError>>,
    ) -> Result<Vec<T>, MmError> {
        let mut values = Vec::with_capacity(answers.len());
        let mut unreachable = Vec::new();
        let mut refused = Vec::new();
        for (node, answer) in asked.iter().zip(answers) {
            match answer {
                Ok(value) => values.push(value),
                Err(MmError::Unreachable {
                    subject,
                    problem,
                    help,
                }) => unreachable.push((self.name_of(node), subject, problem, help)),
                Err(error) => refused.push((node.name.as_deref(), error)),
            }
        }
        if unreachable.is_empty() {
            return match every_refusal(refused) {
                Some(error) => Err(error),
                None => Ok(values),
            };
        }
        let missing = unreachable
            .iter()
            .map(|(name, ..)| *name)
            .collect::<Vec<_>>();
        let others = asked
            .iter()
            .map(|node| self.name_of(node))
            .filter(|name| !missing.contains(name))
            .collect::<Vec<_>>();
        let hint = if self.is_cluster() {
            format!(". {}", unreachable_help(&missing, &others))
        } else {
            String::new()
        };
        let (_, subject, problem, help) = &unreachable[0];
        let also = unreachable
            .iter()
            .skip(1)
            .map(|(_, subject, problem, _)| format!("; {subject} {problem}"))
            .collect::<String>();
        Err(MmError::Unreachable {
            subject: subject.clone(),
            problem: format!("{problem}{also}"),
            help: format!("{help}{hint}"),
        })
    }

    /// Every node of the target: whether it answers, its `mmd` version, its
    /// address on the private network, and what it last applied. Never
    /// fails; a node that does not answer is a row that says why.
    pub async fn node_reports(&self) -> Vec<NodeReport> {
        join_all(self.nodes.iter().map(|node| self.node_report(node))).await
    }

    async fn node_report(&self, node: &RemoteNode) -> NodeReport {
        let mut report = NodeReport {
            name: self.name_of(node).to_owned(),
            host: node.host.clone(),
            port: node.port,
            repo_address: node
                .repo_address
                .as_ref()
                .map(|(host, port)| crate::domain::target::host_and_port(host, *port)),
            mesh_address: node.private_address,
            state: NodeState::Ready,
            reachable: true,
            problem: None,
            mmd_version: None,
            deployed: None,
            rollout: None,
            engine: None,
            daemon: None,
            secret_store: None,
        };
        let failed = |report: &mut NodeReport, state: NodeState, problem: String| {
            report.state = state;
            report.reachable = false;
            report.problem = Some(problem);
        };
        let transport = match &node.transport {
            Ok(transport) => transport,
            Err(unavailable) => {
                failed(
                    &mut report,
                    NodeState::NotSetUp,
                    format!("{}. {}", unavailable.problem, unavailable.help),
                );
                return report;
            }
        };
        match transport.node(node.scope.clone()).await {
            Ok(facts) => {
                report.mmd_version = Some(facts.daemon_version);
                report.deployed = facts.deployed;
                report.rollout = facts.rollout;
                report.engine = facts.engine;
                report.daemon = facts.daemon;
                report.secret_store = facts.secret_store;
                if let Some(requested) = &node.name
                    && facts.node.as_ref() != Some(requested)
                {
                    let mismatch = MmError::WrongNode {
                        requested: requested.clone(),
                        actual: facts.node,
                    };
                    failed(&mut report, NodeState::WrongNode, mismatch.to_string());
                }
            }
            Err(MmError::Unreachable { problem, help, .. }) => {
                let state = if crate::domain::remote::refused_handshake(&problem) {
                    NodeState::Refused
                } else {
                    NodeState::Unreachable
                };
                failed(&mut report, state, format!("{problem}. {help}"));
            }
            Err(error @ (MmError::Remote { .. } | MmError::Security { .. })) => {
                failed(&mut report, NodeState::Refused, error.to_string());
            }
            Err(error) => failed(&mut report, NodeState::Unreachable, error.to_string()),
        }
        report
    }
}

/// Says which node a refusal came from.
/// The first node's refusal, naming every other node that refused too:
/// what one node refuses for its configuration, others often refuse too.
fn every_refusal(refused: Vec<(Option<&str>, MmError)>) -> Option<MmError> {
    let mut refused = refused.into_iter();
    let (node, first) = refused.next()?;
    let code = first.code();
    let others = refused
        .map(|(other, error)| {
            let name = other.unwrap_or("the host");
            if error.code() == code {
                format!("'{name}'")
            } else {
                format!("'{name}' ({})", error.code())
            }
        })
        .collect::<Vec<_>>();
    let first = in_node(node, first);
    Some(match others.as_slice() {
        [] => first,
        [other] => with_message(first, |message| {
            format!("{message}; node {other} refused it too")
        }),
        [rest @ .., last] => with_message(first, |message| {
            format!(
                "{message}; nodes {} and {last} refused it too",
                rest.join(", ")
            )
        }),
    })
}

fn in_node(node: Option<&str>, error: MmError) -> MmError {
    let Some(node) = node else {
        return error;
    };
    with_message(error, |message| format!("node '{node}': {message}"))
}

/// `error` with its message changed by `named`, for the kinds that carry
/// one; any other kind as it is.
fn with_message(error: MmError, named: impl FnOnce(String) -> String) -> MmError {
    match error {
        MmError::Security { message } => MmError::Security {
            message: named(message),
        },
        MmError::Target { message } => MmError::Target {
            message: named(message),
        },
        MmError::Environment { message } => MmError::Environment {
            message: named(message),
        },
        MmError::State { message } => MmError::State {
            message: named(message),
        },
        MmError::RuntimeResponse { message } => MmError::RuntimeResponse {
            message: named(message),
        },
        MmError::Launch { message } => MmError::Launch {
            message: named(message),
        },
        MmError::Placement { message } => MmError::Placement {
            message: named(message),
        },
        MmError::Build { message } => MmError::Build {
            message: named(message),
        },
        MmError::Secret { message } => MmError::Secret {
            message: named(message),
        },
        MmError::Remote { code, message } => MmError::Remote {
            code,
            message: named(message),
        },
        MmError::Timeout { message } => MmError::Timeout {
            message: named(message),
        },
        MmError::Cancelled { message } => MmError::Cancelled {
            message: named(message),
        },
        MmError::ReleaseInput { message } => MmError::ReleaseInput {
            message: named(message),
        },
        MmError::InOwnWords(error) => with_message(*error, named).in_own_words(),
        other => other,
    }
}
