//! Secrets and variables set on a remote environment's hosts: every node
//! stages a change, or none does, and the next deployment makes it live.

use std::collections::BTreeSet;

use futures_util::future::join_all;
use zeroize::Zeroizing;

use super::{RemoteEnvironment, RemoteNode, in_node};
use crate::domain::error::MmError;
use crate::domain::staged::{
    NodeValues, RemoteValues, StageStep, StagedAction, StagedPayload, StagedPayloadValue,
    UnreachableNode, ValueChange, ValueKind, ValueStage, validate_key, validate_value,
};

/// Seals a stage's values to one node's store recipient.
pub trait ValueSealer: Send + Sync {
    fn seal(&self, recipient: &str, payload: &StagedPayload) -> Result<String, MmError>;
}

/// What to do with one key.
pub enum ValueEdit {
    Set(Zeroizing<String>),
    Unset,
    Discard,
}

/// One requested change. The value never leaves this machine unsealed.
pub struct KeyEdit {
    pub kind: ValueKind,
    pub key: String,
    pub edit: ValueEdit,
}

/// What a stage changed, and on which nodes. Never a value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct StageReport {
    pub target: String,
    pub environment: String,
    pub stage: uuid::Uuid,
    pub nodes: Vec<String>,
    pub changes: Vec<ValueChange>,
}

impl RemoteEnvironment {
    /// The secrets whose readers all fall back to a template default; a
    /// secret among them with nothing live reads as `defaulted`.
    #[must_use]
    pub fn with_template_defaults(mut self, secrets: BTreeSet<String>) -> Self {
        self.template_defaults = secrets;
        self
    }

    /// What every node holds and has staged for the deployed commit's
    /// secrets and variables. A node that does not answer is listed.
    pub async fn host_values(&self) -> Result<RemoteValues, MmError> {
        let answers = join_all(self.nodes.iter().map(|node| async move {
            node.connect()?
                .host_values(node.scope.clone())
                .await
                .map_err(|error| in_node(node.name.as_deref(), error))
        }))
        .await;
        let mut values = RemoteValues {
            target: self.target.clone(),
            environment: self.target_scope().environment,
            nodes: Vec::new(),
            unreachable: Vec::new(),
        };
        let mut first_error = None;
        for (node, answer) in self.nodes.iter().zip(answers) {
            match answer {
                Ok(answer) => values.nodes.push(self.mark_template_defaults(answer)),
                Err(error) => {
                    values.unreachable.push(UnreachableNode {
                        node: self.name_of(node).to_owned(),
                        problem: error.to_string(),
                    });
                    first_error.get_or_insert(error);
                }
            }
        }
        match first_error {
            Some(error) if values.nodes.is_empty() => Err(error),
            _ => Ok(values),
        }
    }

    fn mark_template_defaults(&self, mut answer: NodeValues) -> NodeValues {
        for value in &mut answer.values {
            value.defaulted = value.kind == ValueKind::Secret
                && value.active.is_none()
                && value.origin.is_none()
                && self.template_defaults.contains(&value.key);
        }
        answer
    }

    /// Stages `edits` on every node of the environment, or on none: each
    /// node checks and keeps the change, then every node commits it. When a
    /// node cannot, the nodes that took it put back what they had.
    pub async fn stage_values(
        &self,
        edits: Vec<KeyEdit>,
        sealer: &dyn ValueSealer,
    ) -> Result<StageReport, MmError> {
        let stage = uuid::Uuid::new_v4();
        let scope = self.target_scope();
        let mut changes = Vec::with_capacity(edits.len());
        let mut values = Vec::new();
        for edit in edits {
            validate_key(&edit.key).map_err(|message| MmError::Environment { message })?;
            let action = match edit.edit {
                ValueEdit::Set(value) => {
                    validate_value(&edit.key, &value)
                        .map_err(|message| MmError::Environment { message })?;
                    let revision = uuid::Uuid::new_v4().to_string();
                    values.push(StagedPayloadValue {
                        key: edit.key.clone(),
                        kind: edit.kind,
                        revision: revision.clone(),
                        value,
                    });
                    StagedAction::Set { revision }
                }
                ValueEdit::Unset => StagedAction::Unset {
                    revision: uuid::Uuid::new_v4().to_string(),
                },
                ValueEdit::Discard => StagedAction::Discard,
            };
            changes.push(ValueChange {
                key: edit.key,
                kind: edit.kind,
                action,
            });
        }
        let payload = StagedPayload {
            project_id: scope.project_id,
            environment: scope.environment.clone(),
            stage,
            values,
        };
        let sealed = self.sealed_for_nodes(&payload, sealer).await?;
        let nodes = self.nodes.iter().collect::<Vec<_>>();
        let prepared = join_all(nodes.iter().zip(&sealed).map(|(node, sealed)| {
            let step = StageStep::Prepare {
                changes: changes.clone(),
                sealed: sealed.clone(),
            };
            self.stage_step(node, stage, step)
        }))
        .await;
        if prepared.iter().any(Result::is_err) {
            let took = nodes
                .iter()
                .zip(&prepared)
                .filter(|(_, answer)| answer.is_ok())
                .map(|(node, _)| *node)
                .collect::<Vec<_>>();
            let left = self.revert(&took, stage).await;
            return Err(self.stage_failed(&nodes, prepared, &left));
        }
        let committed = join_all(
            nodes
                .iter()
                .map(|node| self.stage_step(node, stage, StageStep::Commit)),
        )
        .await;
        if committed.iter().any(Result::is_err) {
            let left = self.revert(&nodes, stage).await;
            return Err(self.stage_failed(&nodes, committed, &left));
        }
        Ok(StageReport {
            target: self.target.clone(),
            environment: scope.environment,
            stage,
            nodes: nodes
                .iter()
                .map(|node| self.name_of(node).to_owned())
                .collect(),
            changes,
        })
    }

    /// Per node, the stage's values sealed to its store: `None` per node
    /// when the stage sets no value.
    async fn sealed_for_nodes(
        &self,
        payload: &StagedPayload,
        sealer: &dyn ValueSealer,
    ) -> Result<Vec<Option<String>>, MmError> {
        if payload.values.is_empty() {
            return Ok(vec![None; self.nodes.len()]);
        }
        let recipients = join_all(self.nodes.iter().map(|node| async move {
            node.connect()?
                .secret_recipient(node.scope.clone())
                .await
                .map_err(|error| in_node(node.name.as_deref(), error))
        }))
        .await;
        let asked = self.nodes.iter().collect::<Vec<_>>();
        self.all_or_explain(&asked, recipients)?
            .iter()
            .map(|recipient| sealer.seal(recipient, payload).map(Some))
            .collect()
    }

    async fn stage_step(
        &self,
        node: &RemoteNode,
        stage: uuid::Uuid,
        step: StageStep,
    ) -> Result<(), MmError> {
        node.connect()?
            .stage_values(node.scope.clone(), ValueStage { stage, step })
            .await
            .map(drop)
            .map_err(|error| in_node(node.name.as_deref(), error))
    }

    /// Reverts `stage` on `nodes`; the names of the nodes that could not.
    async fn revert(&self, nodes: &[&RemoteNode], stage: uuid::Uuid) -> Vec<String> {
        let answers = join_all(
            nodes
                .iter()
                .map(|node| self.stage_step(node, stage, StageStep::Revert)),
        )
        .await;
        nodes
            .iter()
            .zip(answers)
            .filter(|(_, answer)| answer.is_err())
            .map(|(node, _)| self.name_of(node).to_owned())
            .collect()
    }

    fn stage_failed(
        &self,
        nodes: &[&RemoteNode],
        answers: Vec<Result<(), MmError>>,
        left: &[String],
    ) -> MmError {
        let reasons = nodes
            .iter()
            .zip(answers)
            .filter_map(|(_, answer)| answer.err().map(|error| error.to_string()))
            .collect::<Vec<_>>()
            .join("; ");
        let environment = self.target_scope().environment;
        let outcome = if left.is_empty() {
            "No node keeps it; run the command again once every node answers".to_owned()
        } else {
            format!(
                "{} may still hold it staged: `mm -e {environment} secret status` shows what each node holds, and `discard` drops it",
                left.join(", ")
            )
        };
        MmError::Environment {
            message: format!("the change was not staged: {reasons}. {outcome}"),
        }
    }
}
