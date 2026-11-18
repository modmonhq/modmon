//! What a host says about OCI runtimes in its policy.

use super::HostPolicy;
use crate::domain::hardening::{HostRuntimes, validate_runtime_name};

impl HostPolicy {
    pub(super) fn validate_runtimes(&self) -> Result<(), String> {
        for runtime in &self.runtimes {
            validate_runtime_name(runtime).map_err(|problem| format!("runtimes: {problem}"))?;
        }
        match &self.exposed_runtime {
            Some(runtime) if !self.runtimes.contains(runtime) => Err(format!(
                "exposed-runtime '{runtime}' is not one of `runtimes`; add it there: `runtimes: [{runtime}]`"
            )),
            _ => Ok(()),
        }
    }

    /// The runtimes a plan made on this host may use and has to use.
    #[must_use]
    pub fn host_runtimes(&self) -> HostRuntimes {
        HostRuntimes {
            advertised: self.runtimes.clone(),
            exposed: self.exposed_runtime.clone(),
        }
    }
}
