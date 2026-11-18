//! A host that leaves ACME leaves the proxy's ACME file
//! (`domain::cluster_ingress::acme`): the proxy keeps every certificate of
//! that file beside the provided ones and serves either for a host both
//! cover, so the environment's own certificate would lose now and then.

use std::time::Duration;

use super::{IngressOptions, START_PATIENCE, answers, read_tables};
use crate::adapters::node_files::{engine, engine_program, failure, read_whole, write_whole};
use crate::app::ports::ProcessRunner;
use crate::domain::cluster_ingress::{
    ACME_DIRECTORY, ACME_FILE, COMPOSE_FILE, DYNAMIC_DIRECTORY, DYNAMIC_FILE, PROXY_PROJECT,
    forget_hosts, loaded_marker, loaded_path, provided_hosts,
};
use crate::domain::error::MmError;

/// Takes the certificates of every host whose environment provides its
/// certificate out of the proxy's ACME file, and restarts the proxy so
/// that it forgets them; returns their domains. The proxy reads that file
/// only when it starts and writes all of it whenever it gets a
/// certificate, so it is stopped first. With nothing to take out, nothing
/// restarts: this happens once, when a host moves off ACME.
pub async fn forget_provided_certificates(
    options: &IngressOptions,
    process: &dyn ProcessRunner,
) -> Result<Vec<String>, MmError> {
    let hosts = provided_hosts(&read_tables(&options.root)?);
    let acme = options.root.join(ACME_DIRECTORY).join(ACME_FILE);
    let Some(stored) = read_whole(&acme)? else {
        return Ok(Vec::new());
    };
    let forgotten = match forget_hosts(&stored, &hosts) {
        Ok(Some(forgotten)) => forgotten,
        Ok(None) => return Ok(Vec::new()),
        // The file is the proxy's: one it cannot read either leaves it
        // without ACME certificates, which is not a reason to refuse the
        // routes of every environment on the node.
        Err(message) => {
            tracing::warn!(
                file = %acme.display(),
                "{message}; mm leaves it as it is, and the proxy asks for its ACME certificates again once the file is removed"
            );
            return Ok(Vec::new());
        }
    };
    tracing::info!(
        domains = ?forgotten.domains,
        "the ingress proxy restarts to forget the ACME certificates of hosts that provide their own"
    );
    let compose = options.root.join(COMPOSE_FILE);
    let compose = compose.to_string_lossy();
    run_compose(options, process, &compose, &["stop"]).await?;
    let written = write_whole(&acme, &forgotten.file, 0o600);
    // Started again whatever the write did: the proxy serves every
    // environment on the node.
    let started = run_compose(
        options,
        process,
        &compose,
        &["up", "--detach", "--remove-orphans"],
    )
    .await;
    written?;
    started?;
    // Running again, and on the file the keeper wrote last.
    let dynamic = read_whole(&options.root.join(DYNAMIC_DIRECTORY).join(DYNAMIC_FILE))?;
    let loaded = dynamic
        .as_deref()
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(loaded_marker)
        .map_or_else(|| "/ping".to_owned(), loaded_path);
    let deadline = tokio::time::Instant::now() + START_PATIENCE;
    while !answers(options.check_port, &loaded).await {
        if tokio::time::Instant::now() >= deadline {
            return Err(MmError::RuntimeResponse {
                message: format!(
                    "the ingress proxy does not answer on 127.0.0.1:{} after it restarted to forget the ACME certificates of {}; {} on the node says why",
                    options.check_port,
                    forgotten.domains.join(", "),
                    options.proxy_log()
                ),
            });
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(forgotten.domains)
}

async fn run_compose(
    options: &IngressOptions,
    process: &dyn ProcessRunner,
    compose: &str,
    rest: &[&str],
) -> Result<(), MmError> {
    let mut args = vec![
        "compose",
        "--project-name",
        PROXY_PROJECT,
        "--file",
        compose,
    ];
    args.extend_from_slice(rest);
    let output = engine(process, options.engine, &args, START_PATIENCE).await?;
    if output.success() {
        return Ok(());
    }
    Err(MmError::RuntimeResponse {
        message: format!(
            "`{} {}` failed: {}",
            engine_program(options.engine),
            args.join(" "),
            failure(&output)
        ),
    })
}
