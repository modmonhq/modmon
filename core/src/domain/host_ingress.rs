use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PublicIngressStatus {
    pub installed: bool,
    pub running: bool,
    pub failed: bool,
    pub disabled: bool,
}

impl PublicIngressStatus {
    /// Stopped with `mm ingress stop`: off at boot, and deploys leave it off.
    #[must_use]
    pub const fn kept_stopped(self) -> bool {
        self.installed && self.disabled && !self.running
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicIngressState {
    Running,
    Stopped,
}

impl PublicIngressState {
    pub const fn description(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }
}
