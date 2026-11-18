//! What an environment still needs before it can start, and running a fix
//! for it in the plain terminal.

use async_trait::async_trait;

use crate::domain::readiness::SetupIssue;
use crate::presentation::tui::model::{Model, Notice};
use crate::presentation::tui::update::Action;

/// The setup state of the environment on screen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SetupView {
    /// What is left to fix, in the order to fix it.
    pub steps: Vec<SetupStep>,
    /// Why the environment could not be read at all. Its views are empty
    /// until this clears.
    pub unavailable: Option<String>,
}

impl SetupView {
    /// Whether the Overview has anything to say about setup.
    #[must_use]
    pub fn is_open(&self) -> bool {
        !self.steps.is_empty() || self.unavailable.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupStep {
    pub title: String,
    pub detail: String,
    /// The command that does the same from a shell.
    pub command: Option<String>,
    /// The diagnostic that reports the same problem, so it is shown once.
    pub code: Option<&'static str>,
    /// What running the step fixes; `None` when the fix is manual.
    pub issue: Option<SetupIssue>,
    /// False while an earlier step has to be finished first.
    pub ready: bool,
}

/// Runs the fix for a setup step. Implementations call the handler of the
/// matching command.
#[async_trait]
pub trait SetupGuide: Send + Sync {
    /// Runs in the plain terminal while the TUI is suspended: it may print,
    /// ask questions, and take minutes. It reports its own errors and returns
    /// whether it finished without one.
    async fn run(&self, issue: &SetupIssue) -> bool;
}

/// `enter` on the Overview's setup list.
pub(super) fn run_selected(model: &mut Model) -> Option<Action> {
    let step = model.setup.steps.get(model.scroll)?;
    if let Some(busy) = &model.busy {
        model.notice = Some(Notice::info(format!(
            "{} is still running; wait for it to finish.",
            busy.progress(model.remote().is_some())
        )));
        return Some(Action::None);
    }
    if !step.ready {
        model.notice = Some(Notice::info(
            "This step needs the ones above it. Do those first.",
        ));
        return Some(Action::None);
    }
    if step.issue.is_none() {
        model.notice = Some(Notice::info(match &step.command {
            Some(command) => format!(
                "mm cannot do this step for you. Start with `{command}`; the list updates by itself."
            ),
            None => {
                "mm cannot do this step for you. Fix it and the list updates by itself.".to_owned()
            }
        }));
        return Some(Action::None);
    }
    Some(Action::RunSetup(model.scroll))
}

/// Start is pointless while setup is unfinished: say where the steps are.
pub(super) fn blocks_start(model: &mut Model) -> bool {
    if model.remote().is_some() || model.setup.steps.is_empty() {
        return false;
    }
    let count = model.setup.steps.len();
    model.notice = Some(Notice::info(format!(
        "{} cannot start yet: {} left on the Overview.",
        model.snapshot.environment,
        if count == 1 {
            "1 setup step is".to_owned()
        } else {
            format!("{count} setup steps are")
        }
    )));
    true
}

#[cfg(test)]
mod tests {
    use super::SetupView;
    use crate::presentation::tui::fixtures::{model, setup_view};
    use crate::presentation::tui::model::LiveStatus;

    #[test]
    fn an_environment_that_becomes_readable_is_due_for_a_reload() {
        let snapshot = model().snapshot;
        let live = |setup: Option<SetupView>| LiveStatus {
            snapshot: snapshot.clone(),
            environment: None,
            tls: None,
            installation_services: None,
            setup,
        };
        let mut model = model();
        model.setup = SetupView {
            unavailable: Some("target 'local' is not configured".to_owned()),
            ..setup_view()
        };
        model.scroll = 1;

        // A refresh that did not read setup keeps what is shown.
        model.apply_live(Ok(live(None)));
        assert!(model.setup.unavailable.is_some());
        assert!(!model.reload_due);

        // Still unavailable, one step fewer: the list follows, no reload.
        let mut fewer = model.setup.clone();
        fewer.steps.truncate(1);
        model.apply_live(Ok(live(Some(fewer))));
        assert_eq!(model.setup.steps.len(), 1);
        assert_eq!(model.scroll, 0, "selection stays on a step");
        assert!(!model.reload_due);

        model.apply_live(Ok(live(Some(SetupView::default()))));
        assert!(model.reload_due);
        assert!(!model.setup.is_open());

        // Steps clearing on a readable environment need no reload.
        model.reload_due = false;
        model.setup = setup_view();
        model.apply_live(Ok(live(Some(SetupView::default()))));
        assert!(!model.reload_due);
    }
}
