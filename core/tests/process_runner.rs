//! The process runner as its callers use it: what a child gets from its
//! parent, what comes back, and what happens to the child when it is
//! cancelled or late. (Moved here from the crate's unit tests, unchanged in
//! what they check: they need only the public API, and the unit-test build
//! is close to its memory limit.)

use std::time::Duration;

use mm::adapters::process::TokioProcessRunner;
use mm::app::ports::{
    CommandSpec, ProcessRunner, ProcessStreamEvent, StreamCommandSpec, StreamingProcessRunner,
};
use mm::domain::error::MmError;
use mm::domain::observability::{ExecTtyMode, OperationCancellation};

#[cfg(unix)]
#[tokio::test]
async fn clears_the_ambient_process_environment() -> Result<(), mm::domain::error::MmError> {
    let output = TokioProcessRunner
        .run(CommandSpec::new("/usr/bin/env").environment("MM_EXPLICIT_TEST", "present"))
        .await?;
    assert_eq!(output.stdout.as_str(), "MM_EXPLICIT_TEST=present\n");
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn terminal_reading_children_stay_in_the_foreground_process_group()
-> Result<(), mm::domain::error::MmError> {
    // Field 5 of /proc/self/stat is the process group; the name in field 2 has no spaces.
    let group = |output: mm::app::ports::CommandOutput| {
        output.stdout.split_whitespace().nth(4).map(str::to_owned)
    };
    let own = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|stat| stat.split_whitespace().nth(4).map(str::to_owned));
    let interactive = TokioProcessRunner
        .run(
            CommandSpec::new("/bin/cat")
                .arg("/proc/self/stat")
                .inherit_stdin(),
        )
        .await?;
    let background = TokioProcessRunner
        .run(CommandSpec::new("/bin/cat").arg("/proc/self/stat"))
        .await?;
    assert_eq!(group(interactive), own);
    assert_ne!(group(background), own);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn a_child_that_does_not_read_all_its_input_ends_with_its_own_status() -> Result<(), MmError>
{
    // Four MiB do not fit the pipe: the writer is still writing when the
    // child ends, as with a host that already holds a delivered bundle.
    let input = || zeroize::Zeroizing::new("x".repeat(4 * 1024 * 1024));
    let done = TokioProcessRunner
        .run(
            CommandSpec::new("/bin/sh")
                .args(["-c", "head -c 1 >/dev/null; echo done"])
                .sensitive_stdin(input()),
        )
        .await?;
    assert_eq!((done.code, done.stdout.as_str()), (Some(0), "done\n"));
    let failed = TokioProcessRunner
        .run(
            CommandSpec::new("/bin/sh")
                .args(["-c", "exit 3"])
                .sensitive_stdin(input()),
        )
        .await?;
    assert_eq!(failed.code, Some(3));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn sensitive_stdin_is_piped_and_redacted() -> Result<(), mm::domain::error::MmError> {
    let sentinel = "stdin-secret-sentinel";
    let specification = CommandSpec::new("/bin/sh")
        .args(["-c", "cat"])
        .sensitive_stdin(zeroize::Zeroizing::new(sentinel.to_owned()));
    assert!(!specification.redacted_display().contains(sentinel));
    assert!(!format!("{specification:?}").contains(sentinel));
    let output = TokioProcessRunner.run(specification).await?;
    assert_eq!(output.stdout.as_str(), "[REDACTED]");
    assert!(!format!("{output:?}").contains(sentinel));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn cancellation_reaps_the_streaming_process_group() -> Result<(), Box<dyn std::error::Error>>
{
    let cancellation = OperationCancellation::default();
    let mut stream = TokioProcessRunner
        .spawn(
            StreamCommandSpec::new("/bin/sh")
                .args(["-c", "printf '%s\\n' \"$$\"; exec sleep 30"])
                .timeout(Duration::from_secs(60)),
            cancellation.clone(),
        )
        .await?;
    let pid = match tokio::time::timeout(Duration::from_secs(2), stream.recv())
        .await?
        .ok_or_else(|| std::io::Error::other("stream ended before reporting its process id"))?
    {
        ProcessStreamEvent::Output { bytes, .. } => {
            String::from_utf8(bytes)?.trim().parse::<u32>()?
        }
        event => return Err(format!("expected process id, received {event:?}").into()),
    };
    cancellation.cancel();
    while tokio::time::timeout(Duration::from_secs(2), stream.recv())
        .await?
        .is_some()
    {}
    let pid = pid.to_string();
    let probe = TokioProcessRunner
        .run(CommandSpec::new("/bin/kill").args(["-0", pid.as_str()]))
        .await?;
    assert!(!probe.success(), "cancelled process {pid} is still alive");
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn tty_stream_reports_the_exact_remote_exit_without_capturing_stdio()
-> Result<(), Box<dyn std::error::Error>> {
    let mut stream = TokioProcessRunner
        .spawn(
            StreamCommandSpec::new("/bin/sh")
                .args(["-c", "exit 17"])
                .tty(ExecTtyMode::Enabled),
            OperationCancellation::default(),
        )
        .await?;
    let event = tokio::time::timeout(Duration::from_secs(2), stream.recv()).await?;
    assert!(matches!(
        event,
        Some(ProcessStreamEvent::Exited { code: Some(17) })
    ));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn streaming_timeout_reports_transport_failure_and_reaps_child()
-> Result<(), Box<dyn std::error::Error>> {
    let mut stream = TokioProcessRunner
        .spawn(
            StreamCommandSpec::new("/bin/sh")
                .args(["-c", "printf '%s\\n' \"$$\"; exec sleep 30"])
                .timeout(Duration::from_millis(50)),
            OperationCancellation::default(),
        )
        .await?;
    let pid = match stream
        .recv()
        .await
        .ok_or_else(|| std::io::Error::other("timeout stream did not report its process id"))?
    {
        ProcessStreamEvent::Output { bytes, .. } => {
            String::from_utf8(bytes)?.trim().parse::<u32>()?
        }
        event => return Err(format!("expected process id, received {event:?}").into()),
    };
    let failure = tokio::time::timeout(Duration::from_secs(2), stream.recv())
        .await?
        .ok_or_else(|| std::io::Error::other("timeout stream ended without an error"))?;
    assert!(matches!(
        failure,
        ProcessStreamEvent::Failed(MmError::ProcessTimeout { .. })
    ));
    let pid = pid.to_string();
    let probe = TokioProcessRunner
        .run(CommandSpec::new("/bin/kill").args(["-0", pid.as_str()]))
        .await?;
    assert!(!probe.success(), "timed-out process {pid} is still alive");
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn internal_stdout_keeps_values_that_stderr_still_redacts()
-> Result<(), mm::domain::error::MmError> {
    let sentinel = "render-secret-sentinel";
    let specification = CommandSpec::new("/bin/sh")
        .args([
            "-c",
            "printf 'KEY: %s\\n' \"$1\"; printf '%s' \"$1\" >&2",
            "sh",
        ])
        .sensitive_arg(sentinel.to_owned())
        .internal_stdout();
    let output = TokioProcessRunner.run(specification).await?;
    assert_eq!(output.stdout.as_str(), format!("KEY: {sentinel}\n"));
    assert_eq!(output.stderr.as_str(), "[REDACTED]");
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn values_too_short_to_tell_apart_are_not_redacted() -> Result<(), mm::domain::error::MmError>
{
    let specification = CommandSpec::new("/bin/sh")
        .args(["-c", "printf '%s /tmp/1 %s' \"$1\" \"$2\" >&2", "sh"])
        .sensitive_arg("1".to_owned())
        .sensitive_arg("long-secret-value".to_owned());
    let output = TokioProcessRunner.run(specification).await?;
    assert_eq!(output.stderr.as_str(), "1 /tmp/1 [REDACTED]");
    Ok(())
}
