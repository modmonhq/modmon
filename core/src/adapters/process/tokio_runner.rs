use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
#[cfg(windows)]
use process_wrap::tokio::JobObject;
#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use zeroize::Zeroizing;

use super::supervisor::{Requester, supervise};
use crate::app::ports::{
    CommandOutput, CommandSpec, ProcessOutputSource, ProcessRunner, ProcessStream,
    ProcessStreamEvent, StreamCommandSpec, StreamingProcessRunner,
};
use crate::domain::error::MmError;
use crate::domain::observability::{ExecStdinMode, ExecTtyMode, OperationCancellation};

#[derive(Debug, Clone, Copy, Default)]
pub struct TokioProcessRunner;

#[async_trait]
impl ProcessRunner for TokioProcessRunner {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let command_display = specification.redacted_display();
        tracing::debug!(command = ?command_display, "running process");
        let _phase = crate::app::timing::phase_of("process", &command_display);
        supervise(move |requester| run_to_end(specification, requester)).await
    }
}

/// How a supervised child ended.
enum Ended {
    Completed(std::io::Result<(std::process::ExitStatus, BoundedOutput, BoundedOutput)>),
    /// Its deadline passed.
    Late,
    /// Whoever asked for it stopped waiting.
    Abandoned,
}

/// Starts the child, drains its output, and ends its process group when
/// its deadline passes or nobody waits for it any more. Runs on the
/// supervisor's thread.
async fn run_to_end(
    mut specification: CommandSpec,
    mut requester: Requester,
) -> Result<CommandOutput, MmError> {
    let redaction = output_redaction(&specification);
    let sensitive_stdin = specification.take_sensitive_stdin();
    let failed_to_start = |source| MmError::ProcessSpawn {
        program: specification.program().to_owned(),
        source,
    };
    let mut child = start(&specification, sensitive_stdin.is_some()).map_err(failed_to_start)?;
    // On the list of children until it is gone: one that is still there
    // after its deadline is what `mm doctor` reports.
    let _listed = super::supervisor::Listed::child(
        child.id(),
        specification.program(),
        specification.timeout_value(),
    );
    let pipe = |name: &str| std::io::Error::other(format!("{name} pipe was not created"));
    let stdout = child
        .stdout()
        .take()
        .ok_or_else(|| failed_to_start(pipe("stdout")))?;
    let stderr = child
        .stderr()
        .take()
        .ok_or_else(|| failed_to_start(pipe("stderr")))?;
    let stdin = child.stdin().take();
    let maximum = specification.output_limit();
    let timeout = specification.timeout_value();
    let ended = {
        let write_stdin = async move {
            match (stdin, sensitive_stdin) {
                (Some(mut stdin), Some(value)) => {
                    let written = async {
                        stdin.write_all(value.as_bytes()).await?;
                        stdin.shutdown().await
                    };
                    // A child may finish without reading all of its input,
                    // as `mmd access sync` does with a bundle it already
                    // holds: its exit status says how it went.
                    match written.await {
                        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                        other => other,
                    }
                }
                (None, None) => Ok(()),
                _ => Err(pipe("sensitive stdin")),
            }
        };
        let completed = async {
            let (status, stdout, stderr, ()) = tokio::try_join!(
                child.wait(),
                read_bounded(stdout, maximum),
                read_bounded(stderr, maximum),
                write_stdin,
            )?;
            Ok::<_, std::io::Error>((status, stdout, stderr))
        };
        tokio::select! {
            biased;
            completed = completed => Ended::Completed(completed),
            () = tokio::time::sleep(timeout) => Ended::Late,
            () = requester.gone() => Ended::Abandoned,
        }
    };
    let primary = match ended {
        Ended::Completed(result) => {
            let (status, stdout, stderr) = result.map_err(failed_to_start)?;
            return Ok(command_output(status, stdout, stderr, &redaction));
        }
        Ended::Late => MmError::ProcessTimeout {
            program: specification.program().to_owned(),
            seconds: timeout.as_secs(),
        },
        Ended::Abandoned => MmError::Cancelled {
            message: format!(
                "{} was ended because nothing waited for it any more",
                specification.program()
            ),
        },
    };
    match Box::into_pin(child.kill()).await {
        Ok(()) => Err(primary),
        Err(cleanup) => Err(MmError::operation_failure(
            specification.program(),
            crate::domain::error::MutationStatus::Unknown,
            primary,
            vec![cleanup.to_string()],
            "verify and terminate the timed-out process group before retrying",
        )),
    }
}

/// Starts the child of `specification` as the leader of a process group
/// of its own, so the whole group can be ended.
fn start(specification: &CommandSpec, pipes_stdin: bool) -> std::io::Result<Box<dyn ChildWrapper>> {
    let mut command = Command::new(specification.program());
    let stdin = if pipes_stdin {
        Stdio::piped()
    } else if specification.should_inherit_stdin() {
        Stdio::inherit()
    } else {
        Stdio::null()
    };
    command
        .args(specification.arguments())
        .env_clear()
        .envs(specification.environment_values())
        .envs(rootful_podman_socket(
            specification.program(),
            specification.environment_values().map(|(key, _)| key),
        ))
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(current_dir) = specification.current_directory() {
        command.current_dir(current_dir);
    }

    let mut wrapped = CommandWrap::from(command);
    wrapped.wrap(KillOnDrop);
    // A child reading the terminal (such as a sudo prompt) must stay in the foreground
    // process group: a background group cannot disable echo or read the password, and
    // it never receives the operator's Ctrl-C.
    #[cfg(unix)]
    if !specification.should_inherit_stdin() {
        wrapped.wrap(ProcessGroup::leader());
    }
    #[cfg(windows)]
    wrapped.wrap(JobObject);
    wrapped.spawn()
}

fn command_output(
    status: std::process::ExitStatus,
    stdout: BoundedOutput,
    stderr: BoundedOutput,
    (sensitive_values, internal_stdout): &(Vec<Zeroizing<String>>, bool),
) -> CommandOutput {
    let has_sensitive_values = !sensitive_values.is_empty();
    let truncated = stdout.truncated || stderr.truncated;
    CommandOutput {
        code: status.code(),
        stdout: if *internal_stdout && !stdout.truncated {
            Zeroizing::new(stdout.value)
        } else {
            Zeroizing::new(redact_output(
                stdout,
                sensitive_values,
                has_sensitive_values,
            ))
        },
        stderr: Zeroizing::new(redact_output(
            stderr,
            sensitive_values,
            has_sensitive_values,
        )),
        truncated,
    }
}

#[async_trait]
impl StreamingProcessRunner for TokioProcessRunner {
    async fn spawn(
        &self,
        specification: StreamCommandSpec,
        cancellation: OperationCancellation,
    ) -> Result<ProcessStream, MmError> {
        supervise(move |_requester| async move { start_stream(&specification, cancellation) }).await
    }
}

/// Starts a child whose output is streamed. Its output is forwarded and
/// its deadline enforced by tasks on the supervisor's thread; a receiver
/// that goes away cancels it.
fn start_stream(
    specification: &StreamCommandSpec,
    cancellation: OperationCancellation,
) -> Result<ProcessStream, MmError> {
    let program = specification.program().to_owned();
    let tty = specification.tty_mode() == ExecTtyMode::Enabled;
    let mut command = Command::new(&program);
    command
        .args(specification.arguments())
        .env_clear()
        .stdin(if specification.stdin_mode() == ExecStdinMode::Inherit {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .stdout(if tty {
            Stdio::inherit()
        } else {
            Stdio::piped()
        })
        .stderr(if tty {
            Stdio::inherit()
        } else {
            Stdio::piped()
        });
    for key in specification.inherited_environment() {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.envs(specification.environment_values());
    command.envs(rootful_podman_socket(
        &program,
        specification
            .environment_values()
            .map(|(key, _)| key)
            .chain(
                specification
                    .inherited_environment()
                    .iter()
                    .copied()
                    .filter(|key| std::env::var_os(key).is_some()),
            ),
    ));
    if let Some(current_dir) = specification.current_directory() {
        command.current_dir(current_dir);
    }

    let mut wrapped = CommandWrap::from(command);
    wrapped.wrap(KillOnDrop);
    #[cfg(unix)]
    if !tty {
        wrapped.wrap(ProcessGroup::leader());
    }
    #[cfg(windows)]
    wrapped.wrap(JobObject);
    let mut child = wrapped.spawn().map_err(|source| MmError::ProcessSpawn {
        program: program.clone(),
        source,
    })?;
    let stdout = child.stdout().take();
    let stderr = child.stderr().take();
    if !tty && (stdout.is_none() || stderr.is_none()) {
        return Err(MmError::ProcessSpawn {
            program,
            source: std::io::Error::other("stream output pipes were not created"),
        });
    }
    let event_limit = specification.event_limit().clamp(1, 1024);
    let timeout = specification.timeout_value();
    let (sender, receiver) = tokio::sync::mpsc::channel(event_limit);
    let stdout_task = stdout.map(|reader| {
        tokio::spawn(forward_output(
            reader,
            ProcessOutputSource::Stdout,
            sender.clone(),
            cancellation.clone(),
        ))
    });
    let stderr_task = stderr.map(|reader| {
        tokio::spawn(forward_output(
            reader,
            ProcessOutputSource::Stderr,
            sender.clone(),
            cancellation.clone(),
        ))
    });
    let task_cancellation = cancellation.clone();
    tokio::spawn(monitor_stream_child(StreamMonitor {
        child,
        stdout_task,
        stderr_task,
        sender,
        cancellation: task_cancellation,
        tty,
        timeout,
        program,
    }));
    Ok(ProcessStream::new(receiver, cancellation))
}

struct StreamMonitor {
    child: Box<dyn ChildWrapper>,
    stdout_task: Option<tokio::task::JoinHandle<()>>,
    stderr_task: Option<tokio::task::JoinHandle<()>>,
    sender: tokio::sync::mpsc::Sender<ProcessStreamEvent>,
    cancellation: OperationCancellation,
    tty: bool,
    timeout: Duration,
    program: String,
}

async fn monitor_stream_child(monitor: StreamMonitor) {
    let StreamMonitor {
        mut child,
        stdout_task,
        stderr_task,
        sender,
        cancellation,
        tty,
        timeout,
        program,
    } = monitor;
    let started = tokio::time::Instant::now();
    let status = loop {
        if cancellation.is_cancelled() {
            #[cfg(unix)]
            if let Some(pid) = child
                .id()
                .and_then(|pid| rustix::process::Pid::from_raw(pid as i32))
            {
                if let Err(error) = if tty {
                    rustix::process::kill_process(pid, rustix::process::Signal::INT)
                } else {
                    rustix::process::kill_process_group(pid, rustix::process::Signal::INT)
                } {
                    tracing::debug!(%error, %program, "process interrupt signal failed; escalating to kill");
                }
                match tokio::time::timeout(Duration::from_millis(500), child.wait()).await {
                    Ok(Ok(status)) => break Some(status),
                    Ok(Err(source)) => {
                        send_process_error(&sender, &program, source).await;
                        break None;
                    }
                    Err(_) => {}
                }
            }
            kill_child_best_effort(&mut child, &sender, &program).await;
            break None;
        }
        if started.elapsed() >= timeout {
            kill_child_best_effort(&mut child, &sender, &program).await;
            if sender
                .send(ProcessStreamEvent::Failed(MmError::ProcessTimeout {
                    program: program.clone(),
                    seconds: timeout.as_secs(),
                }))
                .await
                .is_err()
            {
                cancellation.cancel();
            }
            break None;
        }
        match tokio::time::timeout(Duration::from_millis(25), child.wait()).await {
            Ok(Ok(status)) => break Some(status),
            Ok(Err(source)) => {
                send_process_error(&sender, &program, source).await;
                break None;
            }
            Err(_) => {}
        }
    };
    if let Some(task) = stdout_task
        && let Err(error) = task.await
    {
        send_process_error(&sender, &program, std::io::Error::other(error)).await;
    }
    if let Some(task) = stderr_task
        && let Err(error) = task.await
    {
        send_process_error(&sender, &program, std::io::Error::other(error)).await;
    }
    if let Some(status) = status
        && sender
            .send(ProcessStreamEvent::Exited {
                code: status.code(),
            })
            .await
            .is_err()
    {
        cancellation.cancel();
    }
}

async fn kill_child_best_effort(
    child: &mut Box<dyn ChildWrapper>,
    sender: &tokio::sync::mpsc::Sender<ProcessStreamEvent>,
    program: &str,
) {
    report_kill_result(Box::into_pin(child.kill()).await, sender, program).await;
}

async fn report_kill_result(
    result: std::io::Result<()>,
    sender: &tokio::sync::mpsc::Sender<ProcessStreamEvent>,
    program: &str,
) {
    if let Err(source) = result {
        send_process_error(sender, program, source).await;
    }
}

async fn send_process_error(
    sender: &tokio::sync::mpsc::Sender<ProcessStreamEvent>,
    program: &str,
    source: std::io::Error,
) {
    if sender
        .send(ProcessStreamEvent::Failed(MmError::ProcessSpawn {
            program: program.to_owned(),
            source,
        }))
        .await
        .is_err()
    {
        tracing::debug!(%program, "process stream receiver closed before failure diagnostics");
    }
}

async fn forward_output(
    mut reader: impl AsyncRead + Unpin,
    source: ProcessOutputSource,
    sender: tokio::sync::mpsc::Sender<ProcessStreamEvent>,
    cancellation: OperationCancellation,
) {
    let mut buffer = vec![0_u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => {
                if sender
                    .send(ProcessStreamEvent::Output {
                        source,
                        bytes: buffer[..read].to_vec(),
                    })
                    .await
                    .is_err()
                {
                    cancellation.cancel();
                    break;
                }
            }
            Err(source) => {
                if sender
                    .send(ProcessStreamEvent::Failed(MmError::Io {
                        context: "reading streaming process output".to_owned(),
                        source,
                    }))
                    .await
                    .is_err()
                {
                    tracing::debug!("process stream receiver closed after output failure");
                }
                cancellation.cancel();
                break;
            }
        }
    }
}

/// The values to redact from output, and whether standard output keeps them.
fn output_redaction(specification: &CommandSpec) -> (Vec<Zeroizing<String>>, bool) {
    (
        sensitive_values(specification),
        specification.keeps_internal_stdout(),
    )
}

/// Shorter values are not redacted from output: a value such as `0` or
/// `true` cannot be told apart from ordinary text, and replacing every
/// occurrence of it destroys the output while hiding nothing.
const MIN_REDACTED_LENGTH: usize = 8;

fn sensitive_values(specification: &CommandSpec) -> Vec<Zeroizing<String>> {
    let mut values = specification
        .sensitive_values()
        .filter(|value| value.chars().count() >= MIN_REDACTED_LENGTH)
        .map(|value| Zeroizing::new(value.to_owned()))
        .collect::<Vec<_>>();
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    values.dedup();
    values
}

fn redact(value: String, sensitive_values: &[Zeroizing<String>]) -> String {
    let mut value = Zeroizing::new(value);
    for sensitive in sensitive_values {
        value = Zeroizing::new(value.replace(sensitive.as_str(), "[REDACTED]"));
    }
    value.to_string()
}

fn redact_output(
    output: BoundedOutput,
    sensitive_values: &[Zeroizing<String>],
    has_sensitive_values: bool,
) -> String {
    if output.truncated && has_sensitive_values {
        drop(Zeroizing::new(output.value));
        "[truncated output suppressed because the command used sensitive values]".to_owned()
    } else {
        redact(output.value, sensitive_values)
    }
}

struct BoundedOutput {
    value: String,
    truncated: bool,
}

async fn read_bounded(
    mut reader: impl AsyncRead + Unpin,
    maximum: usize,
) -> Result<BoundedOutput, std::io::Error> {
    let mut retained = Vec::with_capacity(maximum.min(64 * 1024));
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;

    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let available = maximum.saturating_sub(retained.len());
        let keep = available.min(read);
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep < read;
    }

    let retained = Zeroizing::new(retained);
    let mut value = String::from_utf8_lossy(&retained).into_owned();
    if truncated {
        value.push_str("\n[output truncated by mm]");
    }
    Ok(BoundedOutput { value, truncated })
}

#[cfg(test)]
#[path = "tokio_runner/tests.rs"]
mod tests;

const ROOTFUL_PODMAN_SOCKET: &str = "/run/podman/podman.sock";

fn rootful_podman_socket<'a>(
    program: &str,
    mut named: impl Iterator<Item = &'a str>,
) -> Option<(&'static str, String)> {
    let is_podman = std::path::Path::new(program)
        .file_name()
        .is_some_and(|name| name == "podman");
    if !is_podman || !running_as_root() {
        return None;
    }
    if named.any(|key| key == "CONTAINER_HOST" || key == "CONTAINER_CONNECTION") {
        return None;
    }
    rootful_podman_socket_at(std::path::Path::new(ROOTFUL_PODMAN_SOCKET))
}

fn rootful_podman_socket_at(socket: &std::path::Path) -> Option<(&'static str, String)> {
    socket
        .exists()
        .then(|| ("CONTAINER_HOST", format!("unix://{}", socket.display())))
}

#[cfg(unix)]
fn running_as_root() -> bool {
    rustix::process::geteuid().is_root()
}

#[cfg(not(unix))]
fn running_as_root() -> bool {
    false
}

#[cfg(test)]
mod rootful_podman_tests {
    use super::{rootful_podman_socket, rootful_podman_socket_at};

    #[test]
    fn only_podman_with_no_engine_named_and_a_socket_goes_through_it()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = crate::test_support::tempdir()?;
        let socket = directory.path().join("podman.sock");
        assert_eq!(rootful_podman_socket_at(&socket), None);
        std::fs::write(&socket, b"")?;
        assert_eq!(
            rootful_podman_socket_at(&socket),
            Some(("CONTAINER_HOST", format!("unix://{}", socket.display())))
        );
        assert_eq!(rootful_podman_socket("docker", std::iter::empty()), None);
        assert_eq!(
            rootful_podman_socket("/usr/bin/podman", ["CONTAINER_HOST"].into_iter()),
            None
        );
        assert_eq!(
            rootful_podman_socket("podman", ["CONTAINER_CONNECTION"].into_iter()),
            None
        );
        Ok(())
    }
}
