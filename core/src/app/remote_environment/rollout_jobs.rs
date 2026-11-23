//! The jobs of a rollout stage: first, on each node that runs them, the
//! services the stage's `before-deploy` jobs' services depend on; then
//! each job on its node; only then the stage's batches.

use super::*;

impl Walk<'_> {
    /// How long a node may take for a stack: its convergence window, the
    /// work around it, and what Compose waits for while containers stop
    /// and dependencies become healthy.
    pub(super) fn stack_patience(&self) -> Duration {
        Duration::from_secs(self.rollout.convergence_timeout_seconds)
            + STACK_OVERHEAD
            + self.environment.compose_wait
    }

    /// How long a node may take to finish or roll back, which may start or
    /// stop every stack again.
    pub(super) fn undo_patience(&self) -> Duration {
        let stacks = u32::try_from(self.rollout.stages.len()).unwrap_or(u32::MAX);
        SETUP_PATIENCE + self.environment.compose_wait.saturating_mul(stacks)
    }

    /// The jobs of a stage, one after the other on their nodes, before any
    /// node of the stage is replaced. A job that fails stops the rollout.
    pub(super) async fn stage_jobs(&mut self, stage: &RolloutStage) -> Option<Fault> {
        if !stage.jobs.is_empty()
            && let Some(fault) = self.stage_ahead(stage).await
        {
            return Some(fault);
        }
        for planned in &stage.jobs {
            let Some(index) = self
                .nodes
                .iter()
                .position(|node| node.name.as_deref() == Some(planned.node.as_str()))
            else {
                return Some(Fault {
                    index: 0,
                    stack: Some(stage.stack.clone()),
                    service: None,
                    text: format!(
                        "the rollout runs job '{}' on node '{}', which the plan does not cover",
                        planned.job, planned.node
                    ),
                    log: Vec::new(),
                    undone: false,
                    preparing: false,
                    code: None,
                });
            };
            let fault = |text: String, log: Vec<String>| Fault {
                index,
                stack: Some(stage.stack.clone()),
                service: None,
                text,
                log,
                undone: false,
                preparing: false,
                code: None,
            };
            if !self.begun.contains(&index) {
                self.say(index, ProgressStep::Preparing);
                self.begun.push(index);
                if let Err(error) = self.step(index, RolloutStep::Begin, SETUP_PATIENCE).await {
                    return Some(Fault {
                        preparing: true,
                        code: Some(error.code().to_owned()),
                        ..fault(format!("while preparing: {error}"), Vec::new())
                    });
                }
            }
            self.say(
                index,
                ProgressStep::Note(format!("{}: job {}", stage.stack, planned.job)),
            );
            let step = RolloutStep::Job {
                stack: stage.stack.clone(),
                job: planned.job.clone(),
            };
            let patience = Duration::from_secs(planned.timeout_seconds) + STACK_OVERHEAD;
            match self.step(index, step, patience).await {
                Ok(RolloutAnswer::JobRan(run)) => {
                    self.say(
                        index,
                        ProgressStep::Note(format!("job {} succeeded (run {})", run.job, run.id)),
                    );
                }
                Ok(RolloutAnswer::Failed(failure)) => {
                    return Some(Fault {
                        code: failure.code.clone(),
                        ..fault(failure.to_string(), failure.log)
                    });
                }
                Ok(_) => {
                    return Some(fault(
                        "the node gave another answer than a job's".to_owned(),
                        Vec::new(),
                    ));
                }
                Err(error) => {
                    return Some(Fault {
                        code: Some(error.code().to_owned()),
                        ..fault(error.to_string(), Vec::new())
                    });
                }
            }
        }
        None
    }

    /// Before a stage's jobs: on each node that runs them, the services the
    /// jobs' services depend on, healthy and resolved everywhere.
    async fn stage_ahead(&mut self, stage: &RolloutStage) -> Option<Fault> {
        let patience = self.stack_patience();
        for name in &stage.ahead {
            let index = self
                .nodes
                .iter()
                .position(|node| node.name.as_deref() == Some(name.as_str()))?;
            let fault = |text: String, log: Vec<String>| Fault {
                index,
                stack: Some(stage.stack.clone()),
                service: None,
                text,
                log,
                undone: false,
                preparing: false,
                code: None,
            };
            if !self.begun.contains(&index) {
                self.say(index, ProgressStep::Preparing);
                self.begun.push(index);
                if let Err(error) = self.step(index, RolloutStep::Begin, SETUP_PATIENCE).await {
                    return Some(Fault {
                        preparing: true,
                        code: Some(error.code().to_owned()),
                        ..fault(format!("while preparing: {error}"), Vec::new())
                    });
                }
            }
            self.say(
                index,
                ProgressStep::Note(format!("{}: what its jobs need", stage.stack)),
            );
            let step = RolloutStep::Ahead {
                stack: stage.stack.clone(),
            };
            let addresses = match self.step(index, step, patience).await {
                Ok(RolloutAnswer::Applied { addresses }) => addresses,
                Ok(RolloutAnswer::Failed(failure)) => {
                    return Some(Fault {
                        service: failure.service.clone(),
                        code: failure.code.clone(),
                        ..fault(failure.to_string(), failure.log)
                    });
                }
                Ok(_) => {
                    return Some(fault(
                        "the node gave another answer than a stack step's".to_owned(),
                        Vec::new(),
                    ));
                }
                Err(error) => {
                    return Some(Fault {
                        code: Some(error.code().to_owned()),
                        ..fault(error.to_string(), Vec::new())
                    });
                }
            };
            if let Err(problem) = self.resolved_everywhere(&addresses).await {
                return Some(fault(
                    format!("stack '{}': {problem}", stage.stack),
                    Vec::new(),
                ));
            }
        }
        None
    }
}

impl Walk<'_> {
    /// Says which services the node started next to the old; returns their addresses.
    pub(super) fn started_next(
        &self,
        index: usize,
        stack: String,
        answer: RolloutAnswer,
    ) -> Vec<Ipv4Addr> {
        let RolloutAnswer::Started {
            services,
            addresses,
        } = answer
        else {
            return Vec::new();
        };
        if !services.is_empty() {
            self.say(index, ProgressStep::StartingNext { stack, services });
        }
        addresses
    }
}
