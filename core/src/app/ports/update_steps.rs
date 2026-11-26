use std::path::Path;

use async_trait::async_trait;

use crate::domain::error::MmError;
use crate::domain::update_steps::{HostsView, LocalUserService, McpServerProcess};

/// The user services mm installs on this machine, and this user's `mm`
/// processes. `executable` is the `mm` on disk that processes are compared
/// with.
#[async_trait]
pub trait LocalUserServices: Send + Sync {
    async fn services(&self, executable: &Path) -> Result<Vec<LocalUserService>, MmError>;
    async fn restart(&self, service: &LocalUserService) -> Result<(), MmError>;
    async fn running(&self, service: &LocalUserService) -> Result<bool, MmError>;
    async fn mcp_servers(&self, executable: &Path) -> Result<Vec<McpServerProcess>, MmError>;
}

/// The release each node of the project's remote environments runs.
#[async_trait]
pub trait HostVersions: Send + Sync {
    async fn hosts(&self, version: &str) -> HostsView;
}
