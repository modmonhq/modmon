//! The OCI runtimes of a remote host: what its policy offers, and whether
//! its engine really has them.

use std::path::Path;

use futures_util::future::join_all;

use crate::domain::error::MmError;
use crate::domain::hardening::{HostRuntimeStatus, HostRuntimes, RuntimeCheck};

/// What the host policy at `path` says about runtimes; `None` where no
/// policy applies, such as the machine `mm` runs on.
pub(super) fn host_runtimes(path: Option<&Path>) -> Result<Option<HostRuntimes>, MmError> {
    let Some(path) = path else {
        return Ok(None);
    };
    let (policy, _) = crate::adapters::remote::load_host_policy(path)?;
    Ok(Some(policy.host_runtimes()))
}

/// Tries each runtime the host policy at `path` offers on the `runtime`
/// engine, all at once.
pub(super) async fn check(
    runtime: crate::domain::runtime::RuntimeKind,
    path: Option<&Path>,
) -> Result<HostRuntimeStatus, MmError> {
    let Some(host) = host_runtimes(path)? else {
        return Ok(HostRuntimeStatus::default());
    };
    let driver = super::RuntimeComposition::new().driver(runtime);
    let runtimes = join_all(host.advertised.iter().map(|name| {
        let driver = driver.clone();
        async move {
            let problem = driver.check_oci_runtime(name).await.err();
            RuntimeCheck {
                name: name.clone(),
                works: problem.is_none(),
                problem,
            }
        }
    }))
    .await;
    Ok(HostRuntimeStatus {
        runtimes,
        exposed_runtime: host.exposed,
    })
}
