//! The steps of `mm update` after the binaries: restart the local user
//! services that run `mm`, name the `mm mcp serve` processes left on the old
//! release, and list the hosts that run an older one.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::app::ports::{HostVersions, LocalUserServices};
use crate::domain::error::MmError;
use crate::domain::update_steps::{
    FollowUpPlan, LocalUserService, ServiceOutcome, ServiceReport, UpdateSummary,
};

const SETTLE: Duration = Duration::from_secs(1);
const CHECKS: u32 = 10;
const STEADY: u32 = 2;

#[derive(Clone)]
pub struct UpdateFollowUpService {
    services: Arc<dyn LocalUserServices>,
    hosts: Arc<dyn HostVersions>,
    settle: Duration,
    checks: u32,
}

impl UpdateFollowUpService {
    #[must_use]
    pub fn new(services: Arc<dyn LocalUserServices>, hosts: Arc<dyn HostVersions>) -> Self {
        Self {
            services,
            hosts,
            settle: SETTLE,
            checks: CHECKS,
        }
    }

    /// How long to wait between the checks that a restarted service runs,
    /// and how many checks to make.
    #[must_use]
    pub fn with_checks(mut self, settle: Duration, checks: u32) -> Self {
        self.settle = settle;
        self.checks = checks;
        self
    }

    /// What follows the binaries. With `replacing`, every running service
    /// that starts `executable` restarts; without, only those whose process
    /// is older than `executable`.
    pub async fn plan(
        &self,
        executable: &Path,
        version: &str,
        replacing: bool,
    ) -> Result<FollowUpPlan, MmError> {
        let services = self.services.services(executable).await?;
        let restart = services
            .iter()
            .filter(|service| {
                service.running
                    && starts(service, executable)
                    && (replacing || service.older == Some(true))
            })
            .map(|service| service.name.clone())
            .collect();
        let mcp_servers = self
            .services
            .mcp_servers(executable)
            .await
            .unwrap_or_default();
        let hosts = self.hosts.hosts(version).await;
        Ok(FollowUpPlan {
            services,
            restart,
            mcp_servers,
            hosts,
        })
    }

    /// Restarts what `plan` names and checks each comes back; never fails,
    /// each step that could not be done says so in the summary.
    pub async fn apply(
        &self,
        plan: &FollowUpPlan,
        executable: &Path,
        version: &str,
    ) -> UpdateSummary {
        let mut services = Vec::new();
        for service in &plan.services {
            services.push(
                self.service_report(plan, service, executable, version)
                    .await,
            );
        }
        let mcp_servers = match self.services.mcp_servers(executable).await {
            Ok(found) => found,
            Err(_) => plan.mcp_servers.clone(),
        };
        UpdateSummary {
            version: version.to_owned(),
            services,
            mcp_servers,
            hosts: plan.hosts.clone(),
        }
    }

    async fn service_report(
        &self,
        plan: &FollowUpPlan,
        service: &LocalUserService,
        executable: &Path,
        version: &str,
    ) -> ServiceReport {
        let mut next = Vec::new();
        let outcome = if !service.runs_mm() {
            ServiceOutcome::NotMm
        } else if !starts(service, executable) {
            if let Some(program) = &service.program {
                next.push(format!("{} update", program.display()));
            }
            ServiceOutcome::OtherMm
        } else if !service.running {
            ServiceOutcome::Stopped
        } else if plan.restart.contains(&service.name) {
            self.restart(service).await
        } else {
            ServiceOutcome::Current
        };
        match &outcome {
            ServiceOutcome::RestartFailed { .. } => next.push(service.restart_command()),
            ServiceOutcome::Unhealthy => next.extend(service.inspect_commands()),
            _ => {}
        }
        let version = matches!(outcome, ServiceOutcome::Restarted | ServiceOutcome::Current)
            .then(|| version.to_owned());
        ServiceReport {
            name: service.name.clone(),
            program: service.program.clone(),
            outcome,
            version,
            next,
        }
    }

    async fn restart(&self, service: &LocalUserService) -> ServiceOutcome {
        if let Err(error) = self.services.restart(service).await {
            return ServiceOutcome::RestartFailed {
                problem: error.to_string(),
            };
        }
        let mut steady = 0;
        for _ in 0..self.checks {
            tokio::time::sleep(self.settle).await;
            if matches!(self.services.running(service).await, Ok(true)) {
                steady += 1;
                if steady >= STEADY {
                    return ServiceOutcome::Restarted;
                }
            } else {
                steady = 0;
            }
        }
        ServiceOutcome::Unhealthy
    }
}

fn starts(service: &LocalUserService, executable: &Path) -> bool {
    service.runs_mm() && service.program.as_deref() == Some(executable)
}
