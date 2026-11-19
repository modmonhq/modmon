use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "mmd", version, about = "Privileged mm target daemon")]
struct Arguments {
    /// Root-owned strict JSON daemon configuration.
    #[arg(long, default_value = "/etc/mm/mmd.json")]
    config: PathBuf,
    /// Print the persistent daemon identity fingerprint and exit.
    #[arg(long)]
    print_identity: bool,
    /// Inspect host access, apply an emergency block, or import secrets.
    #[command(subcommand)]
    command: Option<DaemonCommand>,
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// Host access: users come from each project's users file; the host
    /// policy sets limits and break-glass keys.
    Access {
        #[command(subcommand)]
        command: AccessCommand,
    },
    /// Secrets and the git deploy key pushed from the operator's machine,
    /// kept in the project's age store under the daemon's mm home, and this
    /// host's own connections to an external OpenBao or Vault.
    Secrets {
        #[command(subcommand)]
        command: SecretsCommand,
    },
    /// The member of an environment's managed secret store on this host.
    /// `mm target setup` runs these as root; nothing they print holds a key
    /// or a value.
    Openbao {
        #[command(subcommand)]
        command: OpenbaoCommand,
    },
    /// The WireGuard interface mm0 that connects this host to the other
    /// nodes of its cluster. `mm target setup` and `mm target mesh rotate`
    /// run these as root; the private key never leaves this host and no
    /// command prints it.
    Mesh {
        #[command(subcommand)]
        command: MeshCommand,
    },
    /// The networks that span the nodes of this host's cluster: the routes
    /// to the other nodes' addresses, mm's firewall chains MM-CLUSTER and
    /// MM-CLUSTER-NAT, and each network's bridge.
    Network {
        #[command(subcommand)]
        command: NetworkCommand,
    },
    /// A container of this host's cluster node.
    Container {
        #[command(subcommand)]
        command: ContainerCommand,
    },
    /// Shared storage between the nodes of this host's cluster: the
    /// firewall chains MM-STORAGE and MM-STORAGE-IN, which keep containers
    /// away from the storage node's port.
    Storage {
        #[command(subcommand)]
        command: StorageCommand,
    },
    /// Back up and restore this host's own records: the host policy, the
    /// node's name, each environment's access, blocks, and deployment
    /// history, the cluster network addresses, and the audit log. Never a
    /// key, a secret value, or a passphrase.
    State {
        #[command(subcommand)]
        command: StateCommand,
    },
}

#[derive(Debug, Subcommand)]
enum StateCommand {
    /// Write the records as a tar archive to standard output, for example
    /// `mmd state export > node.tar`, and what it holds to standard error.
    Export,
    /// Read an archive from standard input and put its records in place,
    /// for example `mmd state import < node.tar`, with mmd stopped. Refuses
    /// another node's archive, and one with anything an export never holds.
    Import,
}

#[derive(Debug, Subcommand)]
enum ContainerCommand {
    /// Take a container out of service without dropping a request: every
    /// node stops answering its name and sending it requests through the
    /// ingress, it serves what it still has for its drain time, then it
    /// gets its stop signal and `stop_grace_period` and is removed. The
    /// running mmd does this and goes on after a restart; this command
    /// asks for it, prints the leave as JSON, and returns.
    Leave {
        /// The container's name or id.
        container: String,
        /// How long it keeps serving once no node sends it new requests,
        /// such as `30s`. The default is its service's `drain`, else `1m`.
        #[arg(long, value_name = "DURATION")]
        drain: Option<String>,
        /// Restart the container once drained, instead of removing it: it
        /// is answered again when it is healthy.
        #[arg(long)]
        restart: bool,
    },
}

#[derive(Debug, Subcommand)]
enum StorageCommand {
    /// Load the firewall rules that keep shared storage between the hosts:
    /// what this host's containers send to the storage port through the
    /// mesh is dropped, and storage traffic for this host's mesh address
    /// is taken from the other nodes' mesh addresses only. `mm target
    /// setup` runs this, and `mm-mesh.service` at boot.
    Up,
    /// Remove those rules: for a host that is no longer a node of a
    /// cluster. Shared volumes and their files stay where they are.
    Leave,
}

#[derive(Debug, Subcommand)]
enum NetworkCommand {
    /// Print the addresses the last deployment stored and what on this
    /// host differs from them, as JSON. Changes nothing.
    Show,
    /// Bring this host to the stored state. `mm-network.service` runs this
    /// after the container engine starts and whenever mmd applies a
    /// deployment; `mm-mesh.service` runs it with `--before-engine` at boot.
    Up {
        /// Load only the firewall chains, which have to be in place before
        /// the engine starts a container.
        #[arg(long)]
        before_engine: bool,
    },
    /// Remove mm's firewall chains, the rules that enter them, and the
    /// stored addresses: for a host that is no longer a node of a cluster.
    Leave,
}

#[derive(Debug, Subcommand)]
enum MeshCommand {
    /// Print what this host holds, as JSON: its public key, the interface
    /// state last applied, and whether an earlier mm version's wg0 is
    /// still there. Reads files only.
    Show,
    /// Create this host's key pair when it has none and print
    /// `{"public-key": "…"}`. A host an earlier mm version set up keeps the
    /// key of its wg0.
    Key,
    /// Read the interface state (JSON) from stdin, bring mm0 to it, and
    /// store it. Only what differs changes: applying the same state again
    /// does nothing, and a peer that does not change keeps its session.
    Apply,
    /// Bring mm0 to the stored state. `mm-mesh.service` runs this at boot.
    Up,
    /// Print the interface and, per peer, its endpoint, the age of its last
    /// handshake, and its traffic, as JSON.
    Status,
    /// Replace this host's key pair in two steps, so the other nodes can
    /// learn the new public key in between.
    Rotate {
        /// Create the new key pair next to the one in use and print its
        /// public key. Repeating it prints the same pending key.
        #[arg(
            long,
            conflicts_with = "activate",
            required_unless_present = "activate"
        )]
        stage: bool,
        /// Switch mm0 to the pending key pair.
        #[arg(long)]
        activate: bool,
    },
    /// Remove mm0, the stored state, and this host's key pair: for a host
    /// that is no longer a node of the cluster.
    Leave,
}

#[derive(Debug, Subcommand)]
enum SecretsCommand {
    /// Create the project's host age store when missing and print the age
    /// recipient to seal bundles to, as `{"recipient": "age1…"}`.
    Recipient {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
    },
    /// Read a sealed secret bundle from stdin, store its secrets with their
    /// revisions, and install its deploy key. The environment must be
    /// enrolled. Prints what changed, never values.
    Import {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
    },
    /// Let this host read an external OpenBao or Vault for one environment
    /// it runs. Reads the AppRole role ID and secret ID from stdin, one per
    /// line, logs in with them, and stores them encrypted for root only.
    /// Run it again to replace the credential. Prints the connection,
    /// never the credential.
    Connect {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
        /// The provider's name under `providers:` in the project.
        #[arg(long, value_name = "NAME")]
        provider: String,
        #[arg(long, value_enum, default_value_t = ProviderDriver::Openbao)]
        driver: ProviderDriver,
        /// Where this host reaches the store, `https://HOST:PORT`.
        #[arg(long, value_name = "URL")]
        address: String,
        /// The PEM certificate authority that signed the store's certificate.
        #[arg(long, value_name = "FILE")]
        ca_file: PathBuf,
        #[arg(long, value_name = "NAME")]
        namespace: Option<String>,
        /// The KV version 2 mount, as `storage-mount` in the project.
        #[arg(long, value_name = "MOUNT", default_value = "mm-kv")]
        storage_mount: String,
        /// The AppRole mount, as `automation-auth-mount` in the project.
        #[arg(long, value_name = "MOUNT", default_value = "mm-approle")]
        auth_mount: String,
    },
    /// Forget this host's connection to a provider for a project, with
    /// every environment's credential.
    Disconnect {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        provider: String,
        #[arg(long, value_enum, default_value_t = ProviderDriver::Openbao)]
        driver: ProviderDriver,
    },
}

#[derive(Debug, Subcommand)]
enum OpenbaoCommand {
    /// Write the member described as JSON on stdin, then start it, or
    /// restart it when it changed and unseal it again. Prints a certificate
    /// request when the member holds no certificate of the environment's
    /// authority yet.
    Member {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
    },
    /// Initialise the store on this member: unseal it, mount the KV store,
    /// make mmd's logins and the audit device, revoke the root token, and
    /// print the recovery document sealed to each age recipient on stdin.
    Init {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
    },
    /// Keep the recovery document sealed to this host's store on stdin, and
    /// unseal this host's member with it.
    Join {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
    },
    /// Print the newest records of the member's audit device, oldest
    /// first, from its audit.log and the rotated files `.1` to `.4` (mmd
    /// rotates it at 16 MiB, as its own audit log). Values are hashed.
    Audit {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// What this host's member says: its release, its seal, the Raft
    /// members and the leader.
    Status {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
    },
    /// Remove a member whose data is gone from the Raft configuration,
    /// through this host's member, before it joins again. Never this host's
    /// own member, nor the one that leads.
    RemovePeer {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
        /// The node whose old entry goes.
        #[arg(long, value_name = "NODE")]
        peer: String,
    },
    /// Print every secret of the environment the store keeps, with its
    /// versions and when each was written; never a value.
    Versions {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum ProviderDriver {
    Openbao,
    Vault,
}

impl From<ProviderDriver> for mm::adapters::secrets::HostProviderDriver {
    fn from(driver: ProviderDriver) -> Self {
        match driver {
            ProviderDriver::Openbao => Self::OpenBao,
            ProviderDriver::Vault => Self::Vault,
        }
    }
}

#[derive(Debug, Subcommand)]
enum AccessCommand {
    /// Validate a host policy file (default: the configured one) without using it.
    Validate { path: Option<PathBuf> },
    /// Show break-glass keys, enrolled projects, and each environment's
    /// reconciled users, blocks, and deployed commit.
    Show,
    /// Block a key in one environment at once, before the repo removes it.
    /// The block lifts when a reconcile no longer lists the key.
    Block {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
        /// `SHA256:` key fingerprint, as `mm key show` prints it.
        fingerprint: String,
    },
    /// Lift an emergency block.
    Unblock {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
        fingerprint: String,
    },
    /// Rewrite sshd's authorized_keys from the current policy, for example
    /// after editing the host policy.
    RenderKeys,
    /// Enroll a project in the host policy, creating the policy on a new
    /// host, and create the project's read-only git deploy key. Re-running
    /// adds environments and break-glass keys and keeps every other setting.
    /// `mm target setup` runs this.
    Enroll {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        /// The project's name from its .config/mm.yaml.
        #[arg(long, value_name = "NAME")]
        name: String,
        /// Git URL to fetch the project from.
        #[arg(long, value_name = "URL", required_unless_present = "delivery")]
        origin: Option<String>,
        /// `push`: clients send each commit's files, and this host never
        /// contacts the git server. Takes no origin and creates no deploy
        /// key.
        #[arg(long, value_name = "MODE", value_parser = ["push"], conflicts_with_all = ["origin", "origin_host_key"])]
        delivery: Option<String>,
        /// The git server's SSH host key, `<type> <base64>`; required for
        /// SSH origins.
        #[arg(long, value_name = "KEY")]
        origin_host_key: Option<String>,
        /// An environment this host may run; repeat for more.
        #[arg(long = "environment", value_name = "NAME", required = true)]
        environments: Vec<String>,
        /// A recovery key, `NAME='ssh-ed25519 AAAA… comment'`; repeat for more.
        #[arg(long = "break-glass", value_name = "NAME=KEY")]
        break_glass: Vec<String>,
    },
    /// Reconcile one environment's users file from a commit, as root on
    /// this host and without the relay: the first sync of a new host.
    /// Checks the commit like `mm team sync` and prints a JSON summary. Only
    /// moves access forward; re-running at the recorded commit changes
    /// nothing.
    Sync {
        #[arg(long, value_name = "UUID")]
        project: uuid::Uuid,
        #[arg(long, value_name = "NAME")]
        environment: String,
        /// Full 40-character commit SHA on the project's origin.
        #[arg(long, value_name = "SHA")]
        commit: String,
        /// Print what would change and write no access state.
        #[arg(long)]
        dry_run: bool,
        /// On a host the project is pushed to: read the commit's bundle
        /// from standard input first, as `mm target setup` sends it. The
        /// bundle is checked to be the commit before anything is kept.
        #[arg(long)]
        bundle_stdin: bool,
    },
    /// Print the newest audit records, oldest first, from
    /// /var/lib/mm/audit.jsonl and its rotated files (`audit.jsonl.1` to
    /// `.4`: the log is rotated at 16 MiB).
    Audit {
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

/// The threads that serve requests carry this name, so a look at a host
/// (`ps -L`) tells them from the ones that answer names (`mm-names`), watch
/// over child processes (`mm-children`), and report to systemd
/// (`mm-watchdog`).
const REQUEST_THREADS: &str = "mm-requests";

fn main() -> ExitCode {
    // The sockets systemd kept open while the previous mmd restarted,
    // taken before any other thread reads the environment.
    let sockets = mm::adapters::kept_sockets::KeptSockets::from_environment();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name(REQUEST_THREADS)
        .build();
    match runtime {
        Ok(runtime) => runtime.block_on(serve(sockets)),
        Err(error) => {
            eprintln!("error[MM_IO]: starting the request runtime failed: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(sockets: mm::adapters::kept_sockets::KeptSockets) -> ExitCode {
    // Warnings and errors reach the journal through stderr; RUST_LOG raises
    // the level. ANSI colours would only clutter the journal.
    // `MM_TIMING=1` in the unit's environment adds the timing trace.
    if !mm::composition::install_timing_trace(false, false) {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
        if let Err(error) = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_ansi(false)
            .with_writer(std::io::stderr)
            .try_init()
        {
            eprintln!("warning[MM_TELEMETRY]: tracing initialization failed: {error}");
        }
    }
    match run(Arguments::parse(), sockets).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error[{}]: {error}", error.code());
            ExitCode::from(error.exit_code())
        }
    }
}

async fn run(
    arguments: Arguments,
    sockets: mm::adapters::kept_sockets::KeptSockets,
) -> Result<(), mm::MmError> {
    mm::composition::serve_as_host();
    let config = mm::adapters::remote::DaemonConfig::load(&arguments.config)?;
    match arguments.command {
        Some(DaemonCommand::Access { command }) => return run_access(&config, command).await,
        Some(DaemonCommand::Secrets { command }) => return run_secrets(&config, command).await,
        Some(DaemonCommand::Openbao { command }) => return run_openbao(&config, command).await,
        Some(DaemonCommand::Mesh { command }) => return run_mesh(&config, command).await,
        Some(DaemonCommand::Network { command }) => return run_network(&config, command).await,
        Some(DaemonCommand::Storage { command }) => return run_storage(&config, command).await,
        Some(DaemonCommand::Container {
            command:
                ContainerCommand::Leave {
                    container,
                    drain,
                    restart,
                },
        }) => {
            let drain = drain
                .map(|drain| {
                    mm::domain::leaving::drain_of(Some(&mm::domain::config::DurationSpec(drain)))
                        .map_err(|problem| mm::MmError::Environment {
                            message: format!("--drain {problem}"),
                        })
                })
                .transpose()?;
            let leave = mm::composition::request_leave(&config, &container, drain, restart).await;
            return print_json(&leave);
        }
        Some(DaemonCommand::State { command }) => return run_state(&config, &command),
        None => {}
    }
    mm::composition::ensure_local_target(Some(&config.mm_home))?;
    if arguments.print_identity {
        println!("{}", daemon(config, None, None)?.identity().fingerprint);
        return Ok(());
    }
    // The node's name resolver runs next to the daemon for as long as the
    // process lives; on a host that holds no cluster network it is idle.
    let names = mm::composition::name_resolver(&config, sockets.clone());
    // Next to it: the keeper that starts containers which could not start
    // while the cluster's storage node was away.
    let keeper = mm::composition::storage_keeper(&config);
    let engine = config.host_runtime();
    // And the keepers of containers that leave and of the ingress proxy.
    // What they kept on disk holds before anyone is answered: a leaving
    // container stays out, and no leave is confirmed until the proxy runs
    // a file of this process.
    let leaving = mm::composition::leave_keeper(&config, names.clone());
    let ingress = mm::composition::ingress_keeper(&config, names.clone());
    leaving.restore()?;
    ingress.restore()?;
    // The sampler that keeps four hours of each container's CPU and
    // memory in memory. It runs on a thread of its own. The autoscaler
    // decides from it and runs beside the daemon's requests.
    let sampler = mm::composition::container_metrics(names.clone(), engine);
    sampler.start();
    let autoscale =
        mm::composition::autoscaler(&config, names.clone(), sampler.metrics(), leaving.clone());
    let node_host = (
        leaving.clone() as Arc<dyn mm::app::ports::ContainerLeaving>,
        ingress.clone() as Arc<dyn mm::app::ports::IngressHost>,
        autoscale.clone() as Arc<dyn mm::app::ports::ScaleState>,
    );
    // And the keeper of the job runs nobody waits for, as this node.
    let jobs = mm::composition::JobKeeper::new(
        config.clone(),
        Arc::new(use_cases(
            &config,
            Some(names.clone()),
            Some(node_host.clone()),
        )),
    );
    let healing = mm::composition::healer(&config, names.clone());
    let config_for_unsealer = config.clone();
    let daemon = daemon(config, Some(names.clone()), Some(node_host))?
        .with_autoscale(autoscale)
        .with_healing(healing);
    // Both on threads of their own: a request runtime that is busy or
    // stuck does not delay a container's lookup, and the watchdog asks
    // the resolver's thread too before it reports the daemon alive.
    let resolving = mm::adapters::own_runtime::OwnRuntime::start("mm-names")?;
    resolving.spawn(names.run());
    mm::adapters::own_runtime::OwnRuntime::start("mm-storage")?.spawn(keeper.run());
    let keepers = mm::adapters::own_runtime::OwnRuntime::start("mm-ingress")?;
    keepers.spawn(leaving.run());
    keepers.spawn(ingress.run());
    let keeping = mm::adapters::own_runtime::OwnRuntime::start("mm-jobs")?;
    keeping.spawn(jobs.run());
    mm::adapters::own_runtime::OwnRuntime::start("mm-openbao")?
        .spawn(mm::composition::store_unsealer(&config_for_unsealer)?.run());
    let liveness = mm::adapters::remote::Liveness::from_environment()
        .keeping(sockets)
        .watching("the name resolver", move |patience| {
            resolving.answers(patience)
        })
        .watching(
            "the watch over child processes",
            mm::adapters::process::supervisor_answers,
        )
        .watching("the job keeper", move |patience| keeping.answers(patience));
    daemon.run_reporting(liveness).await
}

/// The keepers a daemon's rollouts use on a cluster node.
type NodeHost = (
    Arc<dyn mm::app::ports::ContainerLeaving>,
    Arc<dyn mm::app::ports::IngressHost>,
    Arc<dyn mm::app::ports::ScaleState>,
);

fn daemon(
    config: mm::adapters::remote::DaemonConfig,
    names: Option<Arc<dyn mm::app::ports::ServiceNames>>,
    node_host: Option<NodeHost>,
) -> Result<mm::adapters::remote::RemoteDaemon, mm::MmError> {
    let use_cases: Arc<dyn mm::app::ports::RemoteUseCases> =
        Arc::new(use_cases(&config, names, node_host));
    mm::adapters::remote::RemoteDaemon::new(config, use_cases)
}

fn use_cases(
    config: &mm::adapters::remote::DaemonConfig,
    names: Option<Arc<dyn mm::app::ports::ServiceNames>>,
    node_host: Option<NodeHost>,
) -> mm::composition::DaemonUseCaseAdapter {
    let mut use_cases = mm::composition::DaemonUseCaseAdapter::new(config.mm_home.clone())
        .with_mesh(mm::composition::mesh_host(config))
        .with_networks(mm::composition::cluster_networks(config))
        .with_storage(mm::composition::shared_storage(config))
        .with_host_policy(config.host_policy_path.clone());
    if let Some(names) = names {
        use_cases = use_cases.with_names(names);
    }
    if let Some((leaving, ingress, scale)) = node_host {
        use_cases = use_cases
            .with_node_host(leaving, ingress)
            .with_scale_state(scale);
    }
    use_cases
}

async fn run_access(
    config: &mm::adapters::remote::DaemonConfig,
    command: AccessCommand,
) -> Result<(), mm::MmError> {
    use mm::adapters::remote::HostAccessStore;

    let store = HostAccessStore::new(config);
    match command {
        AccessCommand::Validate { path } => {
            let path = path.unwrap_or_else(|| config.host_policy_path.clone());
            let (host, _) = mm::adapters::remote::load_host_policy(&path)?;
            println!(
                "{} is valid: {} break-glass key(s), {} enrolled project(s)",
                path.display(),
                host.break_glass.len(),
                host.projects.len()
            );
            Ok(())
        }
        AccessCommand::Show => print_json(&show(&store)),
        AccessCommand::Block {
            project,
            environment,
            fingerprint,
        } => change_block(&store, project, &environment, &fingerprint, true).await,
        AccessCommand::Unblock {
            project,
            environment,
            fingerprint,
        } => change_block(&store, project, &environment, &fingerprint, false).await,
        AccessCommand::RenderKeys => {
            store.render_current_keys().await?;
            println!("rewrote {}", config.authorized_keys_path.display());
            Ok(())
        }
        AccessCommand::Audit { limit } => print_audit(&config.state_root, limit),
        AccessCommand::Sync {
            project,
            environment,
            commit,
            dry_run,
            bundle_stdin,
        } => {
            let request = mm::adapters::remote::RootAccessSyncRequest {
                project_id: mm::domain::workspace::ProjectId(project),
                environment,
                commit,
                dry_run,
            };
            let daemon = daemon(config.clone(), None, None)?;
            if bundle_stdin {
                let delivered = daemon.deliver_as_root(
                    request.project_id,
                    &request.environment,
                    &mut std::io::stdin().lock(),
                )?;
                if delivered.commit != request.commit {
                    return Err(mm::MmError::Environment {
                        message: format!(
                            "the bundle on standard input is commit {}, not {}",
                            delivered.commit, request.commit
                        ),
                    });
                }
            }
            let summary = daemon.sync_access_as_root(&request).await?;
            print_json(&Ok::<_, mm::MmError>(summary))
        }
        AccessCommand::Enroll {
            project,
            name,
            origin,
            origin_host_key,
            delivery,
            environments,
            break_glass,
        } => {
            let pushed = delivery.is_some();
            let enrollment = mm::domain::host_policy::ProjectEnrollment {
                project_id: mm::domain::workspace::ProjectId(project),
                name,
                origin: origin.unwrap_or_default(),
                origin_host_key,
                delivery: if pushed {
                    mm::domain::delivery::Delivery::Push
                } else {
                    mm::domain::delivery::Delivery::Fetch
                },
                environments: environments.into_iter().collect(),
                break_glass: break_glass
                    .iter()
                    .map(|value| mm::domain::host_policy::parse_break_glass(value))
                    .collect::<Result<_, _>>()
                    .map_err(|message| mm::MmError::Security { message })?,
            };
            let changed = store.enroll(&enrollment).await?;
            // A host that fetches nothing has no key to fetch with, and
            // keeps none that fetching left.
            let (deploy_key, removed) = if pushed {
                (
                    String::new(),
                    store.remove_fetch_material(enrollment.project_id)?,
                )
            } else {
                (store.ensure_deploy_key(enrollment.project_id)?, Vec::new())
            };
            store.render_current_keys().await?;
            print_json(&Ok::<_, mm::MmError>(serde_json::json!({
                "changed": changed,
                "host-policy": config.host_policy_path,
                "project": project,
                "deploy-key": deploy_key,
                "removed": removed,
            })))
        }
    }
}

async fn run_mesh(
    config: &mm::adapters::remote::DaemonConfig,
    command: MeshCommand,
) -> Result<(), mm::MmError> {
    let mesh = mm::composition::mesh_host(config);
    let key = |public_key: String| {
        print_json(&Ok::<_, mm::MmError>(
            serde_json::json!({ "public-key": public_key }),
        ))
    };
    match command {
        MeshCommand::Show => print_json(&Ok::<_, mm::MmError>(mesh.facts().await?)),
        // With the key it joins the mesh with, a node says what addresses
        // and routes it has: setup stops before the mesh when one of them
        // is in a range the cluster is about to use.
        MeshCommand::Key => print_json(&Ok::<_, mm::MmError>(serde_json::json!({
            "public-key": mesh.ensure_key().await?,
            "host-ranges": mesh.host_ranges().await?,
        }))),
        MeshCommand::Apply => {
            use std::io::Read;

            const LIMIT: u64 = 1024 * 1024;
            let mut text = String::new();
            std::io::stdin()
                .lock()
                .take(LIMIT + 1)
                .read_to_string(&mut text)
                .map_err(|source| mm::MmError::Io {
                    context: "reading the mesh state from stdin".to_owned(),
                    source,
                })?;
            if text.len() as u64 > LIMIT {
                return Err(mm::MmError::Security {
                    message: "the mesh state on stdin exceeds 1 MiB".to_owned(),
                });
            }
            let desired: mm::domain::mesh::MeshState =
                serde_json::from_str(&text).map_err(|error| mm::MmError::Security {
                    message: format!("stdin does not hold a mesh state: {error}"),
                })?;
            print_json(&Ok::<_, mm::MmError>(mesh.apply(&desired).await?))
        }
        MeshCommand::Up => {
            let applied = mesh.up().await?;
            // One line per change in the journal; nothing when in place.
            for action in &applied.actions {
                println!("mesh: {action}");
            }
            Ok(())
        }
        MeshCommand::Status => {
            let mut status = mesh.status().await?;
            status.node = config.node_env()?.map(|node| node.name);
            print_json(&Ok::<_, mm::MmError>(status))
        }
        MeshCommand::Rotate { stage: true, .. } => key(mesh.stage_key().await?),
        MeshCommand::Rotate { .. } => key(mesh.activate_key().await?),
        MeshCommand::Leave => print_json(&Ok::<_, mm::MmError>(mesh.leave().await?)),
    }
}

async fn run_storage(
    config: &mm::adapters::remote::DaemonConfig,
    command: StorageCommand,
) -> Result<(), mm::MmError> {
    let storage = mm::composition::shared_storage_host(config);
    match command {
        StorageCommand::Up => {
            // One line per change in the journal; nothing when in place.
            for action in &storage.up().await?.actions {
                println!("storage: {action}");
            }
            Ok(())
        }
        StorageCommand::Leave => print_json(&Ok::<_, mm::MmError>(storage.leave().await?)),
    }
}

async fn run_network(
    config: &mm::adapters::remote::DaemonConfig,
    command: NetworkCommand,
) -> Result<(), mm::MmError> {
    let networks = mm::composition::cluster_networks(config);
    match command {
        // The kernel and the engine are both compared with what is stored,
        // as in the answer to a client's request.
        NetworkCommand::Show => print_json(&Ok::<_, mm::MmError>(
            mm::composition::node_network_status(config).await?,
        )),
        NetworkCommand::Up { before_engine } => {
            let applied = networks.up(before_engine).await?;
            // One line per change in the journal; nothing when in place.
            for action in &applied.actions {
                println!("network: {action}");
            }
            Ok(())
        }
        NetworkCommand::Leave => print_json(&Ok::<_, mm::MmError>(networks.leave().await?)),
    }
}

fn run_state(
    config: &mm::adapters::remote::DaemonConfig,
    command: &StateCommand,
) -> Result<(), mm::MmError> {
    use std::io::IsTerminal;

    match command {
        StateCommand::Export => {
            let stdout = std::io::stdout();
            if stdout.is_terminal() {
                return Err(mm::MmError::State {
                    message:
                        "the archive is binary: send it to a file, `mmd state export > node.tar`"
                            .to_owned(),
                });
            }
            let manifest =
                mm::adapters::remote::export_state(config, stdout.lock(), now_unix_ms())?;
            eprintln!(
                "exported {} file(s) of {}: {}",
                manifest.files.len(),
                manifest.node.as_deref().unwrap_or("this host"),
                manifest.files.join(", ")
            );
            Ok(())
        }
        StateCommand::Import => print_json(&mm::adapters::remote::import_state(
            config,
            std::io::stdin().lock(),
        )),
    }
}

/// The AppRole credential and the certificate authority are small; more
/// than this is not one.
const MAX_CONNECT_INPUT_BYTES: u64 = 64 * 1024;
const MAX_STORE_INPUT_BYTES: u64 = 256 * 1024;

fn provider_name(
    provider: String,
) -> Result<mm::domain::secret_backend::ProviderInstanceName, mm::MmError> {
    mm::domain::secret_backend::ProviderInstanceName::new(provider)
        .map_err(|message| mm::MmError::Secret { message })
}

async fn run_secrets(
    config: &mm::adapters::remote::DaemonConfig,
    command: SecretsCommand,
) -> Result<(), mm::MmError> {
    match command {
        SecretsCommand::Connect {
            project,
            environment,
            provider,
            driver,
            address,
            ca_file,
            namespace,
            storage_mount,
            auth_mount,
        } => {
            use std::io::Read;

            let ca_pem = std::fs::read_to_string(&ca_file).map_err(|source| mm::MmError::Io {
                context: format!("reading the certificate authority {}", ca_file.display()),
                source,
            })?;
            let mut input = zeroize::Zeroizing::new(String::new());
            std::io::stdin()
                .lock()
                .take(MAX_CONNECT_INPUT_BYTES)
                .read_to_string(&mut input)
                .map_err(|source| mm::MmError::Io {
                    context: "reading the AppRole role ID and secret ID from stdin".to_owned(),
                    source,
                })?;
            let (role_id, secret_id) = mm::adapters::secrets::approle_from_lines(&input)?;
            let connection = mm::composition::connect_host_provider(
                config,
                driver.into(),
                mm::adapters::secrets::HostProviderConnect {
                    project: mm::domain::workspace::ProjectId(project),
                    environment,
                    provider: provider_name(provider)?,
                    address,
                    namespace,
                    storage_mount,
                    auth_mount,
                    ca_pem,
                    role_id,
                    secret_id,
                },
            )
            .await?;
            print_json(&Ok::<_, mm::MmError>(connection))
        }
        SecretsCommand::Disconnect {
            project,
            provider,
            driver,
        } => {
            let removed = mm::composition::disconnect_host_provider(
                config,
                driver.into(),
                &provider_name(provider)?,
                mm::domain::workspace::ProjectId(project),
            )?;
            print_json(&Ok::<_, mm::MmError>(
                serde_json::json!({ "removed": removed }),
            ))
        }
        SecretsCommand::Recipient {
            project,
            environment,
        } => {
            let recipient = mm::composition::host_secret_recipient(
                config,
                mm::domain::workspace::ProjectId(project),
                &environment,
            )?;
            print_json(&Ok::<_, mm::MmError>(
                serde_json::json!({ "recipient": recipient }),
            ))
        }
        SecretsCommand::Import {
            project,
            environment,
        } => {
            use std::io::Read;

            let limit = mm::adapters::secrets::MAX_SEALED_SECRET_BUNDLE_BYTES;
            let mut sealed = Vec::new();
            std::io::stdin()
                .lock()
                .take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1))
                .read_to_end(&mut sealed)
                .map_err(|source| mm::MmError::Io {
                    context: "reading the sealed secret bundle from stdin".to_owned(),
                    source,
                })?;
            if sealed.len() > limit {
                return Err(mm::MmError::Secret {
                    message: format!("sealed secret bundle exceeds the {limit}-byte limit"),
                });
            }
            let summary = mm::composition::import_host_secrets(
                config,
                mm::domain::workspace::ProjectId(project),
                &environment,
                &sealed,
                now_unix_ms(),
            )?;
            print_json(&Ok::<_, mm::MmError>(summary))
        }
    }
}

async fn run_openbao(
    config: &mm::adapters::remote::DaemonConfig,
    command: OpenbaoCommand,
) -> Result<(), mm::MmError> {
    use std::io::Read;

    let read_stdin = || {
        let mut input = zeroize::Zeroizing::new(String::new());
        std::io::stdin()
            .lock()
            .take(MAX_STORE_INPUT_BYTES)
            .read_to_string(&mut input)
            .map(|_| input)
            .map_err(|source| mm::MmError::Io {
                context: "reading the secret store's input from stdin".to_owned(),
                source,
            })
    };
    let scope = |project: uuid::Uuid, environment: &str| {
        (
            mm::domain::workspace::ProjectId(project),
            environment.to_owned(),
        )
    };
    match command {
        OpenbaoCommand::Member {
            project,
            environment,
        } => {
            let (project, environment) = scope(project, &environment);
            let answer =
                mm::composition::store_member(config, (project, &environment), &read_stdin()?)
                    .await;
            print_json(&answer)
        }
        OpenbaoCommand::Init {
            project,
            environment,
        } => {
            let (project, environment) = scope(project, &environment);
            let answer =
                mm::composition::store_init(config, (project, &environment), &read_stdin()?).await;
            print_json(&answer)
        }
        OpenbaoCommand::Join {
            project,
            environment,
        } => {
            let (project, environment) = scope(project, &environment);
            let answer =
                mm::composition::store_join(config, (project, &environment), &read_stdin()?).await;
            print_json(&answer)
        }
        OpenbaoCommand::Audit {
            project,
            environment,
            limit,
        } => {
            let (project, environment) = scope(project, &environment);
            print_json(&mm::composition::store_audit(
                config,
                (project, &environment),
                limit,
            ))
        }
        OpenbaoCommand::Status {
            project,
            environment,
        } => {
            let (project, environment) = scope(project, &environment);
            print_json(&mm::composition::store_status(config, (project, &environment)).await)
        }
        OpenbaoCommand::RemovePeer {
            project,
            environment,
            peer,
        } => {
            let (project, environment) = scope(project, &environment);
            print_json(
                &mm::composition::store_remove_peer(config, (project, &environment), &peer).await,
            )
        }
        OpenbaoCommand::Versions {
            project,
            environment,
        } => {
            let (project, environment) = scope(project, &environment);
            print_json(&mm::composition::store_versions(config, (project, &environment)).await)
        }
    }
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn show(store: &mm::adapters::remote::HostAccessStore) -> Result<serde_json::Value, mm::MmError> {
    let snapshot = store.load()?;
    let mut projects = serde_json::Map::new();
    for (id, project) in &snapshot.host.projects {
        let mut environments = serde_json::Map::new();
        for environment in &project.environments {
            let record = snapshot
                .records
                .iter()
                .find(|record| record.project_id == *id && &record.environment == environment);
            let ceiling = project.ceiling_for(environment);
            let users = record.map(|record| {
                record
                    .users
                    .iter()
                    .map(|(name, user)| {
                        serde_json::json!({
                            "user": name,
                            "machine": user.machine,
                            "access": user.access.within(&ceiling),
                            "keys": user.keys.iter().map(|key| &key.fingerprint).collect::<Vec<_>>(),
                            // The user's AI agent: its own keys and narrower access.
                            "agent": user.agent.as_ref().map(|agent| serde_json::json!({
                                "access": agent.access.within(&user.access).within(&ceiling).without_admin(),
                                "keys": agent.keys.iter().map(|key| &key.fingerprint).collect::<Vec<_>>(),
                            })),
                        })
                    })
                    .collect::<Vec<_>>()
            });
            environments.insert(
                environment.clone(),
                serde_json::json!({
                    "ceiling": ceiling,
                    "access-commit": record.map(|record| &record.commit),
                    "deploy-from": record.and_then(|record| record.deploy_from.clone())
                        .or_else(|| project.initial_deploy_from.get(environment).cloned()),
                    "deployed-commit": store.deployed(*id, environment)?.map(|deployed| deployed.commit),
                    "users": users,
                    "blocks": snapshot.blocks.iter()
                        .filter(|block| block.project_id == *id && &block.environment == environment)
                        .collect::<Vec<_>>(),
                }),
            );
        }
        projects.insert(
            id.0.to_string(),
            serde_json::json!({
                "name": project.name,
                "origin": project.origin,
                "delivery": project.delivery.name(),
                "environments": environments,
            }),
        );
    }
    Ok(serde_json::json!({
        "break-glass": snapshot.host.break_glass.iter().map(|entry| &entry.name).collect::<Vec<_>>(),
        "policy-revision": snapshot.policy.revision,
        "projects": projects,
    }))
}

async fn change_block(
    store: &mm::adapters::remote::HostAccessStore,
    project: uuid::Uuid,
    environment: &str,
    fingerprint: &str,
    block: bool,
) -> Result<(), mm::MmError> {
    let project = mm::domain::workspace::ProjectId(project);
    store
        .set_block(project, environment, fingerprint, block, now_unix_ms())
        .await?;
    println!(
        "{} {fingerprint} in {environment}",
        if block { "blocked" } else { "unblocked" }
    );
    Ok(())
}

fn print_audit(state_root: &std::path::Path, limit: usize) -> Result<(), mm::MmError> {
    let records = mm::adapters::remote::AuditLog::new(state_root).read_last(limit)?;
    print_json(&Ok::<_, mm::MmError>(records))
}

fn print_json<T: serde::Serialize>(value: &Result<T, mm::MmError>) -> Result<(), mm::MmError> {
    let value = value.as_ref().map_err(|error| mm::MmError::State {
        message: error.to_string(),
    })?;
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|error| mm::MmError::State {
            message: format!("serializing administrator output: {error}"),
        })?
    );
    Ok(())
}
