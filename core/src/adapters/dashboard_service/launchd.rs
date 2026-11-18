//! The dashboard control API as a launchd user agent on macOS.
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;

use super::{read_existing, verified_unit};
use crate::adapters::launchd::{
    AgentFile, AgentInstallation, Launchctl, LaunchdJobState, check_agent_owner,
    ensure_log_directory, install_agent, uninstall_agent,
};
use crate::app::ports::{DashboardServiceInstaller, DashboardServiceStatus, ProcessRunner};
use crate::domain::error::MmError;
use crate::domain::service_stack::ServiceStackPlan;
use crate::domain::target_ingress::{LAUNCHD_DASHBOARD_LABEL, MACOS_LAUNCH_AGENTS_DIRECTORY};

const OWNER_FILE: &str = "Library/Application Support/mm/dashboard/local.mm.dashboard.owner.json";

#[derive(Clone)]
pub struct LaunchdDashboardServiceInstaller {
    launchctl: Launchctl,
    home: PathBuf,
}

impl std::fmt::Debug for LaunchdDashboardServiceInstaller {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LaunchdDashboardServiceInstaller")
            .field("home", &self.home)
            .finish_non_exhaustive()
    }
}

impl LaunchdDashboardServiceInstaller {
    pub fn new(process: Arc<dyn ProcessRunner>) -> Result<Self, MmError> {
        let home = directories::BaseDirs::new()
            .map(|directories| directories.home_dir().to_path_buf())
            .ok_or_else(|| MmError::State {
                message: "could not resolve the current user's home directory".to_owned(),
            })?;
        Ok(Self {
            launchctl: Launchctl::current_user(process),
            home,
        })
    }

    #[cfg(test)]
    pub(super) fn with_home(process: Arc<dyn ProcessRunner>, home: PathBuf) -> Self {
        Self {
            launchctl: Launchctl::current_user(process),
            home,
        }
    }

    fn plist_path(&self) -> PathBuf {
        self.home
            .join(MACOS_LAUNCH_AGENTS_DIRECTORY)
            .join(format!("{LAUNCHD_DASHBOARD_LABEL}.plist"))
    }

    fn installation(&self, plan: &ServiceStackPlan, plist: Vec<u8>) -> AgentInstallation {
        AgentInstallation {
            label: LAUNCHD_DASHBOARD_LABEL.to_owned(),
            target_id: plan.target_id,
            artifact_hash: plan.plan_hash.clone(),
            owner_path: self.home.join(OWNER_FILE),
            plist: AgentFile {
                destination: self.plist_path(),
                bytes: plist,
                mode: 0o644,
            },
            files: Vec::new(),
        }
    }

    /// Removal needs only the fixed paths, not a reviewed plist.
    fn removal(&self, plan: &ServiceStackPlan) -> AgentInstallation {
        self.installation(plan, Vec::new())
    }

    fn require_macos_plan(plan: &ServiceStackPlan) -> Result<(), MmError> {
        if plan.ingress.manifest.macos_host.is_some() {
            Ok(())
        } else {
            Err(MmError::Security {
                message: "the launchd dashboard service requires a macOS service plan".to_owned(),
            })
        }
    }
}

#[async_trait]
impl DashboardServiceInstaller for LaunchdDashboardServiceInstaller {
    async fn preflight(&self, plan: &ServiceStackPlan) -> Result<(), MmError> {
        Self::require_macos_plan(plan)?;
        verified_unit(plan)?;
        check_agent_owner(&self.removal(plan))
    }

    async fn install(&self, plan: &ServiceStackPlan) -> Result<(), MmError> {
        Self::require_macos_plan(plan)?;
        let Some(plist) = verified_unit(plan)? else {
            return uninstall_agent(&self.launchctl, &self.removal(plan)).await;
        };
        ensure_log_directory(&self.home)?;
        install_agent(&self.launchctl, &self.installation(plan, plist))
            .await
            .map(drop)
    }

    async fn status(&self, plan: &ServiceStackPlan) -> Result<DashboardServiceStatus, MmError> {
        Self::require_macos_plan(plan)?;
        let expected = verified_unit(plan)?;
        let installed = read_existing(&self.plist_path())?;
        if installed.is_none() {
            return Ok(DashboardServiceStatus {
                installed: false,
                running: false,
            });
        }
        check_agent_owner(&self.removal(plan))?;
        if read_existing(&self.home.join(OWNER_FILE))?.is_none() {
            return Err(MmError::Security {
                message: "dashboard launchd agent has no ownership record".to_owned(),
            });
        }
        if expected.is_some_and(|expected| installed.as_deref() != Some(expected.as_slice())) {
            return Err(MmError::Security {
                message: "dashboard service installation differs from its reviewed plan".to_owned(),
            });
        }
        Ok(DashboardServiceStatus {
            installed: true,
            running: self.launchctl.state(LAUNCHD_DASHBOARD_LABEL).await?
                == LaunchdJobState::Running,
        })
    }

    async fn uninstall(&self, plan: &ServiceStackPlan) -> Result<(), MmError> {
        plan.validate()
            .map_err(|message| MmError::Security { message })?;
        uninstall_agent(&self.launchctl, &self.removal(plan)).await
    }
}
