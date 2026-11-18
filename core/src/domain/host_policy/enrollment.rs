//! Adding a project to the host policy during `mm target setup`. The merge
//! keeps everything an administrator already wrote: other projects, ceilings,
//! guardrail exceptions, and existing break-glass keys.

use std::collections::BTreeSet;

use super::{BreakGlassKey, EnrolledProject, HOST_POLICY_SCHEMA_VERSION, HostPolicy};
use crate::domain::workspace::ProjectId;

/// What setup writes into the host policy for one project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEnrollment {
    pub project_id: ProjectId,
    pub name: String,
    /// Empty when the project's files are pushed.
    pub origin: String,
    pub origin_host_key: Option<String>,
    pub delivery: crate::domain::delivery::Delivery,
    pub environments: BTreeSet<String>,
    pub break_glass: Vec<BreakGlassKey>,
}

impl HostPolicy {
    /// A policy with no projects and no break-glass keys. It is not valid
    /// until a key is added, so callers enroll into it before writing it.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            schema_version: HOST_POLICY_SCHEMA_VERSION,
            break_glass: Vec::new(),
            projects: std::collections::BTreeMap::new(),
            runtimes: std::collections::BTreeSet::new(),
            exposed_runtime: None,
        }
    }

    /// Enrolls the project, or updates its origin and delivery and adds environments when
    /// it is already enrolled. Break-glass keys are added unless the same key
    /// is already listed; a name that already holds a different key is an
    /// error, so setup never silently replaces a recovery key.
    pub fn enroll(&mut self, enrollment: &ProjectEnrollment) -> Result<(), String> {
        if enrollment.name.is_empty()
            || enrollment.name.len() > 128
            || enrollment.name.chars().any(char::is_control)
        {
            return Err(
                "project name must be 1-128 characters without control characters".to_owned(),
            );
        }
        match (enrollment.delivery.is_push(), enrollment.origin.is_empty()) {
            // A host the project is pushed to has nothing to fetch from.
            (true, true) if enrollment.origin_host_key.is_none() => {}
            (true, _) => {
                return Err(
                    "a project delivered by push takes no origin and no origin host key".to_owned(),
                );
            }
            (false, _) => {
                validate_origin(&enrollment.origin, enrollment.origin_host_key.as_deref())?;
            }
        }
        if enrollment.environments.is_empty() {
            return Err("enroll at least one environment".to_owned());
        }
        for environment in &enrollment.environments {
            validate_environment_name(environment)?;
        }
        let mut next = self.clone();
        for entry in &enrollment.break_glass {
            let key = super::parse_public_key(&entry.key)
                .map_err(|message| format!("break-glass key '{}': {message}", entry.name))?;
            let mut already_listed = false;
            for existing in &next.break_glass {
                let same_key = super::parse_public_key(&existing.key)
                    .is_ok_and(|existing| existing.fingerprint == key.fingerprint);
                if same_key {
                    already_listed = true;
                } else if existing.name == entry.name {
                    return Err(format!(
                        "break-glass name '{}' already holds a different key; edit the host policy to replace it",
                        entry.name
                    ));
                }
            }
            if !already_listed {
                next.break_glass.push(entry.clone());
            }
        }
        refuse_shared_key(&next, enrollment)?;
        next.projects
            .entry(enrollment.project_id)
            .and_modify(|project| {
                project.name.clone_from(&enrollment.name);
                project.origin.clone_from(&enrollment.origin);
                project
                    .origin_host_key
                    .clone_from(&enrollment.origin_host_key);
                project.delivery = enrollment.delivery;
                project
                    .environments
                    .extend(enrollment.environments.iter().cloned());
            })
            .or_insert_with(|| EnrolledProject {
                name: enrollment.name.clone(),
                origin: enrollment.origin.clone(),
                origin_host_key: enrollment.origin_host_key.clone(),
                delivery: enrollment.delivery,
                environments: enrollment.environments.clone(),
                initial_deploy_from: std::collections::BTreeMap::new(),
                ceiling: std::collections::BTreeMap::new(),
                allow: Vec::new(),
            });
        next.validate()?;
        *self = next;
        Ok(())
    }
}

/// Refuses a project whose `project_key` an enrolled project has.
fn refuse_shared_key(policy: &HostPolicy, enrollment: &ProjectEnrollment) -> Result<(), String> {
    use crate::domain::cluster_network::project_key;

    let key = project_key(enrollment.project_id);
    let Some((other_id, other)) = policy
        .projects
        .iter()
        .find(|(id, _)| **id != enrollment.project_id && project_key(**id) == key)
    else {
        return Ok(());
    };
    Err(format!(
        "project '{}' ({}) cannot be enrolled on this host: project '{}' ({}) is enrolled here, and both ids start with {key}, which names each project's networks, name lookups and routes on the host. Give '{}' an id of its own: set `project.id` in its mm.yaml to a new UUID (`uuidgen` prints one), commit it, and run `mm target setup plan` again",
        enrollment.name, enrollment.project_id.0, other.name, other_id.0, enrollment.name
    ))
}

/// Names both lines of a key given twice; any other message as it is.
#[must_use]
pub fn duplicate_key_message(text: &str, message: &str) -> String {
    let key = message
        .split_once("duplicate entry with key \"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .or_else(|| {
            message
                .split_once("duplicate field `")
                .and_then(|(_, rest)| rest.split_once('`'))
        })
        .map(|(key, _)| key);
    let Some(key) = key else {
        return message.to_owned();
    };
    let given = text
        .lines()
        .enumerate()
        .filter_map(|(number, line)| {
            let content = line.trim_start();
            let rest = content
                .strip_prefix(key)
                .or_else(|| content.strip_prefix(&format!("\"{key}\"")))
                .or_else(|| content.strip_prefix(&format!("'{key}'")))?;
            rest.trim_start()
                .starts_with(':')
                .then_some((line.len() - content.len(), number + 1))
        })
        .collect::<Vec<_>>();
    let twice = given
        .iter()
        .enumerate()
        .find_map(|(index, (indent, first))| {
            given[index + 1..]
                .iter()
                .find(|(other, _)| other == indent)
                .map(|(_, second)| (*first, *second))
        });
    match twice {
        Some((first, second)) => format!(
            "key `{key}` is given twice, at line {first} and again at line {second}; keep one of them"
        ),
        None => message.to_owned(),
    }
}

/// Parses `NAME=KEY`, the form `--break-glass` takes on the command line.
pub fn parse_break_glass(value: &str) -> Result<BreakGlassKey, String> {
    let (name, key) = value.split_once('=').ok_or_else(|| {
        "break-glass keys take the form NAME='ssh-ed25519 AAAA… comment'".to_owned()
    })?;
    let entry = BreakGlassKey {
        name: name.trim().to_owned(),
        key: key.trim().to_owned(),
    };
    if !super::valid_user_name(&entry.name) {
        return Err(format!(
            "break-glass name '{}' must match [a-z][a-z0-9-]{{0,62}}",
            entry.name
        ));
    }
    super::parse_public_key(&entry.key)
        .map_err(|message| format!("break-glass key '{}': {message}", entry.name))?;
    Ok(entry)
}

/// Whether `mmd` reaches this origin over SSH, so it needs a pinned host key.
#[must_use]
pub fn origin_uses_ssh(origin: &str) -> bool {
    if origin.starts_with("ssh://") {
        return true;
    }
    if origin.contains("://") || origin.starts_with('/') {
        return false;
    }
    // scp-like `[user@]host:path`: a colon before any slash.
    origin
        .split_once(':')
        .is_some_and(|(authority, _)| !authority.is_empty() && !authority.contains('/'))
}

/// Origins `mmd`'s git accepts: HTTPS, SSH (`ssh://` or `user@host:path`),
/// and absolute local paths. SSH origins need the git server's host key,
/// `<type> <base64>`; other origins must not set one.
pub fn validate_origin(origin: &str, origin_host_key: Option<&str>) -> Result<(), String> {
    if origin.is_empty()
        || origin.len() > 2048
        || origin.starts_with('-')
        || origin
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        || origin.contains(['{', '}'])
    {
        return Err("origin must be a git URL without spaces, braces, or a leading '-'".to_owned());
    }
    let ssh = origin_uses_ssh(origin);
    let supported = ssh
        || origin.starts_with("https://")
        || origin.starts_with('/')
        || origin.starts_with("file:///");
    if !supported {
        return Err(format!(
            "origin '{origin}' must use https://, ssh://, user@host:path, or an absolute local path"
        ));
    }
    match (ssh, origin_host_key) {
        (true, None) => Err(
            "an SSH origin needs the git server's host key (`<type> <base64>`) so mmd can pin it"
                .to_owned(),
        ),
        (false, Some(_)) => {
            Err("only SSH origins take a host key; remove it for this origin".to_owned())
        }
        (true, Some(key)) => validate_origin_host_key(key),
        (false, None) => Ok(()),
    }
}

fn validate_origin_host_key(key: &str) -> Result<(), String> {
    if key.split_whitespace().count() != 2 || key.contains(['{', '}']) || key.len() > 16 * 1024 {
        return Err("origin host key must be `<type> <base64>`, as ssh-keyscan prints it after the host name".to_owned());
    }
    crate::domain::repo_target::ssh_fingerprint(key)
        .map(drop)
        .map_err(|message| format!("origin host key: {message}"))
}

pub fn validate_environment_name(environment: &str) -> Result<(), String> {
    if environment.is_empty()
        || environment.len() > 63
        || !environment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(format!(
            "environment '{environment}' must be 1-63 ASCII letters, numbers, '-' or '_'"
        ));
    }
    Ok(())
}
