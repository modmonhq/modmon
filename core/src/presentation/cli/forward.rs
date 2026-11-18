//! `mm forward`: reach a remote service's port on localhost. Each accepted
//! connection travels through the mm protocol to that service's container;
//! SSH port forwarding stays disabled on the host.

use super::*;
use crate::domain::forward::{ForwardShortcut, default_bind, is_loopback_bind, parse_forward_spec};

#[derive(Debug, Clone, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct ForwardArgs {
    #[command(subcommand)]
    command: Option<ForwardCommand>,
    /// SERVICE, SERVICE:PORT, or the name of a saved forward. Repeatable.
    #[arg(value_name = "SERVICE[:PORT]")]
    targets: Vec<String>,
    /// Local port; defaults to the service port, or a free port if that is busy.
    #[arg(long, value_name = "PORT")]
    local: Option<u16>,
    /// Local address to listen on. Anything but loopback exposes the service to your network.
    #[arg(long, value_name = "ADDRESS")]
    bind: Option<String>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ForwardCommand {
    /// Save a forward for this project and environment, with a pinned local port.
    Save {
        /// Name to start it by, for example `mm forward prod-db`.
        #[arg(value_name = "NAME")]
        name: String,
        /// Service and optional container port.
        #[arg(value_name = "SERVICE[:PORT]")]
        target: String,
        /// Local port to pin; defaults to the service port.
        #[arg(long, value_name = "PORT")]
        local: Option<u16>,
        /// Local address to listen on; defaults to 127.0.0.1.
        #[arg(long, value_name = "ADDRESS")]
        bind: Option<String>,
        /// Start it with `mm forward up`.
        #[arg(long)]
        auto: bool,
    },
    /// List this project's saved forwards.
    List,
    /// Remove a saved forward.
    Remove {
        /// Name of the saved forward.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Start every saved forward marked --auto.
    Up,
}

/// One listener to open.
struct PlannedForward {
    environment: String,
    service: String,
    port: u16,
    local: Option<u16>,
    bind: String,
    /// A saved forward must get its pinned port or fail.
    pinned: bool,
}

pub(super) async fn run(
    args: ForwardArgs,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let document = composition::project_documents().load_from(&invocation.start)?;
    let project = document.config.project.id.0.to_string();
    let store = composition::forward_shortcuts(invocation.home())?;
    let environment = crate::domain::config::resolve_environment(
        &document,
        invocation.selection.environment.as_deref(),
    )?;
    let planned = match args.command {
        Some(ForwardCommand::Save {
            name,
            target,
            local,
            bind,
            auto,
        }) => {
            let (service, port) =
                parse_forward_spec(&target).map_err(|message| MmError::Environment { message })?;
            let shortcut = ForwardShortcut {
                environment,
                port: resolve_port(invocation, &service, port)?,
                service,
                local_port: 0,
                bind: bind.unwrap_or_else(default_bind),
                auto,
            };
            return save(&store, &project, &name, local, shortcut);
        }
        Some(ForwardCommand::List) => return list(invocation, &store.list(&project)?),
        Some(ForwardCommand::Remove { name }) => {
            if !store.remove(&project, &name)? {
                return Err(MmError::Environment {
                    message: format!("no saved forward named '{name}'"),
                });
            }
            crate::outln!("Removed '{}'", output::terminal_safe(&name));
            return Ok(0);
        }
        Some(ForwardCommand::Up) => store
            .list(&project)?
            .into_values()
            .filter(|shortcut| shortcut.auto)
            .map(from_shortcut)
            .collect::<Vec<_>>(),
        None => {
            if args.targets.is_empty() {
                return Err(MmError::Environment {
                    message: "name a service to forward, for example `mm forward postgres`"
                        .to_owned(),
                });
            }
            if args.targets.len() > 1 && args.local.is_some() {
                return Err(MmError::Environment {
                    message: "--local applies to one forward at a time".to_owned(),
                });
            }
            let saved = store.list(&project)?;
            let mut planned = Vec::new();
            for target in &args.targets {
                if let Some(shortcut) = saved.get(target) {
                    planned.push(from_shortcut(shortcut.clone()));
                    continue;
                }
                let (service, port) = parse_forward_spec(target)
                    .map_err(|message| MmError::Environment { message })?;
                planned.push(PlannedForward {
                    environment: environment.clone(),
                    port: resolve_port(invocation, &service, port)?,
                    service,
                    local: args.local,
                    bind: args.bind.clone().unwrap_or_else(default_bind),
                    pinned: false,
                });
            }
            planned
        }
    };
    if planned.is_empty() {
        crate::outln!("No saved forwards are marked --auto.");
        return Ok(0);
    }
    serve(invocation, planned).await
}

fn save(
    store: &composition::ForwardShortcutStore,
    project: &str,
    name: &str,
    local: Option<u16>,
    mut shortcut: ForwardShortcut,
) -> Result<u8, MmError> {
    shortcut.local_port = local.unwrap_or(shortcut.port);
    store.save(project, name, shortcut.clone())?;
    crate::outln!(
        "Saved '{}': {}:{} -> {}/{}:{}{}",
        output::terminal_safe(name),
        output::terminal_safe(&shortcut.bind),
        shortcut.local_port,
        output::terminal_safe(&shortcut.environment),
        output::terminal_safe(&shortcut.service),
        shortcut.port,
        if shortcut.auto { " (auto)" } else { "" }
    );
    Ok(0)
}

fn list(
    invocation: &super::dispatch::Invocation,
    shortcuts: &std::collections::BTreeMap<String, ForwardShortcut>,
) -> Result<u8, MmError> {
    if invocation.format == OutputFormat::Json {
        output::write_json(shortcuts)?;
        return Ok(0);
    }
    if shortcuts.is_empty() {
        crate::outln!("No saved forwards. `mm forward save NAME SERVICE[:PORT]` adds one.");
    }
    for (name, shortcut) in shortcuts {
        crate::outln!(
            "{}  {}:{} -> {}/{}:{}{}",
            output::terminal_safe(name),
            output::terminal_safe(&shortcut.bind),
            shortcut.local_port,
            output::terminal_safe(&shortcut.environment),
            output::terminal_safe(&shortcut.service),
            shortcut.port,
            if shortcut.auto { "  auto" } else { "" }
        );
    }
    Ok(0)
}

fn from_shortcut(shortcut: ForwardShortcut) -> PlannedForward {
    PlannedForward {
        environment: shortcut.environment,
        service: shortcut.service,
        port: shortcut.port,
        local: Some(shortcut.local_port),
        bind: shortcut.bind,
        pinned: true,
    }
}

fn resolve_port(
    invocation: &super::dispatch::Invocation,
    service: &str,
    explicit: Option<u16>,
) -> Result<u16, MmError> {
    if let Some(port) = explicit {
        return Ok(port);
    }
    let ports = composition::declared_service_ports(&invocation.start, service)?;
    match ports.as_slice() {
        [port] => Ok(*port),
        [] => Err(MmError::Environment {
            message: format!("service '{service}' declares no port; use {service}:PORT"),
        }),
        many => Err(MmError::Environment {
            message: format!(
                "service '{service}' has several ports ({}); use {service}:PORT",
                many.iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }),
    }
}

async fn serve(
    invocation: &super::dispatch::Invocation,
    planned: Vec<PlannedForward>,
) -> Result<u8, MmError> {
    let mut tasks = Vec::new();
    for forward in planned {
        let mut selection = invocation.selection.clone();
        selection.environment = Some(forward.environment.clone());
        let Some(environment) = composition::remote_environment(
            &selection,
            invocation.home(),
            invocation.runtime_kind,
        )?
        else {
            crate::outln!(
                "{} is local: reach {} through its published port or local ingress.",
                output::terminal_safe(&forward.environment),
                output::terminal_safe(&forward.service)
            );
            continue;
        };
        // On a cluster the forward goes to the node that runs the service.
        let (transport, scope) = environment.node_running(&forward.service)?.connection()?;
        if !is_loopback_bind(&forward.bind) {
            eprintln!(
                "warning: listening on {}; anyone who can reach this machine can reach {}",
                output::terminal_safe(&forward.bind),
                output::terminal_safe(&forward.service)
            );
        }
        let listener = listen(&forward).await?;
        let local = listener.local_addr().map_err(|source| MmError::Io {
            context: "reading the forward's local address".to_owned(),
            source,
        })?;
        crate::outln!(
            "Forwarding {local} -> {}/{}:{}",
            output::terminal_safe(&forward.environment),
            output::terminal_safe(&forward.service),
            forward.port
        );
        tasks.push(tokio::spawn(accept_loop(
            listener, transport, scope, forward,
        )));
    }
    if tasks.is_empty() {
        return Ok(0);
    }
    crate::outln!("Press Ctrl-C to stop.");
    tokio::signal::ctrl_c()
        .await
        .map_err(|source| MmError::Io {
            context: "waiting for Ctrl-C".to_owned(),
            source,
        })?;
    for task in &tasks {
        task.abort();
    }
    Ok(0)
}

async fn listen(forward: &PlannedForward) -> Result<tokio::net::TcpListener, MmError> {
    let bind = if forward.bind == "localhost" {
        "127.0.0.1"
    } else {
        forward.bind.as_str()
    };
    let wanted = forward.local.unwrap_or(forward.port);
    match tokio::net::TcpListener::bind((bind, wanted)).await {
        Ok(listener) => Ok(listener),
        Err(error) if forward.pinned || forward.local.is_some() => Err(MmError::Io {
            context: format!("listening on {bind}:{wanted} for {}", forward.service),
            source: error,
        }),
        Err(_) => {
            let listener = tokio::net::TcpListener::bind((bind, 0))
                .await
                .map_err(|source| MmError::Io {
                    context: format!("listening on {bind} for {}", forward.service),
                    source,
                })?;
            eprintln!(
                "note: port {wanted} is busy; using a free port. `mm forward save NAME {}:{} --local PORT` pins one.",
                forward.service, forward.port
            );
            Ok(listener)
        }
    }
}

async fn accept_loop(
    listener: tokio::net::TcpListener,
    transport: Arc<dyn crate::app::ports::RemoteTransport>,
    scope: crate::domain::remote::RemoteScope,
    forward: PlannedForward,
) {
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                eprintln!("forward {}: accept failed: {error}", forward.service);
                continue;
            }
        };
        let transport = Arc::clone(&transport);
        let scope = scope.clone();
        let service = forward.service.clone();
        let port = forward.port;
        tokio::spawn(async move {
            match transport
                .forward(scope, service.clone(), port, stream)
                .await
            {
                Ok(totals) => tracing::debug!(%peer, ?totals, "forward closed"),
                Err(error) => eprintln!(
                    "forward {}: {}",
                    output::terminal_safe(&service),
                    output::terminal_safe(&error.to_string())
                ),
            }
        });
    }
}
