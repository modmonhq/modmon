//! One log stream from several: the stacks of an environment on one host,
//! or the nodes of a cluster.
//!
//! Every source is opened at once and read as it arrives; the lines go out
//! in the order they were written (`LogMerge`). A source that cannot be
//! opened or breaks off does not end the read when it is a node of a
//! cluster: the stream says so in one line and goes on with the others. A
//! node that stays silent is waited for only so long.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::app::ports::LogStream;
use crate::domain::error::MmError;
use crate::domain::observability::{LogMerge, LogRecord, OperationCancellation};
use crate::domain::runtime::ServiceIdentity;

/// How often a merged read checks whether it was cancelled.
const CANCEL_CHECK: Duration = Duration::from_millis(100);

/// A source's stream, once it could be opened.
pub(crate) type Opening = Pin<Box<dyn Future<Output = Result<LogStream, MmError>> + Send>>;

/// One source of a merged read.
pub(crate) struct LogSourceRead {
    /// The cluster node the source is; `None` for a source on this host.
    pub node: Option<String>,
    pub opening: Opening,
}

/// What to merge, and how patiently.
pub(crate) struct MergedRead {
    pub sources: Vec<LogSourceRead>,
    pub follow: bool,
    pub tail: u32,
    /// How long a followed line waits for an older one from another source.
    pub hold: Duration,
    /// How long the sources that have not answered are waited for once
    /// the first one has. `None` waits for every source.
    pub patience: Option<Duration>,
    /// What the lines mm adds about a node are filed under.
    pub identity: ServiceIdentity,
    pub buffered_records: usize,
}

enum Arrival {
    Opened(usize),
    Line(usize, LogRecord),
    Ended(usize, Option<MmError>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceState {
    Pending,
    Open,
    /// Ended, failed, or given up on.
    Done,
}

/// Starts reading every source and returns the merged stream.
pub(crate) fn merged(read: MergedRead, cancellation: OperationCancellation) -> LogStream {
    let (sender, receiver) = tokio::sync::mpsc::channel(read.buffered_records.max(1));
    let task_cancellation = cancellation.clone();
    tokio::spawn(async move {
        if let Err(error) = Merging::start(read).run(&sender, &task_cancellation).await
            && sender.send(Err(error)).await.is_err()
        {
            tracing::debug!("log receiver closed before failure diagnostics");
        }
        task_cancellation.cancel();
    });
    LogStream::new(receiver, cancellation)
}

struct Merging {
    nodes: Vec<Option<String>>,
    states: Vec<SourceState>,
    merge: LogMerge,
    follow: bool,
    tail: usize,
    patience: Option<Duration>,
    identity: ServiceIdentity,
    arrived: tokio::sync::mpsc::Receiver<Arrival>,
    /// Dropping the readers ends their streams.
    _readers: tokio::task::JoinSet<()>,
    started: tokio::time::Instant,
    /// When the first source answered.
    first_answer: Option<tokio::time::Instant>,
    /// The lines of a read that ends, until it is complete.
    kept: Vec<LogRecord>,
    /// What mm says about nodes, for the end of a read that ends.
    notices: Vec<LogRecord>,
    /// Why the first source that failed did, in the sources' order.
    failure: Option<(usize, MmError)>,
    /// Whether a source's stream ended because it was over, not broken.
    ended_cleanly: bool,
    /// The first node whose open stream broke, and why.
    broken: Option<(String, String)>,
    sent: u64,
}

impl Merging {
    fn start(read: MergedRead) -> Self {
        let (arrivals, arrived) = tokio::sync::mpsc::channel(256);
        let mut readers = tokio::task::JoinSet::new();
        let mut nodes = Vec::with_capacity(read.sources.len());
        for (source, LogSourceRead { node, opening }) in read.sources.into_iter().enumerate() {
            nodes.push(node);
            let arrivals = arrivals.clone();
            readers.spawn(async move {
                let ended = match opening.await {
                    Ok(mut stream) => {
                        if arrivals.send(Arrival::Opened(source)).await.is_err() {
                            return;
                        }
                        loop {
                            match stream.recv().await {
                                Some(Ok(record)) => {
                                    if arrivals.send(Arrival::Line(source, record)).await.is_err() {
                                        return;
                                    }
                                }
                                Some(Err(error)) => break Some(error),
                                None => break None,
                            }
                        }
                    }
                    Err(error) => Some(error),
                };
                if arrivals.send(Arrival::Ended(source, ended)).await.is_err() {
                    tracing::debug!("a merged log read ended before one of its sources");
                }
            });
        }
        let count = nodes.len();
        Self {
            states: vec![SourceState::Pending; count],
            nodes,
            merge: if read.follow {
                LogMerge::following(count, read.hold)
            } else {
                LogMerge::complete(count)
            },
            follow: read.follow,
            tail: usize::try_from(read.tail).unwrap_or(usize::MAX),
            patience: read.patience,
            identity: read.identity,
            arrived,
            _readers: readers,
            started: tokio::time::Instant::now(),
            first_answer: None,
            kept: Vec::new(),
            notices: Vec::new(),
            failure: None,
            ended_cleanly: false,
            broken: None,
            sent: 0,
        }
    }

    async fn run(
        mut self,
        sender: &tokio::sync::mpsc::Sender<Result<LogRecord, MmError>>,
        cancellation: &OperationCancellation,
    ) -> Result<(), MmError> {
        while !self.is_complete() {
            let release = self.merge.next_release().map(|at| self.started + at);
            let give_up = self.gives_up_at();
            tokio::select! {
                arrival = self.arrived.recv() => match arrival {
                    Some(arrival) => self.take(arrival, sender).await?,
                    None => break,
                },
                () = sleep_until(release) => {}
                () = sleep_until(give_up) => self.give_up(sender).await?,
                () = tokio::time::sleep(CANCEL_CHECK) => {
                    if cancellation.is_cancelled() {
                        return Ok(());
                    }
                }
            }
            if !self.release(sender).await {
                return Ok(());
            }
        }
        self.finish(sender).await
    }

    /// Whether nothing more will come: every source is done. A read that
    /// follows goes on for as long as a source is open or may still open.
    fn is_complete(&self) -> bool {
        self.states.iter().all(|state| *state == SourceState::Done)
    }

    /// When the sources that have not answered are given up on.
    fn gives_up_at(&self) -> Option<tokio::time::Instant> {
        let waiting = self.states.contains(&SourceState::Pending);
        match (self.patience, self.first_answer) {
            (Some(patience), Some(first)) if waiting => Some(first + patience),
            _ => None,
        }
    }

    async fn take(
        &mut self,
        arrival: Arrival,
        sender: &tokio::sync::mpsc::Sender<Result<LogRecord, MmError>>,
    ) -> Result<(), MmError> {
        match arrival {
            Arrival::Opened(source) => {
                // A node that was given up on and answers after all is
                // read from here on.
                self.states[source] = SourceState::Open;
                self.first_answer
                    .get_or_insert_with(tokio::time::Instant::now);
            }
            Arrival::Line(source, mut record) => {
                if let Some(node) = &self.nodes[source] {
                    record.node = Some(node.clone());
                }
                self.merge.push(source, record, self.started.elapsed());
            }
            Arrival::Ended(source, error) => {
                let answered = self.states[source] == SourceState::Open;
                let given_up = self.states[source] == SourceState::Done;
                self.states[source] = SourceState::Done;
                self.merge.end(source);
                let Some(error) = error else {
                    self.ended_cleanly = true;
                    return Ok(());
                };
                match self.nodes[source].clone() {
                    // On this host a source that fails is the read's failure.
                    None => return Err(error),
                    // A node that was given up on already has its line.
                    Some(_) if given_up => {}
                    Some(node) => {
                        let message = if answered {
                            self.broken
                                .get_or_insert_with(|| (node.clone(), node_problem(&error)));
                            format!("the stream ended: {}", node_problem(&error))
                        } else {
                            format!("no answer: {}", node_problem(&error))
                        };
                        if !answered
                            && self
                                .failure
                                .as_ref()
                                .is_none_or(|(first, _)| source < *first)
                        {
                            self.failure = Some((source, error));
                        }
                        self.notice(&node, message, sender).await;
                    }
                }
            }
        }
        Ok(())
    }

    /// Stops waiting for the sources that have not answered.
    async fn give_up(
        &mut self,
        sender: &tokio::sync::mpsc::Sender<Result<LogRecord, MmError>>,
    ) -> Result<(), MmError> {
        let seconds = self.patience.unwrap_or_default().as_secs();
        for source in 0..self.states.len() {
            if self.states[source] != SourceState::Pending {
                continue;
            }
            self.states[source] = SourceState::Done;
            self.merge.end(source);
            if let Some(node) = self.nodes[source].clone() {
                let message = format!(
                    "no answer after {seconds} seconds; the other nodes' lines are shown without it"
                );
                self.notice(&node, message, sender).await;
            }
        }
        Ok(())
    }

    /// One line about a node: at once when the read follows, after the
    /// lines when it ends.
    async fn notice(
        &mut self,
        node: &str,
        message: String,
        sender: &tokio::sync::mpsc::Sender<Result<LogRecord, MmError>>,
    ) {
        let record = LogRecord::about_node(self.identity.clone(), node, message);
        if self.follow {
            self.send(record, sender).await;
        } else {
            self.notices.push(record);
        }
    }

    /// Sends what the merge releases; `false` when nobody reads any more.
    async fn release(
        &mut self,
        sender: &tokio::sync::mpsc::Sender<Result<LogRecord, MmError>>,
    ) -> bool {
        for record in self.merge.release(self.started.elapsed()) {
            if !self.follow {
                self.kept.push(record);
                // Never more than about twice the tail is held.
                if self.kept.len() > self.tail.saturating_mul(2).saturating_add(1024) {
                    let dropped = self.kept.len().saturating_sub(self.tail);
                    self.kept.drain(..dropped);
                }
            } else if !self.send(record, sender).await {
                return false;
            }
        }
        true
    }

    async fn send(
        &mut self,
        mut record: LogRecord,
        sender: &tokio::sync::mpsc::Sender<Result<LogRecord, MmError>>,
    ) -> bool {
        self.sent += 1;
        record.sequence = self.sent;
        sender.send(Ok(record)).await.is_ok()
    }

    /// The end of the read: the kept lines of a read that ends, then what
    /// mm has to say about nodes. When no source answered at all, the
    /// first one's failure is the read's.
    async fn finish(
        mut self,
        sender: &tokio::sync::mpsc::Sender<Result<LogRecord, MmError>>,
    ) -> Result<(), MmError> {
        if self.first_answer.is_none()
            && let Some((_, error)) = self.failure.take()
        {
            return Err(error);
        }
        if !self.release(sender).await {
            return Ok(());
        }
        // A read that follows and has no stream left, none of them over by
        // itself, did not end as asked: it fails, with the first reason.
        if self.follow
            && !self.ended_cleanly
            && let Some((node, problem)) = self.broken.take()
        {
            return Err(MmError::Unreachable {
                subject: "the followed log".to_owned(),
                problem: format!(
                    "has no node's stream left: node '{node}' stopped sending ({problem})"
                ),
                help: "run the command again; `mm target nodes` shows which nodes answer"
                    .to_owned(),
            });
        }
        let dropped = self.kept.len().saturating_sub(self.tail);
        let kept = self.kept.split_off(dropped);
        for record in kept.into_iter().chain(std::mem::take(&mut self.notices)) {
            if !self.send(record, sender).await {
                break;
            }
        }
        Ok(())
    }
}

async fn sleep_until(instant: Option<tokio::time::Instant>) {
    match instant {
        Some(instant) => tokio::time::sleep_until(instant).await,
        None => std::future::pending().await,
    }
}

/// Why a node has no lines, in the words `mm target nodes` uses.
fn node_problem(error: &MmError) -> String {
    match error {
        MmError::Unreachable { problem, help, .. } => format!("{problem}. {help}"),
        other => other.to_string(),
    }
}
