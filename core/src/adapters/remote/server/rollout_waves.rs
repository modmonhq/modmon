// Several stacks of a rollout on this host in one step (`RolloutStep::Stacks`).

impl RemoteDaemon {
    /// `Start` then `Apply` for each of `stacks`, side by side. Each stack
    /// is noted as changed before the step runs, and taken back out when
    /// it was left as it was; a stack that is not healthy is an answer.
    async fn roll_stacks(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        stacks: &[String],
        (timeout, pass): (Duration, &MutationPass),
    ) -> Result<crate::domain::rollout::RolloutAnswer, MmError> {
        use crate::domain::rollout::{RolloutAnswer, StackRolled, StepFailure};

        self.held_rollout(context, hash, RolloutState::Running)?;
        let section = self.rollout_section(&context.scope, hash)?;
        let plan = section.plan.as_ref().ok_or_else(|| MmError::State {
            message: format!("plan {hash} places nothing on this host, so it has no stacks here"),
        })?;
        let budget = i64::try_from(timeout.as_millis()).unwrap_or(i64::MAX);
        let mut noted_before = Vec::new();
        self.note_rollout(context, hash, |rollout, now| {
            rollout.expires_at_unix_ms = now.saturating_add(budget).saturating_add(LOCK_LEASE_MS);
            if let Some(changes) = &mut rollout.changes {
                for stack in stacks {
                    if changes.whole(stack) {
                        noted_before.push(stack.clone());
                    }
                    changes.note(stack, false);
                }
            }
        })?;
        let rolling = section.context(context);
        let rolling_stacks = self.use_cases.roll_stacks(&rolling, &plan.plan_hash, stacks);
        let rolled = self.watched(context, pass, timeout, rolling_stacks).await;
        if was_stopped(pass) {
            for stack in stacks {
                self.take_back_leaves(&rolling, (hash, &plan.plan_hash), stack).await;
            }
        }
        let outcomes = match rolled {
            Err(error @ MmError::Security { .. }) => return Err(self.lease_ends(context, hash, error)),
            Ok(outcomes) => outcomes,
            Err(error) => {
                let error = host_failure(context, "stacks", error);
                stacks
                    .iter()
                    .map(|stack| {
                        (stack.clone(), StackOutcome::Failed(StepFailure {
                            stack: stack.clone(),
                            service: None,
                            reason: error.to_string(),
                            log: Vec::new(),
                            code: Some(error.code().to_owned()),
                        }))
                    })
                    .collect()
            }
        };
        let answers = outcomes
            .into_iter()
            .map(|(stack, outcome)| match outcome {
                StackOutcome::Done { unchanged, addresses, .. } => {
                    (unchanged && !noted_before.contains(&stack), StackRolled { stack, addresses, failure: None })
                }
                StackOutcome::Failed(failure) => {
                    (false, StackRolled { stack, addresses: Vec::new(), failure: Some(failure) })
                }
            })
            .collect::<Vec<_>>();
        let nobody = nobody_renews(pass);
        self.note_rollout(context, hash, |rollout, now| {
            rollout.expires_at_unix_ms = if nobody { now } else { now.saturating_add(LOCK_LEASE_MS) };
            for (left_alone, answer) in &answers {
                if *left_alone && let Some(changes) = &mut rollout.changes {
                    changes.forget(&answer.stack);
                }
                match &answer.failure {
                    None => rollout.stacks.push(answer.stack.clone()),
                    Some(failure) => {
                        if rollout.failure.is_none() {
                            rollout.failure = Some(failure.clone());
                        }
                    }
                }
            }
        })?;
        Ok(RolloutAnswer::StacksRolled(answers.into_iter().map(|(_, answer)| answer).collect()))
    }
}
