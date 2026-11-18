use super::*;

#[test]
fn command_spec_exposes_execution_values_but_redacts_every_recording_surface() {
    let runtime_environment = RuntimeEnvironment::from_sensitive_values(BTreeMap::from([(
        "TOKEN".to_owned(),
        "runtime-secret".to_owned(),
    )]));
    let mut specification = CommandSpec::new("tool")
        .args(["first", "second"])
        .args_append(["third"])
        .arg("public")
        .sensitive_arg("argument-secret".to_owned())
        .redact_value(String::new())
        .redact_value("output-secret".to_owned())
        .environment("PUBLIC", "visible")
        .sensitive_environment("PASSWORD", "environment-secret".to_owned())
        .interpolation_environment(&runtime_environment)
        .current_dir("/workspace")
        .inherit_stdin()
        .sensitive_stdin(Zeroizing::new("stdin-secret".to_owned()))
        .timeout(Duration::from_secs(9))
        .with_output_limit(123);

    assert_eq!(specification.program(), "tool");
    assert_eq!(
        specification.arguments().collect::<Vec<_>>(),
        ["first", "second", "third", "public", "argument-secret"]
    );
    assert_eq!(
        specification
            .environment_values()
            .collect::<BTreeMap<_, _>>(),
        BTreeMap::from([
            ("PASSWORD", "environment-secret"),
            ("PUBLIC", "visible"),
            ("TOKEN", "runtime-secret"),
        ])
    );
    let sensitive = specification.sensitive_values().collect::<Vec<_>>();
    for expected in [
        "argument-secret",
        "environment-secret",
        "runtime-secret",
        "output-secret",
        "stdin-secret",
    ] {
        assert!(sensitive.contains(&expected), "{expected}");
    }
    assert_eq!(
        specification.current_directory(),
        Some(&PathBuf::from("/workspace"))
    );
    assert_eq!(specification.timeout_value(), Duration::from_secs(9));
    assert_eq!(specification.output_limit(), 123);
    assert!(!specification.should_inherit_stdin());
    assert_eq!(
        specification.redacted_display(),
        "tool first second third public [REDACTED]"
    );
    let debug = format!("{specification:?}");
    assert!(!debug.contains("secret"));
    assert!(debug.contains("[REDACTED]"));
    assert_eq!(
        specification
            .take_sensitive_stdin()
            .as_deref()
            .map(String::as_str),
        Some("stdin-secret")
    );
    assert!(specification.take_sensitive_stdin().is_none());

    let empty = CommandSpec::new("tool");
    assert_eq!(empty.redacted_display(), "tool");
    assert!(empty.redacted_record().arguments.is_empty());
}

#[cfg(unix)]
#[test]
fn command_spec_ignores_non_utf8_inherited_environment_values()
-> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    const CHILD: &str = "MM_TEST_NON_UTF8_ENV_CHILD";
    const KEY: &str = "MM_TEST_NON_UTF8_ENV_VALUE";
    if std::env::var_os(CHILD).is_some() {
        let specification = CommandSpec::new("tool").inherit_environment(&[KEY]);
        assert!(specification.redacted_record().environment_keys.is_empty());
        return Ok(());
    }

    let status = std::process::Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg(
                "app::ports::process::tests::command_spec_ignores_non_utf8_inherited_environment_values",
            )
            .env(CHILD, "1")
            .env(KEY, OsString::from_vec(vec![b'v', 0xff]))
            .status()?;
    assert!(status.success());
    Ok(())
}

#[test]
fn stream_specification_preserves_process_controls_without_exposing_environment_values() {
    let environment = RuntimeEnvironment::from_sensitive_values(BTreeMap::from([(
        "TOKEN".to_owned(),
        "stream-secret".to_owned(),
    )]));
    let specification = StreamCommandSpec::new("stream-tool")
        .args(["logs", "service"])
        .current_dir("/workspace")
        .sensitive_environment(environment)
        .inherit_environment(&["PATH", "HOME"])
        .stdin(ExecStdinMode::Inherit)
        .tty(ExecTtyMode::Enabled)
        .timeout(Duration::from_secs(15))
        .buffered_events(42);
    assert_eq!(specification.program(), "stream-tool");
    assert_eq!(
        specification.arguments().collect::<Vec<_>>(),
        ["logs", "service"]
    );
    assert_eq!(
        specification.current_directory(),
        Some(&PathBuf::from("/workspace"))
    );
    assert_eq!(
        specification.environment_values().collect::<Vec<_>>(),
        [("TOKEN", "stream-secret")]
    );
    assert_eq!(specification.inherited_environment(), ["PATH", "HOME"]);
    assert_eq!(specification.stdin_mode(), ExecStdinMode::Inherit);
    assert_eq!(specification.tty_mode(), ExecTtyMode::Enabled);
    assert_eq!(specification.timeout_value(), Duration::from_secs(15));
    assert_eq!(specification.event_limit(), 42);
    let record = specification.redacted_record();
    assert_eq!(record.environment_keys, ["TOKEN"]);
    assert_eq!(record.environment_count, 1);
    assert!(!format!("{specification:?}").contains("stream-secret"));
}

#[test]
fn command_output_debug_is_redacted_and_success_requires_zero() {
    let success = CommandOutput {
        code: Some(0),
        stdout: Zeroizing::new("stdout-secret".to_owned()),
        stderr: Zeroizing::new("stderr-secret".to_owned()),
        truncated: true,
    };
    assert!(success.success());
    let debug = format!("{success:?}");
    assert!(!debug.contains("stdout-secret"));
    assert!(!debug.contains("stderr-secret"));
    assert!(debug.contains("[REDACTED]"));
    for code in [None, Some(1)] {
        let output = CommandOutput {
            code,
            stdout: Zeroizing::new(String::new()),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        };
        assert!(!output.success());
    }
}

#[tokio::test]
async fn process_stream_cancels_only_incomplete_streams()
-> Result<(), tokio::sync::mpsc::error::SendError<ProcessStreamEvent>> {
    let cancellation = OperationCancellation::default();
    let (sender, receiver) = tokio::sync::mpsc::channel(2);
    let mut stream = ProcessStream::new(receiver, cancellation.clone());
    sender
        .send(ProcessStreamEvent::Output {
            source: ProcessOutputSource::Stdout,
            bytes: b"output".to_vec(),
        })
        .await?;
    sender
        .send(ProcessStreamEvent::Exited { code: Some(0) })
        .await?;
    assert!(matches!(
        stream.recv().await,
        Some(ProcessStreamEvent::Output { .. })
    ));
    assert!(matches!(
        stream.recv().await,
        Some(ProcessStreamEvent::Exited { code: Some(0) })
    ));
    drop(stream);
    assert!(!cancellation.is_cancelled());

    let cancellation = OperationCancellation::default();
    let (_sender, receiver) = tokio::sync::mpsc::channel(1);
    let stream = ProcessStream::new(receiver, cancellation.clone());
    stream.cancel();
    assert!(cancellation.is_cancelled());
    Ok(())
}
