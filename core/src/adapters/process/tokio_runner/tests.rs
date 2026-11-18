use tokio::io::{AsyncWriteExt, duplex};

use super::{
    BoundedOutput, read_bounded, redact, redact_output, report_kill_result, send_process_error,
    sensitive_values,
};
use crate::app::ports::{CommandSpec, ProcessStreamEvent};
use crate::domain::error::MmError;

#[tokio::test]
async fn drains_output_while_retaining_only_the_limit() -> Result<(), std::io::Error> {
    let (mut writer, reader) = duplex(64);
    let writing = tokio::spawn(async move {
        writer.write_all(&vec![b'x'; 1024]).await?;
        writer.shutdown().await
    });
    let output = read_bounded(reader, 16).await?;
    writing.await.map_err(std::io::Error::other)??;
    assert!(output.truncated);
    assert!(output.value.starts_with("xxxxxxxxxxxxxxxx"));
    assert!(output.value.ends_with("[output truncated by mm]"));
    Ok(())
}

#[test]
fn redacts_marked_arguments_and_environment_values() {
    let specification = CommandSpec::new("example")
        .args(["--token"])
        .sensitive_arg("secret-value".to_owned())
        .sensitive_environment("PASSWORD", "another-secret".to_owned());
    let values = sensitive_values(&specification);
    assert_eq!(
        redact(
            "secret-value and another-secret must not escape".to_owned(),
            &values
        ),
        "[REDACTED] and [REDACTED] must not escape"
    );
    let debug = format!("{specification:?}");
    assert!(!debug.contains("secret-value"));
    assert!(!debug.contains("another-secret"));
    assert_eq!(
        redact_output(
            BoundedOutput {
                value: "partial-secret".to_owned(),
                truncated: true,
            },
            &values,
            true,
        ),
        "[truncated output suppressed because the command used sensitive values]"
    );
}

#[tokio::test]
async fn process_kill_failure_is_delivered_to_the_stream() {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    report_kill_result(
        Err(std::io::Error::other("injected kill failure")),
        &sender,
        "faulty-process",
    )
    .await;
    assert!(matches!(
        receiver.recv().await,
        Some(ProcessStreamEvent::Failed(MmError::ProcessSpawn { .. }))
    ));
}

#[tokio::test]
async fn closed_receiver_before_final_event_is_an_expected_shutdown() {
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    drop(receiver);
    send_process_error(
        &sender,
        "closed-stream",
        std::io::Error::other("injected late failure"),
    )
    .await;
}
