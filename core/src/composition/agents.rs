//! `mm agent`: what agents may do in this project, the user's trust in the
//! block that says so, and connecting coding tools to `mm mcp serve`.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::adapters::agent_clients::{
    AgentClient, AgentSetupPlan, apply_setup, installed_clients, plan_project_setup,
    plan_user_setup,
};
use crate::adapters::config::ConfigLoader;
use crate::adapters::mcp::{AgentTrustStore, McpPolicyStore};
use crate::adapters::state::MmHome;
use crate::domain::agents::{agent_access_in, agents_policy_hash};
use crate::domain::error::MmError;

#[derive(Debug, Clone, Serialize)]
pub struct AgentEnvironmentStatus {
    pub environment: String,
    /// What the `agents:` block grants.
    pub granted: String,
    /// What applies now: `view` until the block is trusted, and never more
    /// than this device's private ceiling.
    pub in_force: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentStatus {
    pub project_root: PathBuf,
    /// The project declares an `agents:` block.
    pub declared: bool,
    /// Identifies the exact block; `mm agent trust --plan` takes it.
    pub policy_hash: String,
    pub trusted: bool,
    pub environments: Vec<AgentEnvironmentStatus>,
    /// Fingerprint of the key agents reach remote environments with.
    pub agent_key: Option<String>,
    /// This device's private limits file, when present.
    pub private_limits: Option<PathBuf>,
}

pub(crate) fn agent_status(
    start: &Path,
    explicit_home: Option<&Path>,
) -> Result<AgentStatus, MmError> {
    let document = ConfigLoader.load_from(start)?;
    let home = MmHome::resolve(explicit_home)?;
    let limits_path = McpPolicyStore::default_path(&home.root);
    let limits = McpPolicyStore::load(&limits_path, false)?;
    let agents = &document.config.agents;
    let trusted = agents.is_empty()
        || AgentTrustStore::new(&home.root).is_trusted(
            &document.root,
            document.config.project.id,
            agents,
        );
    let environments = document
        .config
        .environments
        .keys()
        .map(|environment| {
            let granted = agent_access_in(agents, environment);
            let limited = limits.limit(environment, &granted);
            AgentEnvironmentStatus {
                environment: environment.clone(),
                granted: granted.summary(),
                in_force: if trusted {
                    limited.summary()
                } else {
                    crate::domain::agents::AgentAccess::VIEW.summary()
                },
            }
        })
        .collect();
    Ok(AgentStatus {
        project_root: document.root.clone(),
        declared: !agents.is_empty(),
        policy_hash: agents_policy_hash(agents),
        trusted,
        environments,
        agent_key: super::agent_key(explicit_home)?.map(|key| key.fingerprint),
        private_limits: limits_path.is_file().then_some(limits_path),
    })
}

/// Records the project's current `agents:` block as trusted on this device.
/// With `reviewed`, the block must still be the one the user reviewed.
pub(crate) fn trust_agents(
    start: &Path,
    explicit_home: Option<&Path>,
    reviewed: Option<&str>,
) -> Result<AgentStatus, MmError> {
    let document = ConfigLoader.load_from(start)?;
    let hash = agents_policy_hash(&document.config.agents);
    if reviewed.is_some_and(|reviewed| reviewed != hash) {
        return Err(MmError::State {
            message: "the agents: block changed since it was reviewed; run `mm agent trust` again and review the new block".to_owned(),
        });
    }
    let home = MmHome::resolve(explicit_home)?;
    AgentTrustStore::new(&home.root).trust(
        &document.root,
        document.config.project.id,
        &document.config.agents,
        chrono::Utc::now().timestamp_millis(),
    )?;
    agent_status(start, explicit_home)
}

/// Where `mm agent setup` writes: this project, or the user's own tool
/// configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentSetupScope {
    Project,
    User,
}

/// The tools to connect: the ones named, else the ones installed.
pub(crate) fn plan_agent_setup(
    start: &Path,
    scope: AgentSetupScope,
    requested: &[AgentClient],
) -> Result<(Vec<AgentClient>, AgentSetupPlan), MmError> {
    let mut clients = if requested.is_empty() {
        installed_clients()
    } else {
        requested.to_vec()
    };
    clients.sort();
    clients.dedup();
    let plan = match scope {
        AgentSetupScope::Project => {
            let document = ConfigLoader.load_from(start)?;
            plan_project_setup(&document.root, &clients)?
        }
        AgentSetupScope::User => {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .ok_or_else(|| MmError::Environment {
                    message: "HOME is not set to an absolute path".to_owned(),
                })?;
            let mm = std::env::current_exe().map_err(|source| MmError::Io {
                context: "resolving the mm binary".to_owned(),
                source,
            })?;
            plan_user_setup(&home, &clients, &mm)?
        }
    };
    Ok((clients, plan))
}

pub(crate) fn apply_agent_setup(plan: &AgentSetupPlan) -> Result<Vec<String>, MmError> {
    apply_setup(plan)
}
