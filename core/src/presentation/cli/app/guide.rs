use super::*;

/// Runs a setup step for the TUI with the handler of its command.
pub(super) struct SetupGuide {
    pub(super) invocation: super::super::dispatch::Invocation,
}

#[async_trait::async_trait]
impl tui::SetupGuide for SetupGuide {
    async fn run(&self, issue: &crate::domain::readiness::SetupIssue) -> bool {
        let invocation = &self.invocation;
        let Err(error) = super::super::setup::fix(issue, invocation).await else {
            return true;
        };
        if let Err(error) = output::write_error_with_context(
            &error,
            OutputFormat::Human,
            &invocation.recovery_context,
        ) {
            tracing::warn!(%error, "could not report a failed setup step");
        }
        false
    }
}
