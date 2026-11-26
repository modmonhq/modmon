//! What `mm update` does after it replaces `mm`: the local user services
//! that run `mm`, the `mm mcp serve` processes agent clients started, and
//! the environments whose hosts run an older `mm` and `mmd`.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::cluster_setup::ReleaseConsent;
use crate::domain::release::stable_version;
use crate::domain::target::SetupCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceManager {
    Systemd,
    Launchd,
}

/// A user service mm installs on this machine, as its manager reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct LocalUserService {
    /// The systemd unit or launchd label, such as `mm-dashboard.service`.
    pub name: String,
    pub manager: ServiceManager,
    pub running: bool,
    /// The program it starts, as its unit or plist names it.
    pub program: Option<PathBuf>,
    /// Whether its process started before the `mm` on disk was written;
    /// `None` when no process of it was found.
    pub older: Option<bool>,
}

impl LocalUserService {
    #[must_use]
    pub fn runs_mm(&self) -> bool {
        self.program
            .as_ref()
            .and_then(|program| program.file_name())
            .is_some_and(|name| name == "mm")
    }

    /// The commands that show why the service did not come back.
    #[must_use]
    pub fn inspect_commands(&self) -> Vec<String> {
        match self.manager {
            ServiceManager::Systemd => vec![
                format!("systemctl --user status {}", self.name),
                format!("journalctl --user -u {} -n 50", self.name),
            ],
            ServiceManager::Launchd => {
                vec![format!("launchctl print gui/$(id -u)/{}", self.name)]
            }
        }
    }

    #[must_use]
    pub fn restart_command(&self) -> String {
        match self.manager {
            ServiceManager::Systemd => format!("systemctl --user restart {}", self.name),
            ServiceManager::Launchd => format!("launchctl kickstart -k gui/$(id -u)/{}", self.name),
        }
    }
}

/// An `mm mcp serve` process of this user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct McpServerProcess {
    pub pid: u32,
    pub command: String,
    /// Whether it started before the `mm` on disk was written.
    pub older: bool,
}

/// What `mm update` did with a local user service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", tag = "outcome")]
pub enum ServiceOutcome {
    /// Restarted, and running again on the new `mm`.
    Restarted,
    /// Restarted, but not running a few seconds later.
    Unhealthy,
    /// The restart itself failed.
    RestartFailed { problem: String },
    /// Runs this `mm` already; left as it is.
    Current,
    /// Installed and stopped; left stopped.
    Stopped,
    /// Starts another program, such as the container engine; left as it is.
    NotMm,
    /// Starts an `mm` other than the one this update replaced.
    OtherMm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ServiceReport {
    pub name: String,
    pub program: Option<PathBuf>,
    #[serde(flatten)]
    pub outcome: ServiceOutcome,
    /// The release of `mm` it runs, when it runs this `mm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The commands to run next, when something is left to do.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next: Vec<String>,
}

/// A node, as its `mmd` answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct NodeVersion {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub older: bool,
    /// Why the node did not answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// The nodes of one remote environment, and how to upgrade them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct EnvironmentHosts {
    pub environment: String,
    pub target: String,
    pub cluster: bool,
    pub nodes: Vec<NodeVersion>,
    /// Why the environment's nodes could not be listed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// The commands that upgrade its hosts, when one runs an older release.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upgrade: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl EnvironmentHosts {
    #[must_use]
    pub fn needs_upgrade(&self) -> bool {
        self.nodes.iter().any(|node| node.older)
    }
}

/// The hosts `mm update` could see from where it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", tag = "scope")]
pub enum HostsView {
    /// Not run in a project, so no environment is known.
    NoProject {
        note: String,
    },
    Project {
        environments: Vec<EnvironmentHosts>,
    },
}

/// What `mm update` does besides replacing the binaries, before it asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUpPlan {
    pub services: Vec<LocalUserService>,
    pub restart: Vec<String>,
    pub mcp_servers: Vec<McpServerProcess>,
    pub hosts: HostsView,
}

/// What runs which release once `mm update` is done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct UpdateSummary {
    pub version: String,
    pub services: Vec<ServiceReport>,
    pub mcp_servers: Vec<McpServerProcess>,
    pub hosts: HostsView,
}

pub const MCP_RESTART: &str = "restart it from the agent client that started it (reload its MCP servers, or restart the client)";
pub const NO_PROJECT_HOSTS: &str = "Run mm update in a project to list its environments' hosts. To upgrade a project's hosts: `mm --environment ENVIRONMENT target setup plan --upgrade`, then run the apply command that plan prints.";
const SINGLE_HOST_NOTE: &str = "add the --origin URL (or --delivery push) and --break-glass flags you set the host up with; the plan prints the apply command with every flag it needs";
const CLUSTER_NOTE: &str = "then run the apply command that plan prints";

/// The commands that upgrade the hosts of `environment`, and what to add.
#[must_use]
pub fn upgrade_commands(environment: &str, cluster: bool) -> (Vec<String>, String) {
    let plan = SetupCommand::new(environment)
        .with_release(ReleaseConsent {
            downgrade: false,
            upgrade: true,
        })
        .plan();
    let note = if cluster {
        CLUSTER_NOTE
    } else {
        SINGLE_HOST_NOTE
    };
    (vec![plan], note.to_owned())
}

/// Whether a host on `host` runs an older release than `this`. A version
/// that is not stable SemVer is not called older.
#[must_use]
pub fn host_is_older(host: &str, this: &str) -> bool {
    match (stable_version(host), stable_version(this)) {
        (Ok(host), Ok(this)) => host < this,
        _ => false,
    }
}

/// Seconds in a `ps -o etime` value: `[[dd-]hh:]mm:ss`.
#[must_use]
pub fn elapsed_seconds(value: &str) -> Option<u64> {
    let (days, clock) = match value.trim().split_once('-') {
        Some((days, clock)) => (days.parse::<u64>().ok()?, clock),
        None => (0, value.trim()),
    };
    let parts = clock
        .split(':')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    let (hours, minutes, seconds) = match parts.as_slice() {
        [minutes, seconds] => (0, *minutes, *seconds),
        [hours, minutes, seconds] => (*hours, *minutes, *seconds),
        _ => return None,
    };
    Some(((days * 24 + hours) * 60 + minutes) * 60 + seconds)
}

/// Whether `arguments` run `mm mcp serve`.
#[must_use]
pub fn is_mcp_server(arguments: &[&str]) -> bool {
    arguments.first().is_some_and(|program| {
        std::path::Path::new(program)
            .file_name()
            .is_some_and(|name| name == "mm")
    }) && arguments.windows(2).any(|pair| pair == ["mcp", "serve"])
}

/// The program a systemd unit's `ExecStart=` starts.
#[must_use]
pub fn unit_program(unit: &str) -> Option<PathBuf> {
    unit.lines()
        .find_map(|line| line.trim().strip_prefix("ExecStart="))
        .map(|command| command.trim_start_matches(['-', '@', '+', '!', ':']))
        .and_then(|command| command.split_whitespace().next())
        .map(PathBuf::from)
}

/// The first of a launchd plist's `ProgramArguments`.
#[must_use]
pub fn plist_program(plist: &str) -> Option<PathBuf> {
    let after = plist.split_once("<key>ProgramArguments</key>")?.1;
    let start = after.find("<string>")? + "<string>".len();
    let end = after[start..].find("</string>")?;
    Some(PathBuf::from(unescape(&after[start..start + end])))
}

fn unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
