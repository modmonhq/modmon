//! A rollout wave by wave (`ClusterRollout::waves`): a stage of its own as
//! before, and the stages of a shared wave side by side, each node rolling
//! out its stacks of the wave in one `stacks` step.

use super::*;
use crate::domain::rollout::STACKS_AT_ONCE;

impl Walk<'_> {
    /// Every wave in order; returns where the rollout stopped, if it did.
    pub(super) async fn waves(&mut self) -> Option<Fault> {
        let rollout = self.rollout;
        for wave in &rollout.waves {
            let stages = wave
                .iter()
                .filter_map(|index| rollout.stages.get(*index))
                .collect::<Vec<_>>();
            let fault = match stages.as_slice() {
                [stage] => self.stage(stage).await,
                _ => self.shared_wave(&stages).await,
            };
            if fault.is_some() {
                return fault;
            }
        }
        None
    }

    /// The stages of one wave on every node that runs one of them, at the
    /// same time; then every node has to resolve what they started.
    async fn shared_wave(&mut self, stages: &[&RolloutStage]) -> Option<Fault> {
        let indices = (0..self.nodes.len())
            .filter(|index| {
                stages.iter().any(|stage| {
                    stage
                        .batches
                        .iter()
                        .flatten()
                        .any(|node| node == self.name(*index))
                })
            })
            .collect::<Vec<_>>();
        let rolled = join_all(indices.iter().map(|index| {
            let stacks = stages
                .iter()
                .filter(|stage| {
                    stage
                        .batches
                        .iter()
                        .flatten()
                        .any(|node| node == self.name(*index))
                })
                .map(|stage| stage.stack.clone())
                .collect::<Vec<_>>();
            self.roll_stacks_on(*index, stacks, self.begun.contains(index))
        }))
        .await;
        let mut fault = None;
        let mut addresses = Vec::new();
        let mut healthy = Vec::new();
        for (step, (stacks, found)) in rolled {
            if step.began && !self.begun.contains(&step.index) {
                self.begun.push(step.index);
            }
            fault = fault.or(step.fault);
            addresses.extend(found);
            healthy.push((step.index, stacks));
        }
        if fault.is_some() {
            return fault;
        }
        if let Err(problem) = self.resolved_everywhere(&addresses).await {
            let names = stages
                .iter()
                .map(|stage| stage.stack.as_str())
                .collect::<Vec<_>>();
            return Some(Fault {
                index: indices.first().copied().unwrap_or_default(),
                stack: None,
                service: None,
                text: format!("stacks '{}': {problem}", names.join("', '")),
                log: Vec::new(),
                undone: false,
                preparing: false,
                code: None,
            });
        }
        for (index, stacks) in healthy {
            for stack in stacks {
                self.say(index, ProgressStep::Healthy(stack));
            }
        }
        None
    }

    /// One node's stacks of a shared wave: begin if it has not, then one
    /// `stacks` step. Returns the stacks it rolled out and their addresses.
    async fn roll_stacks_on(
        &self,
        index: usize,
        stacks: Vec<String>,
        begun: bool,
    ) -> (NodeStep, (Vec<String>, Vec<Ipv4Addr>)) {
        let fault = |stack: Option<String>, text: String| Fault {
            index,
            stack,
            service: None,
            text,
            log: Vec::new(),
            undone: false,
            preparing: false,
            code: None,
        };
        let stopped = |fault: Fault| {
            let step = NodeStep {
                index,
                began: true,
                fault: Some(fault),
            };
            (step, (Vec::new(), Vec::new()))
        };
        if !begun {
            self.say(index, ProgressStep::Preparing);
            if let Err(error) = self.step(index, RolloutStep::Begin, SETUP_PATIENCE).await {
                let mut preparing = fault(None, format!("while preparing: {error}"));
                preparing.preparing = true;
                preparing.code = Some(error.code().to_owned());
                return stopped(preparing);
            }
        }
        for stack in &stacks {
            self.say(index, ProgressStep::Stack(stack.clone()));
        }
        let rounds = u32::try_from(stacks.len().div_ceil(STACKS_AT_ONCE)).unwrap_or(u32::MAX);
        let patience = self.stack_patience().saturating_mul(rounds.max(1));
        let step = RolloutStep::Stacks {
            stacks: stacks.clone(),
        };
        let rolled = match self.step(index, step, patience).await {
            Ok(RolloutAnswer::StacksRolled(rolled)) => rolled,
            Ok(_) => {
                return stopped(fault(
                    None,
                    "the node gave another answer than a stacks step's".to_owned(),
                ));
            }
            Err(error) => {
                let mut failed = fault(
                    None,
                    format!("during stacks '{}': {error}", stacks.join("', '")),
                );
                failed.code = Some(error.code().to_owned());
                return stopped(failed);
            }
        };
        if let Some(failure) = rolled.iter().find_map(|stack| stack.failure.clone()) {
            let mut failed = fault(Some(failure.stack.clone()), failure.to_string());
            failed.service = failure.service.clone();
            failed.log = failure.log;
            failed.code = failure.code;
            return stopped(failed);
        }
        let addresses = rolled
            .into_iter()
            .flat_map(|stack| stack.addresses)
            .collect();
        let step = NodeStep {
            index,
            began: true,
            fault: None,
        };
        (step, (stacks, addresses))
    }
}

/// An error to hand on when the original is only borrowed.
pub(super) fn clone_error(error: &MmError) -> MmError {
    match error {
        MmError::Remote { code, message } => MmError::Remote {
            code,
            message: message.clone(),
        },
        MmError::Unreachable {
            subject,
            problem,
            help,
        } => MmError::Unreachable {
            subject: subject.clone(),
            problem: problem.clone(),
            help: help.clone(),
        },
        MmError::Security { message } => MmError::Security {
            message: message.clone(),
        },
        MmError::Timeout { message } => MmError::Timeout {
            message: message.clone(),
        },
        MmError::Cancelled { message } => MmError::Cancelled {
            message: message.clone(),
        },
        MmError::Launch { message } => MmError::Launch {
            message: message.clone(),
        },
        other => MmError::Environment {
            message: other.to_string(),
        },
    }
}
