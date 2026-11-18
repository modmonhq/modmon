//! `mm team`: who may reach remote environments, from the repo's users file.

use super::*;
use crate::domain::onboarding::{ConfigMutationAction, ConfigMutationRequest, UserProposal};
use crate::presentation::inline;

#[derive(Debug, Clone, Subcommand)]
pub enum TeamCommand {
    /// Add yourself to the users file with this device's key (reviewed edit).
    Join {
        /// User name, [a-z][a-z0-9-]*. Defaults to $USER.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
        /// Access to request, for example production=view. Repeatable.
        #[arg(long = "request", value_name = "ENV=LEVEL")]
        requests: Vec<String>,
        /// Add this device's agent key to your existing entry instead, marked
        /// `agent: true`. Create it first with `mm key create --agent`. After
        /// `mm key rotate --agent` it takes the place of the old agent key.
        #[arg(long, conflicts_with = "requests")]
        agent: bool,
        #[command(flatten)]
        review: TeamReview,
    },
    /// Add a user with a given public key (reviewed edit).
    Add {
        #[arg(value_name = "NAME")]
        name: String,
        /// OpenSSH public key line, `ssh-ed25519 AAAA… comment`.
        #[arg(long, value_name = "PUBLIC_KEY")]
        key: String,
        /// A CI or automation identity rather than a person.
        #[arg(long)]
        machine: bool,
        /// Access per environment, for example staging=deploy or *=view. Repeatable.
        #[arg(long = "access", value_name = "ENV=LEVEL")]
        access: Vec<String>,
        #[command(flatten)]
        review: TeamReview,
    },
    /// Remove a user from the users file (reviewed edit).
    Remove {
        #[arg(value_name = "NAME")]
        name: String,
        #[command(flatten)]
        review: TeamReview,
    },
    /// Show the users file: each user's keys and access per environment.
    Show,
    /// Show who the remote environment thinks you are and what you may do.
    Whoami,
    /// Reconcile the selected remote environment's access from a commit's
    /// users file without deploying.
    Sync {
        /// Commit whose users file to apply; defaults to HEAD.
        #[arg(long, value_name = "SHA")]
        commit: Option<String>,
        /// On a target that lists nodes, plan only these nodes. The others
        /// keep the access they enforce now.
        #[arg(
            long,
            value_name = "NODE,...",
            value_delimiter = ',',
            conflicts_with = "plan"
        )]
        nodes: Vec<String>,
        #[command(flatten)]
        review: TeamReview,
    },
}

#[derive(Debug, Clone, Args)]
pub struct TeamReview {
    /// Exact hash printed by an earlier review of the same change.
    #[arg(long, value_name = "HASH", requires = "yes")]
    plan: Option<String>,
    /// Apply the exact reviewed plan.
    #[arg(long, requires = "plan")]
    yes: bool,
}

pub(super) async fn run_team(
    command: TeamCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    match command {
        TeamCommand::Join {
            name,
            agent: true,
            review,
            ..
        } => join_existing(
            invocation,
            name,
            review,
            crate::domain::key::ManagedKeyKind::Agent,
        ),
        TeamCommand::Join {
            name,
            requests,
            review,
            ..
        } => join(invocation, name, &requests, review),
        TeamCommand::Add {
            name,
            key,
            machine,
            access,
            review,
        } => propose_user(
            invocation,
            UserProposal {
                name,
                key,
                machine,
                access: UserProposal::parse_access(&access).map_err(environment_error)?,
            }
            .request(),
            review,
        ),
        TeamCommand::Remove { name, review } => propose_user(
            invocation,
            ConfigMutationRequest::user(ConfigMutationAction::Remove, &name, None),
            review,
        ),
        TeamCommand::Show => {
            let document = composition::project_documents().load_from(&invocation.start)?;
            output::write_team(&document, invocation.format)?;
            Ok(0)
        }
        TeamCommand::Whoami => {
            let (transport, scope) = remote(invocation)?;
            let identity = transport.whoami(scope).await?;
            output::write_remote_identity(&identity, invocation.format)?;
            Ok(0)
        }
        TeamCommand::Sync {
            commit,
            nodes,
            review,
        } => sync(invocation, commit, &nodes, review).await,
    }
}

/// `mm team sync`: plans an access sync on the environment's host, or on
/// every node of a cluster target under one hash, and applies a reviewed one.
async fn sync(
    invocation: &super::dispatch::Invocation,
    commit: Option<String>,
    nodes: &[String],
    review: TeamReview,
) -> Result<u8, MmError> {
    let environment = composition::remote_environment(
        &invocation.selection,
        invocation.home(),
        invocation.runtime_kind,
    )?
    .ok_or_else(local_environment)?;
    // The hosts stored the reviewed plan and check it is unchanged,
    // unexpired, and unclaimed; planning again would mint a new one.
    if let Some(reviewed) = review.plan {
        let started = std::time::Instant::now();
        let progress = inline::Progress::new(invocation.format);
        let report = inline::step(
            &progress,
            (inline::ROLLOUT, "Syncing users on every node"),
            environment.apply_team_sync(&reviewed, &|step| progress.rollout(step)),
            |_| Some("Synced users on every node".to_owned()),
        )
        .await;
        let surface = progress.surface();
        progress.finish();
        let report = report?;
        if environment.is_cluster() {
            inline::print_card(
                surface,
                || {
                    output::apply_report_card(
                        &report,
                        "Team sync",
                        output::access_outcome,
                        started.elapsed(),
                    )
                },
                || output::write_cluster_access_report(&report, invocation.format),
            )?;
        } else if let Some(change) = report.nodes.first().and_then(|node| node.result.as_ref()) {
            output::write_access_change(change, invocation.format)?;
        }
        return match report.stopped("mm team sync") {
            Some(error) => Err(error),
            None => Ok(0),
        };
    }
    let commit = composition::resolve_commit(&invocation.start, commit.as_deref())?;
    let only = (!nodes.is_empty()).then_some(nodes);
    let progress = inline::Progress::new(invocation.format);
    let plan = inline::step(
        &progress,
        ("plan", "Planning the users file on every node"),
        environment.plan_team_sync(&commit, only),
        |plan| {
            Some(format!(
                "Planned on {}",
                inline::count(plan.nodes.len(), "node", "nodes")
            ))
        },
    )
    .await?;
    progress.finish();
    if environment.is_cluster() {
        output::write_cluster_access_plan(&plan, invocation.format)?;
    } else if let Some(host) = plan.nodes.first() {
        output::write_access_plan(&host.plan, invocation.format)?;
    }
    Ok(0)
}

fn local_environment() -> MmError {
    MmError::Environment {
        message: "the selected environment is local; pass --environment with a remote environment"
            .to_owned(),
    }
}

fn remote(
    invocation: &super::dispatch::Invocation,
) -> Result<
    (
        Arc<dyn crate::app::ports::RemoteTransport>,
        crate::domain::remote::RemoteScope,
    ),
    MmError,
> {
    composition::remote_transport_context(
        &invocation.selection,
        invocation.home(),
        invocation.runtime_kind,
    )?
    .ok_or_else(local_environment)
}

/// `mm team join`: a new user, or during a rotation the entry that lists the key.
fn join(
    invocation: &super::dispatch::Invocation,
    name: Option<String>,
    requests: &[String],
    review: TeamReview,
) -> Result<u8, MmError> {
    let kind = crate::domain::key::ManagedKeyKind::Device;
    let keys = composition::key_service(invocation.home())?;
    let document = composition::project_documents().load_from(&invocation.start);
    let users = document
        .as_ref()
        .map(|document| document.config.users.clone())
        .unwrap_or_default();
    let key = match keys.next_key(kind)? {
        Some(next) => {
            let listed = name.as_ref().is_none_or(|name| users.contains_key(name));
            if listed {
                return join_existing(invocation, name, review, kind);
            }
            next.public_key
        }
        None => {
            composition::device_key(invocation.home())?
                .ok_or_else(|| MmError::Environment {
                    message: "this device has no mm key yet; run `mm key create` first".to_owned(),
                })?
                .public_key
        }
    };
    let name = name
        .or_else(|| std::env::var("USER").ok())
        .ok_or_else(|| MmError::Environment {
            message: "pass --name; $USER is not set".to_owned(),
        })?;
    let fingerprint = crate::domain::host_policy::parse_public_key(&key)
        .map(|parsed| parsed.fingerprint)
        .map_err(environment_error)?;
    if let Some(owner) =
        crate::domain::users::users_listing(&users, std::slice::from_ref(&fingerprint)).first()
    {
        return Err(MmError::Environment {
            message: format!(
                "user '{owner}' already lists this device's mm key ({fingerprint}), so it is not proposed for a new user '{name}'. Change '{owner}''s access in its entry, or run `mm team join --name {owner}` while a rotation of the key (`mm key rotate`) is in progress"
            ),
        });
    }
    propose_user(
        invocation,
        UserProposal {
            name,
            key,
            machine: false,
            access: UserProposal::parse_access(requests).map_err(environment_error)?,
        }
        .request(),
        review,
    )
}

/// The entry `mm team join` edits: `--name`, the one listing the key, or `$USER`.
fn entry_name(
    name: Option<String>,
    users: &std::collections::BTreeMap<String, crate::domain::users::UserConfig>,
    rotating: Option<&Vec<String>>,
    kind: crate::domain::key::ManagedKeyKind,
) -> Result<String, MmError> {
    match (name, rotating) {
        (Some(name), _) => Ok(name),
        (None, Some(fingerprints)) => {
            crate::domain::users::rotating_entry(users, fingerprints, kind.flag())
                .map_err(environment_error)
        }
        (None, None) => std::env::var("USER").map_err(|_| MmError::Environment {
            message: "pass --name; $USER is not set".to_owned(),
        }),
    }
}

/// Puts this device's key of `kind` in the caller's existing users entry:
/// the agent key, or the new key of a rotation in progress, in place of
/// the keys of this device it replaces.
fn join_existing(
    invocation: &super::dispatch::Invocation,
    name: Option<String>,
    review: TeamReview,
    kind: crate::domain::key::ManagedKeyKind,
) -> Result<u8, MmError> {
    let keys = composition::key_service(invocation.home())?;
    // The keys of this device that a rotation replaces: mm wrote them down
    // or still holds them, so these lines and no others leave the entry.
    // A device without the key is told to create it.
    let (key, previous) = keys.joining(kind)?;
    let agent = kind == crate::domain::key::ManagedKeyKind::Agent;
    let document = composition::project_documents().load_from(&invocation.start)?;
    let rotating = keys.next_key(kind)?.is_some();
    let mut fingerprints = previous.clone();
    fingerprints.push(key.fingerprint.clone());
    let name = entry_name(
        name,
        &document.config.users,
        rotating.then_some(&fingerprints),
        kind,
    )?;
    let mut user = document
        .config
        .users
        .get(&name)
        .cloned()
        .ok_or_else(|| MmError::Environment {
            message: format!(
                "user '{name}' is not in the users file; join with `mm team join` first, or pass --name"
            ),
        })?;
    let fingerprint = |line: &str| {
        crate::domain::host_policy::parse_public_key(line)
            .map(|key| key.fingerprint)
            .ok()
    };
    let listed = user.keys.len();
    let mut replaced = Vec::new();
    user.keys.retain(|existing| {
        let Some(found) = fingerprint(existing.line()).filter(|found| previous.contains(found))
        else {
            return true;
        };
        replaced.push(found);
        false
    });
    let lists_new = user
        .keys
        .iter()
        .any(|existing| fingerprint(existing.line()) == Some(key.fingerprint.clone()));
    if lists_new && user.keys.len() == listed {
        return Err(MmError::Environment {
            message: format!("user '{name}' already lists {}", kind.description()),
        });
    }
    if !lists_new {
        user.keys.push(if agent {
            crate::domain::users::UserKeyConfig::Detailed {
                key: key.public_key,
                expires: None,
                agent: true,
            }
        } else {
            crate::domain::users::UserKeyConfig::Line(key.public_key)
        });
    }
    let definition = serde_json::to_value(&user).map_err(|error| MmError::State {
        message: format!("serializing user '{name}': {error}"),
    })?;
    let applying = review.plan.is_some();
    let code = propose_user(
        invocation,
        ConfigMutationRequest::user(ConfigMutationAction::Edit, &name, Some(definition)),
        review,
    )?;
    if invocation.format == OutputFormat::Human {
        for fingerprint in &replaced {
            crate::outln!(
                "{} the previous {} {} of this device in user '{}'; hosts stop accepting it after the next `mm team sync`.",
                if applying {
                    "Replaced"
                } else {
                    "This replaces"
                },
                if agent { "agent key" } else { "mm key" },
                output::terminal_safe(fingerprint),
                output::terminal_safe(&name)
            );
        }
    }
    if applying {
        // The entry has the new key: nothing is left to take out later.
        keys.forget_replaced(kind)?;
    }
    Ok(code)
}

fn environment_error(message: String) -> MmError {
    MmError::Environment { message }
}

/// Plans a users-file change, or applies it when its review is given.
fn propose_user(
    invocation: &super::dispatch::Invocation,
    request: Result<ConfigMutationRequest, String>,
    review: TeamReview,
) -> Result<u8, MmError> {
    let request = request.map_err(environment_error)?;
    let service = composition::onboarding_service();
    let plan = service.plan_mutation(&invocation.start, request.clone())?;
    let Some(reviewed) = review.plan else {
        output::write_config_mutation_plan(&plan, invocation.format)?;
        return Ok(0);
    };
    let result = service.apply_mutation(&invocation.start, request, &reviewed)?;
    output::write_config_mutation_result(&result, invocation.format)?;
    if invocation.format == OutputFormat::Human {
        crate::outln!(
            "Commit the change and open a pull request; access applies when an admin deploys or runs `mm team sync`."
        );
    }
    Ok(0)
}
