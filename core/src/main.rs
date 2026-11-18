use std::process::ExitCode;

use clap::Parser;
use mm::presentation::cli::Cli;

#[tokio::main]
async fn main() -> ExitCode {
    initialize_tracing();
    let parsing = mm::app::timing::phase("cli.parse");
    let parsed = Cli::try_parse();
    drop(parsing);
    let cli = match parsed {
        Ok(cli) => cli,
        Err(error) => {
            let code = error.exit_code();
            if requested_json_output() && code != 0 {
                let value = serde_json::json!({
                    "error": {
                        "code": "MM_CLI_PARSE",
                        "message": error.to_string(),
                    }
                });
                eprintln!("{value}");
            } else if let Err(print_error) = error.print() {
                eprintln!("error[MM_OUTPUT]: {print_error}");
            }
            return ExitCode::from(u8::try_from(code).unwrap_or(2));
        }
    };
    let output = cli.output;
    let recovery_context = cli.recovery_context();
    let mcp_stdio = matches!(
        &cli.command,
        Some(mm::presentation::cli::Command::Mcp { .. })
    );
    let recording = mm::composition::CommandRecording::start(&cli);
    let cancellation = mm::domain::observability::OperationCancellation::default();
    let operation = mm::composition::run(cli, cancellation.clone());
    tokio::pin!(operation);
    let result = tokio::select! {
        result = &mut operation => Some(result),
        () = shutdown_signal() => {
            cancellation.cancel();
            match tokio::time::timeout(std::time::Duration::from_secs(2), &mut operation).await {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => tracing::warn!(code = error.code(), %error, "operation failed during cancellation"),
                Err(_) => tracing::warn!("operation did not stop within the cancellation deadline"),
            }
            None
        },
    };
    mm::app::timing::mark("command finished");
    if let Some(recording) = recording {
        match &result {
            None => recording.finish(None, None),
            Some(Ok(code)) => recording.finish(Some(*code), None),
            Some(Err(error)) => recording.finish(Some(error.exit_code()), Some(error)),
        }
    }
    match result {
        None => ExitCode::from(130),
        Some(Ok(code)) => ExitCode::from(code),
        Some(Err(error)) => {
            if mcp_stdio {
                eprintln!("error[{}]: {error}", error.code());
            } else if let Err(write_error) = mm::presentation::output::write_error_with_context(
                &error,
                output,
                &recovery_context,
            ) {
                eprintln!("error[MM_OUTPUT]: {write_error}");
            }
            ExitCode::from(error.exit_code())
        }
    }
}

#[cfg(unix)]
async fn shutdown_signal() {
    let terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
    if let Ok(mut terminate) = terminate {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    } else {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(%error, "Ctrl-C signal listener failed");
        }
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::warn!(%error, "Ctrl-C signal listener failed");
    }
}

fn requested_json_output() -> bool {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    arguments.windows(2).any(|pair| {
        pair[0] == "--output"
            && pair[1]
                .to_str()
                .is_some_and(|value| value.eq_ignore_ascii_case("json"))
    }) || arguments
        .iter()
        .filter_map(|argument| argument.to_str())
        .any(|argument| argument.eq_ignore_ascii_case("--output=json"))
}

fn initialize_tracing() {
    use tracing_subscriber::EnvFilter;

    let json =
        std::env::var("MM_LOG_FORMAT").is_ok_and(|format| format.eq_ignore_ascii_case("json"));
    // `MM_TIMING=1` adds the timing trace (CONTRIBUTING.md).
    if mm::composition::install_timing_trace(json, true) {
        return;
    }
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    if json {
        if let Err(error) = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .with_writer(std::io::stderr)
            .try_init()
        {
            eprintln!("warning[MM_TELEMETRY]: tracing initialization failed: {error}");
        }
    } else {
        if let Err(error) = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .try_init()
        {
            eprintln!("warning[MM_TELEMETRY]: tracing initialization failed: {error}");
        }
    }
}
