// Services that change node. A node applies its section first and keeps a
// service that moved away running; once every node of the cluster plan has
// applied, the client asks each of those nodes to stop what moved.

impl RemoteDaemon {
    /// Whether the engine has to plan for this node although the commit
    /// places nothing on it: its last deployment left containers here.
    fn runs_containers(&self, scope: &RemoteScope) -> Result<bool, MmError> {
        Ok(self
            .access
            .deployed(scope.project_id, &scope.environment)?
            .is_some_and(|deployed| deployed.runs_services))
    }

    /// Stops the services this node's section of cluster plan `hash` moved
    /// to other nodes. Only the caller that applied the section may finish
    /// it, and only while it is the last deployment applied here.
    async fn finish_moves<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        hash: &str,
        pass: &MutationPass,
        timeout: Duration,
    ) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        let scope = &context.scope;
        let node = scope.node.as_deref().unwrap_or_default();
        let section = self.sealed_section(scope, hash)?.ok_or_else(|| MmError::Security {
            message: "only a cluster node moves services; the request names no node".to_owned(),
        })?;
        let not_applied = || MmError::State {
            message: format!(
                "node '{node}' has not applied its part of cluster plan {hash}, so nothing was stopped; apply the plan first"
            ),
        };
        let StoredRemotePlan::Deployment(remote) =
            self.journal.applied_plan(&section, scope)?.ok_or_else(not_applied)?
        else {
            return Err(MmError::Security {
                message: format!("cluster plan {hash} is not a deployment plan"),
            });
        };
        if remote.binding.principal_id != context.principal_id {
            return Err(MmError::Security {
                message: "another principal applied this plan; only they can finish it".to_owned(),
            });
        }
        let stopped = match &remote.plan {
            Some(plan) if plan.node.as_ref().is_some_and(|node| node.moved().next().is_some()) => {
                let root = self.repositories.checkout_path(
                    remote.binding.project_id,
                    &remote.binding.environment,
                    &remote.binding.commit,
                );
                let mut finishing = context.clone();
                finishing.project_root = root;
                self.journal.set_request_phase(context.operation_nonce, "running")?;
                let stopping = self.use_cases.finish_moves(&finishing, &plan.plan_hash);
                let stopped = self.watched(context, pass, timeout, stopping).await?;
                // With the moved services gone, the node runs what the plan
                // places on it and nothing else.
                if let Some(mut deployed) =
                    self.access.deployed(scope.project_id, &scope.environment)?
                {
                    deployed.runs_services = !plan.stacks.is_empty();
                    self.access
                        .locked("mmd, recording moved services", |guard| {
                            self.access.write_deployed(guard, &deployed)
                        })
                        .await?;
                }
                stopped
            }
            _ => Vec::new(),
        };
        self.journal.audit(&serde_json::json!({
            "timestamp_unix_ms": now_unix_ms()?, "request_id": context.request_id,
            "principal": context.principal, "principal_id": context.principal_id,
            "project_id": scope.project_id, "environment": scope.environment,
            "target": scope.target, "node": node, "plan_hash": section,
            "cluster_plan_hash": hash, "result": "moves-finished",
            "stopped": stopped.iter().map(|service| format!("{}/{}", service.stack, service.service)).collect::<Vec<_>>(),
            "daemon_version": env!("CARGO_PKG_VERSION"),
        }))?;
        send_result(writer, context, RemoteResult::MovesFinished(stopped)).await
    }
}
