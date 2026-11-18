//! Child processes are watched over by a thread of their own, not by the
//! task that asked for them: their output is drained and their deadline
//! enforced while that task is not run, and a task that stops waiting ends
//! its child with everything the child started (a child sat on a
//! full pipe for 80 minutes past its 45-second timeout).

use std::path::Path;
use std::time::{Duration, Instant};

use mm::adapters::process::TokioProcessRunner;
use mm::app::ports::{CommandSpec, ProcessRunner};
use mm::domain::error::MmError;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Whether `pid` is a process that still runs. One that ended and that
/// nobody reaped yet is not.
fn runs(pid: &str) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        stat.rsplit(')')
            .next()
            .is_some_and(|rest| !rest.trim_start().starts_with('Z'))
    })
}

/// Waits for the process id a child wrote to `file`.
fn pid_in(file: &Path) -> Result<String, String> {
    let began = Instant::now();
    while began.elapsed() < Duration::from_secs(5) {
        if let Ok(pid) = std::fs::read_to_string(file)
            && pid.ends_with('\n')
        {
            return Ok(pid.trim().to_owned());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(format!("{} was never written", file.display()))
}

/// A shell that starts a process of its own, says who both are, and then
/// writes more than a pipe holds before it sleeps.
fn noisy(directory: &Path) -> CommandSpec {
    CommandSpec::new("/bin/sh").args([
        "-c".to_owned(),
        format!(
            "sleep 60 & echo $! > {0}/grandchild; echo $$ > {0}/child; head -c 300000 /dev/zero; sleep 60",
            directory.display()
        ),
    ])
}

#[tokio::test]
async fn a_child_is_drained_and_ended_at_its_deadline_while_the_task_that_asked_is_not_run()
-> TestResult {
    let directory = temp_dirs::tempdir()?;
    let command = noisy(directory.path()).timeout(Duration::from_millis(800));
    let asked = tokio::spawn(async move { TokioProcessRunner.run(command).await });
    // Let the task ask for its child once, then hold this runtime's only
    // thread well past the child's deadline.
    tokio::task::yield_now().await;
    let child = pid_in(&directory.path().join("child"))?;
    let grandchild = pid_in(&directory.path().join("grandchild"))?;
    assert!(runs(&child) && runs(&grandchild));
    std::thread::sleep(Duration::from_millis(2_000));
    assert!(
        !runs(&child),
        "the child outlived its deadline while its task was not run"
    );
    assert!(
        !runs(&grandchild),
        "its process group outlived the deadline"
    );
    let outcome = asked.await?;
    assert!(
        matches!(outcome, Err(MmError::ProcessTimeout { .. })),
        "a deadline that passed is reported as one"
    );
    Ok(())
}

#[tokio::test]
async fn a_child_that_fills_its_pipe_finishes_while_the_task_that_asked_is_not_run() -> TestResult {
    let command = CommandSpec::new("/bin/sh")
        .args(["-c", "head -c 300000 /dev/zero | tr '\\0' x; echo done"])
        .timeout(Duration::from_secs(30));
    let asked = tokio::spawn(async move { TokioProcessRunner.run(command).await });
    tokio::task::yield_now().await;
    std::thread::sleep(Duration::from_millis(1_500));
    // The answer is ready: nothing waited for this runtime to drain a pipe.
    let output = tokio::time::timeout(Duration::from_millis(200), asked).await???;
    assert!(output.success());
    assert_eq!(output.stdout.len(), 300_005);
    assert!(output.stdout.ends_with("done\n"));
    Ok(())
}

#[tokio::test]
async fn a_task_that_stops_waiting_ends_its_child_and_what_the_child_started() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let command = noisy(directory.path()).timeout(Duration::from_secs(60));
    let asked = tokio::spawn(async move { TokioProcessRunner.run(command).await });
    let child = loop {
        tokio::time::sleep(Duration::from_millis(10)).await;
        if directory.path().join("child").exists() {
            break pid_in(&directory.path().join("child"))?;
        }
    };
    let grandchild = pid_in(&directory.path().join("grandchild"))?;
    assert!(runs(&child) && runs(&grandchild));
    // What a request's deadline or a dropped connection does to the task.
    asked.abort();
    let began = Instant::now();
    while (runs(&child) || runs(&grandchild)) && began.elapsed() < Duration::from_secs(3) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        !runs(&child),
        "the child outlived the task that asked for it"
    );
    assert!(!runs(&grandchild), "its process group outlived the task");
    Ok(())
}
