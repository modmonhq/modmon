//! Where a node serves its metrics, decided again every few seconds from
//! the host policy, the cluster networks the node holds and what its
//! deployments ask for (`domain::node_metrics::listeners`).

use std::collections::BTreeSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use zeroize::Zeroizing;

use super::server::MetricsServer;
use crate::app::NodeMetricsCollector;
use crate::app::ports::{ClusterNetworkHost, MeshHost};
use crate::domain::host_policy::HostMetrics;
use crate::domain::node_metrics::{METRICS_PORT, MetricsListener, listeners};

/// How often the node looks again where it should serve.
const REFRESH: Duration = Duration::from_secs(10);
const MAX_TOKEN_BYTES: u64 = 4096;

/// What decides where the node serves.
pub struct EndpointSources {
    /// `/etc/mm/host.yaml`.
    pub host_policy: PathBuf,
    pub networks: Arc<dyn ClusterNetworkHost>,
    pub mesh: Arc<dyn MeshHost>,
}

pub struct MetricsEndpoint {
    collector: Arc<NodeMetricsCollector>,
    server: Arc<MetricsServer>,
    sources: EndpointSources,
    policy: Mutex<Option<(SystemTime, u64, HostMetrics)>>,
    problems: Mutex<BTreeSet<String>>,
}

impl std::fmt::Debug for MetricsEndpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MetricsEndpoint")
            .field("host_policy", &self.sources.host_policy)
            .finish_non_exhaustive()
    }
}

impl MetricsEndpoint {
    #[must_use]
    pub fn new(collector: Arc<NodeMetricsCollector>, sources: EndpointSources) -> Arc<Self> {
        Arc::new(Self {
            server: MetricsServer::new(Arc::clone(&collector), METRICS_PORT),
            collector,
            sources,
            policy: Mutex::new(None),
            problems: Mutex::new(BTreeSet::new()),
        })
    }

    /// Serves until the process ends, looking every few seconds where.
    pub async fn run(self: Arc<Self>) {
        loop {
            let mut problems = Vec::new();
            if let Some(wanted) = self.wanted(&mut problems).await {
                problems.extend(self.server.serve(wanted));
            }
            self.report(problems);
            tokio::time::sleep(REFRESH).await;
        }
    }

    /// Warns about each problem once, until it goes away.
    fn report(&self, problems: Vec<String>) {
        let current = problems.into_iter().collect::<BTreeSet<_>>();
        let mut known = self.problems.lock().unwrap_or_else(PoisonError::into_inner);
        for problem in current.difference(&known) {
            tracing::warn!("{problem}");
        }
        *known = current;
    }

    fn host_metrics(&self, problems: &mut Vec<String>) -> Option<HostMetrics> {
        let path = &self.sources.host_policy;
        let metadata = std::fs::metadata(path).ok()?;
        let stamp = (
            metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            metadata.len(),
        );
        let mut known = self.policy.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((modified, length, policy)) = known.as_ref()
            && (*modified, *length) == stamp
        {
            return Some(policy.clone());
        }
        match crate::adapters::remote::load_host_policy(path) {
            Ok((host, _)) => {
                let policy = host.metrics.unwrap_or_default();
                *known = Some((stamp.0, stamp.1, policy.clone()));
                Some(policy)
            }
            Err(error) => {
                problems.push(format!(
                    "metrics: cannot read {}: {error}; the node serves no metrics until it can",
                    path.display()
                ));
                *known = None;
                None
            }
        }
    }

    async fn wanted(&self, problems: &mut Vec<String>) -> Option<Vec<MetricsListener>> {
        let Some(policy) = self.host_metrics(problems) else {
            return Some(Vec::new());
        };
        let read = |key: &str, path: Option<&PathBuf>| match path {
            Some(path) => read_token(key, path).map(Some),
            None => Ok(None),
        };
        let tokens = read("token-file", policy.token_file.as_ref()).and_then(|project| {
            let mesh = if policy.mesh {
                read("mesh-token-file", policy.mesh_token_file.as_ref())?
            } else {
                None
            };
            Ok((project, mesh))
        });
        match tokens {
            Ok((project, mesh)) => self.server.set_tokens(project, mesh),
            Err(problem) => {
                problems.push(format!(
                    "metrics: {problem}; the node serves no metrics until it can"
                ));
                return Some(Vec::new());
            }
        }
        if !policy.enabled {
            return Some(Vec::new());
        }
        let deployed = self.collector.deployed().await?;
        let networks = self.sources.networks.stored().ok().flatten();
        let mesh = self.sources.mesh.stored().ok().flatten();
        Some(listeners(
            &policy,
            networks.as_ref(),
            &deployed,
            mesh.as_ref(),
        ))
    }
}

/// The token in `path`, the file of the host policy's `metrics.<key>`: a
/// regular file only root can read, of at most a few kilobytes, opened
/// without following a link. Never says what it holds.
fn read_token(key: &str, path: &Path) -> Result<Zeroizing<Vec<u8>>, String> {
    use rustix::fs::{FileType, Mode, OFlags};
    let shown = path.display();
    let file = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| {
        format!(
            "cannot open metrics.{key} {shown} ({error}); it must be a regular file, not a link"
        )
    })?;
    let stat = rustix::fs::fstat(&file)
        .map_err(|error| format!("cannot read metrics.{key} {shown} ({error})"))?;
    let size = u64::try_from(stat.st_size).unwrap_or(u64::MAX);
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile || size > MAX_TOKEN_BYTES {
        return Err(format!(
            "metrics.{key} {shown} must be a regular file of at most {MAX_TOKEN_BYTES} bytes"
        ));
    }
    if stat.st_uid != rustix::process::getuid().as_raw()
        || Mode::from_raw_mode(stat.st_mode).intersects(Mode::RWXG | Mode::RWXO)
    {
        return Err(format!(
            "metrics.{key} {shown} must belong to root and be readable by root only: chown root: {shown} && chmod 600 {shown}"
        ));
    }
    let mut content = Zeroizing::new(Vec::new());
    std::fs::File::from(file)
        .take(MAX_TOKEN_BYTES)
        .read_to_end(&mut content)
        .map_err(|error| format!("cannot read metrics.{key} {shown} ({error})"))?;
    let token = Zeroizing::new(content.trim_ascii().to_vec());
    if token.is_empty() {
        return Err(format!("metrics.{key} {shown} is empty"));
    }
    Ok(token)
}
