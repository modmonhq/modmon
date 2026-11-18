use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicIngressStatus {
    pub installed: bool,
    pub running: bool,
    pub failed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicIngressState {
    Running,
    Stopped,
}

impl PublicIngressState {
    pub const fn command(self) -> &'static str {
        match self {
            Self::Running => "start",
            Self::Stopped => "stop",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }
}
