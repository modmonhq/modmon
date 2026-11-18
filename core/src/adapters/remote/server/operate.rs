// What `operate` adds to `view` on a host: restarting a service's
// containers and running the jobs the deployed configuration declares.
// Neither changes what is deployed. Each is audited with its principal,
// what it acted on, and how it ended.

impl RemoteDaemon {
    async fn operate<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        method: RemoteMethod,
        pass: Option<&MutationPass>,
        timeout: Duration,
    ) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        let checked = || pass.ok_or_else(|| MmError::State {
            message: "an operation that changes the host ran without its check".to_owned(),
        });
        let result = match method {
            RemoteMethod::Restart(request) => {
                self.require_placed(context, &request.selector.service)?;
                let restarting = self.use_cases.restart(context, request);
                let outcome = self.watched(context, checked()?, timeout, restarting).await?;
                self.audit_operation(context, "service-restarted", serde_json::json!({
                    "service": outcome.identity.service, "containers": outcome.containers,
                }))?;
                self.forget_restarts(&context.scope, &outcome.identity.service);
                RemoteResult::Restarted(Box::new(outcome))
            }
            RemoteMethod::RunJob(mut request) => {
                self.journal.set_request_phase(context.operation_nonce, "running")?;
                // A run outlives a client that stops waiting: its container
                // runs detached and the host finishes the record later. It
                // takes the request's ID, so `mm deploy cancel` with the
                // operation's ID stops the run too.
                request.run = Some(context.request_id);
                let running = self.use_cases.run_job(context, request);
                let run = self.watched(context, checked()?, timeout, running).await?;
                self.audit_operation(context, "job-run", serde_json::json!({
                    "job": run.job, "run_id": run.id, "state": run.state, "exit_code": run.exit_code,
                }))?;
                RemoteResult::JobRun(Box::new(run))
            }
            RemoteMethod::JobRuns(_) if context.project_root.as_os_str().is_empty() => {
                RemoteResult::JobRuns(Vec::new())
            }
            RemoteMethod::JobRuns(request) => {
                RemoteResult::JobRuns(timed(timeout, self.use_cases.job_runs(context, request)).await?)
            }
            _ => {
                return Err(MmError::State {
                    message: "this is no restart or job request".to_owned(),
                });
            }
        };
        send_result(writer, context.request_id, result).await
    }

    /// One audit record of what an operation did, beside the request's own.
    fn audit_operation(
        &self,
        context: &RemoteRequestContext,
        result: &str,
        detail: serde_json::Value,
    ) -> Result<(), MmError> {
        let scope = &context.scope;
        self.journal.audit(&serde_json::json!({
            "timestamp_unix_ms": now_unix_ms()?, "request_id": context.request_id,
            "principal": context.principal, "principal_id": context.principal_id,
            "project_id": scope.project_id, "environment": scope.environment,
            "target": scope.target, "node": scope.node, "result": result, "detail": detail,
            "daemon_version": env!("CARGO_PKG_VERSION"),
        }))
    }
}
