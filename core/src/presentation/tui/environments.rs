//! Every environment of the project: where each runs, how it is doing, and
//! switching the TUI to another one.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use crate::domain::error::MmError;
use crate::domain::runtime::{RuntimeKind, ServiceSnapshot, ServiceState};
use crate::domain::workspace::WorkspaceSnapshot;
use crate::presentation::tui::loading::{LoadProgress, Loaded};
use crate::presentation::tui::model::{Model, Notice, TeamMemberView};
use crate::presentation::tui::session::LaunchRequest;

/// A check older than this is repeated when the Environments view opens.
const STALE_AFTER: Duration = Duration::from_secs(30);

/// One configured environment and where it runs, read from configuration alone.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvironmentEntry {
    pub name: String,
    /// The project's default environment.
    pub default: bool,
    pub target: String,
    /// The remote host, or `None` when the environment runs on this machine.
    pub remote: Option<String>,
    pub runtime: Option<RuntimeKind>,
    /// Branches a remote deploy must come from; empty when unrestricted.
    pub deploy_from: Vec<String>,
    /// Why the target or runtime could not be resolved.
    pub problem: Option<String>,
}

/// What one status check of an environment found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvironmentProbe {
    pub services: Vec<ServiceLine>,
    pub routes: usize,
    /// Warnings and errors reported by the environment.
    pub issues: usize,
    /// Who the host says you are, for a remote environment.
    pub principal: Option<String>,
    pub access: Option<String>,
    pub deployed_commit: Option<String>,
    /// The nodes of a cluster target, in declared order.
    pub nodes: Vec<NodeLine>,
}

/// One service as the Environments view lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceLine {
    pub name: String,
    pub state: ServiceState,
    /// The state and the ready containers in the words of `mm service
    /// status`: `running 3/3`, `starting 2/3`.
    pub text: String,
}

impl From<&ServiceSnapshot> for ServiceLine {
    fn from(service: &ServiceSnapshot) -> Self {
        Self {
            name: service.name.clone(),
            state: service.state,
            text: crate::presentation::output::service_state_text(service),
        }
    }
}

/// One node of a cluster target as the Environments view lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLine {
    pub name: String,
    pub ready: bool,
    /// The node's version and deployed commit, or what is wrong with it.
    pub detail: String,
}

impl From<&crate::domain::remote::NodeReport> for NodeLine {
    fn from(report: &crate::domain::remote::NodeReport) -> Self {
        let detail = match (&report.problem, &report.mmd_version) {
            (Some(problem), _) => problem.clone(),
            (None, version) => format!(
                "mmd {}  {}",
                version.as_deref().unwrap_or("?"),
                report.deployed.as_ref().map_or_else(
                    || "nothing deployed".to_owned(),
                    |deployed| deployed.commit.chars().take(12).collect(),
                )
            ),
        };
        // The engine `mm target nodes --output json` names, in a few words.
        let detail = match &report.engine {
            Some(engine) => format!("{detail}  {}", engine.summary()),
            None => detail,
        };
        let detail = match report.rollout_summary() {
            Some(rollout) => format!("{detail}  {rollout}"),
            None => detail,
        };
        let detail = match &report.repo_address {
            Some(_) => format!(
                "{detail}  via {} (this device's address)",
                crate::domain::target::host_and_port(&report.host, report.port)
            ),
            None => detail,
        };
        Self {
            name: report.name.clone(),
            ready: report.reachable,
            detail,
        }
    }
}

impl NodeLine {
    /// Adds the node's mesh in a few words, from the report
    /// `mm target mesh status` prints.
    #[must_use]
    pub fn with_mesh(mut self, mesh: Option<&crate::domain::mesh::MeshNodeReport>) -> Self {
        if let Some(summary) = mesh.and_then(crate::domain::mesh::MeshNodeReport::summary) {
            self.detail.push_str("  ");
            self.detail.push_str(&summary);
        }
        self
    }
}

impl NodeLine {
    /// Adds the node's shared storage in a few words, from the report
    /// `mm target storage status` prints.
    #[must_use]
    pub fn with_storage(
        mut self,
        report: Option<&crate::domain::shared_storage::StorageReport>,
    ) -> Self {
        let summary = report.and_then(|report| {
            report
                .nodes
                .iter()
                .find(|node| node.name == self.name)
                .and_then(|node| node.summary(!report.volumes.is_empty()))
        });
        if let Some(summary) = summary {
            self.detail.push_str("  ");
            self.detail.push_str(&summary);
        }
        self
    }
}

/// A node as the live status of the environment on screen knows it: that it
/// answers and the commit it runs, or what is wrong with it. The status
/// carries no `mmd` version and no mesh; a check of the target's nodes does.
impl From<&crate::domain::workspace::NodeStatus> for NodeLine {
    fn from(status: &crate::domain::workspace::NodeStatus) -> Self {
        let detail = match (&status.problem, &status.deployment) {
            (Some(problem), _) => problem.clone(),
            (None, _) if status.is_pending() => "not answered yet".to_owned(),
            (None, Some(deployment)) => format!(
                "answering  {}",
                deployment.commit.chars().take(12).collect::<String>()
            ),
            (None, None) => "answering".to_owned(),
        };
        Self {
            name: status.name.clone(),
            ready: status.reachable,
            detail,
        }
    }
}

impl EnvironmentProbe {
    #[must_use]
    pub fn running(&self) -> usize {
        self.services
            .iter()
            .filter(|service| service.state.is_up())
            .count()
    }
}

#[derive(Debug, Clone)]
pub enum ProbeState {
    Checking,
    Ready {
        probe: EnvironmentProbe,
        at: Instant,
    },
    Failed {
        message: String,
        at: Instant,
    },
}

/// Reads other environments and builds a session for one of them.
#[async_trait]
pub trait EnvironmentDirectory: Send + Sync {
    /// Every configured environment, read again from the configuration.
    fn environments(&self) -> Vec<EnvironmentEntry>;
    /// This device's key fingerprint and the team in the users file, with
    /// each member's access in `environment`.
    fn team(&self, environment: &str) -> (Option<String>, Vec<TeamMemberView>);
    /// This device's public key line, created first when it has none.
    fn device_key(&self) -> Result<String, MmError>;
    /// Checks one environment's status without changing anything.
    async fn probe(&self, environment: &str) -> Result<EnvironmentProbe, MmError>;
    /// Checks only the nodes of a cluster environment: their `mmd`, mesh,
    /// and shared storage. The environment on screen has its status live.
    async fn probe_nodes(&self, environment: &str) -> Result<Vec<NodeLine>, MmError> {
        self.probe(environment).await.map(|probe| probe.nodes)
    }
    /// Builds everything the TUI needs to show `environment`. An environment
    /// on this machine that is not set up still opens, with its setup steps.
    async fn launch(&self, environment: &str) -> Result<LaunchRequest, MmError>;
    /// Builds the session of the environment the TUI opened on, reporting
    /// each part to `progress` as soon as it is read. Whatever state the
    /// environment is in, the session opens: what is missing is shown.
    async fn load(
        &self,
        environment: &str,
        _progress: LoadProgress,
    ) -> Result<LaunchRequest, MmError> {
        self.launch(environment).await
    }
}

#[derive(Debug, Clone, Default)]
pub struct EnvironmentsView {
    pub entries: Vec<EnvironmentEntry>,
    /// The last check of each environment other than the current one.
    pub probes: BTreeMap<String, ProbeState>,
    /// The environment a switch is loading.
    pub switching: Option<String>,
    /// The load is of the environment already on screen, after its setup changed.
    pub reloading: bool,
}

impl EnvironmentsView {
    #[must_use]
    pub fn new(entries: Vec<EnvironmentEntry>) -> Self {
        Self {
            entries,
            ..Self::default()
        }
    }

    /// Marks the given environments as being checked and returns those that
    /// were not already; `force` re-checks fresh results too. The current
    /// environment is live and is checked only when it is a `cluster`: its
    /// status does not carry what the check reads from every node.
    pub fn begin_checks(&mut self, current: &str, cluster: bool, force: bool) -> Vec<String> {
        let mut started = Vec::new();
        for entry in &self.entries {
            if (entry.name == current && !cluster) || entry.problem.is_some() {
                continue;
            }
            let due = match self.probes.get(&entry.name) {
                None => true,
                Some(ProbeState::Checking) => false,
                Some(ProbeState::Ready { at, .. } | ProbeState::Failed { at, .. }) => {
                    force || at.elapsed() >= STALE_AFTER
                }
            };
            if due {
                started.push(entry.name.clone());
            }
        }
        for name in &started {
            self.probes.insert(name.clone(), ProbeState::Checking);
        }
        started
    }
}

impl EnvironmentsView {
    /// The nodes of the environment on screen. Whether a node answers is
    /// live; its `mmd` version, mesh, and shared storage are from the last
    /// check of the target's nodes, for a node that answered both.
    #[must_use]
    pub fn current_nodes(&self, snapshot: &WorkspaceSnapshot) -> Vec<NodeLine> {
        let checked = match self.probes.get(&snapshot.environment) {
            Some(ProbeState::Ready { probe, .. }) => probe.nodes.as_slice(),
            _ => &[],
        };
        snapshot
            .nodes
            .iter()
            .map(|node| {
                checked
                    .iter()
                    .find(|line| node.reachable && line.ready && line.name == node.name)
                    .cloned()
                    .unwrap_or_else(|| NodeLine::from(node))
            })
            .collect()
    }
}

/// Starts the checks the Environments view shows: the other environments,
/// and the nodes of the one on screen when it runs on a cluster.
pub(super) fn begin_checks(model: &mut Model, force: bool) -> Vec<String> {
    let cluster = model.snapshot.nodes.len() > 1;
    model
        .environments
        .begin_checks(&model.snapshot.environment, cluster, force)
}

pub(super) enum EnvironmentEvent {
    Probed {
        environment: String,
        result: Result<EnvironmentProbe, String>,
    },
    Launched {
        environment: String,
        result: Result<Box<LaunchRequest>, String>,
    },
    /// A part of the session `load` is reading.
    Loaded { environment: String, part: Loaded },
    /// The session `load` read, or why it could not be built at all.
    Read {
        environment: String,
        result: Result<Box<LaunchRequest>, MmError>,
    },
}

/// Runs environment checks and switches off the input loop.
pub(super) struct EnvironmentRunner {
    directory: Arc<dyn EnvironmentDirectory>,
    sender: mpsc::UnboundedSender<EnvironmentEvent>,
    launch: Mutex<Option<AbortHandle>>,
}

impl EnvironmentRunner {
    pub(super) fn new(
        directory: Arc<dyn EnvironmentDirectory>,
    ) -> (Self, mpsc::UnboundedReceiver<EnvironmentEvent>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Self {
                directory,
                sender,
                launch: Mutex::new(None),
            },
            receiver,
        )
    }

    pub(super) fn directory(&self) -> &dyn EnvironmentDirectory {
        self.directory.as_ref()
    }

    /// Checks `environments`. Of `current`, the one on screen, only the
    /// nodes are checked: its status is live already.
    pub(super) fn probe(&self, environments: Vec<String>, current: &str) {
        for environment in environments {
            let directory = Arc::clone(&self.directory);
            let sender = self.sender.clone();
            let nodes_only = environment == current;
            tokio::spawn(async move {
                let result = if nodes_only {
                    directory
                        .probe_nodes(&environment)
                        .await
                        .map(|nodes| EnvironmentProbe {
                            nodes,
                            ..EnvironmentProbe::default()
                        })
                } else {
                    directory.probe(&environment).await
                }
                .map_err(|error| crate::presentation::output::error_line(&error));
                if sender
                    .send(EnvironmentEvent::Probed {
                        environment,
                        result,
                    })
                    .is_err()
                {
                    tracing::debug!("TUI closed before an environment check finished");
                }
            });
        }
    }

    fn replace_launch(&self, next: Option<AbortHandle>) {
        match self.launch.lock() {
            Ok(mut current) => {
                if let Some(previous) = std::mem::replace(&mut *current, next) {
                    previous.abort();
                }
            }
            Err(_) => tracing::warn!("TUI environment switch registry is poisoned"),
        }
    }

    pub(super) fn switch(&self, environment: String) {
        let directory = Arc::clone(&self.directory);
        let sender = self.sender.clone();
        let task = tokio::spawn(async move {
            let result = directory
                .launch(&environment)
                .await
                .map(Box::new)
                .map_err(|error| crate::presentation::output::error_line(&error));
            if sender
                .send(EnvironmentEvent::Launched {
                    environment,
                    result,
                })
                .is_err()
            {
                tracing::debug!("TUI closed before an environment switch finished");
            }
        });
        self.replace_launch(Some(task.abort_handle()));
    }

    pub(super) fn cancel_switch(&self) {
        self.replace_launch(None);
    }

    /// Reads the session of the environment on screen in the background,
    /// part by part, without taking the screen.
    pub(super) fn load(&self, environment: String) {
        let directory = Arc::clone(&self.directory);
        let sender = self.sender.clone();
        let parts = self.sender.clone();
        let shown = environment.clone();
        let progress = LoadProgress::to(move |part| {
            if parts
                .send(EnvironmentEvent::Loaded {
                    environment: shown.clone(),
                    part,
                })
                .is_err()
            {
                tracing::debug!("TUI closed while its session was being read");
            }
        });
        let task = tokio::spawn(async move {
            let result = directory.load(&environment, progress).await.map(Box::new);
            if sender
                .send(EnvironmentEvent::Read {
                    environment,
                    result,
                })
                .is_err()
            {
                tracing::debug!("TUI closed before its session was read");
            }
        });
        self.replace_launch(Some(task.abort_handle()));
    }
}

/// Records a finished check, or returns the session to switch to.
pub(super) fn apply_event(
    model: &mut Model,
    event: EnvironmentEvent,
) -> Option<Box<LaunchRequest>> {
    match event {
        EnvironmentEvent::Probed {
            environment,
            result,
        } => {
            let at = Instant::now();
            let state = match result {
                Ok(probe) => ProbeState::Ready { probe, at },
                Err(message) => ProbeState::Failed { message, at },
            };
            model.environments.probes.insert(environment, state);
            None
        }
        EnvironmentEvent::Loaded { environment, part } => {
            if environment == model.snapshot.environment {
                super::loading::apply(model, part);
            }
            None
        }
        // The read of a session that is no longer on screen: the user
        // switched away while it ran. The session loop takes the others.
        EnvironmentEvent::Read { .. } => None,
        EnvironmentEvent::Launched {
            environment,
            result,
        } => {
            if model.environments.switching.as_deref() != Some(environment.as_str()) {
                return None;
            }
            model.environments.switching = None;
            match result {
                Ok(request) => Some(request),
                Err(error) => {
                    let reloading = std::mem::take(&mut model.environments.reloading);
                    model.notice = Some(Notice::error(if reloading {
                        format!("Could not read {environment} again: {error}")
                    } else {
                        format!("Could not switch to {environment}: {error}")
                    }));
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_node_line_carries_its_mesh_in_a_few_words() {
        use crate::domain::mesh::{MeshNodeReport, MeshNodeStatus, MeshPeerStatus};

        let line = NodeLine {
            name: "app-1".to_owned(),
            ready: true,
            detail: "mmd 0.1.0  3f2c9a1e5b7d".to_owned(),
        };
        let peer = |age| MeshPeerStatus {
            name: Some("app-2".to_owned()),
            public_key: "key".to_owned(),
            endpoint: None,
            configured_endpoint: None,
            allowed_ips: Vec::new(),
            handshake_age_seconds: age,
            received_bytes: 0,
            sent_bytes: 0,
            on_interface: true,
        };
        let report = MeshNodeReport {
            name: "app-1".to_owned(),
            host: "203.0.113.11".to_owned(),
            reachable: true,
            problem: None,
            status: Some(MeshNodeStatus {
                interface: Some("mm0".to_owned()),
                up: true,
                peers: vec![peer(Some(30)), peer(None)],
                ..MeshNodeStatus::default()
            }),
        };
        assert_eq!(
            line.clone().with_mesh(Some(&report)).detail,
            "mmd 0.1.0  3f2c9a1e5b7d  mesh 1/2 peers"
        );
        // A single host or a node that did not report keeps its line.
        assert_eq!(line.clone().with_mesh(None), line);
    }

    #[test]
    fn checks_skip_the_current_environment_and_fresh_or_running_ones() {
        let entry = |name: &str| EnvironmentEntry {
            name: name.to_owned(),
            ..EnvironmentEntry::default()
        };
        let mut view = EnvironmentsView::new(vec![
            entry("development"),
            entry("staging"),
            entry("production"),
            EnvironmentEntry {
                problem: Some("target 'gone' is not registered".to_owned()),
                ..entry("broken")
            },
        ]);
        assert_eq!(
            view.begin_checks("development", false, false),
            ["staging", "production"]
        );
        assert!(
            view.begin_checks("development", false, true).is_empty(),
            "already checking"
        );
        view.probes.insert(
            "staging".to_owned(),
            ProbeState::Ready {
                probe: EnvironmentProbe::default(),
                at: Instant::now(),
            },
        );
        assert!(view.begin_checks("development", false, false).is_empty());
        assert_eq!(view.begin_checks("development", false, true), ["staging"]);
    }

    #[test]
    fn the_nodes_of_a_cluster_on_screen_are_checked_and_keep_their_live_state()
    -> Result<(), serde_json::Error> {
        use crate::presentation::tui::fixtures::cluster_model;
        use crate::presentation::tui::update::{Action, Message, update};
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut model = cluster_model()?;
        model.environments.probes.remove("production");
        let live = model.environments.current_nodes(&model.snapshot);
        assert_eq!(live[0].detail, "answering  3f2c9a1e5b7d");
        // Opening the view checks the environment on screen too.
        let opened = update(
            &mut model,
            Message::Key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE)),
        );
        assert_eq!(
            opened,
            Action::CheckEnvironments(vec!["production".to_owned()]),
            "the other environments were checked a moment ago"
        );

        let mut model = cluster_model()?;
        let checked = model.environments.current_nodes(&model.snapshot);
        assert_eq!(checked[0].detail, "mmd 0.1.0  3f2c9a1e5b7d  mesh 2/2 peers");
        // A node that stopped answering since the check says so.
        model.snapshot.nodes[1].reachable = false;
        model.snapshot.nodes[1].problem = Some("did not answer".to_owned());
        let now = model.environments.current_nodes(&model.snapshot);
        assert_eq!(now[1].detail, "did not answer");
        assert!(!now[1].ready);
        Ok(())
    }

    #[test]
    fn esc_on_a_message_dismisses_it_and_the_next_esc_leaves() -> Result<(), serde_json::Error> {
        use crate::presentation::tui::model::Tab;
        use crate::presentation::tui::update::{Message, update};
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut model = crate::presentation::tui::fixtures::rich_model()?;
        let press = |model: &mut Model, code| {
            update(model, Message::Key(KeyEvent::new(code, KeyModifiers::NONE)));
        };
        press(&mut model, KeyCode::Char('2'));
        // `enter` on the current environment answers with a message.
        press(&mut model, KeyCode::Enter);
        assert!(model.notice.is_some());
        press(&mut model, KeyCode::Esc);
        assert!(model.notice.is_none());
        assert!(!model.should_quit, "esc only dismissed the message");
        press(&mut model, KeyCode::Char('3'));
        assert_eq!(model.tab, Tab::Services);
        press(&mut model, KeyCode::Esc);
        assert!(model.should_quit);
        Ok(())
    }
}
