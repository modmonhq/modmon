// What the daemon does so that it is noticed when it stops answering, and
// what it does when it starts after a daemon that was in the middle of
// something.

impl RemoteDaemon {
    /// A connection the accept loop took: the daemon's own canary when it
    /// comes from this very process, and a relay session otherwise.
    async fn serve_accepted(&self, stream: UnixStream) -> Result<(), MmError> {
        let own = stream
            .peer_cred()
            .is_ok_and(|peer| peer.pid().and_then(|pid| u32::try_from(pid).ok()) == Some(std::process::id()));
        if own {
            return self.canary(stream).await;
        }
        self.serve_stream(stream).await
    }

    /// Answers the daemon's own canary. The canary came in through the
    /// accept loop and runs as a task like every request, and it takes the
    /// audit journal's lock as every request does. An answer therefore
    /// means requests are being served. It reads no file an administrator
    /// edits: a host policy with a mistake in it refuses requests, and is
    /// no reason to restart the daemon that answers names.
    async fn canary(&self, mut stream: UnixStream) -> Result<(), MmError> {
        use tokio::io::AsyncWriteExt;

        self.journal.is_free()?;
        stream
            .write_all(crate::adapters::remote::liveness::CANARY_ANSWER)
            .await
            .map_err(|source| MmError::Io {
                context: "answering the daemon's own canary".to_owned(),
                source,
            })
    }

    /// What a daemon does before it serves: whatever ran a step when the
    /// last daemon ended is not running now. A rollout that daemon was in
    /// the middle of gives up the environment's deployment lock at once,
    /// and one that had begun is marked interrupted. The next deployment
    /// undoes it without a manual step.
    pub fn resume_after_restart(&self) -> Result<(), MmError> {
        let now = now_unix_ms()?;
        let requests = self.journal.interrupt_open_requests()?;
        if requests > 0 {
            tracing::warn!(requests, "the daemon started while requests were running here; they are marked interrupted");
        }
        // Without a policy that loads there is nothing to resume yet: a
        // rollout's lease runs out by itself.
        let enrolled = match self.access.enrolled_environments() {
            Ok(enrolled) => enrolled,
            Err(error) => {
                tracing::warn!(%error, "rollouts are not looked at after the start");
                return Ok(());
            }
        };
        for (project, environment) in enrolled {
            let interrupted = match self
                .access
                .update_deployments(project, &environment, |stored| Ok(stored.interrupt(now)))
            {
                Ok(interrupted) => interrupted,
                Err(error) => {
                    tracing::warn!(%environment, %error, "the rollout of an environment is not looked at after the start");
                    continue;
                }
            };
            let Some((plan_hash, state)) = interrupted else {
                continue;
            };
            let began = state == crate::domain::rollout::RolloutState::Running;
            tracing::warn!(
                %environment,
                plan = %plan_hash,
                began,
                "the daemon started while a rollout was in progress here; its lock is released{}",
                if began { " and the rollout is marked interrupted" } else { "" }
            );
            self.journal.audit(&serde_json::json!({
                "timestamp_unix_ms": now, "event": "rollout-interrupted",
                "project_id": project, "environment": environment, "plan_hash": plan_hash,
                "began": began, "result": "interrupted",
                "daemon_version": env!("CARGO_PKG_VERSION"),
            }))?;
        }
        Ok(())
    }
}
