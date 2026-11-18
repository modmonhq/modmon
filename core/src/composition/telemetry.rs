//! Log output for the binaries when the timing trace is on.

use tracing_subscriber::layer::{Layer as _, SubscriberExt as _};
use tracing_subscriber::util::SubscriberInitExt as _;

use crate::adapters::timing::TimingLayer;

/// With `MM_TIMING` set, installs the usual log output on standard error
/// together with the timing trace, and returns `true`. Returns `false` when
/// the variable is not set: the caller installs its usual output. Call it
/// first thing in `main`; the trace counts from here.
pub fn install_timing_trace(json: bool, ansi: bool) -> bool {
    let Some(timing) = TimingLayer::from_environment() else {
        return false;
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let output = tracing_subscriber::fmt::layer()
        .with_ansi(ansi)
        .with_writer(std::io::stderr);
    let output = if json {
        output.json().boxed()
    } else {
        output.boxed()
    };
    if let Err(error) = tracing_subscriber::registry()
        .with(output.with_filter(filter))
        .with(timing.with_filter(TimingLayer::filter()))
        .try_init()
    {
        eprintln!("warning[MM_TELEMETRY]: tracing initialization failed: {error}");
    }
    true
}
