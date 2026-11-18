pub struct RemoteDaemon {
    config: Arc<DaemonConfig>,
    identity: DaemonIdentity,
    instance_id: uuid::Uuid,
    journal: Arc<RemoteJournal>,
    access: HostAccessStore,
    repositories: ProjectRepositories,
    use_cases: Arc<dyn RemoteUseCases>,
    gate: MutationGate,
    operations: Operations,
    started_at_unix_ms: i64,
    autoscale: Option<Arc<crate::adapters::autoscale::Autoscaler>>,
    healing: Option<Arc<crate::adapters::healing::Healer>>,
}

struct ReceivedRequest {
    request_id: uuid::Uuid,
    operation_nonce: uuid::Uuid,
    deadline_unix_ms: i64,
    scope: crate::domain::remote::RemoteScope,
    method: Box<RemoteMethod>,
    now_unix_ms: i64,
}

struct PreparedRequest {
    context: RemoteRequestContext,
    method: RemoteMethod,
    timeout: Duration,
}

/// What a request's principal is, resolved from the SSH key and scope.
struct ResolvedPrincipal {
    id: PrincipalId,
    name: String,
}

impl RemoteDaemon {
    pub fn new(config: DaemonConfig, use_cases: Arc<dyn RemoteUseCases>) -> Result<Self, MmError> {
        config.validate()?;
        let access = HostAccessStore::new(&config);
        // A host policy with a mistake does not stop the daemon: names and
        // the node's own work go on, sshd keeps the keys of the last policy
        // that loaded, and every request is refused with the mistake until
        // the file is fixed, as when it breaks while the daemon runs.
        match access.load() {
            Ok(snapshot) => access.render_authorized_keys(&snapshot.policy)?,
            Err(error) => tracing::error!(code = error.code(), %error, "serving no request"),
        }
        let journal = Arc::new(RemoteJournal::open(config.state_root.clone())?);
        let identity = DaemonIdentity::load_or_create(&config.state_root)?;
        let repositories =
            ProjectRepositories::new(config.state_root.clone(), config.projects_root.clone());
        let operations = Operations::default();
        operations.watch_deadlines();
        Ok(Self {
            config: Arc::new(config),
            identity,
            instance_id: uuid::Uuid::new_v4(),
            journal,
            access,
            repositories,
            use_cases,
            gate: MutationGate::default(),
            operations,
            started_at_unix_ms: now_unix_ms()?,
            autoscale: None,
            healing: None,
        })
    }

    #[must_use]
    pub fn identity(&self) -> &DaemonIdentity {
        &self.identity
    }

    /// Serves requests until the process ends, telling the service
    /// manager when it is ready and for as long as it answers.
    pub async fn run(self) -> Result<(), MmError> {
        self.run_reporting(crate::adapters::remote::Liveness::from_environment()).await
    }

    /// As `run`, reporting to `liveness`.
    pub async fn run_reporting(
        self,
        liveness: crate::adapters::remote::Liveness,
    ) -> Result<(), MmError> {
        let listener = bind_socket(&self.config, liveness.sockets())?;
        self.resume_after_restart()?;
        liveness.start(self.config.socket_path.clone());
        let shared = Arc::new(self);
        if let Some(autoscale) = shared.autoscale.clone() {
            tokio::spawn(autoscale.run(Arc::clone(&shared) as Arc<dyn crate::adapters::autoscale::ScaleHost>));
        }
        if let Some(healing) = shared.healing.clone() {
            tokio::spawn(healing.run(Arc::clone(&shared) as Arc<dyn crate::adapters::healing::HealHost>));
        }
        loop {
            let (stream, _) = listener.accept().await.map_err(|source| MmError::Io {
                context: "accepting a remote daemon Unix client".to_owned(),
                source,
            })?;
            let daemon = Arc::clone(&shared);
            tokio::spawn(async move {
                if let Err(error) = daemon.serve_accepted(stream).await {
                    tracing::warn!(code = error.code(), %error, "remote daemon connection rejected");
                }
            });
        }
    }

    pub async fn serve_stream(&self, stream: UnixStream) -> Result<(), MmError> {
        let credentials = stream.peer_cred().map_err(|source| MmError::Io {
            context: "reading Unix peer credentials".to_owned(),
            source,
        })?;
        // Only the shared relay account may speak
        // to the daemon; everything else about the caller comes from the
        // relay's attribution of the SSH key that authenticated.
        if credentials.uid() != self.config.relay_uid {
            return Err(MmError::Security {
                message: format!(
                    "Unix peer uid {} is not the relay account",
                    credentials.uid()
                ),
            });
        }
        let (mut reader, mut writer) = stream.into_split();
        self.serve(&mut reader, &mut writer).await
    }

    async fn serve<R, W>(&self, reader: &mut R, writer: &mut W) -> Result<(), MmError>
    where
        R: AsyncRead + Unpin + ?Sized,
        W: AsyncWrite + Unpin + ?Sized,
    {
        // Frames are read in loops that also wait for other things; the
        // reader keeps a frame that arrives in pieces.
        let reader = &mut FrameReader::new(reader);
        let session = self.authenticate_relay(reader, writer).await?;
        self.handshake(reader, writer).await?;
        let Some(received) = self.receive_request(reader, writer).await? else {
            return Ok(());
        };
        let Some(prepared) = self.authorize_request(writer, &session, received).await? else {
            return Ok(());
        };
        let method_name = prepared.method.name();
        let _phase = crate::app::timing::phase_of("request", &method_name);
        let context = prepared.context;
        self.audit(&context, method_name, "authorized", None)?;
        // A request that changes the host waits its turn first. One that
        // is refused there changed nothing.
        let (abort, registration) = futures_util::future::AbortHandle::new_pair();
        let entered = self
            .enter_in_turn(reader, &context, &prepared.method, (prepared.timeout, abort))
            .await;
        let outcome = match entered {
            Ok(entered) => {
                self.run_request(
                    (reader, writer),
                    &context,
                    prepared.method,
                    (prepared.timeout, entered),
                    registration,
                )
                .await
            }
            Err(error) => RequestOutcome::Failed(error, false),
        };
        self.conclude(writer, &context, method_name, outcome).await
    }

    async fn authenticate_relay<R, W>(
        &self, reader: &mut FrameReader<&mut R>, writer: &mut W,
    ) -> Result<AuthenticatedSession, MmError>
    where R: AsyncRead + Unpin + ?Sized, W: AsyncWrite + Unpin + ?Sized {
        let frame = tokio::time::timeout(
            Duration::from_millis(self.config.idle_timeout_ms),
            reader.next::<ClientFrame>(),
        ).await.map_err(|_| protocol("client hello timed out"))??
            .ok_or_else(|| protocol("relay disconnected before session attribution"))?;
        let ClientFrame::RelaySession { version, key_fingerprint, source, session_id } = frame else {
            return Err(protocol("first frame must be trusted relay session attribution"));
        };
        if version != PROTOCOL_VERSION {
            send_error(writer, None, remote_error(
                "MM_REMOTE_UPGRADE_REQUIRED",
                "the relay and mmd on this host are different versions; install both again with `mm target setup plan` and apply it",
                false, MutationState::NotStarted,
            )).await?;
            return Err(protocol("relay session protocol version is incompatible"));
        }
        validate_relay_attribution(&key_fingerprint, &source)?;
        // Which principal a key maps to depends on the request's project and
        // environment, so authentication completes per request.
        Ok(AuthenticatedSession { key_fingerprint, source, session_id })
    }

    async fn handshake<R, W>(&self, reader: &mut FrameReader<&mut R>, writer: &mut W) -> Result<(), MmError>
    where R: AsyncRead + Unpin + ?Sized, W: AsyncWrite + Unpin + ?Sized {
        let first = tokio::time::timeout(
            Duration::from_millis(self.config.idle_timeout_ms),
            reader.next::<ClientFrame>(),
        ).await.map_err(|_| protocol("client hello timed out"))??
            .ok_or_else(|| protocol("client disconnected before hello"))?;
        let ClientFrame::Hello { version, hello } = first else {
            return Err(protocol("relay session must be followed by a client hello"));
        };
        if version != PROTOCOL_VERSION {
            send_error(writer, None, upgrade_error(version)).await?;
            return Err(protocol("unsupported protocol version"));
        }
        if hello.expected_daemon_fingerprint != self.identity.fingerprint {
            send_error(writer, None, remote_error(
                "MM_REMOTE_IDENTITY", "daemon identity does not match the enrolled target",
                false, MutationState::NotStarted,
            )).await?;
            return Err(MmError::Security {
                message: "remote client expected another daemon identity".to_owned(),
            });
        }
        write_frame(writer, &ServerFrame::Hello {
            version: PROTOCOL_VERSION,
            hello: ServerHello {
                client_nonce: hello.client_nonce, server_nonce: uuid::Uuid::new_v4(),
                daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
                daemon_fingerprint: self.identity.fingerprint.clone(), instance_id: self.instance_id,
            },
        }).await.map(|_| ())
    }

    async fn receive_request<R, W>(
        &self, reader: &mut FrameReader<&mut R>, writer: &mut W,
    ) -> Result<Option<ReceivedRequest>, MmError>
    where R: AsyncRead + Unpin + ?Sized, W: AsyncWrite + Unpin + ?Sized {
        let frame = tokio::time::timeout(
            Duration::from_millis(self.config.idle_timeout_ms),
            reader.next::<ClientFrame>(),
        ).await.map_err(|_| protocol("request timed out while idle"))??
            .ok_or_else(|| protocol("client disconnected before request"))?;
        let ClientFrame::Request {
            version, request_id, operation_nonce, deadline_unix_ms, scope, method,
        } = frame else {
            return Err(protocol("hello must be followed by one request"));
        };
        if version != PROTOCOL_VERSION {
            send_error(writer, Some(request_id), upgrade_error(version)).await?;
            return Err(protocol("request protocol version changed"));
        }
        let now = now_unix_ms()?;
        if deadline_unix_ms <= now || deadline_unix_ms.saturating_sub(now)
            > i64::try_from(MAX_REQUEST_TIMEOUT_MS).unwrap_or(i64::MAX)
        {
            send_error(writer, Some(request_id), remote_error(
                "MM_REMOTE_DEADLINE", "request deadline is expired or outside the supported bound",
                true, MutationState::NotStarted,
            )).await?;
            return Ok(None);
        }
        if let Err(message) = scope.validate() {
            send_operation_error(writer, request_id, &MmError::Security { message }, MutationState::NotStarted).await?;
            return Ok(None);
        }
        Ok(Some(ReceivedRequest {
            request_id, operation_nonce, deadline_unix_ms, scope, method, now_unix_ms: now,
        }))
    }

    async fn authorize_request<W>(
        &self, writer: &mut W, session: &AuthenticatedSession, request: ReceivedRequest,
    ) -> Result<Option<PreparedRequest>, MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        let authorized = self
            .check_addressed_node(&request.scope, &request.method)
            .and_then(|()| self.authorize(session, &request.scope, &request.method, request.now_unix_ms));
        let resolved = match authorized {
            Ok((principal, authorization)) => self
                .request_root(&request.scope, &request.method)
                .map(|root| (principal, authorization, root)),
            Err(error) => Err(error),
        };
        let (principal, authorization, project_root) = match resolved {
            Ok(resolved) => resolved,
            Err(error) => {
                self.audit_denied(session, request.request_id, request.operation_nonce,
                    &request.scope, &request.method, error.code())?;
                send_operation_error(writer, request.request_id, &error, MutationState::NotStarted).await?;
                return Ok(None);
            }
        };
        let mut request = request;
        if matches!(request.method.as_ref(), RemoteMethod::Exec(exec)
            if exec.stdin != crate::domain::observability::ExecStdinMode::Closed
                || exec.tty != crate::domain::observability::ExecTtyMode::Disabled)
        {
            send_operation_error(writer, request.request_id,
                &protocol("this remote protocol does not transport exec stdin or TTY state yet"),
                MutationState::NotStarted).await?;
            return Ok(None);
        }
        bind_principal(&mut request.method, &principal.name);
        if let Err(error) = self.journal.begin_request(request.operation_nonce, request.request_id) {
            send_operation_error(writer, request.request_id, &error, MutationState::NotStarted).await?;
            return Ok(None);
        }
        let context = RemoteRequestContext {
            request_id: request.request_id, operation_nonce: request.operation_nonce,
            principal_id: principal.id, principal: principal.name,
            key_fingerprint: session.key_fingerprint.clone(), session_id: session.session_id,
            source: session.source.clone(),
            request_plan_hash: request.method.reviewed_plan().map(str::to_owned),
            authorization, deadline_unix_ms: request.deadline_unix_ms,
            scope: request.scope, project_root, release: None,
        };
        Ok(Some(PreparedRequest {
            context,
            method: *request.method,
            timeout: Duration::from_millis(u64::try_from(
                request.deadline_unix_ms.saturating_sub(request.now_unix_ms),
            ).unwrap_or_default()),
        }))
    }

    /// The checkout a request runs against: the deployed commit for runtime
    /// operations, nothing yet for plans (they check out their own commit).
    /// Before a deployment finished here, reads take the checkout of the
    /// last one that began; job runs read as none without one.
    fn request_root(
        &self,
        scope: &crate::domain::remote::RemoteScope,
        method: &RemoteMethod,
    ) -> Result<std::path::PathBuf, MmError> {
        let read = matches!(method, RemoteMethod::Status | RemoteMethod::Logs(_) | RemoteMethod::JobRuns(_));
        match method {
            RemoteMethod::Status
            | RemoteMethod::Logs(_)
            | RemoteMethod::Exec(_)
            | RemoteMethod::Forward { .. }
            | RemoteMethod::Restart(_)
            | RemoteMethod::RunJob(_)
            | RemoteMethod::JobRuns(_) => {
                if let Some(deployed) = self.access.deployed(scope.project_id, &scope.environment)? {
                    return Ok(deployed.root);
                }
                if read && let Some(root) = self.attempted_root(scope)? {
                    return Ok(root);
                }
                if matches!(method, RemoteMethod::JobRuns(_)) {
                    return Ok(std::path::PathBuf::new());
                }
                Err(MmError::Remote {
                    code: crate::domain::error::NOT_DEPLOYED_CODE,
                    message: format!(
                        "nothing is deployed to '{}' on this host yet; run `mm deploy plan --environment {}`",
                        scope.environment, scope.environment
                    ),
                })
            }
            RemoteMethod::SecretRecipient
            | RemoteMethod::StageValues(_)
            | RemoteMethod::HostValues
            | RemoteMethod::StoreMigrate { .. } => {
                match self.access.deployed(scope.project_id, &scope.environment)? {
                    Some(deployed) => Ok(deployed.root),
                    None => self.repositories.checkouts(scope.project_id, &scope.environment)
                        .into_iter().next().ok_or_else(|| MmError::Environment {
                            message: format!(
                                "this host holds no commit of '{}' yet; run `mm deploy plan --environment {}` first, then set the value",
                                scope.environment, scope.environment
                            ),
                        }),
                }
            }
            _ => Ok(std::path::PathBuf::new()),
        }
    }

    fn attempted_root(
        &self,
        scope: &crate::domain::remote::RemoteScope,
    ) -> Result<Option<std::path::PathBuf>, MmError> {
        let Some(rollout) = self.access.deployments(scope.project_id, &scope.environment)?.rollout else {
            return Ok(None);
        };
        let root = self.repositories.checkout_path(scope.project_id, &scope.environment, &rollout.commit);
        Ok(root.join(crate::adapters::remote::CHECKOUT_MARKER).is_file().then_some(root))
    }

    async fn stream_logs<R, W>(
        &self,
        reader: &mut FrameReader<&mut R>,
        writer: &mut W,
        context: &RemoteRequestContext,
        mut stream: LogStream,
        cancellation: OperationCancellation,
        timeout: Duration,
    ) -> Result<(), MmError>
    where
        R: AsyncRead + Unpin + ?Sized,
        W: AsyncWrite + Unpin + ?Sized,
    {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut count = 0_u64;
        let mut bytes = 0_u64;
        loop {
            tokio::select! {
                item = stream.recv() => match item {
                    Some(Ok(record)) => {
                        count = count.saturating_add(1);
                        let frame = ServerFrame::Stream {
                            version: PROTOCOL_VERSION,
                            request_id: context.request_id,
                            sequence: count,
                            item: Box::new(RemoteStreamItem::Log(record)),
                        };
                        let encoded = encode_frame(&frame)?;
                        bytes = add_stream_bytes(bytes, encoded.len(), count)?;
                        write_raw_frame(writer, &encoded).await?;
                    }
                    Some(Err(error)) => return Err(error),
                    None => break,
                },
                frame = reader.next::<ClientFrame>() => {
                    if is_cancel(frame?, context.request_id)? {
                        cancellation.cancel();
                        break;
                    }
                }
                () = tokio::time::sleep_until(deadline) => {
                    cancellation.cancel();
                    return Err(protocol("stream deadline exceeded"));
                }
            }
        }
        send_end(writer, context.request_id, count, bytes).await
    }

    async fn stream_exec<R, W>(
        &self,
        reader: &mut FrameReader<&mut R>,
        writer: &mut W,
        context: &RemoteRequestContext,
        (mut stream, pass): (ExecStream, &MutationPass),
        cancellation: OperationCancellation,
        timeout: Duration,
    ) -> Result<(), MmError>
    where
        R: AsyncRead + Unpin + ?Sized,
        W: AsyncWrite + Unpin + ?Sized,
    {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut count = 0_u64;
        let mut bytes = 0_u64;
        let mut look = tokio::time::interval(ACCESS_RECHECK);
        loop {
            tokio::select! {
                // A principal that lost the right to run commands here
                // does not keep the one it has running.
                _ = look.tick() => {
                    if let Err(error) = self.recheck(context, pass) {
                        cancellation.cancel();
                        return Err(error);
                    }
                }
                item = stream.recv() => match item {
                    Some(Ok(event)) => {
                        count = count.saturating_add(1);
                        let frame = ServerFrame::Stream {
                            version: PROTOCOL_VERSION,
                            request_id: context.request_id,
                            sequence: count,
                            item: Box::new(RemoteStreamItem::Exec(event)),
                        };
                        let encoded = encode_frame(&frame)?;
                        bytes = add_stream_bytes(bytes, encoded.len(), count)?;
                        write_raw_frame(writer, &encoded).await?;
                    }
                    Some(Err(error)) => return Err(error),
                    None => break,
                },
                frame = reader.next::<ClientFrame>() => {
                    if is_cancel(frame?, context.request_id)? {
                        cancellation.cancel();
                        return Err(protocol(
                            "exec stream was cancelled; command outcome is unknown",
                        ));
                    }
                }
                () = tokio::time::sleep_until(deadline) => {
                    cancellation.cancel();
                    return Err(protocol("stream deadline exceeded"));
                }
            }
        }
        send_end(writer, context.request_id, count, bytes).await
    }

    fn audit(
        &self,
        context: &RemoteRequestContext,
        method: &str,
        outcome: &str,
        error_code: Option<&str>,
    ) -> Result<(), MmError> {
        self.journal.audit(&serde_json::json!({
            "timestamp_unix_ms": now_unix_ms()?,
            "request_id": context.request_id,
            "operation_nonce": context.operation_nonce,
            "principal": context.principal,
            "principal_id": context.principal_id,
            "key_fingerprint": context.key_fingerprint,
            "session_id": context.session_id,
            "source": context.source,
            "project_id": context.scope.project_id,
            "environment": context.scope.environment,
            "target": context.scope.target,
            "capability": context.authorization.capability,
            "method": method,
            "result": outcome,
            "plan_hash": context.request_plan_hash,
            "authorization_hash": context.authorization.authorization_hash,
            "policy_revision": context.authorization.policy_revision,
            "daemon_version": env!("CARGO_PKG_VERSION"),
            "error_code": error_code,
        }))
    }

    fn authorize(
        &self,
        session: &AuthenticatedSession,
        scope: &crate::domain::remote::RemoteScope,
        method: &RemoteMethod,
        now_unix_ms: i64,
    ) -> Result<(ResolvedPrincipal, Authorization), MmError> {
        let snapshot = self.access.load()?;
        let enrolled = snapshot.host.projects.get(&scope.project_id).ok_or_else(|| {
            MmError::Security { message: "project is not enrolled on this host".to_owned() }
        })?;
        if !enrolled.environments.contains(&scope.environment) {
            return Err(MmError::Security {
                message: not_set_up(&scope.environment),
            });
        }
        let principal = snapshot
            .policy
            .authenticate(&session.key_fingerprint, scope.project_id, &scope.environment, now_unix_ms)
            .map_err(|message| MmError::Security { message })?;
        let resolved = ResolvedPrincipal {
            id: principal.id,
            name: principal.display_name.clone(),
        };
        let (capability, service) = method_capability(method);
        let authorization = snapshot
            .policy
            .authorize(&AccessRequest {
                principal_id: principal.id,
                capability,
                project_id: scope.project_id,
                environment: &scope.environment,
                service,
                job: None,
                gate: None,
            })
            .or_else(|message| {
                self.continued_rollout(&snapshot, (&session.key_fingerprint, principal.id), scope, method)
                    .ok_or(message)
            })
            .map_err(|message| MmError::Security { message })?;
        Ok((resolved, authorization))
    }

    fn reauthorize_with_policy(
        &self,
        context: &RemoteRequestContext,
        method: &RemoteMethod,
        policy: &crate::domain::access::AccessPolicy,
    ) -> Result<Authorization, MmError> {
        let principal = policy
            .authenticate(
                &context.key_fingerprint,
                context.scope.project_id,
                &context.scope.environment,
                now_unix_ms()?,
            )
            .map_err(|message| MmError::Security { message })?;
        if principal.id != context.principal_id {
            return Err(MmError::Security {
                message: "authenticated principal mapping changed before mutation".to_owned(),
            });
        }
        let (capability, service) = method_capability(method);
        self.reauthorize_capability_with_policy(context, capability, service, policy)
    }

    fn reauthorize_capability_with_policy(
        &self,
        context: &RemoteRequestContext,
        capability: Capability,
        service: Option<&str>,
        policy: &crate::domain::access::AccessPolicy,
    ) -> Result<Authorization, MmError> {
        policy
            .authorize(&AccessRequest {
                principal_id: context.principal_id,
                capability,
                project_id: context.scope.project_id,
                environment: &context.scope.environment,
                service,
                job: None,
                gate: None,
            })
            .map_err(|message| MmError::Security { message })
    }

    fn audit_denied(
        &self,
        session: &AuthenticatedSession,
        request_id: uuid::Uuid,
        operation_nonce: uuid::Uuid,
        scope: &crate::domain::remote::RemoteScope,
        method: &RemoteMethod,
        error_code: &str,
    ) -> Result<(), MmError> {
        let (capability, _) = method_capability(method);
        self.journal.audit(&serde_json::json!({
            "timestamp_unix_ms": now_unix_ms()?,
            "request_id": request_id,
            "operation_nonce": operation_nonce,
            "key_fingerprint": session.key_fingerprint,
            "session_id": session.session_id,
            "source": session.source,
            "project_id": scope.project_id,
            "environment": scope.environment,
            "target": scope.target,
            "capability": capability,
            "method": method.name(),
            "result": "denied",
            "error_code": error_code,
            "daemon_version": env!("CARGO_PKG_VERSION"),
        }))
    }
}
