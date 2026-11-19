use super::*;
use crate::domain::readiness::SetupIssue;
use crate::domain::runtime::RuntimeKind;
use crate::domain::workspace::DoctorReport;

/// What the selected environment still needs before it can start. Read-only.
pub(super) async fn inspect(
    invocation: &super::dispatch::Invocation,
) -> Result<DoctorReport, MmError> {
    Ok(
        composition::readiness_service(invocation.home(), &invocation.services)?
            .inspect(&invocation.selection)
            .await,
    )
}

/// Fixes one prerequisite with the handler of its command, so the guided step
/// and the command cannot drift apart. It prints and may ask questions.
pub(super) async fn fix(
    issue: &SetupIssue,
    invocation: &super::dispatch::Invocation,
) -> Result<(), MmError> {
    match issue {
        SetupIssue::MissingTarget { name, .. } if name == "local" => {
            composition::ensure_local_target(invocation.home())?;
            writeln!(
                crate::presentation::record::stdout(),
                "Registered this machine as the local target."
            )
            .map_err(|source| MmError::Io {
                context: "writing setup result".to_owned(),
                source,
            })
        }
        SetupIssue::MissingTarget { .. } => Err(MmError::Terminal {
            message: issue.help().to_owned(),
        }),
        SetupIssue::MissingRuntime => {
            let report = inspect(invocation).await?;
            select_runtime(&report, invocation)
        }
        SetupIssue::LocalTlsRequired => {
            composition::authorize_local_setup(invocation.home(), invocation.services.process())
                .await?;
            super::dev::run(
                Some(super::dev::DevCommand::Setup {
                    command: None,
                    no_trust: false,
                    system: false,
                    install_services: false,
                    smtp_port: None,
                    yes: false,
                }),
                invocation,
            )
            .await?;
            Ok(())
        }
        SetupIssue::MissingImages { .. } => {
            super::build::run(None, crate::app::BuildRequest::from(true), false, invocation).await?;
            Ok(())
        }
        SetupIssue::LegacyState { .. } => {
            super::state::recover_interactively(invocation).map(|_| ())
        }
        SetupIssue::MissingDevelopmentServices { .. } => install_services(invocation).await,
    }
}

async fn install_services(invocation: &super::dispatch::Invocation) -> Result<(), MmError> {
    let manager = composition::service_stack_manager(
        invocation.home(),
        invocation.runtime_kind,
        &invocation.services,
    )?;
    if super::dev::run_services_setup(&manager, None, false, OutputFormat::Human).await? != 0 {
        return Err(MmError::State {
            message: "shared-services setup was cancelled or remains incomplete".to_owned(),
        });
    }
    Ok(())
}

fn select_runtime(
    report: &DoctorReport,
    invocation: &super::dispatch::Invocation,
) -> Result<(), MmError> {
    let candidates = report
        .readiness
        .as_ref()
        .map(|report| report.candidates.as_slice())
        .unwrap_or_default();
    let available = candidates
        .iter()
        .filter(|candidate| {
            candidate.probe.available
                && !candidate
                    .probe
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity == Severity::Error)
        })
        .collect::<Vec<_>>();
    if available.is_empty() {
        return Err(MmError::Terminal {
            message: "no compatible runtime is available; start or install Docker or Podman, then try again".to_owned(),
        });
    }
    let options = available
        .iter()
        .enumerate()
        .map(|(index, candidate)| format!("{}. {}", index + 1, runtime_name(candidate.runtime)))
        .collect::<Vec<_>>()
        .join("\n");
    let choice = prompt_choice(&format!(
        "{options}\nWhich one should this environment use? mm remembers it on this machine only (Enter cancels): "
    ))?;
    if choice.trim().is_empty() {
        return Ok(());
    }
    let candidate = choice
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|index| index.checked_sub(1))
        .and_then(|index| available.get(index))
        .ok_or_else(|| MmError::Terminal {
            message: "choose one of the listed runtime numbers".to_owned(),
        })?;
    let runtime = match candidate.runtime {
        RuntimeKind::DockerCompose => RuntimeKindArg::DockerCompose,
        RuntimeKind::PodmanCompose => RuntimeKindArg::PodmanCompose,
    };
    super::target::run_prefer_command(
        PreferCommand::SetRuntime {
            runtime,
            environment: report.environment.clone(),
        },
        &invocation.start,
        invocation.home(),
        OutputFormat::Human,
        invocation.clock.clone(),
    )?;
    Ok(())
}

fn runtime_name(runtime: RuntimeKind) -> &'static str {
    match runtime {
        RuntimeKind::DockerCompose => "docker-compose",
        RuntimeKind::PodmanCompose => "podman-compose",
    }
}

fn prompt_choice(prompt: &str) -> Result<String, MmError> {
    let mut stdout = crate::presentation::record::stdout();
    write!(stdout, "{prompt}")
        .and_then(|()| stdout.flush())
        .map_err(|source| MmError::Io {
            context: "writing setup prompt".to_owned(),
            source,
        })?;
    let mut value = String::new();
    std::io::stdin()
        .read_line(&mut value)
        .map_err(|source| MmError::Io {
            context: "reading setup choice".to_owned(),
            source,
        })?;
    Ok(value)
}
