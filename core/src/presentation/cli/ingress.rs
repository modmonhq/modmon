use super::*;
use crate::domain::host_ingress::PublicIngressState;

#[derive(Debug, Clone, Subcommand)]
pub enum IngressCommand {
    /// Ensure ingress is installed and healthy, then start public forwarding on ports 80 and 443.
    Start {
        /// Approve required installation or repair plans without prompting.
        #[arg(long)]
        yes: bool,
    },
    /// Stop global public forwarding and keep it stopped across reboots and deploys; keep project containers running.
    Stop,
    /// Show public forwarding state and the health of the services it depends on (read-only).
    Status,
    /// Reconcile ingress prerequisites, then restart public forwarding on ports 80 and 443.
    Restart {
        /// Approve required installation or repair plans without prompting.
        #[arg(long)]
        yes: bool,
    },
}

/// Forwarding state is reported separately from its supporting services: a
/// stopped forwarder with healthy services is a normal, intentional state.
#[derive(Debug, serde::Serialize)]
struct IngressStatusReport {
    scope: &'static str,
    ports: [u16; 2],
    state: PublicIngressState,
    forwarder: ForwarderReport,
    services: ServicesReport,
    tls: TlsReport,
    loopback_ports: Option<[u16; 2]>,
    /// Forwarding is running and everything it depends on is healthy.
    ready: bool,
}

#[derive(Debug, serde::Serialize)]
struct ForwarderReport {
    installed: bool,
    running: bool,
    failed: bool,
    kept_stopped: bool,
}

#[derive(Debug, serde::Serialize)]
struct ServicesReport {
    installed: bool,
    healthy: bool,
    unhealthy_components: Vec<crate::domain::service_stack::ServiceComponentKind>,
    errors: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct TlsReport {
    installed: bool,
    ready: bool,
}

fn services_healthy(services: &crate::domain::service_stack::ServiceStackStatus) -> bool {
    services.installed
        && !services
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
        && services
            .components
            .iter()
            .all(|component| component.healthy)
}

fn tls_ready(tls: &crate::domain::ca::LocalCaStatus) -> bool {
    tls.installed
        && (tls.trust_disabled
            || tls
                .trust
                .as_ref()
                .is_some_and(|trust| trust.installed && !trust.mismatch))
}

fn status_report(
    forwarder: crate::domain::host_ingress::PublicIngressStatus,
    services: Result<crate::domain::service_stack::ServiceStackStatus, MmError>,
    tls: Result<crate::domain::ca::LocalCaStatus, MmError>,
    loopback_ports: Option<(u16, u16)>,
) -> IngressStatusReport {
    let services = match services {
        Ok(status) => ServicesReport {
            installed: status.installed,
            healthy: services_healthy(&status),
            unhealthy_components: status
                .components
                .iter()
                .filter(|component| !component.healthy)
                .map(|component| component.kind)
                .collect(),
            errors: status
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == Severity::Error)
                .map(|diagnostic| diagnostic.message.clone())
                .collect(),
        },
        Err(error) => ServicesReport {
            installed: false,
            healthy: false,
            unhealthy_components: Vec::new(),
            errors: vec![error.to_string()],
        },
    };
    let tls = tls.map_or(
        TlsReport {
            installed: false,
            ready: false,
        },
        |status| TlsReport {
            installed: status.installed,
            ready: tls_ready(&status),
        },
    );
    let running = forwarder.installed && forwarder.running;
    IngressStatusReport {
        scope: "workstation",
        ports: [80, 443],
        state: if running {
            PublicIngressState::Running
        } else {
            PublicIngressState::Stopped
        },
        forwarder: ForwarderReport {
            installed: forwarder.installed,
            running,
            failed: forwarder.failed,
            kept_stopped: forwarder.kept_stopped(),
        },
        loopback_ports: loopback_ports.map(|(http, https)| [http, https]),
        ready: running && services.healthy && tls.ready,
        services,
        tls,
    }
}

async fn inspect_status(
    invocation: &super::dispatch::Invocation,
    control: &dyn crate::app::ports::PublicIngressControl,
) -> Result<IngressStatusReport, MmError> {
    let forwarder = control.inspect().await?;
    let services = match composition::local_services_runtime(
        invocation.home(),
        invocation.selection.runtime,
        &invocation.services,
    )
    .await
    {
        Ok(runtime) => {
            composition::inspect_service_stack(invocation.home(), runtime, &invocation.services)
                .await
        }
        Err(error) => Err(error),
    };
    let tls = match composition::local_ca_manager(
        invocation.home(),
        false,
        invocation.services.process(),
    ) {
        Ok(ca) => ca.status().await,
        Err(error) => Err(error),
    };
    let loopback_ports = composition::local_ingress_loopback_ports(invocation.home())
        .ok()
        .flatten();
    Ok(status_report(forwarder, services, tls, loopback_ports))
}

fn write_status(report: &IngressStatusReport, format: OutputFormat) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return output::write_json(report);
    }
    let forwarder = &report.forwarder;
    let forwarding = match (forwarder.installed, forwarder.running, forwarder.failed) {
        (false, _, _) => "not installed; run `mm ingress start`",
        (true, true, _) => "running (ports 80 and 443)",
        _ if forwarder.kept_stopped => {
            "stopped by `mm ingress stop`; ports 80 and 443 stay free for another proxy until `mm ingress start`"
        }
        (true, false, true) => "failed; run `mm ingress restart`",
        (true, false, false) if cfg!(target_os = "linux") => {
            "stopped until the next deploy or reboot; run `mm ingress start`, or `mm ingress stop` to keep it stopped"
        }
        (true, false, false) => "stopped; run `mm ingress start`",
    };
    let services = if report.services.healthy {
        "healthy".to_owned()
    } else if !report.services.installed {
        "not installed".to_owned()
    } else if report.services.unhealthy_components.is_empty() {
        "unhealthy".to_owned()
    } else {
        let names = report
            .services
            .unhealthy_components
            .iter()
            .filter_map(|kind| serde_json::to_value(kind).ok())
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        format!("unhealthy: {}", names.join(", "))
    };
    let tls = match (report.tls.installed, report.tls.ready) {
        (_, true) => "ready",
        (true, false) => "certificate authority not trusted; run `mm dev tls repair`",
        (false, false) => "not set up",
    };
    let mut stdout = crate::presentation::record::stdout();
    let write = |stdout: &mut crate::presentation::record::Stdout, line: String| {
        writeln!(stdout, "{line}").map_err(|source| MmError::Io {
            context: "writing public ingress status".to_owned(),
            source,
        })
    };
    write(&mut stdout, format!("Public ingress: {forwarding}"))?;
    if let Some([http, https]) = report.loopback_ports {
        write(
            &mut stdout,
            format!("Traefik loopback: HTTP 127.0.0.1:{http}, HTTPS 127.0.0.1:{https}"),
        )?;
    }
    write(&mut stdout, format!("Shared services: {services}"))?;
    for error in &report.services.errors {
        write(&mut stdout, format!("  {error}"))?;
    }
    write(&mut stdout, format!("Local TLS: {tls}"))
}

pub(super) async fn run(
    command: IngressCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let allow_prompt = invocation.format == OutputFormat::Human
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal();
    let control = composition::public_ingress_control(invocation.services.process())?;
    let mut already_stopped = false;
    let state = match command {
        IngressCommand::Status => {
            let report = inspect_status(invocation, control.as_ref()).await?;
            write_status(&report, invocation.format)?;
            return Ok(0);
        }
        IngressCommand::Restart { yes } => {
            stop_if_running(control.as_ref(), allow_prompt).await?;
            let code = ensure_started(invocation, control.as_ref(), allow_prompt, yes).await?;
            if code != 0 {
                return Ok(code);
            }
            PublicIngressState::Running
        }
        IngressCommand::Start { yes } => {
            let code = ensure_started(invocation, control.as_ref(), allow_prompt, yes).await?;
            if code != 0 {
                return Ok(code);
            }
            PublicIngressState::Running
        }
        IngressCommand::Stop => {
            already_stopped = !keep_stopped(control.as_ref(), allow_prompt).await?;
            PublicIngressState::Stopped
        }
    };
    if invocation.format == OutputFormat::Json {
        output::write_json(
            &serde_json::json!({ "scope": "workstation", "state": state, "ports": [80, 443] }),
        )?;
    } else {
        if already_stopped {
            writeln!(
                crate::presentation::record::stdout(),
                "Public ingress is not running."
            )
            .map_err(|source| MmError::Io {
                context: "writing public ingress status".to_owned(),
                source,
            })?;
            return Ok(0);
        }
        let kept = state == PublicIngressState::Stopped && control.inspect().await?.kept_stopped();
        writeln!(
            crate::presentation::record::stdout(),
            "Public ingress {} (ports 80 and 443).{}",
            state.description(),
            if kept {
                " It stays stopped across reboots and deploys until `mm ingress start`."
            } else {
                ""
            }
        )
        .map_err(|source| MmError::Io {
            context: "writing public ingress status".to_owned(),
            source,
        })?;
    }
    Ok(0)
}

async fn stop_if_running(
    control: &dyn crate::app::ports::PublicIngressControl,
    allow_prompt: bool,
) -> Result<bool, MmError> {
    let status = control.inspect().await?;
    if !status.installed || !status.running {
        return Ok(false);
    }
    control
        .set_state(PublicIngressState::Stopped, allow_prompt)
        .await?;
    Ok(true)
}

async fn keep_stopped(
    control: &dyn crate::app::ports::PublicIngressControl,
    allow_prompt: bool,
) -> Result<bool, MmError> {
    let status = control.inspect().await?;
    if !status.installed || status.kept_stopped() {
        return Ok(false);
    }
    control
        .set_state(PublicIngressState::Stopped, allow_prompt)
        .await?;
    Ok(true)
}

async fn ensure_started(
    invocation: &super::dispatch::Invocation,
    control: &dyn crate::app::ports::PublicIngressControl,
    allow_prompt: bool,
    yes: bool,
) -> Result<u8, MmError> {
    let runtime = composition::local_services_runtime(
        invocation.home(),
        invocation.selection.runtime,
        &invocation.services,
    )
    .await?;
    let forwarder = control.inspect().await?;
    let services =
        composition::inspect_service_stack(invocation.home(), runtime, &invocation.services)
            .await?;
    let ca =
        composition::local_ca_manager(invocation.home(), false, invocation.services.process())?;
    let tls = ca.status().await?;
    let tls_ready = tls_ready(&tls);
    let needs_setup =
        !forwarder.installed || forwarder.failed || !tls_ready || !services_healthy(&services);
    if needs_setup {
        if !allow_prompt && !yes {
            let authorization = if cfg!(target_os = "linux") {
                " after authorizing sudo"
            } else {
                ""
            };
            return Err(MmError::Terminal {
                message: format!(
                    "ingress needs installation or repair; run `mm ingress start` in a terminal to review, or pass --yes{authorization}"
                ),
            });
        }
        if allow_prompt {
            composition::authorize_local_setup(invocation.home(), invocation.services.process())
                .await?;
        }
        composition::prepare_public_ingress(invocation.services.process(), allow_prompt).await?;
        composition::ensure_local_target(invocation.home())?;
        if !tls_ready {
            let status = ca.setup(!tls.trust_disabled).await?;
            if invocation.format == OutputFormat::Human {
                output::write_local_ca_status(&status, invocation.format)?;
            }
        }
        let manager =
            composition::service_stack_manager(invocation.home(), runtime, &invocation.services)?;
        let code = super::dev::run_services_setup_reporting(
            &manager,
            None,
            yes,
            invocation.format,
            invocation.format == OutputFormat::Human,
        )
        .await?;
        if code != 0 {
            return Ok(code);
        }
    }
    if !control.inspect().await?.running {
        control
            .set_state(PublicIngressState::Running, allow_prompt)
            .await?;
    }
    Ok(0)
}

#[cfg(test)]
mod tests;
