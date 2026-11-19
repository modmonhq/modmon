use super::*;

#[derive(Debug, Clone, Subcommand)]
pub enum TargetCommand {
    /// Register this machine as the local target, preserving existing targets.
    Local,
    /// List targets configured beneath MM_HOME.
    List,

    /// Show one target from the shared catalog.
    Show {
        #[arg(value_name = "NAME")]
        name: String,
    },

    /// List every node of the selected environment's target: whether it
    /// answers (and why not), its mmd version, its mesh address, and the
    /// commit and time of its last applied deployment.
    Nodes,

    /// The WireGuard mesh between the nodes of the selected environment's
    /// target: look at it, or replace one node's key.
    Mesh {
        #[command(subcommand)]
        command: super::target_mesh::TargetMeshCommand,
    },

    /// The shared volumes of the selected environment's target: the node
    /// that keeps them, their sizes, and which nodes mount them; or delete
    /// one volume's files.
    Storage {
        #[command(subcommand)]
        command: super::target_storage::TargetStorageCommand,
    },

    /// List the SSH connections this device keeps open to remote hosts
    /// between commands, or close them.
    #[command(
        after_help = "Examples:\n  mm target connections\n  mm target connections --close\n\nThe first command to a host leaves its connection open for five idle minutes, so the next ones start faster. While it is open, a program running as you can reach that host's mm relay through it. MM_SSH_REUSE=off keeps none."
    )]
    Connections {
        /// Close every kept connection.
        #[arg(long)]
        close: bool,
    },

    /// Review a repo-declared target's host key and daemon identity, and
    /// accept a change with --yes.
    Trust {
        #[arg(value_name = "NAME")]
        name: String,
        /// Accept the repo's current keys after reviewing them.
        #[arg(long)]
        yes: bool,
    },

    /// Add an SSH target to the shared catalog.
    Add {
        #[arg(value_name = "NAME")]
        name: String,
        /// SSH hostname or address.
        #[arg(long)]
        host: String,
        /// SSH user.
        #[arg(long)]
        user: String,
        /// SSH port.
        #[arg(long, default_value_t = 22)]
        port: u16,
        /// OpenSSH SHA256 fingerprint accepted for this host.
        #[arg(long)]
        host_key_fingerprint: String,
        /// Full reviewed OpenSSH host public key used for strict known-hosts verification.
        #[arg(long, value_name = "TYPE BASE64")]
        host_key: Option<String>,
        /// Absolute private-key path used for public-key client authentication.
        #[arg(long, value_name = "PATH")]
        identity_file: Option<PathBuf>,
        /// Absolute project checkout path on the remote host.
        #[arg(long, value_name = "PATH")]
        remote_root: Option<PathBuf>,
        /// BLAKE3 daemon identity printed by `mmd --print-identity`.
        #[arg(long, value_name = "BLAKE3")]
        daemon_fingerprint: Option<String>,
    },

    /// Update an SSH target without changing its stable identity.
    Update {
        #[arg(value_name = "NAME")]
        name: String,
        /// SSH hostname or address.
        #[arg(long)]
        host: String,
        /// SSH user.
        #[arg(long)]
        user: String,
        /// SSH port.
        #[arg(long, default_value_t = 22)]
        port: u16,
        /// OpenSSH SHA256 fingerprint accepted for this host.
        #[arg(long)]
        host_key_fingerprint: String,
        /// Full reviewed OpenSSH host public key used for strict known-hosts verification.
        #[arg(long, value_name = "TYPE BASE64")]
        host_key: Option<String>,
        /// Absolute private-key path used for public-key client authentication.
        #[arg(long, value_name = "PATH")]
        identity_file: Option<PathBuf>,
        /// Absolute project checkout path on the remote host.
        #[arg(long, value_name = "PATH")]
        remote_root: Option<PathBuf>,
        /// BLAKE3 daemon identity printed by `mmd --print-identity`.
        #[arg(long, value_name = "BLAKE3")]
        daemon_fingerprint: Option<String>,
        /// Fingerprint from the reviewed target list.
        #[arg(long)]
        expected_fingerprint: String,
    },

    /// Remove a target only if its reviewed fingerprint still matches.
    Remove {
        #[arg(value_name = "NAME")]
        name: String,
        /// Fingerprint from `mm target show`.
        #[arg(long)]
        expected_fingerprint: String,
        /// Confirm catalog mutation.
        #[arg(long)]
        yes: bool,
    },

    /// Select a private target preference for this project.
    Select {
        #[arg(value_name = "TARGET")]
        target: String,
        /// Limit the preference to one environment.
        #[arg(long, value_name = "NAME")]
        environment: Option<String>,
    },

    /// Show or change private runtime and target preferences.
    Preference {
        #[command(subcommand)]
        command: PreferCommand,
    },

    /// Plan or apply target provisioning and project enrollment.
    Setup {
        #[command(subcommand)]
        command: TargetBootstrapCommand,
    },

    /// Configure and plan the target-owned ingress stack.
    Ingress {
        #[command(subcommand)]
        command: TargetIngressCommand,
    },

    /// Administer remote developer access on a target.
    Access {
        #[command(subcommand)]
        command: TargetAccessCommand,
    },
}

pub(super) async fn run(
    command: TargetCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    match command {
        TargetCommand::Local => {
            composition::ensure_local_target(invocation.home())?;
            show("local", invocation)
        }
        TargetCommand::List => list(invocation),
        TargetCommand::Show { name } => show(&name, invocation),
        TargetCommand::Nodes => super::target_nodes::run(invocation).await,
        TargetCommand::Mesh { command } => super::target_mesh::run(command, invocation).await,
        TargetCommand::Storage { command } => super::target_storage::run(command, invocation).await,
        TargetCommand::Connections { close } => super::target_connections::run(close, invocation),
        TargetCommand::Trust { name, yes } => trust(&name, yes, invocation),
        TargetCommand::Add {
            name,
            host,
            user,
            port,
            host_key_fingerprint,
            host_key,
            identity_file,
            remote_root,
            daemon_fingerprint,
        } => add(
            SshTargetArgs {
                name,
                host,
                user,
                port,
                host_key_fingerprint,
                host_key,
                identity_file,
                remote_root,
                daemon_fingerprint,
            },
            invocation,
        ),
        TargetCommand::Update {
            name,
            host,
            user,
            port,
            host_key_fingerprint,
            host_key,
            identity_file,
            remote_root,
            daemon_fingerprint,
            expected_fingerprint,
        } => update(
            SshTargetArgs {
                name,
                host,
                user,
                port,
                host_key_fingerprint,
                host_key,
                identity_file,
                remote_root,
                daemon_fingerprint,
            },
            &expected_fingerprint,
            invocation,
        ),
        TargetCommand::Remove {
            name,
            expected_fingerprint,
            yes,
        } => remove(&name, &expected_fingerprint, yes, invocation),
        TargetCommand::Select {
            target,
            environment,
        } => run_prefer_command(
            PreferCommand::SetTarget {
                target,
                environment,
            },
            &invocation.start,
            invocation.home(),
            invocation.format,
            Arc::clone(&invocation.clock),
        ),
        TargetCommand::Preference { command } => run_prefer_command(
            command,
            &invocation.start,
            invocation.home(),
            invocation.format,
            Arc::clone(&invocation.clock),
        ),
        TargetCommand::Setup { command } => bootstrap(command, invocation).await,
        TargetCommand::Ingress { command } => ingress(command, invocation),
        TargetCommand::Access { command } => access(command, invocation),
    }
}

/// Shows what the repo records for a target next to what this machine
/// trusts, and with `yes` trusts what the repo records.
fn trust(name: &str, yes: bool, invocation: &super::dispatch::Invocation) -> Result<u8, MmError> {
    let review = composition::trust_repo_target(&invocation.start, invocation.home(), name, yes)?;
    output::write_target_trust(&review, invocation.format)?;
    Ok(0)
}

struct SshTargetArgs {
    name: String,
    host: String,
    user: String,
    port: u16,
    host_key_fingerprint: String,
    host_key: Option<String>,
    identity_file: Option<PathBuf>,
    remote_root: Option<PathBuf>,
    daemon_fingerprint: Option<String>,
}

fn target_service(
    invocation: &super::dispatch::Invocation,
) -> Result<composition::TargetServices, MmError> {
    composition::target_services(
        invocation.home(),
        invocation.services.process(),
        Arc::clone(&invocation.clock),
    )
}

fn list(invocation: &super::dispatch::Invocation) -> Result<u8, MmError> {
    output::write_target_catalog(
        &target_service(invocation)?.manager.list()?,
        invocation.format,
    )?;
    Ok(0)
}

fn show(name: &str, invocation: &super::dispatch::Invocation) -> Result<u8, MmError> {
    output::write_target_details(
        &target_service(invocation)?.manager.show(name)?,
        invocation.format,
    )?;
    Ok(0)
}

fn ssh_input(arguments: SshTargetArgs) -> crate::app::SshTargetInput {
    crate::app::SshTargetInput {
        host: arguments.host,
        port: arguments.port,
        user: arguments.user,
        host_key_fingerprint: arguments.host_key_fingerprint,
        host_key: arguments.host_key,
        identity_file: arguments.identity_file,
        remote_root: arguments.remote_root,
        daemon_fingerprint: arguments.daemon_fingerprint,
    }
}

fn add(arguments: SshTargetArgs, invocation: &super::dispatch::Invocation) -> Result<u8, MmError> {
    let name = arguments.name.clone();
    output::write_target(
        &target_service(invocation)?
            .manager
            .add_ssh(&name, ssh_input(arguments))?,
        invocation.format,
    )?;
    Ok(0)
}

fn update(
    arguments: SshTargetArgs,
    expected_fingerprint: &str,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let name = arguments.name.clone();
    output::write_target_updated(
        &target_service(invocation)?.manager.update_ssh(
            &name,
            expected_fingerprint,
            ssh_input(arguments),
        )?,
        invocation.format,
    )?;
    Ok(0)
}

fn remove(
    name: &str,
    expected_fingerprint: &str,
    yes: bool,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    if !yes {
        return Err(MmError::Terminal {
            message: "target removal requires --yes with the reviewed fingerprint".to_owned(),
        });
    }
    let removed = target_service(invocation)?
        .manager
        .remove(name, expected_fingerprint)?;
    output::write_target_removed(&removed, invocation.format)?;
    Ok(0)
}

async fn bootstrap(
    command: TargetBootstrapCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    if let Some(code) = super::target_cluster::route(&command, invocation).await? {
        return Ok(code);
    }
    let services = target_service(invocation)?;
    // Planning is metadata-only: read private preferences when state exists,
    // but never create it or require the Ansible executor.
    let state = services.existing_state()?;
    let access = match &command {
        TargetBootstrapCommand::Plan { access, cluster }
        | TargetBootstrapCommand::Apply {
            access, cluster, ..
        } => access.request(cluster.release)?,
        TargetBootstrapCommand::DeployKey | TargetBootstrapCommand::Record => {
            return Err(MmError::Target {
                message: "deploy-key and record need a repo target with nodes:".to_owned(),
            });
        }
    };
    let preview = services
        .manager
        .bootstrap_plan_on_host(&invocation.selection, state.as_deref(), access.as_ref())
        .await?;
    drop(state);
    let reviewed_plan = match command {
        TargetBootstrapCommand::Plan { .. }
        | TargetBootstrapCommand::DeployKey
        | TargetBootstrapCommand::Record => {
            output::write_target_bootstrap_plan(&preview, invocation.format)?;
            return Ok(0);
        }
        TargetBootstrapCommand::Apply { plan, yes, .. } => {
            if preview.plan_hash != plan {
                return Err(MmError::State {
                    message: format!(
                        "target bootstrap plan changed (expected {plan}, current {}); review the new plan",
                        preview.plan_hash
                    ),
                });
            }
            if invocation.format == OutputFormat::Json && !yes {
                return Err(MmError::Terminal {
                    message: "JSON target bootstrap requires --yes".to_owned(),
                });
            }
            if invocation.format == OutputFormat::Human {
                output::write_target_bootstrap_plan(&preview, invocation.format)?;
            }
            if !yes
                && !confirm(&format!(
                    "Apply target bootstrap plan {}?",
                    output::terminal_safe(&plan)
                ))?
            {
                return Ok(1);
            }
            plan
        }
    };
    let result = services
        .bootstrap()?
        .apply_reviewed(&invocation.selection, &reviewed_plan, access.as_ref())
        .await?;
    output::write_target_bootstrap_result(&result, invocation.format)?;
    Ok(0)
}

fn ingress(
    command: TargetIngressCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let service = target_service(invocation)?;
    match command {
        TargetIngressCommand::Configure {
            target,
            image,
            acme_email,
            acme_ca_server,
            acme_ca_certificate,
            expected_fingerprint,
        } => {
            let acme_ca_certificate = acme_ca_certificate
                .map(|path| {
                    std::fs::read_to_string(&path).map_err(|source| MmError::Io {
                        context: format!("read ACME CA certificate {}", path.display()),
                        source,
                    })
                })
                .transpose()?;
            output::write_target_updated(
                &service.manager.configure_traefik_acme(
                    &target,
                    &expected_fingerprint,
                    image,
                    acme_email,
                    acme_ca_server,
                    acme_ca_certificate,
                )?,
                invocation.format,
            )?;
        }
        TargetIngressCommand::Plan { target } => output::write_target_ingress_plan(
            &service
                .ingress()
                .plan_for_runtime(&target, invocation.runtime_kind)?,
            invocation.format,
        )?,
    }
    Ok(0)
}

fn access(
    command: TargetAccessCommand,
    _invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    match command {
        TargetAccessCommand::Status => {
            Err(MmError::Security {
                message: "remote developer-access administration is target-root policy; use authenticated `mmd access` on the selected target"
                    .to_owned(),
            })
        }
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum TargetAccessCommand {
    /// Explain the privileged remote-access administration boundary.
    Status,
}

#[derive(Debug, Clone, Subcommand)]
pub enum TargetBootstrapCommand {
    /// Produce a deterministic target setup plan. For a cluster target
    /// (`nodes:`), the plan also scans each node's SSH host key.
    Plan {
        #[command(flatten)]
        access: BootstrapAccessArgs,
        #[command(flatten)]
        cluster: super::target_cluster::ClusterSetupArgs,
    },
    /// Execute an exact plan hash printed by `mm target setup plan`.
    Apply {
        /// Hash printed by `mm target setup plan`.
        #[arg(long, value_name = "HASH")]
        plan: String,
        /// Skip the bootstrap confirmation prompt.
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        access: BootstrapAccessArgs,
        #[command(flatten)]
        cluster: super::target_cluster::ClusterSetupArgs,
    },
    /// Give the environment its cluster target's read-only git deploy key,
    /// stored in the secret `setup.deploy-key.secret` names, and print its
    /// public half. A target has one key: it is copied from another
    /// environment of the target that holds it, and created only when none
    /// does. Prints the existing key when there is one.
    DeployKey,
    /// Write each node's SSH host key, daemon identity, and private address
    /// from the last cluster setup into `.config/mm/targets.yaml`.
    Record,
}

/// Install the remote access layer: `mm` and `mmd`, the locked `mm` relay
/// account, the sshd drop-in, the host policy, a deploy key, and `mmd`.
/// Pass the same flags to `plan` and `apply`.
#[derive(Debug, Clone, Default, clap::Args)]
#[command(next_help_heading = "Remote access (pass the same flags to plan and apply)")]
pub struct BootstrapAccessArgs {
    /// Git URL the host fetches the project from with its own read-only
    /// deploy key. Turns on the remote access layer.
    #[arg(long, value_name = "URL", requires = "break_glass")]
    origin: Option<String>,
    /// `push`: the host fetches nothing and mm sends it each commit's
    /// files, so it needs no origin and no deploy key. Turns on the remote
    /// access layer, like --origin, and takes its place.
    #[arg(
        long,
        value_name = "MODE",
        value_parser = ["push"],
        conflicts_with = "origin",
        requires = "break_glass"
    )]
    delivery: Option<String>,
    /// The git server's SSH host key, `<type> <base64>`, for SSH origins.
    #[arg(long, value_name = "KEY", requires = "origin")]
    origin_host_key: Option<String>,
    /// A recovery key for the host policy, NAME='ssh-ed25519 AAAA… comment'.
    /// Repeat for more. Keep the private half offline.
    #[arg(long = "break-glass", value_name = "NAME=KEY")]
    break_glass: Vec<String>,
    /// Directory with `mm` and `mmd` built for the host, such as an
    /// extracted Linux release. Default: for each host's CPU, the directory
    /// of this `mm` when it holds them, otherwise this mm's own release,
    /// downloaded, checked, and kept under the mm home.
    #[arg(long, value_name = "DIR")]
    host_binaries: Option<std::path::PathBuf>,
}

impl BootstrapAccessArgs {
    pub(super) fn host_binaries(&self) -> Option<std::path::PathBuf> {
        self.host_binaries.clone()
    }

    fn request(
        &self,
        release: super::target_cluster::ReleaseArgs,
    ) -> Result<Option<crate::app::BootstrapAccessRequest>, MmError> {
        let origin = match (&self.origin, &self.delivery) {
            (Some(origin), _) => origin.clone(),
            // No origin: the host is enrolled to take pushed files.
            (None, Some(_)) => String::new(),
            (None, None) if self.break_glass.is_empty() => return Ok(None),
            (None, None) => {
                return Err(MmError::Target {
                    message: "--break-glass sets up remote access, which needs --origin URL (the host fetches the project) or --delivery push (mm sends the files)".to_owned(),
                });
            }
        };
        let break_glass = self
            .break_glass
            .iter()
            .map(|value| crate::domain::host_policy::parse_break_glass(value))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|message| MmError::Target { message })?;
        Ok(Some(crate::app::BootstrapAccessRequest {
            origin,
            origin_host_key: self.origin_host_key.clone(),
            break_glass,
            binaries: self.host_binaries.clone(),
            upgrade: release.upgrade,
            downgrade: release.downgrade,
        }))
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum TargetIngressCommand {
    /// Configure digest-pinned Traefik and optional ACME metadata.
    Configure {
        #[arg(value_name = "TARGET")]
        target: String,
        /// Digest-pinned Traefik image reference.
        #[arg(long)]
        image: String,
        /// Optional ACME account email.
        #[arg(long)]
        acme_email: Option<String>,
        /// ACME directory URL, such as Let's Encrypt staging or an internal ACME server.
        #[arg(long, value_name = "URL")]
        acme_ca_server: Option<String>,
        /// PEM file of the CA that signs the ACME server's HTTPS certificate.
        #[arg(long, value_name = "PATH")]
        acme_ca_certificate: Option<PathBuf>,
        /// Fingerprint from the reviewed target list.
        #[arg(long)]
        expected_fingerprint: String,
    },

    /// Publish a deterministic target-owned ingress artifact and plan.
    Plan {
        #[arg(value_name = "TARGET")]
        target: String,
    },
}

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
            output::write_json(&serde_json::json!({
                "project_id": project_id.0.to_string(),
                "project": project.as_ref().map(|pref| {
                    serde_json::json!({
                        "runtime": pref.runtime,
                        "target": pref.target,
                    })
                }),
            }))?;
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
