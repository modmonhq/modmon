//! The proxy's Compose project on an ingress node: written from the tables
//! the node serves, started, and attached to every environment's network.

use std::time::Duration;

use super::{IngressKeeper, PROXY_CONTAINER, START_PATIENCE};
use crate::adapters::node_files::{directory, read_whole, write_whole};
use crate::domain::cluster_ingress::{
    ACME_DIRECTORY, ACME_FILE, Backends, CERTIFICATES_DIRECTORY, COMPOSE_FILE, DYNAMIC_DIRECTORY,
    DYNAMIC_FILE, IngressTable, TABLES_DIRECTORY, proxy_compose, render_dynamic,
};
use crate::domain::error::MmError;

impl IngressKeeper {
    /// Writes the proxy's Compose file for the tables the node serves and
    /// `adding`, runs it when `start` asks or the file changed, attaches the
    /// proxy to every environment's network, and waits until it answers.
    pub(super) async fn converge(
        &self,
        adding: Option<(&str, &IngressTable)>,
        start: bool,
    ) -> Result<(), MmError> {
        let tables = self.tables()?;
        let serving = tables
            .iter()
            .map(|(name, table)| (name.as_str(), table))
            .chain(adding)
            .collect::<Vec<_>>();
        let (compose, changed) = self.prepare(&serving)?;
        if start || changed {
            self.engine(
                &self.compose_args(&compose, &["up", "--detach", "--remove-orphans"]),
                START_PATIENCE,
            )
            .await?;
        }
        let mut networks = serving
            .iter()
            .filter_map(|(_, table)| table.engine_network.as_deref())
            .collect::<Vec<_>>();
        networks.sort_unstable();
        networks.dedup();
        self.attach(&networks).await?;
        let deadline = tokio::time::Instant::now() + START_PATIENCE;
        while !self.answers("/ping").await {
            if tokio::time::Instant::now() >= deadline {
                return Err(MmError::RuntimeResponse {
                    message: format!(
                        "the ingress proxy does not answer on 127.0.0.1:{} after it started; {} on the node says why",
                        self.options.check_port,
                        self.options.proxy_log()
                    ),
                });
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Ok(())
    }

    async fn attach(&self, networks: &[&str]) -> Result<(), MmError> {
        if networks.is_empty() {
            return Ok(());
        }
        let attached = self
            .engine(
                &[
                    "inspect",
                    "--type",
                    "container",
                    "--format",
                    "{{range $name, $value := .NetworkSettings.Networks}}{{$name}} {{end}}",
                    PROXY_CONTAINER,
                ],
                Duration::from_secs(30),
            )
            .await?;
        for network in networks {
            if !attached.split_whitespace().any(|name| name == *network) {
                self.engine(
                    &["network", "connect", network, PROXY_CONTAINER],
                    Duration::from_secs(60),
                )
                .await?;
            }
        }
        Ok(())
    }

    /// The directories and files the proxy mounts, made before it starts,
    /// and its Compose file; says whether that file changed.
    fn prepare(&self, serving: &[(&str, &IngressTable)]) -> Result<(String, bool), MmError> {
        let root = &self.options.root;
        directory(root, 0o755)?;
        directory(&root.join(TABLES_DIRECTORY), 0o700)?;
        directory(&root.join(DYNAMIC_DIRECTORY), 0o755)?;
        directory(&root.join(CERTIFICATES_DIRECTORY), 0o700)?;
        directory(&root.join(ACME_DIRECTORY), 0o700)?;
        let acme = root.join(ACME_DIRECTORY).join(ACME_FILE);
        if read_whole(&acme)?.is_none() {
            write_whole(&acme, b"", 0o600)?;
        }
        let dynamic = root.join(DYNAMIC_DIRECTORY).join(DYNAMIC_FILE);
        if read_whole(&dynamic)?.is_none() {
            let empty = render_dynamic(&[], &Backends::new(), 0)
                .map_err(|message| MmError::State { message })?;
            write_whole(&dynamic, empty.as_bytes(), 0o644)?;
        }
        let compose = root.join(COMPOSE_FILE);
        let text = proxy_compose(&root.to_string_lossy(), &self.options.image, serving)
            .map_err(|message| MmError::State { message })?;
        let changed = read_whole(&compose)?.as_deref() != Some(text.as_bytes());
        if changed {
            write_whole(&compose, text.as_bytes(), 0o644)?;
        }
        Ok((compose.to_string_lossy().into_owned(), changed))
    }
}
