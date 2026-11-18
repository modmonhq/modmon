//! Chooses the workstation's host integration: systemd on Linux, launchd user
//! agents with Docker Desktop on macOS. Remote targets are planned as Linux.
use super::*;
use crate::app::ports::{
    DashboardServiceInstaller, PrivilegedIngressInstaller, PublicIngressControl,
    RootlessIngressInstaller,
};
use crate::domain::target_ingress::IngressHostProfile;

pub(crate) fn local_ingress_host_profile() -> IngressHostProfile {
    if cfg!(target_os = "macos") {
        crate::adapters::macos_host::probe_local_ingress_host()
    } else {
        IngressHostProfile::LinuxSystemd
    }
}

pub(crate) fn target_ingress_manager(
    home: &MmHome,
    targets: Arc<dyn TargetStore>,
) -> TargetIngressManager {
    TargetIngressManager::new(
        targets,
        Arc::new(FilesystemTargetIngressArtifactStore::new(
            home.ingress_directory.clone(),
        )),
    )
    .with_local_host_profile(local_ingress_host_profile())
}

pub(crate) fn rootless_ingress_installer(
    process: Arc<dyn ProcessRunner>,
) -> Result<Arc<dyn RootlessIngressInstaller>, MmError> {
    if cfg!(target_os = "macos") {
        Ok(Arc::new(
            crate::adapters::target::LaunchdAutostartInstaller::new(process)?,
        ))
    } else {
        Ok(Arc::new(SystemdRootlessIngressInstaller::new(process)?))
    }
}

pub(crate) fn privileged_ingress_installer(
    process: Arc<dyn ProcessRunner>,
) -> Result<Arc<dyn PrivilegedIngressInstaller>, MmError> {
    if cfg!(target_os = "macos") {
        Ok(Arc::new(
            crate::adapters::target::LaunchdForwarderInstaller::new(process)?,
        ))
    } else {
        Ok(Arc::new(SudoPrivilegedIngressInstaller::new(process)?))
    }
}

pub(crate) fn dashboard_service_installer(
    process: Arc<dyn ProcessRunner>,
) -> Result<Arc<dyn DashboardServiceInstaller>, MmError> {
    if cfg!(target_os = "macos") {
        Ok(Arc::new(
            crate::adapters::dashboard_service::LaunchdDashboardServiceInstaller::new(process)?,
        ))
    } else {
        Ok(Arc::new(SystemdDashboardServiceInstaller::new(process)?))
    }
}

pub(crate) fn public_ingress_control(
    process: Arc<dyn ProcessRunner>,
) -> Result<Box<dyn PublicIngressControl>, MmError> {
    if cfg!(target_os = "macos") {
        Ok(Box::new(
            crate::adapters::target::LaunchdPublicIngressControl::new(process)?,
        ))
    } else {
        Ok(Box::new(
            crate::adapters::target::SystemdPublicIngressControl::new(process),
        ))
    }
}
