//! Who this process acts for, as the command log and secret writes record it.

use std::collections::BTreeMap;
use std::path::Path;

use crate::domain::command_log::{CommandPrincipal, principal_name};
use crate::domain::users::UserConfig;

/// The person at this machine: the team user listing this device's key,
/// else the login name.
pub(crate) fn user_principal(
    explicit_home: Option<&Path>,
    users: &BTreeMap<String, UserConfig>,
) -> CommandPrincipal {
    CommandPrincipal::User {
        name: user_name(explicit_home, users),
    }
}

/// An agent working through `mm mcp` for the person at this machine. The
/// default MCP principal names the agent tool when its marker is set.
pub(crate) fn agent_principal(
    configured: &str,
    explicit_home: Option<&Path>,
    users: &BTreeMap<String, UserConfig>,
) -> CommandPrincipal {
    CommandPrincipal::Agent {
        name: agent_name(configured),
        user: Some(user_name(explicit_home, users)),
    }
}

fn user_name(explicit_home: Option<&Path>, users: &BTreeMap<String, UserConfig>) -> String {
    let team = super::device_key(explicit_home)
        .ok()
        .flatten()
        .map(|key| crate::domain::users::users_listing(users, &[key.fingerprint]))
        .unwrap_or_default();
    let login = ["USER", "LOGNAME", "USERNAME"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()));
    principal_name(&team, login)
}

fn agent_name(configured: &str) -> String {
    if configured != crate::domain::mcp::DEFAULT_MCP_PRINCIPAL {
        return configured.to_owned();
    }
    let variables = std::env::vars().collect::<Vec<_>>();
    crate::domain::agents::detect_agent(
        variables
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
    )
    .map_or_else(
        || configured.to_owned(),
        |marker| crate::domain::agents::agent_label(marker).to_owned(),
    )
}
