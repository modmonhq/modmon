// Pulling a rollout's images on a host before any node is replaced.

impl RemoteDaemon {
    /// Pulls every image this host's part of plan `hash` runs. The host may
    /// not have begun yet: its section is read as reviewed, and no
    /// container changes.
    async fn pull_rollout(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        timeout: Duration,
        pass: &MutationPass,
    ) -> Result<crate::domain::rollout::RolloutAnswer, MmError> {
        self.holder(context, hash)?;
        let scope = &context.scope;
        let node_hash = self.sealed_section(scope, hash)?.unwrap_or_else(|| hash.to_owned());
        let StoredRemotePlan::Deployment(remote) = self.journal.read_plan(&node_hash, scope, now_unix_ms()?)? else {
            return Err(MmError::Security {
                message: format!("plan {hash} is not a deployment plan"),
            });
        };
        let section = RolloutSection::of(self, *remote);
        let Some(plan) = &section.plan else {
            return Ok(crate::domain::rollout::RolloutAnswer::Pulled);
        };
        let rolling = section.context(context);
        let pulling = self.use_cases.pull_planned(&rolling, plan);
        self.watched(context, pass, timeout, pulling)
            .await
            .map_err(|error| host_failure(context, "pull", error))?;
        Ok(crate::domain::rollout::RolloutAnswer::Pulled)
    }
}
