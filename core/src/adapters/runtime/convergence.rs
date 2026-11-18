//! Waiting for a Compose project to become healthy.
//!
//! One loop for both engines: look at the project, decide, look again a
//! little later. A wait ends early when a service cannot become healthy by
//! waiting: its health check fails, its container exited, or the engine
//! keeps restarting it.

use std::collections::BTreeMap;

use crate::app::ports::RuntimeDriver;
use crate::domain::error::MmError;
use crate::domain::runtime::{
    ContainerObservation, ConvergenceCancellation, ConvergenceOutcome, ConvergencePolicy,
    ConvergenceState, ObservedProject, RuntimeTarget, ServiceObservation, ServiceState,
};

/// How many looks in a row may find a service's container between
/// restarts before the service counts as failing.
const RESTARTING_LOOKS: u32 = 3;

/// How long a container without a health check has to stay up before its
/// service counts as healthy. A container whose command exits at once is
/// still running when the engine says it started; without this a look in
/// that moment called a service healthy that never was.
pub(crate) const STEADY_START: std::time::Duration = std::time::Duration::from_secs(5);

/// How many looks in a row may find a service's container paused before
/// the service counts as failing: only an unpause resumes a paused
/// container, so the rest of the window would change nothing. A pause of a
/// moment, such as `docker commit` makes, passes.
const PAUSED_LOOKS: u32 = 3;

/// What one look at a project says about the wait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Assessment {
    pub state: ConvergenceState,
    /// The service that failed, or the first one that is not healthy yet.
    pub service: Option<String>,
    pub diagnostic_code: Option<String>,
    pub message: Option<String>,
}

/// What the looks of one wait have seen so far.
#[derive(Debug, Default)]
pub(crate) struct ConvergenceWatch {
    /// The services to wait for; `None` waits for every one.
    only: Option<Vec<String>>,
    restarting: BTreeMap<String, u32>,
    /// How often each service's containers were restarted, at the last
    /// look, for an engine that counts it.
    restarts: BTreeMap<String, u64>,
    paused: BTreeMap<String, u32>,
}

impl ConvergenceWatch {
    pub(crate) fn new(only: Option<&[String]>) -> Self {
        Self {
            only: only.map(<[String]>::to_vec),
            restarting: BTreeMap::new(),
            restarts: BTreeMap::new(),
            paused: BTreeMap::new(),
        }
    }

    fn watches(&self, service: &str) -> bool {
        self.only
            .as_ref()
            .is_none_or(|only| only.iter().any(|name| name == service))
    }

    /// The state of the wait after one more look.
    pub(crate) fn assess(&mut self, observation: &ObservedProject) -> Assessment {
        self.assess_at(observation, chrono::Utc::now())
    }

    /// As `assess`, at the moment `now`.
    pub(crate) fn assess_at(
        &mut self,
        observation: &ObservedProject,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Assessment {
        let services = observation
            .services
            .iter()
            .filter(|service| self.watches(&service.identity.service))
            .collect::<Vec<_>>();
        if services.is_empty() {
            return Assessment {
                state: ConvergenceState::Accepted,
                service: None,
                diagnostic_code: None,
                message: None,
            };
        }
        for service in &services {
            let name = &service.identity.service;
            let containers = observation
                .containers
                .iter()
                .filter(|container| &container.service == name)
                .collect::<Vec<_>>();
            // A container that just started proves nothing yet: one in a
            // restart loop is up for a moment between its exits.
            let fresh = just_started(service, &observation.containers, now).is_some();
            if let Some(failure) = self.failure(service, &containers, fresh) {
                return failure;
            }
        }
        let waiting = services
            .iter()
            .find(|service| !healthy(service))
            .map(|service| {
                (
                    service.identity.service.clone(),
                    format!(
                        "{} of {} container(s) running, {} healthy",
                        service.running_count, service.desired_count, service.healthy_count
                    ),
                )
            })
            .or_else(|| {
                services
                    .iter()
                    .find_map(|service| just_started(service, &observation.containers, now))
            });
        Assessment {
            state: if waiting.is_some() {
                ConvergenceState::Converging
            } else {
                ConvergenceState::Succeeded
            },
            diagnostic_code: None,
            message: waiting.as_ref().map(|(_, message)| message.clone()),
            service: waiting.map(|(service, _)| service),
        }
    }

    /// Why waiting longer will not make `service` healthy, if so.
    fn failure(
        &mut self,
        service: &ServiceObservation,
        containers: &[&ContainerObservation],
        fresh: bool,
    ) -> Option<Assessment> {
        let name = &service.identity.service;
        let failed = |code: &str, message: String| Assessment {
            state: ConvergenceState::Failed,
            service: Some(name.clone()),
            diagnostic_code: Some(code.to_owned()),
            message: Some(message),
        };
        if let Some(code) = &service.diagnostic_code {
            let message = if code == "MM_RUNTIME_UNHEALTHY" {
                "its health check fails".to_owned()
            } else {
                "its container failed".to_owned()
            };
            return Some(failed(code, message));
        }
        let paused = containers
            .iter()
            .find(|container| container.state.eq_ignore_ascii_case("paused"));
        let paused_looks = self.paused.entry(name.clone()).or_default();
        *paused_looks = if paused.is_some() {
            *paused_looks + 1
        } else {
            0
        };
        if let Some(container) = paused.filter(|_| *paused_looks >= PAUSED_LOOKS) {
            // Compose lists the name; without one, the id names it too.
            let container = if container.name.is_empty() {
                &container.id
            } else {
                &container.name
            };
            return Some(failed(
                "MM_RUNTIME_PAUSED",
                format!(
                    "its container {container} is paused, and only unpausing it on the node resumes it (`docker unpause {container}`, or `podman unpause {container}`)"
                ),
            ));
        }
        // A container that runs again says nothing about its last exit
        // in its own exit code on every engine.
        let exit_code = containers
            .iter()
            .filter(|container| !container.is_running())
            .find_map(|container| exit_code_of(container));
        if service.state == ServiceState::Stopped {
            let message = match exit_code {
                Some(0) => "its container exited with code 0; a service has to keep running (a task that runs once is a job, not a service)".to_owned(),
                Some(code) => format!("its container exited with code {code}"),
                None => "its container exited".to_owned(),
            };
            return Some(failed("MM_RUNTIME_EXITED", message));
        }
        // Docker shows a container it restarts as `restarting`; Podman never
        // does, and counts the restarts instead (`podman ps`'s `Restarts`):
        // a count that grew since the last look is a restart too.
        let restarted = containers
            .iter()
            .filter_map(|container| container.restart_count)
            .sum::<u64>();
        let grew = self
            .restarts
            .insert(name.clone(), restarted)
            .is_some_and(|before| restarted > before);
        let restarting = grew
            || containers
                .iter()
                .any(|container| container.state.eq_ignore_ascii_case("restarting"));
        let looks = self.restarting.entry(name.clone()).or_default();
        // Only a container that stayed up ends the count: one the engine
        // restarts is between two exits whenever a look finds it running.
        if restarting {
            *looks += 1;
        } else if healthy(service) && !fresh {
            *looks = 0;
        }
        (*looks >= RESTARTING_LOOKS).then(|| {
            failed(
                "MM_RUNTIME_RESTARTING",
                match exit_code {
                    Some(0) => "its container keeps exiting with code 0 and the engine restarts it; a service has to keep running (for a task that runs once, set `restart: \"no\"` or make it a job)".to_owned(),
                    Some(code) => format!("its container keeps exiting with code {code} and the engine restarts it"),
                    None => "the engine keeps restarting its container".to_owned(),
                },
            )
        })
    }
}

/// A service whose containers run but one of them, without a health check,
/// started less than `STEADY_START` ago: the service and what it waits for.
fn just_started(
    service: &ServiceObservation,
    containers: &[ContainerObservation],
    now: chrono::DateTime<chrono::Utc>,
) -> Option<(String, String)> {
    let name = &service.identity.service;
    let members = containers
        .iter()
        .filter(|container| &container.service == name)
        .collect::<Vec<_>>();
    let up = crate::domain::runtime::current_containers(&members)
        .into_iter()
        .filter(|container| container.is_running() && container.health.is_none())
        .filter_map(|container| up_for(container, now))
        .min()?;
    (up < STEADY_START).then(|| {
        (
            name.clone(),
            format!(
                "its container has run for {} s; one without a health check has to stay up {} s",
                up.as_secs(),
                STEADY_START.as_secs()
            ),
        )
    })
}

/// How long a running container has been up: from its start time when the
/// engine gave one (its API, Podman), else from the status text Compose
/// prints (`Up 2 seconds`). `None` when neither says.
fn up_for(
    container: &ContainerObservation,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<std::time::Duration> {
    // An engine that prints another format leaves it to the status text.
    if let Some(Ok(started)) = container
        .started_at
        .as_deref()
        .map(chrono::DateTime::parse_from_rfc3339)
    {
        return Some(
            now.signed_duration_since(started)
                .to_std()
                .unwrap_or_default(),
        );
    }
    let up = container.status.trim().strip_prefix("Up ")?;
    if up.starts_with("Less than a second") {
        return Some(std::time::Duration::ZERO);
    }
    let mut words = up.split_whitespace();
    let count = match words.next()?.parse::<u64>() {
        Ok(count) => count,
        // `Up About a minute`: long past the start.
        Err(_) => return Some(STEADY_START),
    };
    let unit = words.next()?;
    // Minutes, hours, days: long past the start.
    unit.starts_with("second")
        .then_some(std::time::Duration::from_secs(count))
        .or(Some(STEADY_START))
}

/// The code a container last exited with. Compose's list prints the code
/// of a container the engine restarts in its status (`Restarting (1) 2
/// seconds ago`) and says 0 in its exit code field.
fn exit_code_of(container: &ContainerObservation) -> Option<i32> {
    let status = container.status.trim();
    let from_status = ["Restarting (", "Exited ("].iter().find_map(|prefix| {
        let (code, _) = status.strip_prefix(prefix)?.split_once(')')?;
        if code.is_empty() || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        code.bytes().try_fold(0_i32, |value, digit| {
            value.checked_mul(10)?.checked_add(i32::from(digit - b'0'))
        })
    });
    from_status.or(container.exit_code)
}

fn healthy(service: &ServiceObservation) -> bool {
    service.state == ServiceState::Running
        && service.running_count == service.desired_count
        && service.healthy_count == service.desired_count
}

/// Looks at the target's project until the services `only` names (every
/// service without it) are healthy, one of them cannot become healthy, or
/// the policy's time is up.
pub(crate) async fn wait_for_project(
    runtime: &(impl RuntimeDriver + ?Sized),
    target: &RuntimeTarget,
    policy: ConvergencePolicy,
    cancellation: ConvergenceCancellation,
    only: Option<&[String]>,
) -> Result<ConvergenceOutcome, MmError> {
    let started = tokio::time::Instant::now();
    let mut backoff = policy.initial_backoff;
    let mut polls = 0;
    let mut watch = ConvergenceWatch::new(only);
    loop {
        if cancellation.is_cancelled() {
            return Ok(ConvergenceOutcome {
                state: ConvergenceState::Cancelled,
                polls,
                observation: Default::default(),
                service: None,
                diagnostic_code: Some("MM_CONVERGENCE_CANCELLED".to_owned()),
                message: Some("runtime convergence was cancelled".to_owned()),
            });
        }
        let observation = runtime.observe(target).await?;
        polls += 1;
        let assessment = watch.assess(&observation);
        if matches!(
            assessment.state,
            ConvergenceState::Succeeded | ConvergenceState::Failed
        ) {
            return Ok(ConvergenceOutcome {
                state: assessment.state,
                polls,
                observation,
                service: assessment.service,
                diagnostic_code: assessment.diagnostic_code,
                message: assessment.message,
            });
        }
        if started.elapsed() >= policy.timeout {
            let waited = policy.timeout.as_secs();
            return Ok(ConvergenceOutcome {
                state: ConvergenceState::TimedOut,
                polls,
                observation,
                diagnostic_code: Some("MM_CONVERGENCE_TIMEOUT".to_owned()),
                message: Some(match assessment.message {
                    Some(detail) => format!("not healthy after {waited} s ({detail})"),
                    None => format!(
                        "Compose project '{}' did not converge before the deadline",
                        target.project_name
                    ),
                }),
                service: assessment.service,
            });
        }
        tokio::time::sleep(
            backoff.min(
                policy
                    .timeout
                    .saturating_sub(started.elapsed())
                    .max(std::time::Duration::from_millis(50)),
            ),
        )
        .await;
        backoff = backoff.saturating_mul(2).min(policy.maximum_backoff);
    }
}

#[cfg(test)]
#[path = "convergence_tests.rs"]
mod tests;
