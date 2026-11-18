//! Service actions the TUI runs off the input loop: logs, commands, and restarts.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use crate::app::ports::{ExecStream, LogStream};
use crate::domain::error::MmError;
use crate::domain::observability::{ExecEvent, ExecOutputSource, LogRecord, LogSource};
use crate::presentation::output::{LogLines, error_line};
use crate::presentation::tui::model::{Model, Notice};
use crate::presentation::tui::panel::{LogLine, Panel, StreamState};

/// How many past log lines a newly opened log view asks for.
pub(super) const LOG_TAIL: u32 = 200;

/// What a service action applies to: a whole service, or one of its containers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceTarget {
    pub stack: String,
    pub service: String,
    /// The container ID when the action applies to one replica.
    pub container: Option<String>,
    /// The name shown for the target: the service, or the container's name.
    pub label: String,
    /// On a cluster target: the one node the action applies to.
    pub node: Option<String>,
}

/// Service actions requested from the TUI; implementations call the same use
/// cases as `mm service logs`, `exec`, and `restart`.
#[async_trait]
pub trait ServiceOperations: Send + Sync {
    /// Follows the target's logs, starting with the last `tail` lines.
    async fn logs(&self, target: &ServiceTarget, tail: u32) -> Result<LogStream, MmError>;
    /// Runs an exact argument vector without a TTY and streams its output.
    async fn exec(&self, target: &ServiceTarget, argv: Vec<String>) -> Result<ExecStream, MmError>;
    /// Restarts the target and returns a one-line summary.
    async fn restart(&self, target: &ServiceTarget) -> Result<String, MmError>;
    /// What a remote environment's hosts hold and have staged; `None` for
    /// an environment on this machine.
    async fn host_values(&self) -> Result<Option<crate::domain::staged::RemoteValues>, MmError> {
        Ok(None)
    }
    /// Stages a change on every host of a remote environment and returns a
    /// one-line summary.
    async fn stage(&self, _edit: crate::app::KeyEdit) -> Result<String, MmError> {
        Err(MmError::Environment {
            message: "this environment runs on this machine; nothing is staged here".to_owned(),
        })
    }
}

/// Progress of a background action. `session` names the panel that asked for
/// it, so a closed panel ignores what arrives late.
#[derive(Debug)]
pub enum OperationEvent {
    Log {
        session: u64,
        line: LogLine,
    },
    LogsEnded {
        session: u64,
        error: Option<String>,
    },
    Output {
        session: u64,
        stderr: bool,
        text: String,
    },
    ExecFinished {
        session: u64,
        result: Result<i32, String>,
    },
    Restarted {
        label: String,
        result: Result<String, String>,
    },
    Staged(Result<String, String>),
    HostValues(Box<crate::domain::staged::RemoteValues>),
}

/// Spawns service actions; their events arrive on the receiver returned by `new`.
pub(super) struct OperationRunner {
    operations: Arc<dyn ServiceOperations>,
    sender: mpsc::UnboundedSender<OperationEvent>,
    /// The one log or command stream a panel is showing.
    stream: Mutex<Option<AbortHandle>>,
}

impl OperationRunner {
    pub(super) fn new(
        operations: Arc<dyn ServiceOperations>,
    ) -> (Self, mpsc::UnboundedReceiver<OperationEvent>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Self {
                operations,
                sender,
                stream: Mutex::new(None),
            },
            receiver,
        )
    }

    /// Aborting a stream's task drops the stream, which cancels it at the source.
    fn replace_stream(&self, next: Option<AbortHandle>) {
        match self.stream.lock() {
            Ok(mut current) => {
                if let Some(previous) = std::mem::replace(&mut *current, next) {
                    previous.abort();
                }
            }
            Err(_) => {
                tracing::warn!("TUI stream registry is poisoned; a stream may outlive its panel")
            }
        }
    }

    pub(super) fn close_stream(&self) {
        self.replace_stream(None);
    }

    pub(super) fn follow_logs(&self, session: u64, target: ServiceTarget) {
        let operations = Arc::clone(&self.operations);
        let sender = self.sender.clone();
        let task = tokio::spawn(async move {
            let error = match operations.logs(&target, LOG_TAIL).await {
                Ok(mut stream) => loop {
                    match stream.recv().await {
                        Some(Ok(record)) => {
                            let line = log_line(record);
                            if sender.send(OperationEvent::Log { session, line }).is_err() {
                                return;
                            }
                        }
                        Some(Err(error)) => break Some(error_line(&error)),
                        None => break None,
                    }
                },
                Err(error) => Some(error_line(&error)),
            };
            report(&sender, OperationEvent::LogsEnded { session, error });
        });
        self.replace_stream(Some(task.abort_handle()));
    }

    pub(super) fn run_command(&self, session: u64, target: ServiceTarget, argv: Vec<String>) {
        let operations = Arc::clone(&self.operations);
        let sender = self.sender.clone();
        let task = tokio::spawn(async move {
            let result = match operations.exec(&target, argv).await {
                Ok(mut stream) => {
                    let mut result =
                        Err("the command ended without reporting an exit code".to_owned());
                    while let Some(event) = stream.recv().await {
                        match event {
                            Ok(ExecEvent::Output { source, value, .. }) => {
                                let output = OperationEvent::Output {
                                    session,
                                    stderr: source == ExecOutputSource::Stderr,
                                    text: value,
                                };
                                if sender.send(output).is_err() {
                                    return;
                                }
                            }
                            Ok(ExecEvent::Finished(outcome)) => result = Ok(outcome.exit_code),
                            Ok(ExecEvent::Selected { .. }) => {}
                            Err(error) => {
                                result = Err(error_line(&error));
                                break;
                            }
                        }
                    }
                    result
                }
                Err(error) => Err(error_line(&error)),
            };
            report(&sender, OperationEvent::ExecFinished { session, result });
        });
        self.replace_stream(Some(task.abort_handle()));
    }

    /// Reads what a remote environment's hosts hold.
    pub(super) fn read_host_values(&self) {
        let operations = Arc::clone(&self.operations);
        let sender = self.sender.clone();
        tokio::spawn(async move {
            if let Ok(Some(values)) = operations.host_values().await {
                report(&sender, OperationEvent::HostValues(Box::new(values)));
            }
        });
    }

    /// Stages a change, then reads what the hosts hold again.
    pub(super) fn stage(&self, edit: crate::app::KeyEdit) {
        let operations = Arc::clone(&self.operations);
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let result = operations
                .stage(edit)
                .await
                .map_err(|error| error_line(&error));
            report(&sender, OperationEvent::Staged(result));
            if let Ok(Some(values)) = operations.host_values().await {
                report(&sender, OperationEvent::HostValues(Box::new(values)));
            }
        });
    }

    pub(super) fn restart(&self, target: ServiceTarget) {
        let operations = Arc::clone(&self.operations);
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let result = operations
                .restart(&target)
                .await
                .map_err(|error| error_line(&error));
            report(
                &sender,
                OperationEvent::Restarted {
                    label: target.label,
                    result,
                },
            );
        });
    }
}

/// One record of a log stream as the line a log view draws. The node and
/// the container are named as `mm service logs` names them; the view pads
/// them itself, since it draws every line it holds again.
fn log_line(record: LogRecord) -> LogLine {
    let names = LogLines::default().parts(&record);
    LogLine {
        timestamp: record.timestamp,
        stderr: record.source == LogSource::Stderr,
        node: Some(names.node).filter(|node| !node.is_empty()),
        container: Some(names.container).filter(|container| !container.is_empty()),
        about_node: record.source == LogSource::Runtime,
        text: record.message,
    }
}

fn report(sender: &mpsc::UnboundedSender<OperationEvent>, event: OperationEvent) {
    if sender.send(event).is_err() {
        tracing::debug!("TUI closed before a service action finished");
    }
}

/// Records a background event in the panel that asked for it.
pub(super) fn apply_event(model: &mut Model, event: OperationEvent) {
    match event {
        OperationEvent::Log { session, line } => {
            if let Some(Panel::Logs(logs)) = &mut model.panel
                && logs.session == session
            {
                logs.state = StreamState::Streaming;
                logs.push(line);
            }
        }
        OperationEvent::LogsEnded { session, error } => {
            if let Some(Panel::Logs(logs)) = &mut model.panel
                && logs.session == session
            {
                logs.state = error.map_or(StreamState::Ended, StreamState::Failed);
            }
        }
        OperationEvent::Output {
            session,
            stderr,
            text,
        } => {
            if let Some(Panel::Run(run)) = &mut model.panel
                && run.session == session
            {
                run.push(stderr, &text);
            }
        }
        OperationEvent::ExecFinished { session, result } => {
            if let Some(Panel::Run(run)) = &mut model.panel
                && run.session == session
            {
                run.finish(result);
            }
        }
        OperationEvent::Staged(result) => super::host_values::staged(model, result),
        OperationEvent::HostValues(values) => {
            model.remote_values = Some(*values);
            model.clamp_selection();
        }
        OperationEvent::Restarted { label, result } => {
            model.restarting.retain(|current| current != &label);
            model.notice = Some(match result {
                Ok(summary) => Notice::success(summary),
                Err(error) => Notice::error(format!("Restarting {label} failed: {error}")),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::domain::observability::{ExecOutcome, OperationCancellation};
    use crate::domain::runtime::{RuntimeKind, ServiceIdentity};
    use crate::domain::workspace::ProjectId;
    use crate::presentation::tui::fixtures::model;
    use crate::presentation::tui::model::NoticeKind;
    use crate::presentation::tui::panel::{LogsPanel, RunPanel, RunState};

    struct Scripted;

    fn identity() -> ServiceIdentity {
        ServiceIdentity {
            project_id: ProjectId(uuid::Uuid::from_u128(1)),
            environment: "development".to_owned(),
            stack: "app".to_owned(),
            module: None,
            service: "api".to_owned(),
        }
    }

    #[async_trait]
    impl ServiceOperations for Scripted {
        async fn logs(&self, target: &ServiceTarget, _tail: u32) -> Result<LogStream, MmError> {
            if target.service == "missing" {
                return Err(MmError::RuntimeResponse {
                    message: "service 'missing' is not configured".to_owned(),
                });
            }
            let (sender, receiver) = mpsc::channel(4);
            for (sequence, (source, message)) in [
                (LogSource::Stdout, "listening"),
                (LogSource::Stderr, "\u{1b}[31mfailed\u{1b}[0m"),
            ]
            .into_iter()
            .enumerate()
            {
                let record = LogRecord {
                    identity: identity(),
                    sequence: sequence as u64,
                    source,
                    node: None,
                    container: Some("fixture-app-api-1".to_owned()),
                    timestamp: None,
                    message: message.to_owned(),
                    lossy_utf8: false,
                    truncated: false,
                    reconnect: 0,
                };
                if sender.send(Ok(record)).await.is_err() {
                    break;
                }
            }
            Ok(LogStream::new(receiver, OperationCancellation::default()))
        }

        async fn exec(
            &self,
            _target: &ServiceTarget,
            argv: Vec<String>,
        ) -> Result<ExecStream, MmError> {
            let (sender, receiver) = mpsc::channel(4);
            let events = [
                ExecEvent::Output {
                    source: ExecOutputSource::Stdout,
                    value: format!("{}\n", argv.join(" ")),
                    lossy_utf8: false,
                },
                ExecEvent::Finished(ExecOutcome {
                    runtime: RuntimeKind::DockerCompose,
                    identity: identity(),
                    container_id: "aaaaaaaaaaaa".to_owned(),
                    exit_code: 3,
                    started_at_unix_ms: 1,
                    finished_at_unix_ms: 2,
                }),
            ];
            for event in events {
                if sender.send(Ok(event)).await.is_err() {
                    break;
                }
            }
            Ok(ExecStream::new(receiver, OperationCancellation::default()))
        }

        async fn restart(&self, target: &ServiceTarget) -> Result<String, MmError> {
            if target.service == "missing" {
                return Err(MmError::RuntimeResponse {
                    message: "no containers".to_owned(),
                });
            }
            Ok(format!("Restarted {} (1 container).", target.service))
        }
    }

    fn target(service: &str) -> ServiceTarget {
        ServiceTarget {
            stack: "app".to_owned(),
            service: service.to_owned(),
            container: None,
            label: service.to_owned(),
            node: None,
        }
    }

    /// Applies events until `done` matches one, failing if the runner goes quiet.
    async fn drain(
        model: &mut Model,
        events: &mut mpsc::UnboundedReceiver<OperationEvent>,
        done: impl Fn(&OperationEvent) -> bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
                .await?
                .ok_or("the runner closed its channel")?;
            let finished = done(&event);
            apply_event(model, event);
            if finished {
                return Ok(());
            }
        }
    }

    #[tokio::test]
    async fn the_runner_feeds_logs_output_and_restarts_into_the_model()
    -> Result<(), Box<dyn std::error::Error>> {
        let (runner, mut events) = OperationRunner::new(Arc::new(Scripted));
        let mut model = model();

        model.panel = Some(Panel::Logs(LogsPanel::new(7, target("api"))));
        runner.follow_logs(7, target("api"));
        drain(&mut model, &mut events, |event| {
            matches!(event, OperationEvent::LogsEnded { .. })
        })
        .await?;
        let Some(Panel::Logs(logs)) = &model.panel else {
            return Err("the log view closed".into());
        };
        let lines = logs
            .lines
            .iter()
            .map(|line| (line.stderr, line.text.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(lines, [(false, "listening"), (true, "failed")]);
        // The container is named as Compose names it.
        assert!(
            logs.lines
                .iter()
                .all(|line| line.node.is_none() && line.container.as_deref() == Some("api-1"))
        );
        assert_eq!(logs.state, StreamState::Ended);

        // A stream the view no longer shows must not write into it.
        apply_event(
            &mut model,
            OperationEvent::Log {
                session: 6,
                line: LogLine {
                    text: "late".to_owned(),
                    ..LogLine::default()
                },
            },
        );
        assert!(matches!(&model.panel, Some(Panel::Logs(logs)) if logs.lines.len() == 2));

        model.panel = Some(Panel::Logs(LogsPanel::new(8, target("missing"))));
        runner.follow_logs(8, target("missing"));
        drain(&mut model, &mut events, |event| {
            matches!(event, OperationEvent::LogsEnded { .. })
        })
        .await?;
        assert!(matches!(
            &model.panel,
            Some(Panel::Logs(logs))
                if matches!(&logs.state, StreamState::Failed(message) if message.contains("missing"))
        ));

        let argv = vec!["ls".to_owned(), "-la".to_owned()];
        model.panel = Some(Panel::Run(RunPanel::new(
            9,
            target("api"),
            "ls -la".to_owned(),
            argv.clone(),
        )));
        runner.run_command(9, target("api"), argv);
        drain(&mut model, &mut events, |event| {
            matches!(event, OperationEvent::ExecFinished { .. })
        })
        .await?;
        let Some(Panel::Run(run)) = &model.panel else {
            return Err("the command view closed".into());
        };
        assert_eq!(run.state, RunState::Exited(3));
        assert_eq!(run.lines.len(), 1);
        assert_eq!(run.lines[0].text, "ls -la");

        for (service, kind) in [("api", NoticeKind::Success), ("missing", NoticeKind::Error)] {
            model.restarting.push(service.to_owned());
            runner.restart(target(service));
            drain(&mut model, &mut events, |event| {
                matches!(event, OperationEvent::Restarted { .. })
            })
            .await?;
            assert!(model.restarting.is_empty());
            assert_eq!(model.notice.as_ref().map(|notice| notice.kind), Some(kind));
        }
        runner.close_stream();
        Ok(())
    }
}
