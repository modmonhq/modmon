//! `mm key`: this device's SSH keys. mm creates them, encrypts them under
//! passphrases it keeps in a vault, and is the only program that uses them.

use super::*;
use crate::domain::key::{KeyChangePlan, ManagedKey, ManagedKeyKind};

#[derive(Debug, Clone, Subcommand)]
pub enum KeyCommand {
    /// Create this device's SSH key under MM_HOME, encrypted under a
    /// passphrase mm keeps in the system keychain. mm never uses ~/.ssh.
    Create {
        /// Use a FIDO2 security key (sk-ssh-ed25519); touch it when asked.
        #[arg(long, conflicts_with = "agent")]
        security_key: bool,
        /// Create the key `mm mcp serve` uses instead: a separate key for
        /// your AI agent, limited on the host by the `agents:` block.
        #[arg(long)]
        agent: bool,
    },
    /// Print this device's public key and fingerprint for the users file,
    /// and how the key is protected.
    Show {
        /// Print the agent key instead.
        #[arg(long)]
        agent: bool,
    },
    /// Encrypt this device's plain keys in place (reviewed). Public keys
    /// stay the same, so no host access changes.
    #[command(after_help = "Examples:\n  mm key protect\n  mm key protect --plan HASH --yes")]
    Protect {
        #[command(flatten)]
        review: KeyReview,
    },
    /// Replace a key with a new one (reviewed), without losing access: the
    /// new key is made beside the current one and mm offers both until you
    /// finish the rotation.
    #[command(
        after_help = "Examples:\n  mm key rotate\n  mm key rotate --plan HASH --yes\n  mm team join\n  mm team sync\n  mm key rotate --finish\n  mm key rotate --finish --plan HASH --yes\n\nWhile a rotation is in progress, `mm team join` puts the new key in your entry in place of the current one, and `mm team sync` installs it on the hosts; it authenticates with the current key, so this works for a sole admin. Then `--finish` drops the old key. A security key is replaced at once."
    )]
    Rotate {
        /// Rotate the agent key instead.
        #[arg(long)]
        agent: bool,
        /// Put the new key in place of the current one and delete the old
        /// key. Hosts whose users file does not list the new key refuse
        /// this device from then on.
        #[arg(long)]
        finish: bool,
        /// Finish even though the users file here does not list the new key.
        #[arg(long, requires = "finish")]
        unlisted: bool,
        #[command(flatten)]
        review: KeyReview,
    },
    /// Delete a key and its passphrase from this device (reviewed).
    #[command(
        after_help = "Examples:\n  mm key remove --agent\n  mm key remove --agent --plan HASH --yes"
    )]
    Remove {
        /// Delete the agent key instead.
        #[arg(long)]
        agent: bool,
        #[command(flatten)]
        review: KeyReview,
    },
}

#[derive(Debug, Clone, Args)]
pub struct KeyReview {
    /// Exact hash printed by an earlier review of the same change.
    #[arg(long, value_name = "HASH", requires = "yes")]
    plan: Option<String>,
    /// Apply the exact reviewed plan.
    #[arg(long, requires = "plan")]
    yes: bool,
}

const fn kind(agent: bool) -> ManagedKeyKind {
    if agent {
        ManagedKeyKind::Agent
    } else {
        ManagedKeyKind::Device
    }
}

pub(super) fn run(
    command: KeyCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let keys = composition::key_service(invocation.home())?;
    let format = invocation.format;
    match command {
        KeyCommand::Create {
            agent,
            security_key,
        } => {
            let created = keys.create(kind(agent), security_key)?;
            if format == OutputFormat::Json {
                return output::write_json(&created).map(|()| 0);
            }
            write_key(&created.key);
            write_notes(&created.notes);
            write_next_step(created.key.kind);
        }
        KeyCommand::Show { agent } => {
            let kind = kind(agent);
            let key = keys.key(kind)?.ok_or_else(|| MmError::Environment {
                message: if agent {
                    "this device has no agent key yet; run `mm key create --agent`".to_owned()
                } else {
                    "this device has no mm key yet; run `mm key create`".to_owned()
                },
            })?;
            if format == OutputFormat::Json {
                return output::write_json(&key).map(|()| 0);
            }
            write_key(&key);
            write_next_step(kind);
        }
        KeyCommand::Protect { review } => {
            let Some(reviewed) = review.plan else {
                return write_plan(&keys.protect_plan()?, "mm key protect", format);
            };
            let protected = keys.protect(&reviewed)?;
            if format == OutputFormat::Json {
                return output::write_json(&protected).map(|()| 0);
            }
            for key in &protected {
                write_key(key);
            }
            crate::outln!("Hosts accept these keys as before: their public halves did not change.");
        }
        KeyCommand::Rotate {
            agent,
            finish,
            unlisted,
            review,
        } => {
            if finish && !unlisted {
                new_key_listed(&keys, kind(agent), invocation)?;
            }
            return rotate(&keys, (kind(agent), finish, unlisted), review, format);
        }
        KeyCommand::Remove { agent, review } => {
            let kind = kind(agent);
            let command = format!("mm key remove{}", kind.flag());
            let Some(reviewed) = review.plan else {
                return write_plan(&keys.remove_plan(kind)?, &command, format);
            };
            let removed = keys.remove(kind, &reviewed)?;
            if format == OutputFormat::Json {
                return output::write_json(&removed).map(|()| 0);
            }
            crate::outln!(
                "Deleted {} ({}) and its passphrase.",
                kind.description(),
                output::terminal_safe(&removed.fingerprint)
            );
            crate::outln!(
                "Hosts still list its public key: remove it from the users file and run `mm team sync`."
            );
        }
    }
    Ok(0)
}

fn write_key(key: &ManagedKey) {
    crate::outln!("{}", output::terminal_safe(&key.public_key));
    crate::outln!("  fingerprint: {}", output::terminal_safe(&key.fingerprint));
    crate::outln!(
        "  private key: {}",
        output::terminal_safe(&key.path.display().to_string())
    );
    crate::outln!(
        "  protection:  {}",
        output::terminal_safe(&key.protection_summary())
    );
}

fn write_notes(notes: &[String]) {
    for note in notes {
        crate::outln!("{}", output::terminal_safe(note));
    }
}

fn write_next_step(kind: ManagedKeyKind) {
    match kind {
        ManagedKeyKind::Agent => crate::outln!(
            "Add it to your entry in the users file with `mm team join --agent`. A host gives it your access limited by the `agents:` block."
        ),
        ManagedKeyKind::Device => {
            crate::outln!("Add the public key to the users file with `mm team join`.");
        }
    }
    crate::outln!("This key is for mm only: never pass the private key file to ssh, scp or git.");
}

/// Refuses `--finish` while the users file here lists the current key and not the new one.
fn new_key_listed(
    keys: &crate::app::KeyService,
    kind: ManagedKeyKind,
    invocation: &super::dispatch::Invocation,
) -> Result<(), MmError> {
    if keys.next_key(kind)?.is_none() {
        return Ok(());
    }
    let Ok(document) = composition::project_documents().load_from(&invocation.start) else {
        return Ok(());
    };
    let (next, previous) = keys.joining(kind)?;
    let users = &document.config.users;
    let old = crate::domain::users::users_listing(users, &previous);
    if old.is_empty()
        || !crate::domain::users::users_listing(users, std::slice::from_ref(&next.fingerprint))
            .is_empty()
    {
        return Ok(());
    }
    let flag = kind.flag();
    Err(MmError::Environment {
        message: format!(
            "the users file here lists {} in user '{}' but not the new key {}: finishing now would have this project's hosts refuse this device. Run `mm team join{flag}`, commit, and run `mm team sync` first; or pass --unlisted to finish anyway",
            kind.description(),
            old.join("', '"),
            next.fingerprint
        ),
    })
}

/// `mm key rotate`: starts a rotation, or with `--finish` ends it.
fn rotate(
    keys: &crate::app::KeyService,
    (kind, finish, unlisted): (ManagedKeyKind, bool, bool),
    review: KeyReview,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let command = format!(
        "mm key rotate{}{}{}",
        kind.flag(),
        if finish { " --finish" } else { "" },
        if unlisted { " --unlisted" } else { "" }
    );
    let Some(reviewed) = review.plan else {
        let plan = if finish {
            keys.finish_plan(kind)?
        } else {
            keys.rotate_plan(kind)?
        };
        return write_plan(&plan, &command, format);
    };
    let rotated = if finish {
        keys.finish(kind, &reviewed)?
    } else {
        keys.rotate(kind, &reviewed)?
    };
    if format == OutputFormat::Json {
        return output::write_json(&rotated).map(|()| 0);
    }
    write_key(&rotated.key);
    write_notes(&rotated.notes);
    let old = output::terminal_safe(rotated.replaced.as_deref().unwrap_or("the old key"));
    let flag = kind.flag();
    if finish {
        crate::outln!(
            "The old key ({old}) and its passphrase are deleted. Hosts whose users file still lists only the old key refuse this device now: have an admin put the key above there and run `mm team sync`. Wherever else you listed the old key, such as root's ~/.ssh/authorized_keys on hosts `mm target setup` logs in to, put the key above in its place."
        );
    } else if rotated
        .key
        .path
        .ends_with(format!("{}.next", kind.file_name()))
    {
        crate::outln!(
            "The new key above waits beside the current one ({old}); mm offers both, so this device keeps its access. Run `mm team join{flag}` in each project: it puts the new key in your entry in place of the current one. Commit, run `mm team sync` (it still authenticates with the current key), then `mm key rotate{flag} --finish`. Wherever else you listed the current key, such as root's ~/.ssh/authorized_keys on hosts `mm target setup` logs in to, add the new key before you finish."
        );
    } else {
        crate::outln!(
            "Hosts refuse this security key until the users file lists it in place of the old one ({old}) and an admin runs `mm team sync`."
        );
    }
    Ok(0)
}

/// The review of a key change, with the command that applies it.
fn write_plan(plan: &KeyChangePlan, command: &str, format: OutputFormat) -> Result<u8, MmError> {
    if format == OutputFormat::Json {
        return output::write_json(plan).map(|()| 0);
    }
    if plan.steps.is_empty() {
        crate::outln!("Nothing to do: every key on this device is protected.");
        return Ok(0);
    }
    for step in &plan.steps {
        crate::outln!("- {}", output::terminal_safe(&step.summary()));
    }
    crate::outln!("Apply: {command} --plan {} --yes", plan.plan_hash);
    Ok(0)
}
