//! The keeper of leaving containers on a cluster node (`domain::leaving`).
//!
//! Each leave is a file under `/var/lib/mm/leaving`, named by the
//! container's id. The keeper reads them twice a second and moves each one
//! on: it has the node's resolver leave the container out and waits until
//! every node confirms, then lets the drain run out, then stops the
//! container (its stop signal, then its `stop_grace_period`) and removes
//! it. A rollout asks for a leave and waits for it; `mmd container leave`
//! only writes the file. Because the stage is in the file, an `mmd` that
//! restarts goes on where it was.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;

use crate::adapters::discovery::NameResolver;
use crate::adapters::node_files::{
    engine, engine_program, failure, names, no_such_container, now_unix_ms, read_whole, remove,
    write_whole,
};
use crate::app::ports::{ContainerLeaving, ProcessRunner};
use crate::domain::error::MmError;
use crate::domain::leaving::{LeaveRecord, LeaveStage};
use crate::domain::runtime::RuntimeKind;

/// Where a node keeps its leaves, under the daemon's state directory.
pub const LEAVING_DIRECTORY: &str = "leaving";

/// How often the keeper looks at the leaves.
const LOOK: Duration = Duration::from_millis(500);
/// How long the engine gets to answer about a container.
const ENGINE: Duration = Duration::from_secs(30);
/// How long a stop may take: the container's own grace period, at most the
/// engine's longest, and the engine's work around it.
const STOP: Duration = Duration::from_secs(15 * 60);

pub struct LeaveKeeper {
    directory: PathBuf,
    names: Arc<NameResolver>,
    process: Arc<dyn ProcessRunner>,
    /// The engine setup installed on the node.
    engine: RuntimeKind,
    changed: Notify,
    /// Held while one leave's file changes, and while leaves are taken
    /// back.
    turn: tokio::sync::Mutex<()>,
    /// The ids of the leaves whose stop has begun: a leave is never taken
    /// back half way through its stop.
    stopping: std::sync::Mutex<std::collections::BTreeSet<String>>,
}

impl std::fmt::Debug for LeaveKeeper {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LeaveKeeper")
            .field("directory", &self.directory)
            .finish_non_exhaustive()
    }
}

impl LeaveKeeper {
    #[must_use]
    pub fn new(
        state_root: &Path,
        names: Arc<NameResolver>,
        process: Arc<dyn ProcessRunner>,
        engine: RuntimeKind,
    ) -> Arc<Self> {
        Arc::new(Self {
            directory: state_root.join(LEAVING_DIRECTORY),
            names,
            process,
            engine,
            changed: Notify::new(),
            turn: tokio::sync::Mutex::new(()),
            stopping: std::sync::Mutex::default(),
        })
    }

    /// Leaves out every container that is leaving: called before the
    /// resolver answers anyone, so a restart never brings one back.
    pub fn restore(&self) -> Result<usize, MmError> {
        let records = self.records()?;
        let ids = records
            .iter()
            .map(|record| record.id.clone())
            .collect::<Vec<_>>();
        self.names.mark_leaving(&ids);
        Ok(records.len())
    }

    /// Moves every leave on until the process ends. Leaves stop side by
    /// side.
    pub async fn run(self: Arc<Self>) {
        loop {
            match self.records() {
                Ok(records) => {
                    for record in records {
                        if self.is_stopping(&record.id) {
                            continue;
                        }
                        let turn = self.turn.lock().await;
                        // Taken back meanwhile.
                        if !self.directory.join(record.file_name()).exists() {
                            continue;
                        }
                        if record.stage(now_unix_ms()) == LeaveStage::Stopping {
                            self.mark_stopping(&record.id, true);
                            drop(turn);
                            tokio::spawn(Arc::clone(&self).finish(record));
                            continue;
                        }
                        if let Err(error) = self.advance(record) {
                            tracing::warn!(%error, "a leaving container could not go on");
                        }
                    }
                }
                Err(error) => tracing::warn!(%error, "cannot read the leaving containers"),
            }
            // The next look comes soon; a new leave comes at once.
            tokio::select! {
                () = self.changed.notified() => {}
                () = tokio::time::sleep(LOOK) => {}
            }
        }
    }

    /// One look at one leave that is not stopping yet.
    fn advance(&self, mut record: LeaveRecord) -> Result<(), MmError> {
        self.names.mark_leaving(std::slice::from_ref(&record.id));
        let now = now_unix_ms();
        match record.stage(now) {
            LeaveStage::Marked if self.names.leave_confirmed(&record.id) => {
                tracing::info!(container = %record.name, "every node confirmed that the container is leaving; it drains now");
                record.confirmed_unix_ms = Some(now);
                self.save(&record)
            }
            LeaveStage::Marked | LeaveStage::Draining { .. } | LeaveStage::Stopping => Ok(()),
        }
    }

    /// Stops and removes (or restarts) a drained container, and ends its
    /// leave. One that cannot stop is tried again at the next look.
    async fn finish(self: Arc<Self>, record: LeaveRecord) {
        self.names.mark_leaving(std::slice::from_ref(&record.id));
        let finished = match self.stop(&record).await {
            Ok(()) => remove(&self.directory.join(record.file_name())).map(drop),
            Err(error) => Err(error),
        };
        match finished {
            Ok(()) => {
                self.names.unmark_leaving(std::slice::from_ref(&record.id));
                tracing::info!(container = %record.name, restarted = record.restart, "the leaving container stopped and was removed or restarted");
            }
            Err(error) => tracing::warn!(%error, "a leaving container could not go on"),
        }
        self.mark_stopping(&record.id, false);
    }

    fn is_stopping(&self, id: &str) -> bool {
        self.stopping
            .lock()
            .is_ok_and(|stopping| stopping.contains(id))
    }

    fn mark_stopping(&self, id: &str, stopping: bool) {
        if let Ok(mut ids) = self.stopping.lock() {
            if stopping {
                ids.insert(id.to_owned());
            } else {
                ids.remove(id);
            }
        }
    }

    /// The container's stop signal and grace period, then its removal. A
    /// container that is gone already is done.
    async fn stop(&self, record: &LeaveRecord) -> Result<(), MmError> {
        let steps = if record.restart {
            vec![(["restart", record.id.as_str()], STOP)]
        } else {
            vec![
                (["stop", record.id.as_str()], STOP),
                (["rm", record.id.as_str()], ENGINE),
            ]
        };
        for (args, timeout) in steps {
            let output = engine(self.process.as_ref(), self.engine, &args, timeout).await?;
            if !output.success() && !no_such_container(&output) {
                return Err(MmError::RuntimeResponse {
                    message: format!(
                        "the engine did not {} leaving container {}: {}",
                        args[0],
                        record.name,
                        failure(&output)
                    ),
                });
            }
        }
        Ok(())
    }

    fn records(&self) -> Result<Vec<LeaveRecord>, MmError> {
        let mut records = Vec::new();
        for name in names(&self.directory, ".json")? {
            let path = self.directory.join(&name);
            let Some(bytes) = read_whole(&path)? else {
                continue;
            };
            match serde_json::from_slice::<LeaveRecord>(&bytes) {
                Ok(record) if record.validate().is_ok() && record.file_name() == name => {
                    records.push(record);
                }
                _ => {
                    tracing::warn!(file = %path.display(), "a leave the keeper cannot read is left alone")
                }
            }
        }
        Ok(records)
    }

    fn save(&self, record: &LeaveRecord) -> Result<(), MmError> {
        save(&self.directory, record)
    }

    /// Begins the leave of each container that is not leaving yet.
    pub async fn begin(
        &self,
        containers: &[String],
        drain: Duration,
    ) -> Result<Vec<String>, MmError> {
        self.begin_as(containers, Some(drain), false).await
    }

    /// `begin`, for containers that restart once drained (`restart`).
    /// Without `drain`, each container's own.
    async fn begin_as(
        &self,
        containers: &[String],
        drain: Option<Duration>,
        restart: bool,
    ) -> Result<Vec<String>, MmError> {
        let mut ids = Vec::with_capacity(containers.len());
        for container in containers {
            let record = request(
                &self.directory,
                (self.process.as_ref(), self.engine),
                container,
                (drain, restart),
            )
            .await?;
            ids.push(record.id);
        }
        self.changed.notify_one();
        Ok(ids)
    }

    /// Waits until every node confirmed each of `ids`, or it is gone.
    async fn confirmed(&self, ids: &[String]) {
        let pending = |id: &String| {
            read_whole(&self.directory.join(format!("{id}.json")))
                .ok()
                .flatten()
                .and_then(|bytes| serde_json::from_slice::<LeaveRecord>(&bytes).ok())
                .is_some_and(|record| record.confirmed_unix_ms.is_none())
        };
        while ids.iter().any(pending) {
            tokio::time::sleep(LOOK / 2).await;
        }
    }

    /// Waits until none of `ids` is leaving any more.
    async fn gone(&self, ids: &[String]) {
        while ids
            .iter()
            .any(|id| self.directory.join(format!("{id}.json")).exists())
        {
            tokio::time::sleep(LOOK / 2).await;
        }
    }
}

/// The ids of the containers leaving this node, for a status. A directory
/// that cannot be read lists none.
#[must_use]
pub fn leaving_ids(state_root: &Path) -> Vec<String> {
    names(&state_root.join(LEAVING_DIRECTORY), ".json")
        .unwrap_or_default()
        .into_iter()
        .map(|name| name.trim_end_matches(".json").to_owned())
        .collect()
}

/// The leaves of this node, as kept on disk. One that cannot be read is
/// left out.
#[must_use]
pub fn leaving_records(state_root: &Path) -> Vec<LeaveRecord> {
    let directory = state_root.join(LEAVING_DIRECTORY);
    leaving_ids(state_root)
        .into_iter()
        .filter_map(
            |id| match read_whole(&directory.join(format!("{id}.json"))) {
                Ok(Some(bytes)) => serde_json::from_slice::<LeaveRecord>(&bytes).ok(),
                _ => None,
            },
        )
        .collect()
}

/// Writes the leave of `container` (an id or a name) under the state
/// directory `state_root`, unless it is leaving already: what
/// `mmd container leave` does. The running `mmd` takes it from there.
/// Without `drain`, the drain its service's `drain` put on the container,
/// or the default. With `restart`, the container restarts after its drain.
pub async fn request_leave(
    state_root: &Path,
    node_engine: (&dyn ProcessRunner, RuntimeKind),
    container: &str,
    (drain, restart): (Option<Duration>, bool),
) -> Result<LeaveRecord, MmError> {
    request(
        &state_root.join(LEAVING_DIRECTORY),
        node_engine,
        container,
        (drain, restart),
    )
    .await
}

async fn request(
    directory: &Path,
    node_engine: (&dyn ProcessRunner, RuntimeKind),
    container: &str,
    (drain, restart): (Option<Duration>, bool),
) -> Result<LeaveRecord, MmError> {
    let (id, name, labelled) = identify(node_engine, container).await?;
    let drain = match drain {
        Some(drain) => drain,
        None => labelled
            .as_deref()
            .map(|spec| {
                crate::domain::leaving::drain_of(Some(&crate::domain::config::DurationSpec(
                    spec.to_owned(),
                )))
            })
            .transpose()
            .map_err(|problem| MmError::Environment {
                message: format!(
                    "container {name} carries {} {problem}",
                    crate::domain::leaving::DRAIN_LABEL
                ),
            })?
            .unwrap_or(crate::domain::leaving::DEFAULT_DRAIN),
    };
    let mut record = LeaveRecord::new(&id, &name, drain, now_unix_ms());
    record.restart = restart;
    if let Some(bytes) = read_whole(&directory.join(record.file_name()))?
        && let Ok(leaving) = serde_json::from_slice::<LeaveRecord>(&bytes)
    {
        return Ok(leaving);
    }
    save(directory, &record)?;
    Ok(record)
}

fn save(directory: &Path, record: &LeaveRecord) -> Result<(), MmError> {
    record
        .validate()
        .map_err(|message| MmError::State { message })?;
    crate::adapters::node_files::directory(directory, 0o700)?;
    let bytes = serde_json::to_vec_pretty(record).map_err(|error| MmError::State {
        message: format!("cannot write the leave of {}: {error}", record.name),
    })?;
    write_whole(&directory.join(record.file_name()), &bytes, 0o600)
}

/// The engine's whole id and name of `container` (an id or a name), and the
/// drain its service put on it.
async fn identify(
    (process, kind): (&dyn ProcessRunner, RuntimeKind),
    container: &str,
) -> Result<(String, String, Option<String>), MmError> {
    let format = format!(
        "{{{{.Id}}}} {{{{.Name}}}} {{{{with index .Config.Labels \"{}\"}}}}{{{{.}}}}{{{{end}}}}",
        crate::domain::leaving::DRAIN_LABEL
    );
    let output = engine(
        process,
        kind,
        &[
            "inspect",
            "--type",
            "container",
            "--format",
            &format,
            container,
        ],
        ENGINE,
    )
    .await?;
    let text = output.stdout.trim().to_owned();
    let mut words = text.split_whitespace();
    match (words.next(), words.next()) {
        (Some(id), Some(name)) if output.success() => Ok((
            id.to_owned(),
            name.trim_start_matches('/').to_owned(),
            words.next().map(str::to_owned),
        )),
        _ => Err(MmError::Environment {
            message: format!(
                "no container '{container}' is on this node ({}); `{} ps` lists the ones that are",
                failure(&output),
                engine_program(kind)
            ),
        }),
    }
}

impl LeaveKeeper {
    /// See `ContainerLeaving::recall`.
    async fn take_back(&self, project: &str) -> Result<Vec<String>, MmError> {
        let _turn = self.turn.lock().await;
        if self.records()?.is_empty() {
            return Ok(Vec::new());
        }
        let label = format!("label=com.docker.compose.project={project}");
        let args = [
            "ps",
            "--all",
            "--no-trunc",
            "--quiet",
            "--filter",
            label.as_str(),
        ];
        let output = engine(self.process.as_ref(), self.engine, &args, ENGINE).await?;
        if !output.success() {
            return Err(MmError::RuntimeResponse {
                message: format!(
                    "the engine did not list the containers of {project}: {}",
                    failure(&output)
                ),
            });
        }
        let ids = output
            .stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        while ids.iter().any(|id| self.is_stopping(id)) {
            tokio::time::sleep(LOOK / 4).await;
        }
        let records = self.records()?;
        let mut recalled = Vec::new();
        for record in records
            .into_iter()
            .filter(|record| ids.contains(&record.id.as_str()))
        {
            remove(&self.directory.join(record.file_name()))?;
            self.names.recall_leaving(std::slice::from_ref(&record.id));
            tracing::info!(container = %record.name, "a rollback took the leaving container back; it is routed to again");
            recalled.push(record.name);
        }
        Ok(recalled)
    }
}

#[async_trait::async_trait]
impl ContainerLeaving for LeaveKeeper {
    async fn recall(&self, project: &str) -> Result<Vec<String>, MmError> {
        self.take_back(project).await
    }

    async fn leave(&self, containers: &[String], drain: Duration) -> Result<(), MmError> {
        let ids = self.begin(containers, drain).await?;
        self.gone(&ids).await;
        Ok(())
    }

    async fn leave_confirmed(&self, containers: &[String], drain: Duration) -> Result<(), MmError> {
        let ids = self.begin(containers, drain).await?;
        self.confirmed(&ids).await;
        Ok(())
    }

    async fn restart(&self, containers: &[String], drain: Option<Duration>) -> Result<(), MmError> {
        let ids = self.begin_as(containers, drain, true).await?;
        self.gone(&ids).await;
        Ok(())
    }

    fn leaving_now(&self) -> Result<Vec<String>, MmError> {
        Ok(self
            .records()?
            .into_iter()
            .filter(|record| !record.restart)
            .map(|record| record.id)
            .collect())
    }
}
