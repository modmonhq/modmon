//! `mm secret provider recovery`: the recovery copy of the store mm runs on
//! a cluster environment's nodes, which only this machine's operator can open.

use super::*;

#[derive(Debug, Clone, Subcommand)]
pub enum RecoveryCommand {
    /// Write the recovery copy of an environment's managed store (its
    /// unseal key, mmd's logins and its certificate authority), sealed with
    /// age to you or to another operator, as ASCII armour.
    Export {
        /// The store's name under `providers:`.
        #[arg(value_name = "NAME")]
        name: String,
        /// Seal it to this age recipient (`age1…`) instead of to this
        /// machine's identity.
        #[arg(long, value_name = "AGE_RECIPIENT")]
        recipient: Option<String>,
        /// Write it to this new file instead of standard output.
        #[arg(long, value_name = "FILE")]
        to: Option<PathBuf>,
    },
    /// Keep a recovery copy another machine exported to you, so setup from
    /// this machine can give a reinstalled node its keys.
    Import {
        /// The store's name under `providers:`.
        #[arg(value_name = "NAME")]
        name: String,
        /// The file `recovery export --recipient` wrote.
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
}

pub(super) fn run_recovery(
    command: RecoveryCommand,
    document: &crate::domain::config::ConfigDocument,
    environment: &str,
    explicit_home: Option<&std::path::Path>,
) -> Result<u8, MmError> {
    let name = match &command {
        RecoveryCommand::Export { name, .. } | RecoveryCommand::Import { name, .. } => name,
    };
    let provider = super::provider::provider_name(name.clone())?;
    let declared = document
        .config
        .providers
        .get(&provider)
        .filter(|instance| crate::domain::secret_store::is_cluster_store(instance));
    if declared.is_none() || crate::app::cluster_target(document, environment).is_none() {
        return Err(MmError::Secret {
            message: format!(
                "'{provider}' is not the managed store of a cluster environment; only the store `mm target setup` runs on '{environment}''s nodes (`mode: managed`, `scope: environment`) has a recovery copy"
            ),
        });
    }
    match command {
        RecoveryCommand::Export { recipient, to, .. } => {
            let sealed = composition::export_store_recovery(
                explicit_home,
                document,
                (environment, provider.as_str()),
                recipient.as_deref(),
            )?;
            match to {
                Some(path) => {
                    write_new_private(&path, sealed.as_bytes())?;
                    eprintln!(
                        "Wrote the recovery copy of {provider} ({environment}) to {}, sealed to {}",
                        path.display(),
                        recipient
                            .as_deref()
                            .unwrap_or("this machine's age identity")
                    );
                }
                None => print!("{sealed}"),
            }
        }
        RecoveryCommand::Import { file, .. } => {
            let sealed = std::fs::read_to_string(&file).map_err(|source| MmError::Io {
                context: format!("reading {}", file.display()),
                source,
            })?;
            composition::import_store_recovery(
                explicit_home,
                document,
                (environment, provider.as_str()),
                &sealed,
            )?;
            eprintln!(
                "Kept the recovery copy of {provider} ({environment}); setup from this machine gives it to nodes that lack their keys"
            );
        }
    }
    Ok(0)
}

fn write_new_private(path: &std::path::Path, bytes: &[u8]) -> Result<(), MmError> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|source| MmError::Io {
            context: format!("writing {} (it must not exist yet)", path.display()),
            source,
        })
}

/// What a snapshot or a restore of a cluster environment's store acts on.
type Selected<'a> = (
    &'a std::path::Path,
    &'a crate::domain::config::ConfigDocument,
    &'a str,
    crate::domain::runtime::RuntimeKind,
);

/// Whether `name` is the store mm runs on `environment`'s nodes.
pub(super) fn is_cluster_store(
    document: &crate::domain::config::ConfigDocument,
    environment: &str,
    name: &str,
) -> bool {
    crate::domain::secret_store::declared_store(&document.config)
        .is_some_and(|declared| declared.provider.as_str() == name)
        && crate::app::cluster_target(document, environment).is_some()
}

fn remote_of(
    (start, _, environment, runtime): Selected<'_>,
    explicit_home: Option<&std::path::Path>,
) -> Result<crate::app::RemoteEnvironment, MmError> {
    super::values::remote(start, Some(environment.to_owned()), runtime, explicit_home)?.ok_or_else(
        || MmError::Environment {
            message: format!("environment '{environment}' runs on this machine, not on a cluster"),
        },
    )
}

/// `mm secret provider status` of a cluster environment's managed store.
pub(super) async fn run_status(
    selected: Selected<'_>,
    explicit_home: Option<&std::path::Path>,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let (_, document, environment, _) = selected;
    let remote = remote_of(selected, explicit_home)?;
    let report = composition::cluster_store_report(document, environment, &remote).await?;
    output::write_cluster_store_report(&report, format)?;
    Ok(if report.healthy() { 0 } else { 3 })
}

/// `mm secret provider snapshot`.
pub(super) async fn run_snapshot(
    selected: Selected<'_>,
    explicit_home: Option<&std::path::Path>,
    (name, to): (String, Option<PathBuf>),
    format: OutputFormat,
) -> Result<u8, MmError> {
    let (_, document, environment, _) = selected;
    let remote = remote_of(selected, explicit_home)?;
    let progress = crate::presentation::inline::Progress::new(format);
    let (node, sealed) = crate::presentation::inline::step(
        &progress,
        ("snapshot", "Taking a snapshot of the store"),
        composition::take_store_snapshot(explicit_home, document, (environment, &name), &remote),
        |(node, _)| Some(format!("Took it on {node}")),
    )
    .await?;
    progress.finish();
    match to {
        Some(path) => {
            write_new_private(&path, sealed.as_bytes())?;
            eprintln!(
                "Wrote a snapshot of {name} ({environment}) from {node} to {}, sealed to this machine's age identity; `mm -e {environment} secret provider restore {name} --file {}` puts it back",
                path.display(),
                path.display()
            );
        }
        None => print!("{sealed}"),
    }
    Ok(0)
}

/// `mm secret provider restore` for the store on a cluster's nodes: the
/// plan without `--plan`, the restore with it and `--yes`.
pub(super) async fn run_restore(
    selected: Selected<'_>,
    explicit_home: Option<&std::path::Path>,
    (name, file, reviewed, yes): (String, PathBuf, Option<String>, bool),
    format: OutputFormat,
) -> Result<u8, MmError> {
    let (_, document, environment, _) = selected;
    let sealed = std::fs::read_to_string(&file).map_err(|source| MmError::Io {
        context: format!("reading {}", file.display()),
        source,
    })?;
    let Some(reviewed) = reviewed else {
        let (plan, _) = composition::store_restore_plan(
            explicit_home,
            document,
            (environment, &name),
            &sealed,
        )?;
        output::write_store_restore_plan(&plan, &file, format)?;
        return Ok(0);
    };
    if !yes {
        return Err(MmError::Terminal {
            message: "restoring the store replaces everything it holds; add --yes to the reviewed plan's command".to_owned(),
        });
    }
    let remote = remote_of(selected, explicit_home)?;
    let progress = crate::presentation::inline::Progress::new(format);
    let restored = crate::presentation::inline::step(
        &progress,
        ("restore", "Putting the snapshot back on every node"),
        composition::restore_store(
            explicit_home,
            document,
            (environment, &name),
            (&sealed, &reviewed),
            &remote,
        ),
        |restored| Some(format!("{} steps", restored.len())),
    )
    .await?;
    progress.finish();
    if format == OutputFormat::Json {
        output::write_json(&restored)?;
    } else {
        eprintln!(
            "Restored {name} ({environment}) on every node; `mm -e {environment} target nodes` shows the members"
        );
    }
    Ok(0)
}
