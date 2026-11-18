//! The repo's `users:` key: who may reach each remote environment and how.
//! `mmd` reconciles it per environment on deploy or team sync.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::agents::{AgentsConfig, agent_access_in};
use crate::domain::host_policy::{
    ACCESS_RECORD_SCHEMA_VERSION, AccessLevel, EnvironmentAccess, EnvironmentAccessRecord,
    ReconciledAgent, ReconciledKey, ReconciledUser, parse_public_key, valid_user_name,
};
use crate::domain::workspace::ProjectId;

/// Applies to every environment a user does not name explicitly.
pub const ALL_ENVIRONMENTS: &str = "*";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct UserConfig {
    /// A CI or automation identity rather than a person.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub machine: bool,
    /// OpenSSH public keys: `ssh-ed25519` or `sk-ssh-ed25519@openssh.com`.
    pub keys: Vec<UserKeyConfig>,
    /// Level per environment name, or `*` for every remote environment.
    #[serde(default)]
    pub access: BTreeMap<String, UserAccessConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum UserKeyConfig {
    Line(String),
    Detailed {
        key: String,
        /// `YYYY-MM-DD`; the key stops working at the start of that UTC day.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires: Option<String>,
        /// The key of this user's AI agent (`mm key create --agent`). It gets
        /// the user's access limited by the `agents:` block, never more.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        agent: bool,
    },
}

impl UserKeyConfig {
    #[must_use]
    pub fn line(&self) -> &str {
        match self {
            Self::Line(line) | Self::Detailed { key: line, .. } => line,
        }
    }

    #[must_use]
    pub fn expires(&self) -> Option<&str> {
        match self {
            Self::Line(_) => None,
            Self::Detailed { expires, .. } => expires.as_deref(),
        }
    }

    #[must_use]
    pub const fn is_agent(&self) -> bool {
        match self {
            Self::Line(_) => false,
            Self::Detailed { agent, .. } => *agent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum UserAccessConfig {
    Level(AccessLevel),
    Detailed(EnvironmentAccess),
}

impl UserAccessConfig {
    #[must_use]
    pub fn access(&self) -> EnvironmentAccess {
        match self {
            Self::Level(level) => EnvironmentAccess::level(*level),
            Self::Detailed(access) => access.clone(),
        }
    }
}

impl UserConfig {
    /// The access this user has in `environment`: its own entry, else `*`.
    #[must_use]
    pub fn access_in(&self, environment: &str) -> Option<EnvironmentAccess> {
        self.access
            .get(environment)
            .or_else(|| self.access.get(ALL_ENVIRONMENTS))
            .map(UserAccessConfig::access)
    }
}

/// Problems in a users file, each with the user it concerns.
pub fn validate_users(
    users: &BTreeMap<String, UserConfig>,
    environments: &BTreeSet<String>,
) -> Vec<String> {
    let mut problems = Vec::new();
    let mut owners = BTreeMap::<String, &str>::new();
    for (name, user) in users {
        if !valid_user_name(name) {
            problems.push(format!("user '{name}' must match [a-z][a-z0-9-]{{0,62}}"));
        }
        if user.keys.is_empty() {
            problems.push(format!("user '{name}' needs at least one public key"));
        } else if user.keys.iter().all(UserKeyConfig::is_agent) {
            problems.push(format!(
                "user '{name}' has only agent keys; an agent key belongs to a user who also has a key of their own"
            ));
        }
        if user.machine && user.keys.iter().any(UserKeyConfig::is_agent) {
            problems.push(format!(
                "machine user '{name}' cannot have agent keys; an agent key belongs to a person"
            ));
        }
        for key in &user.keys {
            match parse_public_key(key.line()) {
                Ok(parsed) => {
                    if let Some(owner) = owners.insert(parsed.fingerprint.clone(), name) {
                        problems.push(format!(
                            "users '{owner}' and '{name}' share key {}",
                            parsed.fingerprint
                        ));
                    }
                }
                Err(message) => problems.push(format!("user '{name}': {message}")),
            }
            if let Some(expires) = key.expires()
                && expiry_unix_ms(expires).is_none()
            {
                problems.push(format!(
                    "user '{name}' key expiry '{expires}' must be a YYYY-MM-DD date"
                ));
            }
        }
        for environment in user.access.keys() {
            if environment != ALL_ENVIRONMENTS && !environments.contains(environment) {
                problems.push(format!(
                    "user '{name}' grants access to unknown environment '{environment}'"
                ));
            }
        }
    }
    problems
}

/// The users whose entry lists a key with one of `fingerprints`, by name.
#[must_use]
pub fn users_listing(users: &BTreeMap<String, UserConfig>, fingerprints: &[String]) -> Vec<String> {
    users
        .iter()
        .filter(|(_, user)| {
            user.keys.iter().any(|key| {
                parse_public_key(key.line())
                    .is_ok_and(|parsed| fingerprints.contains(&parsed.fingerprint))
            })
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// The one entry that lists one of `fingerprints`; none or several asks for `--name`.
pub fn rotating_entry(
    users: &BTreeMap<String, UserConfig>,
    fingerprints: &[String],
    flag: &str,
) -> Result<String, String> {
    match users_listing(users, fingerprints).as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(format!(
            "no user in the users file lists this device's key, so `mm team join{flag}` cannot tell which entry the new key of the rotation goes in; pass --name with your entry's name"
        )),
        several => Err(format!(
            "users {} all list this device's key; pass --name with the entry the new key of the rotation goes in",
            several
                .iter()
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The users file of `commit` as it applies to `environment`. Ceilings are
/// applied later, when the host builds its effective policy. A user's agent
/// keys get the user's access limited by the commit's `agents:` block.
pub fn reconcile_users(
    (users, agents): (&BTreeMap<String, UserConfig>, &AgentsConfig),
    deploy_from: Option<Vec<String>>,
    project_id: ProjectId,
    environment: &str,
    commit: &str,
    now_unix_ms: i64,
    reconciled_by: &str,
) -> Result<EnvironmentAccessRecord, String> {
    let mut reconciled = BTreeMap::new();
    for (name, user) in users {
        if !valid_user_name(name) {
            return Err(format!("user '{name}' has an invalid name"));
        }
        let Some(access) = user.access_in(environment) else {
            continue;
        };
        let mut keys = Vec::new();
        let mut agent_keys = Vec::new();
        for key in &user.keys {
            let parsed = parse_public_key(key.line())
                .map_err(|message| format!("user '{name}': {message}"))?;
            let expires_at_unix_ms = match key.expires() {
                Some(expires) => Some(expiry_unix_ms(expires).ok_or_else(|| {
                    format!("user '{name}' key expiry '{expires}' must be a YYYY-MM-DD date")
                })?),
                None => None,
            };
            if expires_at_unix_ms.is_some_and(|expiry| expiry <= now_unix_ms) {
                continue;
            }
            let reconciled_key = ReconciledKey {
                public_key: parsed.openssh(),
                fingerprint: parsed.fingerprint,
                expires_at_unix_ms,
            };
            if key.is_agent() {
                agent_keys.push(reconciled_key);
            } else {
                keys.push(reconciled_key);
            }
        }
        keys.sort();
        keys.dedup();
        agent_keys.sort();
        agent_keys.dedup();
        // Without a working key of their own, the user has no access for an
        // agent to share.
        if keys.is_empty() {
            continue;
        }
        let agent = (!agent_keys.is_empty() && !user.machine).then(|| ReconciledAgent {
            keys: agent_keys,
            access: access.within(&agent_access_in(agents, environment).environment_access()),
        });
        reconciled.insert(
            name.clone(),
            ReconciledUser {
                machine: user.machine,
                keys,
                access,
                agent,
            },
        );
    }
    Ok(EnvironmentAccessRecord {
        schema_version: ACCESS_RECORD_SCHEMA_VERSION,
        project_id,
        environment: environment.to_owned(),
        commit: commit.to_owned(),
        users: reconciled,
        deploy_from,
        reconciled_at_unix_ms: now_unix_ms,
        reconciled_by: reconciled_by.to_owned(),
    })
}

/// One user's change between the enforced and the proposed users file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct AccessChangeEntry {
    pub user: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<EnvironmentAccess>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<EnvironmentAccess>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys_added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys_removed: Vec<String>,
}

/// What reconciling a commit's users file changes in one environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct AccessChange {
    pub environment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_commit: Option<String>,
    pub record: EnvironmentAccessRecord,
    pub entries: Vec<AccessChangeEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy_from_before: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy_from_after: Option<Vec<String>>,
}

impl AccessChange {
    #[must_use]
    pub fn between(
        previous: Option<&EnvironmentAccessRecord>,
        record: EnvironmentAccessRecord,
    ) -> Self {
        let empty = BTreeMap::new();
        let before = previous.map_or(&empty, |record| &record.users);
        let names = before
            .keys()
            .chain(record.users.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut entries = Vec::new();
        for name in names {
            let user = name.clone();
            let old = before.get(&user);
            let new = record.users.get(&user);
            let fingerprints = |user: Option<&ReconciledUser>| {
                user.map(|user| {
                    user.keys
                        .iter()
                        .map(|key| key.fingerprint.clone())
                        .collect::<BTreeSet<_>>()
                })
                .unwrap_or_default()
            };
            let (old_keys, new_keys) = (fingerprints(old), fingerprints(new));
            let entry = AccessChangeEntry {
                user,
                before: old.map(|user| user.access.clone()),
                after: new.map(|user| user.access.clone()),
                keys_added: new_keys.difference(&old_keys).cloned().collect(),
                keys_removed: old_keys.difference(&new_keys).cloned().collect(),
            };
            if entry.before != entry.after
                || !entry.keys_added.is_empty()
                || !entry.keys_removed.is_empty()
            {
                entries.push(entry);
            }
            let old_agent = old.and_then(|user| user.agent.as_ref());
            let new_agent = new.and_then(|user| user.agent.as_ref());
            let agent_fingerprints = |agent: Option<&ReconciledAgent>| {
                agent
                    .map(|agent| {
                        agent
                            .keys
                            .iter()
                            .map(|key| key.fingerprint.clone())
                            .collect::<BTreeSet<_>>()
                    })
                    .unwrap_or_default()
            };
            let (old_keys, new_keys) =
                (agent_fingerprints(old_agent), agent_fingerprints(new_agent));
            let entry = AccessChangeEntry {
                user: agent_display_name(&name),
                before: old_agent.map(|agent| agent.access.clone()),
                after: new_agent.map(|agent| agent.access.clone()),
                keys_added: new_keys.difference(&old_keys).cloned().collect(),
                keys_removed: old_keys.difference(&new_keys).cloned().collect(),
            };
            if entry.before != entry.after
                || !entry.keys_added.is_empty()
                || !entry.keys_removed.is_empty()
            {
                entries.push(entry);
            }
        }
        Self {
            environment: record.environment.clone(),
            previous_commit: previous.map(|record| record.commit.clone()),
            deploy_from_before: previous.and_then(|record| record.deploy_from.clone()),
            deploy_from_after: record.deploy_from.clone(),
            record,
            entries,
        }
    }

    /// Binds the reviewed change into a plan hash. Timestamps are excluded,
    /// so re-planning the same commit yields the same hash.
    pub fn canonical_hash(&self) -> Result<String, String> {
        let mut record = self.record.clone();
        record.reconciled_at_unix_ms = 0;
        let bytes = serde_json::to_vec(&(
            "mm-access-change-v1",
            &self.environment,
            &self.previous_commit,
            &record,
            &self.entries,
        ))
        .map_err(|error| error.to_string())?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

/// How a user's agent appears in plans, `whoami`, and the audit log.
#[must_use]
pub fn agent_display_name(user: &str) -> String {
    format!("{user} (agent)")
}

/// Unix milliseconds at the start of a `YYYY-MM-DD` UTC day.
#[must_use]
pub fn expiry_unix_ms(date: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<i64>().ok()?;
    let day = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() || !(1970..=9999).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days_in_month = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=days_in_month[usize::try_from(month - 1).ok()?]).contains(&day) {
        return None;
    }
    // Days from civil, Howard Hinnant's algorithm.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400_000)
}

#[cfg(test)]
#[path = "users/tests.rs"]
mod tests;
