use std::collections::BTreeSet;

use tokio::sync::broadcast;

use crate::domain::dashboard::DashboardEvent;
use crate::domain::error::MmError;
use crate::domain::service_stack::ServiceStackStatus;

#[async_trait::async_trait]
pub trait InstallationStatusQuery: Send + Sync {
    async fn installation_status(&self) -> Result<ServiceStackStatus, MmError>;
}

pub trait DashboardSubscription: Send + Sync {
    fn connected_event(&self) -> DashboardEvent;
    fn subscribe(&self) -> broadcast::Receiver<DashboardEvent>;
}

pub trait DashboardHostSource: Send + Sync {
    fn project_dashboard_hosts(&self) -> Result<BTreeSet<String>, MmError>;
}
