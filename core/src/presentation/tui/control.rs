use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::domain::error::MmError;
use crate::domain::rollout::RolloutProgress;
use crate::presentation::tui::model::{Model, Notice};
use crate::presentation::tui::update::Action;

/// A reviewed deployment the operator confirms before it is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartReview {
    pub plan_hash: String,
    pub target: String,
    pub stacks: Vec<String>,
    pub routes: usize,
    /// The environment runs on another host or on a cluster's nodes.
    pub remote: bool,
    /// The nodes the plan applies to, in the order of the walk; empty for
    /// one host.
    pub nodes: Vec<String>,
    /// What `mm deploy plan` prints for this plan, line by line.
    pub lines: Vec<String>,
    /// For a rollback: what it undoes and where it returns, as `mm deploy
    /// rollback` says it.
    pub rollback: Option<String>,
}

impl StartReview {
    #[must_use]
    pub fn short_hash(&self) -> &str {
        self.plan_hash.get(..12).unwrap_or(&self.plan_hash)
    }
}

/// Where an apply reports each step a node starts.
pub type ProgressSink = std::sync::Arc<dyn Fn(&RolloutProgress) + Send + Sync>;

/// Workspace mutations requested from the TUI; implementations call reviewed use cases.
#[async_trait]
pub trait WorkspaceControl: Send + Sync {
    /// Plans a deployment of the current configuration without changing anything.
    async fn review_start(&self) -> Result<StartReview, MmError>;
    /// Plans a return to what a remote environment ran before, as `mm
    /// deploy rollback` does, without changing anything.
    async fn review_rollback(&self) -> Result<StartReview, MmError>;
    /// Applies exactly the reviewed plan and returns a summary, telling
    /// `progress` each step a node starts.
    async fn start(&self, plan_hash: String, progress: ProgressSink) -> Result<String, MmError>;
    /// Stops the project's containers, keeping volumes, routes, and configuration.
    async fn stop(&self) -> Result<String, MmError>;
    /// Persists a workspace-local override for a non-secret global key.
    fn set_override(&self, key: &str, value: &str) -> Result<(), MmError>;
    /// Removes a workspace-local override; `false` when none existed.
    fn unset_override(&self, key: &str) -> Result<bool, MmError>;
    /// The secret bound to a global key in the current environment, if any.
    fn secret_for_key(&self, key: &str) -> Result<Option<String>, MmError>;
}

/// Long-running stack operations; they run off the input loop so the UI stays live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackOperation {
    Review,
    /// Plan a rollback for review.
    Rollback,
    Start(String),
    Stop,
}

impl StackOperation {
    /// What the header says while it runs.
    #[must_use]
    pub const fn progress(&self, remote: bool) -> &'static str {
        match self {
            Self::Review => "Reviewing deployment plan",
            Self::Rollback => "Planning rollback",
            Self::Start(_) if remote => "Deploying",
            Self::Start(_) => "Starting stack",
            Self::Stop => "Stopping stack",
        }
    }
}

#[derive(Debug)]
pub enum StackOutcome {
    Reviewed(Result<StartReview, String>),
    /// A node of the apply started a step.
    Progress(RolloutProgress),
    Started(Result<String, String>),
    Stopped(Result<String, String>),
}

/// Spawns stack operations; outcomes arrive on the receiver returned by `new`.
pub(super) struct StackRunner {
    control: Arc<dyn WorkspaceControl>,
    sender: mpsc::UnboundedSender<StackOutcome>,
}

impl StackRunner {
    pub(super) fn new(
        control: Arc<dyn WorkspaceControl>,
    ) -> (Self, mpsc::UnboundedReceiver<StackOutcome>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self { control, sender }, receiver)
    }

    fn spawn(&self, operation: StackOperation) {
        let control = Arc::clone(&self.control);
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let steps = sender.clone();
            let progress: ProgressSink = Arc::new(move |step: &RolloutProgress| {
                if steps.send(StackOutcome::Progress(step.clone())).is_err() {
                    tracing::debug!("TUI closed while a deployment was applied");
                }
            });
            let outcome = match operation {
                StackOperation::Review => {
                    StackOutcome::Reviewed(control.review_start().await.map_err(message))
                }
                StackOperation::Rollback => {
                    StackOutcome::Reviewed(control.review_rollback().await.map_err(message))
                }
                StackOperation::Start(plan_hash) => {
                    StackOutcome::Started(control.start(plan_hash, progress).await.map_err(message))
                }
                StackOperation::Stop => {
                    StackOutcome::Stopped(control.stop().await.map_err(message))
                }
            };
            if sender.send(outcome).is_err() {
                tracing::debug!("TUI closed before a stack operation finished");
            }
        });
    }
}

/// An error as the command line prints it: its code, what happened, and
/// what to run next.
fn message(error: MmError) -> String {
    crate::presentation::output::error_line(&error)
}

/// Applies stack, override, secret, and refresh actions.
pub(super) fn apply_control_action(
    model: &mut Model,
    action: Action,
    use_cases: &super::TuiUseCases<'_>,
) -> Option<super::SecretInputRequest> {
    let control = use_cases.stack.control.as_ref();
    match action {
        Action::Stack(operation) => {
            if model.busy.is_none() {
                model.busy = Some(operation.clone());
                use_cases.stack.spawn(operation);
            }
        }
        Action::SetOverride { key, value } if model.remote().is_some() => {
            use_cases
                .operations
                .stage(super::host_values::StageRequest::SetVariable { key, value }.edit());
        }
        Action::SetOverride { key, value } => {
            model.notice = Some(match control.set_override(&key, &value) {
                Ok(()) => {
                    model.env.pending_restart = true;
                    Notice::success(format!(
                        "Saved override {key}. Press u to restart the stack with it."
                    ))
                }
                Err(error) => Notice::error(crate::presentation::output::error_line(&error)),
            });
            use_cases.refresh_now.notify_one();
        }
        Action::UnsetOverride(key) => {
            model.notice = Some(match control.unset_override(&key) {
                Ok(true) => {
                    model.env.pending_restart = true;
                    Notice::success(format!(
                        "Removed override {key}. Press u to restart the stack without it."
                    ))
                }
                Ok(false) => Notice::info(format!("{key} had no local override.")),
                Err(error) => Notice::error(crate::presentation::output::error_line(&error)),
            });
            use_cases.refresh_now.notify_one();
        }
        Action::EditSecret(key) => return secret_request(model, &key, use_cases),
        Action::EditRemoteSecret(name) => {
            return Some(super::SecretInputRequest {
                prompt: format!("Value for {name}, staged on every host"),
                pending: super::PendingSecretApply::RemoteSecret { name },
            });
        }
        Action::Refresh => use_cases.refresh_now.notify_one(),
        _ => {}
    }
    None
}

fn secret_request(
    model: &mut Model,
    key: &str,
    use_cases: &super::TuiUseCases<'_>,
) -> Option<super::SecretInputRequest> {
    let name = match use_cases.stack.control.secret_for_key(key) {
        Ok(Some(name)) => name,
        Ok(None) => {
            model.notice = Some(Notice::info(format!(
                "{key} is sensitive but not bound to a secret in this environment."
            )));
            return None;
        }
        Err(error) => {
            model.notice = Some(Notice::error(crate::presentation::output::error_line(
                &error,
            )));
            return None;
        }
    };
    match use_cases
        .secrets
        .input_prompt(&model.snapshot.workspace.root, &name)
    {
        Ok(prompt) => Some(super::SecretInputRequest {
            prompt: format!("{prompt} ({name})"),
            pending: super::PendingSecretApply::EnvironmentSecret { name },
        }),
        Err(error) => {
            model.notice = Some(Notice::error(crate::presentation::output::error_line(
                &error,
            )));
            None
        }
    }
}

pub(super) fn secret_saved(
    model: &mut Model,
    name: &str,
    result: Result<crate::domain::secret::SecretRotationResultItem, MmError>,
) {
    model.notice = Some(match result {
        Ok(saved) => {
            model.env.pending_restart = true;
            Notice::success(format!(
                "Saved secret {name} in {} (revision {}). Press u to restart the stack with it.",
                saved.provider, saved.revision
            ))
        }
        Err(error) => Notice::error(crate::presentation::output::error_line(&error)),
    });
}

/// Records a finished stack operation or a step of an apply; after a
/// review, opens it.
pub(super) fn apply_outcome(model: &mut Model, outcome: StackOutcome) {
    if let StackOutcome::Progress(step) = &outcome {
        super::review::progress(model, step);
        return;
    }
    model.busy = None;
    match outcome {
        StackOutcome::Reviewed(Ok(review)) => super::review::open(model, review),
        StackOutcome::Started(result) => {
            if result.is_ok() {
                model.env.pending_restart = false;
            }
            // The review shows how the apply ended, with each node.
            match &mut model.review {
                Some(review) => review.finish(result),
                None => {
                    model.notice = Some(match result {
                        Ok(summary) => Notice::success(summary),
                        Err(error) => Notice::error(error),
                    });
                }
            }
        }
        StackOutcome::Stopped(Ok(summary)) => model.notice = Some(Notice::success(summary)),
        StackOutcome::Reviewed(Err(error)) | StackOutcome::Stopped(Err(error)) => {
            model.notice = Some(Notice::error(error));
        }
        StackOutcome::Progress(_) => {}
    }
}
