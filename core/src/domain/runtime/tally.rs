//! What the containers of one service say about the service.
//!
//! An engine keeps containers that no longer run: one that a newer
//! container replaced, a job that ran once in the service's project. Those
//! are history. A service whose current containers run and are healthy is
//! running, whatever its history holds.

use super::{ContainerObservation, ServiceSnapshot, ServiceState};

/// The label Compose puts on a container started with `compose run`.
const ONE_OFF_LABEL: &str = "com.docker.compose.oneoff";

/// Counts and state of one service on one host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceTally {
    pub desired_count: u32,
    pub running_count: u32,
    pub healthy_count: u32,
    pub state: ServiceState,
    pub diagnostic_code: Option<String>,
}

impl ContainerObservation {
    /// Whether the container is leaving its node (`domain::leaving`): it
    /// still runs and serves what it has, and no node sends it anything
    /// new. Its node's `mmd` marks it so in a status.
    #[must_use]
    pub fn is_leaving(&self) -> bool {
        self.labels
            .contains_key(crate::domain::leaving::LEAVING_LABEL)
    }

    /// Whether the engine runs the container now.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.state.eq_ignore_ascii_case("running") || self.status_word_is(&["running"])
    }

    /// Whether the container ended or never started.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        const STOPPED: [&str; 7] = [
            "stopped", "exited", "shutdown", "complete", "failed", "rejected", "created",
        ];
        STOPPED
            .iter()
            .any(|state| self.state.eq_ignore_ascii_case(state))
            || self.status_word_is(&STOPPED)
    }

    /// Whether the container was started once for a command or a job, not
    /// as the service itself.
    #[must_use]
    pub fn is_one_off(&self) -> bool {
        self.labels
            .get(ONE_OFF_LABEL)
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    }

    /// Whether the container is not running because the engine could not
    /// mount one of mm's shared volumes for it: it starts by itself once
    /// the storage node answers again.
    #[must_use]
    pub fn waits_for_shared_storage(&self) -> bool {
        !self.is_running()
            && self.error.as_deref().is_some_and(|error| {
                error.contains(crate::domain::shared_storage::ENGINE_VOLUME_PREFIX)
            })
    }

    /// Whether the container is alive in the engine's eyes: running, or
    /// about to run again.
    fn is_live(&self) -> bool {
        self.is_running()
            || ["restarting", "paused"]
                .iter()
                .any(|state| self.state.eq_ignore_ascii_case(state))
    }

    /// Whether the container has a health check that has not passed yet
    /// since it started. It is not ready, and it has not failed either.
    #[must_use]
    pub fn health_is_starting(&self) -> bool {
        self.health_is("starting")
    }

    fn health_is(&self, verdict: &str) -> bool {
        self.health
            .as_deref()
            .is_some_and(|health| health.eq_ignore_ascii_case(verdict))
    }

    fn status_word_is(&self, words: &[&str]) -> bool {
        self.status
            .split_whitespace()
            .next()
            .is_some_and(|first| words.iter().any(|word| first.eq_ignore_ascii_case(word)))
    }
}

/// The containers of one service on one host that say how the service is:
/// every live one, and every other one that is not older than the newest
/// live one. A container that stopped before a live one was created was
/// replaced by it; a container started once for a job never counts. With
/// no live container all of them count: the service is down.
#[must_use]
pub fn current_containers<'a>(
    containers: &[&'a ContainerObservation],
) -> Vec<&'a ContainerObservation> {
    let newest_live = containers
        .iter()
        .filter(|container| !container.is_one_off() && container.is_live())
        .filter_map(|container| container.created_at.as_deref())
        .max();
    containers
        .iter()
        .filter(|container| !container.is_one_off())
        .filter(|container| {
            container.is_live()
                || match (newest_live, container.created_at.as_deref()) {
                    (Some(newest), Some(created)) => created >= newest,
                    // Nothing to compare with: it counts.
                    _ => true,
                }
        })
        .copied()
        .collect()
}

/// Counts and state from every container of one service on one host.
#[must_use]
pub fn tally_service(containers: &[&ContainerObservation]) -> ServiceTally {
    let current = current_containers(containers);
    let desired_count = u32::try_from(current.len()).unwrap_or(u32::MAX).max(1);
    let count = |wanted: &dyn Fn(&ContainerObservation) -> bool| {
        u32::try_from(current.iter().filter(|container| wanted(container)).count())
            .unwrap_or(u32::MAX)
    };
    let running_count = count(&|container| container.is_running());
    let healthy_count = count(&|container| {
        container.is_running()
            && container
                .health
                .as_deref()
                .is_none_or(|health| health.eq_ignore_ascii_case("healthy"))
    });
    let diagnostic_code = current
        .iter()
        .find_map(|container| container.diagnostic_code.clone());
    let state = if current.is_empty() {
        ServiceState::Missing
    } else if diagnostic_code.is_some() {
        ServiceState::Degraded
    } else if running_count == desired_count && healthy_count == desired_count {
        ServiceState::Running
    } else if running_count > 0 {
        ServiceState::Degraded
    } else if current.iter().all(|container| container.is_stopped()) {
        ServiceState::Stopped
    } else if current
        .iter()
        .any(|container| container.health_is("unhealthy"))
    {
        // Nothing runs, something is paused or between restarts, and its
        // health check had failed.
        ServiceState::Degraded
    } else {
        ServiceState::Unknown
    };
    ServiceTally {
        desired_count,
        running_count,
        healthy_count,
        state,
        diagnostic_code,
    }
}

impl ServiceSnapshot {
    /// Writes down whether the service, and each node's share of it, is
    /// `Degraded` only because it is still coming up: every container it
    /// should have runs, none failed its health check, and at least one
    /// check has not passed yet (`starting`), as during a rollout step.
    /// `healthy_count` says how many are ready.
    ///
    /// `starting` is what every interface reads. Whoever builds a snapshot
    /// or changes its state, counts, shares or containers calls this last.
    pub fn note_starting(&mut self) {
        let current = self
            .containers
            .iter()
            .filter(|container| !container.history)
            .collect::<Vec<_>>();
        let starting = self.diagnostic_code.is_none()
            && coming_up(self.state, self.desired_count, self.running_count, &current);
        let shares = self
            .nodes
            .iter()
            .map(|share| {
                let here = current
                    .iter()
                    .copied()
                    .filter(|container| container.node.as_deref() == Some(share.node.as_str()))
                    .collect::<Vec<_>>();
                here.iter()
                    .all(|container| container.diagnostic_code.is_none())
                    && coming_up(share.state, share.desired_count, share.running_count, &here)
            })
            .collect::<Vec<_>>();
        self.starting = starting;
        for (share, starting) in self.nodes.iter_mut().zip(shares) {
            share.starting = starting;
        }
    }
}

/// Whether counts, a state and the current containers they were made from
/// describe a service that is only still coming up.
fn coming_up(
    state: ServiceState,
    desired_count: u32,
    running_count: u32,
    current: &[&ContainerObservation],
) -> bool {
    state == ServiceState::Degraded
        && u32::try_from(current.len()).is_ok_and(|count| count == desired_count)
        && running_count == desired_count
        && current.iter().all(|container| {
            container.is_running()
                && (container.health.is_none()
                    || container.health_is("healthy")
                    || container.health_is_starting())
        })
        && current
            .iter()
            .any(|container| container.health_is_starting())
}

/// The state of a service that runs on several nodes, from each node's own.
/// A node that did not answer has the state `Unknown`: with other nodes
/// running the service it is `Degraded`, since nobody knows how much of it
/// runs.
impl ServiceState {
    /// Running, or job only: nothing more is expected of the service.
    #[must_use]
    pub const fn is_up(self) -> bool {
        matches!(self, Self::Running | Self::JobOnly)
    }
}

#[must_use]
pub fn state_across_nodes(states: &[ServiceState]) -> ServiceState {
    let all = |wanted: ServiceState| states.iter().all(|state| *state == wanted);
    if states.contains(&ServiceState::JobOnly) {
        ServiceState::JobOnly
    } else if states.is_empty() || all(ServiceState::Missing) {
        ServiceState::Missing
    } else if all(ServiceState::Running) {
        ServiceState::Running
    } else if all(ServiceState::Stopped) {
        ServiceState::Stopped
    } else if all(ServiceState::Unknown) {
        ServiceState::Unknown
    } else {
        ServiceState::Degraded
    }
}
