//! Phases of a command, for the timing trace.
//!
//! A phase is a span on the `mm::timing` target. Nothing listens to that
//! target unless `MM_TIMING` is set, and then each phase is printed with how
//! long it took (`CONTRIBUTING.md`, "Timing trace"). Name a phase after what
//! is waited for, and put the host or the command in `detail`; never a
//! secret value.

/// The target every phase and mark is recorded on.
pub const TARGET: &str = "mm::timing";

/// Starts a phase. It ends when the returned span is dropped, also across
/// `await`: the trace measures from here to the drop.
#[must_use = "a phase ends when this is dropped"]
pub fn phase(name: &'static str) -> tracing::Span {
    tracing::trace_span!(target: "mm::timing", "phase", name)
}

/// Starts a phase that says what it is about, such as a host or a command.
#[must_use = "a phase ends when this is dropped"]
pub fn phase_of(name: &'static str, detail: &dyn std::fmt::Display) -> tracing::Span {
    tracing::trace_span!(target: "mm::timing", "phase", name, detail = %detail)
}

/// Records a moment, such as the first frame of the terminal UI.
pub fn mark(name: &'static str) {
    tracing::trace!(target: "mm::timing", mark = name);
}
