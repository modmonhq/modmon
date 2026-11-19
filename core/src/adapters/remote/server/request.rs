// One request from its turn to its record: what runs it, what stops it,
// and what the journal and the audit log say about it afterwards.
//
// What a request did and whether its answer reached the client are kept
// apart: an apply that succeeded is journalled and audited as
// succeeded even when its client is gone by the time the answer is
// written, and the host's deployment record says what happened either way.

/// How a request ended.
enum RequestOutcome {
    Done,
    /// It did what it was asked, and its answer did not reach the client.
    Undelivered(MmError),
    /// It failed; `true` when the host may have been changed.
    Failed(MmError, bool),
}

/// The writer of a request's answer, which remembers whether an answer was
/// begun: a request whose answer is being written has done its work.
struct Answering<'a, W: ?Sized> {
    inner: &'a mut W,
    begun: Arc<std::sync::atomic::AtomicBool>,
}

impl<W: AsyncWrite + Unpin + ?Sized> AsyncWrite for Answering<'_, W> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.begun.store(true, std::sync::atomic::Ordering::SeqCst);
        std::pin::Pin::new(&mut *self.inner).poll_write(context, bytes)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut *self.inner).poll_flush(context)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut *self.inner).poll_shutdown(context)
    }
}

/// Completes when the client of a request that has no stream ends its
/// session or asks to cancel: it went away.
async fn client_gone<R: AsyncRead + Unpin + ?Sized>(reader: &mut FrameReader<&mut R>) {
    loop {
        match reader.next::<ClientFrame>().await {
            Ok(Some(ClientFrame::Cancel { .. }) | None) | Err(_) => return,
            Ok(Some(_)) => {}
        }
    }
}

/// Whether a method reads the session as it goes: it answers with a
/// stream, or takes one (a delivery's upload).
const fn streams(method: &RemoteMethod) -> bool {
    matches!(
        method,
        RemoteMethod::Logs(_)
            | RemoteMethod::Exec(_)
            | RemoteMethod::Forward { .. }
            | RemoteMethod::Deliver(_)
    )
}

impl RemoteDaemon {
    /// Lets a request that changes the host in once it has its turn. A
    /// deployment that cannot start because another one holds the
    /// environment is refused at once, not after waiting.
    async fn enter_in_turn<R>(
        &self,
        reader: &mut FrameReader<&mut R>,
        context: &RemoteRequestContext,
        method: &RemoteMethod,
        (timeout, abort): (Duration, futures_util::future::AbortHandle),
    ) -> Result<Option<(MutationPass, Option<Authorization>)>, MmError>
    where
        R: AsyncRead + Unpin + ?Sized,
    {
        if matches!(method, RemoteMethod::ApplyDeployment { .. }) {
            self.refuse_while_rolling(&context.scope)?;
        }
        let entering = self.enter_mutation(context, method, (timeout, abort));
        if !method.takes_turn() {
            return entering.await;
        }
        // A client says nothing while its request waits. One that hangs up
        // meanwhile gave the request up: it must not run later with nobody
        // there to see what it did.
        tokio::select! {
            entered = entering => entered,
            () = client_gone(reader) => Err(MmError::Cancelled {
                message: "the client went away before this request had its turn, so nothing was changed".to_owned(),
            }),
        }
    }

    /// Runs a request that was let in. A request without a stream is
    /// stopped when its client goes away, and dropped when it has not
    /// ended some seconds after its deadline (`Operations::reap`).
    async fn run_request<R, W>(
        &self,
        (reader, writer): (&mut FrameReader<&mut R>, &mut W),
        context: &RemoteRequestContext,
        method: RemoteMethod,
        (timeout, entered): (Duration, Option<(MutationPass, Option<Authorization>)>),
        registration: futures_util::future::AbortRegistration,
    ) -> RequestOutcome
    where
        R: AsyncRead + Unpin + ?Sized,
        W: AsyncWrite + Unpin + ?Sized,
    {
        let mutation = method.is_mutation();
        let running = entered.as_ref().and_then(|(pass, _)| pass.running());
        if streams(&method) {
            // A stream reads the session itself, so its client is not
            // watched here; one that is an operation (a delivery) is still
            // dropped past its deadline.
            let streaming = self.dispatch_stream(reader, writer, context, method, (timeout, entered));
            return match futures_util::future::Abortable::new(streaming, registration).await {
                Ok(Ok(())) => RequestOutcome::Done,
                Ok(Err(error)) => RequestOutcome::Failed(error, mutation),
                Err(futures_util::future::Aborted) => RequestOutcome::Failed(
                    running.as_ref().map_or_else(
                        || deadline_passed(timeout),
                        |running| running.stopping().unwrap_or(Stop::Deadline).error(&running.info()),
                    ),
                    true,
                ),
            };
        }
        // A request whose client left while it waited is not run: a lease
        // renewal from the backlog of a daemon that was stopped must not
        // hold the environment for a client that is gone.
        if futures_util::FutureExt::now_or_never(client_gone(reader)).is_some() {
            return RequestOutcome::Failed(
                MmError::Cancelled {
                    message: "the client went away before this request ran, so it was not run"
                        .to_owned(),
                },
                false,
            );
        }
        let abandoned = running.as_ref().map(|_| method.clone());
        let begun = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut delivery = Answering { inner: writer, begun: Arc::clone(&begun) };
        let work = futures_util::future::Abortable::new(
            self.dispatch_unary(&mut delivery, context, method, (timeout, entered)),
            registration,
        );
        tokio::pin!(work);
        let ended = tokio::select! {
            ended = &mut work => ended,
            () = client_gone(reader) => {
                if let Some(running) = &running {
                    running.ask_to_stop(Stop::ClientGone);
                }
                (&mut work).await
            }
        };
        match ended {
            Ok(Ok(())) => RequestOutcome::Done,
            Ok(Err(error)) if begun.load(std::sync::atomic::Ordering::SeqCst) => {
                let cancelled = running.as_ref().and_then(|running| running.stopping()).is_some_and(|stop| stop.is_cancellation());
                if let Some(method) = abandoned.as_ref().filter(|_| !cancelled)
                    && let Err(record) = self.free_after_undelivered(context, method)
                {
                    tracing::warn!(%record, "could not free the environment of a client that is gone");
                }
                RequestOutcome::Undelivered(error)
            }
            Ok(Err(error)) => RequestOutcome::Failed(error, mutation),
            Err(futures_util::future::Aborted) => {
                let stop = running.as_ref().and_then(|running| running.stopping()).unwrap_or(Stop::Deadline);
                let error = running.as_ref().map_or_else(
                    || deadline_passed(timeout),
                    |running| stop.error(&running.info()),
                );
                if let Some(method) = &abandoned
                    && let Err(record) = self.note_abandoned(context, method, &stop)
                {
                    tracing::warn!(%record, "could not record an operation that was dropped");
                }
                RequestOutcome::Failed(error, true)
            }
        }
    }

    /// A rollout step whose answer did not reach its client: the client is
    /// gone, so nobody renews the environment's lock, which ends now, as
    /// for a step stopped because its client went away. (A step that ends
    /// by itself just as its client leaves, such as one whose engine
    /// command timed out while `mmd` was stopped, would otherwise hold the
    /// environment for another lease.) A step that was cancelled is left to
    /// the cancellation, which frees the environment once it has undone it.
    fn free_after_undelivered(
        &self,
        context: &RemoteRequestContext,
        method: &RemoteMethod,
    ) -> Result<(), MmError> {
        let RemoteMethod::Rollout { plan_hash, step } = method else {
            return Ok(());
        };
        if !step.changes_host() {
            return Ok(());
        }
        self.note_rollout(context, plan_hash, |rollout, now| {
            rollout.expires_at_unix_ms = rollout.expires_at_unix_ms.min(now);
        })
    }

    /// What the host records about an operation that was dropped before it
    /// ended: the deployment it was part of says so, and an environment
    /// whose client is gone is free at once.
    fn note_abandoned(
        &self,
        context: &RemoteRequestContext,
        method: &RemoteMethod,
        stop: &Stop,
    ) -> Result<(), MmError> {
        let (hash, step) = match method {
            RemoteMethod::Rollout { plan_hash, step } if step.changes_host() => (plan_hash, step.name()),
            RemoteMethod::ApplyDeployment { remote_plan_hash } => (remote_plan_hash, "apply"),
            _ => return Ok(()),
        };
        let reason = format!("step {step} was {}", stop.reason());
        let free = stop.frees_environment();
        self.note_rollout(context, hash, |rollout, now| {
            if free {
                rollout.expires_at_unix_ms = now;
            }
            rollout.failure = Some(crate::domain::rollout::StepFailure {
                stack: String::new(),
                service: None,
                reason,
                log: Vec::new(),
                code: Some(stop.error_code().to_owned()),
            });
        })
    }

    /// Journals and audits how a request ended, and tells the client when
    /// it failed.
    async fn conclude<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        method_name: &str,
        outcome: RequestOutcome,
    ) -> Result<(), MmError>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        let operation_nonce = context.operation_nonce;
        match outcome {
            RequestOutcome::Done => {
                self.journal.set_request_phase(operation_nonce, "succeeded")?;
                self.audit(context, method_name, "succeeded", None)
            }
            RequestOutcome::Undelivered(error) => {
                tracing::warn!(
                    request_id = %context.request_id,
                    %error,
                    "a request did what it was asked, and its answer did not reach the client"
                );
                self.journal.set_request_phase(operation_nonce, "succeeded")?;
                self.audit(context, method_name, "succeeded", None)?;
                self.audit(context, method_name, "undelivered", Some(error.code()))
            }
            RequestOutcome::Failed(error, changed) => {
                self.journal.set_request_phase(
                    operation_nonce,
                    if changed { "unknown" } else { "failed" },
                )?;
                self.audit(context, method_name, "failed", Some(error.code()))?;
                send_operation_error(
                    writer,
                    (context.protocol, context.request_id),
                    &error,
                    if changed {
                        MutationState::Unknown
                    } else {
                        MutationState::NotStarted
                    },
                )
                .await
            }
        }
    }

    /// A request that reads the session as it goes: logs, a command, a
    /// forwarded connection, a delivery's upload.
    async fn dispatch_stream<R, W>(
        &self,
        reader: &mut FrameReader<&mut R>,
        writer: &mut W,
        context: &RemoteRequestContext,
        method: RemoteMethod,
        (timeout, entered): (Duration, Option<(MutationPass, Option<Authorization>)>),
    ) -> Result<(), MmError>
    where
        R: AsyncRead + Unpin + ?Sized,
        W: AsyncWrite + Unpin + ?Sized,
    {
        let pass = entered.map(|(pass, _)| pass);
        // What is left of the request's time after it waited its turn.
        let timeout = if pass.is_some() { remaining(context)? } else { timeout };
        match method {
            RemoteMethod::Logs(request) => {
                self.authorize_log_read(context, &request)?;
                let cancellation = OperationCancellation::default();
                let stream = timed(timeout, self.use_cases.logs(context, request.clone(), cancellation.clone())).await?;
                let stream = self.with_restart_lines(context, &request, stream, cancellation.clone());
                send_result(writer, context, RemoteResult::StreamStarted).await?;
                self.stream_logs(reader, writer, context, stream, cancellation, timeout).await
            }
            RemoteMethod::Exec(request) => {
                let pass = pass.as_ref().ok_or_else(|| MmError::State {
                    message: "a command ran without its access check".to_owned(),
                })?;
                self.require_placed(context, &request.selector.service)?;
                self.journal.set_request_phase(context.operation_nonce, "running")?;
                let cancellation = OperationCancellation::default();
                let stream = timed(timeout, self.use_cases.exec(context, request, cancellation.clone())).await?;
                send_result(writer, context, RemoteResult::StreamStarted).await?;
                self.stream_exec(reader, writer, context, (stream, pass), cancellation, timeout).await
            }
            RemoteMethod::Forward { service, port } => {
                self.require_placed(context, &service)?;
                self.forward(reader, writer, context, (&service, port), timeout).await
            }
            RemoteMethod::Deliver(offer) => {
                let pass = pass.as_ref().ok_or_else(|| MmError::State {
                    message: "a delivery ran without its turn".to_owned(),
                })?;
                self.deliver(reader, writer, context, *offer, pass, timeout).await
            }
            _ => Err(MmError::State {
                message: "a request without a stream was dispatched as one".to_owned(),
            }),
        }
    }

    /// A request answered with one result.
    async fn dispatch_unary<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        method: RemoteMethod,
        (timeout, entered): (Duration, Option<(MutationPass, Option<Authorization>)>),
    ) -> Result<(), MmError>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        let method_kind = applied_plan_kind(&method);
        let (pass, plan_authorization) = match entered {
            Some((pass, authorization)) => (Some(pass), authorization),
            None => (None, None),
        };
        // What is left of the request's time after it waited its turn.
        let timeout = if pass.is_some() { remaining(context)? } else { timeout };
        let passed = || pass.as_ref().ok_or_else(|| MmError::State {
            message: "a request that changes the host ran without its turn".to_owned(),
        });
        match method {
            RemoteMethod::Status => {
                let mut result = timed(timeout, self.use_cases.status(context)).await?;
                self.finish_status(context, &mut result)?;
                send_result(writer, context, RemoteResult::Status(Box::new(result))).await
            }
            RemoteMethod::PlanDeployment { commit, receipt, release } => {
                self.plan(writer, context, &commit, PlanRequest::Deployment(receipt, release), timeout).await
            }
            RemoteMethod::PlanTeamSync { commit } => {
                self.plan(writer, context, &commit, PlanRequest::TeamSync, timeout).await
            }
            RemoteMethod::ApplyDeployment { remote_plan_hash }
            | RemoteMethod::ApplyTeamSync { remote_plan_hash } => {
                let kind = method_kind.ok_or_else(|| MmError::State {
                    message: "apply method has no plan kind".to_owned(),
                })?;
                let authorization = plan_authorization.ok_or_else(|| MmError::State {
                    message: "apply authorization was not checked".to_owned(),
                })?;
                self.apply(writer, context, timeout, (&remote_plan_hash, kind), &authorization, passed()?)
                    .await
            }
            RemoteMethod::WhoAmI => self.whoami(writer, context).await,
            RemoteMethod::SealPlan(seal) => self.seal(writer, context, *seal).await,
            RemoteMethod::SealedPlan { cluster_plan_hash } => {
                self.sealed(writer, context, &cluster_plan_hash).await
            }
            RemoteMethod::Node => self.node_facts(writer, context).await,
            RemoteMethod::FinishMoves { cluster_plan_hash } => {
                self.finish_moves(writer, context, &cluster_plan_hash, passed()?, timeout).await
            }
            RemoteMethod::Rollout { plan_hash, step } => {
                let checked = (plan_authorization.as_ref(), pass.as_ref());
                self.rollout(writer, context, timeout, (&plan_hash, step), checked).await
            }
            RemoteMethod::DeploymentStatus => self.deployment_status(writer, context).await,
            RemoteMethod::Cancel { operation } => {
                self.cancel(writer, context, operation.as_deref(), timeout).await
            }
            method @ (RemoteMethod::Restart(_) | RemoteMethod::RunJob(_)) => {
                self.operate(writer, context, method, pass.as_ref(), timeout).await
            }
            method @ (RemoteMethod::SecretRecipient
            | RemoteMethod::StageValues(_)
            | RemoteMethod::HostValues
            | RemoteMethod::StoreSnapshot { .. }
            | RemoteMethod::StoreRestore(_)
            | RemoteMethod::StoreMigrate { .. }) => {
                self.values(writer, context, method, pass.as_ref(), timeout).await
            }
            method => self.dispatch_read(writer, context, method, (timeout, pass.as_ref())).await,
        }
    }

    /// The node's reads of its own parts, its job runs, and shared storage.
    async fn dispatch_read<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        method: RemoteMethod,
        (timeout, pass): (Duration, Option<&MutationPass>),
    ) -> Result<(), MmError>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        match method {
            RemoteMethod::MeshStatus => {
                let mut status = timed(timeout, self.use_cases.mesh_status(context)).await?;
                status.node = self.config.node_env()?.map(|node| node.name);
                send_result(writer, context, RemoteResult::MeshStatus(Box::new(status))).await
            }
            RemoteMethod::Networks => {
                let mut status = timed(timeout, self.use_cases.networks(context)).await?;
                status.node = self.config.node_env()?.map(|node| node.name);
                send_result(writer, context, RemoteResult::Networks(Box::new(status))).await
            }
            RemoteMethod::Names => {
                let report = timed(timeout, self.use_cases.names(context)).await?;
                send_result(writer, context, RemoteResult::Names(Box::new(report))).await
            }
            RemoteMethod::Runtimes => self.runtimes(writer, context, timeout).await,
            method @ RemoteMethod::JobRuns(_) => self.operate(writer, context, method, pass, timeout).await,
            method @ (RemoteMethod::StorageStatus { .. }
            | RemoteMethod::PlanVolumeDelete { .. }
            | RemoteMethod::DeleteVolume { .. }) => {
                self.storage(writer, context, method, pass, timeout).await
            }
            _ => Err(MmError::State {
                message: "a request reached no handler".to_owned(),
            }),
        }
    }
}
