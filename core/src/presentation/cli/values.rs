//! `mm secret` and `mm env` on a remote environment: what the hosts hold,
//! and changes staged on every host for the next deployment.

use super::*;
use crate::app::{KeyEdit, RemoteEnvironment, ValueEdit};
use crate::domain::staged::ValueKind;
use crate::presentation::inline::{self, Progress};

pub(super) enum RemoteValueCommand {
    Status(Option<String>),
    Set(String, Zeroizing<String>),
    Unset(String),
    Discard(String),
}

/// The selected environment's hosts, when it runs on a remote target.
pub(super) fn remote(
    start: &std::path::Path,
    environment: Option<String>,
    runtime: crate::domain::runtime::RuntimeKind,
    explicit_home: Option<&std::path::Path>,
) -> Result<Option<RemoteEnvironment>, MmError> {
    let document = composition::project_documents().load_from(start)?;
    let name = crate::domain::config::resolve_environment(&document, environment.as_deref())?;
    let target = document
        .config
        .environments
        .get(&name)
        .and_then(|overlay| overlay.target.as_deref())
        .unwrap_or(document.config.target.as_str());
    if target == "local" {
        return Ok(None);
    }
    let mut selection = WorkspaceSelection::new(start, environment);
    selection.runtime = Some(runtime);
    composition::remote_environment(&selection, explicit_home, runtime)
}

pub(super) async fn run_remote_values(
    remote: &RemoteEnvironment,
    kind: ValueKind,
    command: RemoteValueCommand,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let progress = Progress::new(format);
    let (key, edit) = match command {
        RemoteValueCommand::Status(key) => {
            let values = inline::step(
                &progress,
                ("read", "Reading what the hosts hold"),
                remote.host_values(),
                |values| Some(inline::count(values.nodes.len(), "host", "hosts")),
            )
            .await?;
            let surface = progress.surface();
            progress.finish();
            inline::print_card(
                surface,
                || output::remote_values_card(&values, key.as_deref()),
                || output::write_remote_values(&values, key.as_deref(), format),
            )?;
            return Ok(u8::from(!values.unreachable.is_empty()));
        }
        RemoteValueCommand::Set(key, value) => (key, ValueEdit::Set(value)),
        RemoteValueCommand::Unset(key) => (key, ValueEdit::Unset),
        RemoteValueCommand::Discard(key) => (key, ValueEdit::Discard),
    };
    let report = inline::step(
        &progress,
        ("stage", "Staging on every host"),
        remote.stage_values(
            vec![KeyEdit { kind, key, edit }],
            &composition::AgeValueSealer,
        ),
        |report| Some(format!("Staged on {}", report.nodes.join(", "))),
    )
    .await?;
    progress.finish();
    output::write_stage_report(&report, format)?;
    Ok(0)
}

/// `discard` on an environment that runs on this machine.
pub(super) fn nothing_staged_here(key: &str) -> MmError {
    MmError::Environment {
        message: format!(
            "this environment runs on this machine, where nothing is staged for '{key}'; discard drops a change staged on a remote environment's hosts"
        ),
    }
}

/// Whether secret `name` is kept in the managed store of cluster environments.
fn kept_in_store(document: &crate::domain::config::ConfigDocument, name: &str) -> bool {
    document
        .config
        .secrets
        .get(name)
        .is_some_and(|declaration| {
            document
                .config
                .providers
                .iter()
                .any(|(provider, instance)| {
                    provider.as_str() == declaration.storage.provider.as_str()
                        && crate::domain::secret_store::is_cluster_store(instance)
                })
        })
}

/// Whether a host holds a value of secret `name` or has one staged.
fn held(values: &crate::domain::staged::RemoteValues, name: &str) -> bool {
    values
        .nodes
        .iter()
        .flat_map(|node| &node.values)
        .any(|value| {
            value.kind == ValueKind::Secret
                && value.key == name
                && (value.active.is_some()
                    || value.staged.as_ref().is_some_and(|staged| !staged.removal))
        })
}

/// `mm secret generate` and `mm secret rotate` for the secrets a remote
/// environment's managed store keeps: made here, staged on every host.
/// `None` when the command is for other secrets or a local environment.
pub(super) async fn generate_in_store(
    command: &super::secret::SecretCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<Option<u8>, MmError> {
    use super::secret::SecretCommand;
    let (requested, all, rotate, stdin) = match command {
        SecretCommand::Generate { key, all, .. } => (key.clone(), *all, false, false),
        SecretCommand::Rotate { key, stdin, .. } => (Some(key.clone()), false, true, *stdin),
        _ => return Ok(None),
    };
    let batch = !rotate && requested.is_none();
    let document = composition::project_documents().load_from(&invocation.start)?;
    let names = if rotate {
        requested.into_iter().collect()
    } else {
        let environment = crate::domain::config::resolve_environment(
            &document,
            invocation.selection.environment.as_deref(),
        )?;
        let selection = super::secret::generation_selection(requested, all);
        crate::app::generated_secrets(&document, &environment, &selection)?
    };
    let (names, outside): (Vec<_>, Vec<_>) = names
        .into_iter()
        .partition(|name| kept_in_store(&document, name));
    if names.is_empty() {
        return Ok(None);
    }
    let Some(remote) = remote(
        &invocation.start,
        invocation.selection.environment.clone(),
        invocation.runtime_kind,
        invocation.home(),
    )?
    else {
        return Ok(None);
    };
    let values = if rotate {
        None
    } else {
        Some(remote.host_values().await?)
    };
    let mut edits = Vec::new();
    for name in names {
        let value = match &document.config.secrets[&name].generate {
            _ if values.as_ref().is_some_and(|values| held(values, &name)) => continue,
            Some(generator) => crate::app::generate_secret(generator)?.into_zeroizing(),
            None if rotate => {
                read_secret_material(stdin, &format!("New value for {name}"))?.into_zeroizing()
            }
            None => continue,
        };
        edits.push(KeyEdit {
            kind: ValueKind::Secret,
            key: name,
            edit: ValueEdit::Set(value),
        });
    }
    if batch && !outside.is_empty() {
        generate_outside_store(outside, invocation).await?;
    }
    stage_generated(&remote, edits, invocation.format).await
}

/// `generate --missing` and `--all` for the secrets the managed store does
/// not keep.
async fn generate_outside_store(
    names: Vec<String>,
    invocation: &super::dispatch::Invocation,
) -> Result<(), MmError> {
    let report = composition::environment_services(
        &invocation.start,
        invocation.home(),
        Arc::clone(&invocation.clock),
    )?
    .secrets
    .generate(
        &invocation.start,
        invocation.selection.environment.as_deref(),
        &crate::domain::secret::SecretGenerationSelection::Listed(names),
    )
    .await?;
    output::write_secret_generation(&report, invocation.format)
}

async fn stage_generated(
    remote: &RemoteEnvironment,
    edits: Vec<KeyEdit>,
    format: OutputFormat,
) -> Result<Option<u8>, MmError> {
    if edits.is_empty() {
        eprintln!("Every generated secret of the store is set or staged on the hosts");
        return Ok(Some(0));
    }
    let progress = Progress::new(format);
    let report = inline::step(
        &progress,
        ("stage", "Staging on every host"),
        remote.stage_values(edits, &composition::AgeValueSealer),
        |report| Some(format!("Staged on {}", report.nodes.join(", "))),
    )
    .await?;
    progress.finish();
    output::write_stage_report(&report, format)?;
    Ok(Some(0))
}

/// `mm secret migrate --from age --to NAME` on a cluster environment.
pub(super) async fn run_migrate(
    provider: &str,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let environment = invocation.selection.environment.clone();
    let document = composition::project_documents().load_from(&invocation.start)?;
    let name = crate::domain::config::resolve_environment(&document, environment.as_deref())?;
    let Some(remote) = remote(
        &invocation.start,
        environment.clone(),
        invocation.runtime_kind,
        invocation.home(),
    )?
    else {
        return Err(MmError::Environment {
            message: "this environment runs on this machine; `mm secret migrate` moves a cluster environment's secrets into the store mm runs on its nodes".to_owned(),
        });
    };
    let progress = Progress::new(invocation.format);
    let moved = inline::step(
        &progress,
        ("migrate", "Moving the age store's secrets into the store"),
        remote.store_migrate(provider),
        |moved| {
            Some(format!(
                "{} of {} written, on {}",
                moved.secrets.iter().filter(|secret| secret.written).count(),
                moved.secrets.len(),
                moved.node
            ))
        },
    )
    .await?;
    progress.finish();
    output::write_store_migrated(&moved, &name, invocation.format)?;
    Ok(0)
}
