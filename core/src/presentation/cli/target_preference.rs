//! `mm target preference`: this device's private runtime, target, and node
//! address choices.

use super::*;

#[derive(Debug, Clone, Subcommand)]
pub enum PreferCommand {
    /// Show resolved private runtime and target preferences.
    Show,
    /// Persist a private runtime preference.
    SetRuntime {
        #[arg(id = "preference_runtime", value_name = "RUNTIME")]
        runtime: RuntimeKindArg,
        /// Limit the preference to one environment.
        #[arg(long, value_name = "NAME")]
        environment: Option<String>,
    },
    /// Remove a private runtime preference.
    UnsetRuntime {
        /// Limit the unset to one environment.
        #[arg(long, value_name = "NAME")]
        environment: Option<String>,
    },
    /// Persist a private target preference.
    SetTarget {
        #[arg(value_name = "TARGET")]
        target: String,
        /// Limit the preference to one environment.
        #[arg(long, value_name = "NAME")]
        environment: Option<String>,
    },
    /// Remove a private target preference.
    UnsetTarget {
        /// Limit the unset to one environment.
        #[arg(long, value_name = "NAME")]
        environment: Option<String>,
    },
    /// Connect this device to a cluster node at an address of its own,
    /// such as a VPN address, instead of the node's host in targets.yaml.
    /// The node's pinned host key and daemon identity must still match.
    #[command(
        after_help = "Examples:\n  mm target preference set-address staging app-1 10.0.0.3\n  mm target preference set-address staging app-2 node-2.internal:2222\n  mm target preference set-address staging app-3 '[fd00::5]:22'\n\nThe address is kept in this device's mm home and nowhere else. Without a port, mm uses the node's port from targets.yaml."
    )]
    SetAddress {
        /// The environment whose target lists the node.
        #[arg(id = "address_environment", value_name = "ENVIRONMENT")]
        environment: String,
        /// The node's name under `nodes:`.
        #[arg(id = "address_node", value_name = "NODE")]
        node: String,
        /// A hostname, IPv4 or IPv6 address, with an optional port.
        #[arg(id = "address", value_name = "HOST[:PORT]")]
        address: String,
    },
    /// Connect to a cluster node at its host in targets.yaml again.
    UnsetAddress {
        /// The environment whose target lists the node.
        #[arg(id = "address_environment", value_name = "ENVIRONMENT")]
        environment: String,
        /// The node's name under `nodes:`.
        #[arg(id = "address_node", value_name = "NODE")]
        node: String,
    },
}

pub(super) fn run_prefer_command(
    command: PreferCommand,
    start: &std::path::Path,
    explicit_home: Option<&std::path::Path>,
    _format: OutputFormat,
    clock: Arc<dyn Clock>,
) -> Result<u8, MmError> {
    let (document, state) = composition::preference_state(start, explicit_home)?;
    let project_id = document.config.project.id;
    let now = clock.now_unix_ms();
    match command {
        PreferCommand::Show => {
            let project = state.local_project_preference(project_id)?;
            let addresses = composition::project_node_addresses(&document, explicit_home)?;
            output::write_json(&serde_json::json!({
                "project_id": project_id.0.to_string(),
                "project": project.as_ref().map(|pref| {
                    serde_json::json!({
                        "runtime": pref.runtime,
                        "target": pref.target,
                    })
                }),
                "node_addresses": addresses,
            }))?;
        }
        PreferCommand::SetAddress {
            environment,
            node,
            address,
        } => {
            composition::set_node_address(
                &document,
                state.as_ref(),
                explicit_home,
                (&environment, &node),
                &address,
            )?;
        }
        PreferCommand::UnsetAddress { environment, node } => {
            composition::unset_node_address(&document, explicit_home, (&environment, &node))?;
        }
        PreferCommand::SetRuntime {
            runtime,
            environment,
        } => set_runtime_preference(
            state.as_ref(),
            project_id,
            environment,
            Some(runtime.into()),
            now,
        )?,
        PreferCommand::UnsetRuntime { environment } => {
            set_runtime_preference(state.as_ref(), project_id, environment, None, now)?
        }
        PreferCommand::SetTarget {
            target,
            environment,
        } => set_target_preference(state.as_ref(), project_id, environment, Some(&target), now)?,
        PreferCommand::UnsetTarget { environment } => {
            set_target_preference(state.as_ref(), project_id, environment, None, now)?
        }
    }
    Ok(0)
}

fn set_runtime_preference(
    state: &dyn crate::app::ports::StateStore,
    project_id: crate::domain::workspace::ProjectId,
    environment: Option<String>,
    runtime: Option<crate::domain::runtime::RuntimeKind>,
    now: i64,
) -> Result<(), MmError> {
    if let Some(environment) = environment {
        let existing = state.local_environment_preference(project_id, &environment)?;
        state.set_local_environment_preference(
            project_id,
            &environment,
            runtime,
            existing
                .as_ref()
                .and_then(|preference| preference.target.as_deref()),
            now,
        )?;
    } else {
        let existing = state.local_project_preference(project_id)?;
        state.set_local_project_preference(
            project_id,
            runtime,
            existing
                .as_ref()
                .and_then(|preference| preference.target.as_deref()),
            now,
        )?;
    }
    Ok(())
}

fn set_target_preference(
    state: &dyn crate::app::ports::StateStore,
    project_id: crate::domain::workspace::ProjectId,
    environment: Option<String>,
    target: Option<&str>,
    now: i64,
) -> Result<(), MmError> {
    if let Some(environment) = environment {
        let existing = state.local_environment_preference(project_id, &environment)?;
        state.set_local_environment_preference(
            project_id,
            &environment,
            existing.and_then(|preference| preference.runtime),
            target,
            now,
        )?;
    } else {
        let existing = state.local_project_preference(project_id)?;
        state.set_local_project_preference(
            project_id,
            existing.and_then(|preference| preference.runtime),
            target,
            now,
        )?;
    }
    Ok(())
}
