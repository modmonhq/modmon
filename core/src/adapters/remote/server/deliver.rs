// Push-mode delivery: a client sends a commit's tracked files to a host
// whose policy says the project is pushed, and the host keeps them as the
// commit's checkout once they are proven to be that commit. A host that
// fetches the project refuses every bundle.

use crate::adapters::remote::bundle::{BundleRecord, DeliveryScope};
use crate::domain::delivery::{BundleOffer, Delivered, Delivery};

/// The text form of a delivery, for `mmd access sync --bundle-stdin`: the
/// offer as one line of JSON, then the bundle as lines of base64.
const BUNDLE_LINE_BYTES: usize = 1024 * 1024;

impl RemoteDaemon {
    /// How the host policy has the project's files delivered.
    fn delivery(&self, project: crate::domain::workspace::ProjectId) -> Result<Delivery, MmError> {
        self.access
            .load()?
            .host
            .projects
            .get(&project)
            .map(|enrolled| enrolled.delivery)
            .ok_or_else(|| MmError::Security { message: "project is not enrolled on this host".to_owned() })
    }

    /// The branches the environment takes commits from now: the last
    /// synced commit's `deploy-from`, or the host's initial one.
    fn deploy_from_in_force(
        &self,
        project: crate::domain::workspace::ProjectId,
        environment: &str,
    ) -> Result<Option<Vec<String>>, MmError> {
        let snapshot = self.access.load()?;
        let initial = snapshot
            .host
            .projects
            .get(&project)
            .and_then(|enrolled| enrolled.initial_deploy_from.get(environment).cloned());
        Ok(snapshot
            .records
            .iter()
            .find(|record| record.project_id == project && record.environment == environment)
            .and_then(|record| record.deploy_from.clone())
            .or(initial))
    }

    /// Refuses a bundle on a host that fetches the project.
    fn require_pushed(&self, project: crate::domain::workspace::ProjectId) -> Result<(), MmError> {
        if self.delivery(project)?.is_push() {
            return Ok(());
        }
        Err(MmError::Security {
            message: "this host fetches this project from its origin and takes no files from a client. Remove `delivery: push` from the target in mm.yaml, or have a host admin set `delivery: push` for the project in /etc/mm/host.yaml (`mm target setup plan` does it for a target that sets it)".to_owned(),
        })
    }

    /// What the host already holds for an offer, when that answers it:
    /// the same bundle, or any bundle when the client has none to send.
    fn already_delivered(
        &self,
        scope: &DeliveryScope<'_>,
        offer: &BundleOffer,
    ) -> Result<Option<BundleRecord>, MmError> {
        let held = self.repositories.delivered(scope.project, scope.environment, &offer.commit)?;
        let Some(record) = held else {
            return match offer.bundle {
                Some(_) => Ok(None),
                None => Err(MmError::Environment {
                    message: format!(
                        "this host does not hold commit {commit}, and the checkout mm runs in does not either, so there is nothing to send. Fetch the commit there (`git fetch origin {commit}`) and run the command again",
                        commit = offer.commit
                    ),
                }),
            };
        };
        if offer.bundle.as_ref().is_some_and(|bundle| bundle.hash != record.bundle_hash) {
            return Ok(None);
        }
        self.repositories.state_branches(scope, record, &offer.branches).map(Some)
    }

    /// Takes one commit's bundle from a client. The upload is written to
    /// disk as it arrives; it ends at the request's deadline, when the
    /// client goes away, and when the principal loses the right to send
    /// it, and each of those leaves nothing behind.
    async fn deliver<R, W>(
        &self,
        reader: &mut FrameReader<&mut R>,
        writer: &mut W,
        context: &RemoteRequestContext,
        offer: BundleOffer,
        pass: &MutationPass,
        timeout: Duration,
    ) -> Result<(), MmError>
    where
        R: AsyncRead + Unpin + ?Sized,
        W: AsyncWrite + Unpin + ?Sized,
    {
        use base64::Engine;

        self.require_pushed(context.scope.project_id)?;
        offer.validate().map_err(|message| MmError::Security { message })?;
        // `deploy-from` is held against the branches the client states
        // before a byte is taken: a commit the plan would refuse is not
        // uploaded first.
        check_stated_branches(
            &offer.commit,
            &offer.branches,
            self.deploy_from_in_force(context.scope.project_id, &context.scope.environment)?
                .as_deref(),
        )?;
        let scope = DeliveryScope {
            project: context.scope.project_id,
            environment: &context.scope.environment,
            principal: &context.principal,
            now_unix_ms: now_unix_ms()?,
        };
        if let Some(record) = self.already_delivered(&scope, &offer)? {
            let delivered = record.delivered(false);
            self.audit_delivery(context, &delivered, &record.branches)?;
            return send_result(writer, context.request_id, RemoteResult::Delivered(Box::new(delivered))).await;
        }
        let mut receiver = self.repositories.receive(&scope, &offer)?;
        self.journal.set_request_phase(context.operation_nonce, "running")?;
        send_result(writer, context.request_id, RemoteResult::StreamStarted).await?;
        let deadline = tokio::time::Instant::now() + timeout;
        // The upload is at work while the loop looks (`mm deploy status`
        // shows it), and `mm deploy cancel` stops it like any operation.
        let mut look = tokio::time::interval(ACCESS_RECHECK);
        let stopped = pass.stopped();
        tokio::pin!(stopped);
        let nothing_kept = |reason: &str| MmError::State { message: format!("{reason}; nothing was kept") };
        while receiver.remaining() > 0 {
            tokio::select! {
                _ = look.tick() => self.look_at(context, pass)?,
                error = &mut stopped => return Err(error),
                () = tokio::time::sleep_until(deadline) => {
                    return Err(MmError::Timeout {
                        message: "the bundle did not arrive before the request's deadline; nothing was kept".to_owned(),
                    });
                }
                frame = reader.next::<ClientFrame>() => match frame? {
                    Some(ClientFrame::Data { request_id, bytes, .. }) if request_id == context.request_id => {
                        let bytes = base64::engine::general_purpose::STANDARD
                            .decode(bytes)
                            .map_err(|_| protocol("bundle data is not base64"))?;
                        receiver.write(&bytes)?;
                    }
                    Some(ClientFrame::Cancel { request_id, .. }) if request_id == context.request_id => {
                        return Err(nothing_kept("the client stopped the upload"));
                    }
                    None => return Err(nothing_kept("the client went away during the upload")),
                    Some(_) => return Err(protocol("unexpected frame during a delivery")),
                }
            }
        }
        // Checking and unpacking reads every file: off the request threads,
        // and still under the request's deadline and the access watch.
        let checking = tokio::task::spawn_blocking(move || receiver.finish());
        let checked = async {
            checking.await.map_err(|error| MmError::State {
                message: format!("checking the bundle stopped: {error}"),
            })?
        };
        let delivered = self.watched(context, pass, remaining(context)?, checked).await?;
        self.audit_delivery(context, &delivered, &offer.branches)?;
        send_result(writer, context.request_id, RemoteResult::Delivered(Box::new(delivered))).await
    }

    /// The audit record of a commit a client delivered or the host held.
    fn audit_delivery(
        &self,
        context: &RemoteRequestContext,
        delivered: &Delivered,
        branches: &[String],
    ) -> Result<(), MmError> {
        self.journal.audit(&serde_json::json!({
            "timestamp_unix_ms": now_unix_ms()?, "event": "bundle-delivered",
            "request_id": context.request_id, "operation_nonce": context.operation_nonce,
            "principal": context.principal, "principal_id": context.principal_id,
            "key_fingerprint": context.key_fingerprint, "session_id": context.session_id,
            "source": context.source, "project_id": context.scope.project_id,
            "environment": context.scope.environment, "target": context.scope.target,
            "method": "deliver", "commit": delivered.commit, "bundle_hash": delivered.bundle_hash,
            "bundle_bytes": delivered.bytes, "bundle_files": delivered.files,
            "received": delivered.received, "stated_branches": branches,
            "result": "delivered", "daemon_version": env!("CARGO_PKG_VERSION"),
        }))
    }

    /// Takes a bundle as host root, without the relay: what `mm target
    /// setup` sends with the first access sync of a host that fetches
    /// nothing. `input` is the offer as one line of JSON, then the bundle
    /// as lines of base64. The bundle is checked exactly as a client's is.
    pub fn deliver_as_root(
        &self,
        project: crate::domain::workspace::ProjectId,
        environment: &str,
        input: &mut dyn std::io::BufRead,
    ) -> Result<Delivered, MmError> {
        use base64::Engine;

        self.require_pushed(project)?;
        let unreadable = |what: &str| MmError::Environment {
            message: format!("the bundle on standard input is not readable: {what}"),
        };
        let mut line = String::new();
        let mut next_line = |line: &mut String| -> Result<bool, MmError> {
            line.clear();
            let mut limited = std::io::Read::take(&mut *input, (BUNDLE_LINE_BYTES + 1) as u64);
            let read = std::io::BufRead::read_line(&mut limited, line).map_err(|source| MmError::Io {
                context: "reading the bundle from standard input".to_owned(),
                source,
            })?;
            if read > BUNDLE_LINE_BYTES {
                return Err(unreadable("a line is too long"));
            }
            Ok(read > 0)
        };
        if !next_line(&mut line)? {
            return Err(unreadable("it is empty"));
        }
        let offer: BundleOffer = serde_json::from_str(line.trim()).map_err(|_| unreadable("its first line is not an offer"))?;
        let scope = DeliveryScope { project, environment, principal: HOST_ROOT, now_unix_ms: now_unix_ms()? };
        let delivered = if let Some(record) = self.already_delivered(&scope, &offer)? {
            record.delivered(false)
        } else {
            let mut receiver = self.repositories.receive(&scope, &offer)?;
            while next_line(&mut line)? {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(line.trim())
                    .map_err(|_| unreadable("a line is not base64"))?;
                receiver.write(&bytes)?;
            }
            receiver.finish()?
        };
        self.journal.audit(&serde_json::json!({
            "timestamp_unix_ms": now_unix_ms()?, "event": "bundle-delivered",
            "principal": HOST_ROOT, "project_id": project, "environment": environment,
            "method": "deliver", "commit": delivered.commit, "bundle_hash": delivered.bundle_hash,
            "bundle_bytes": delivered.bytes, "bundle_files": delivered.files,
            "received": delivered.received, "stated_branches": offer.branches,
            "result": "delivered", "daemon_version": env!("CARGO_PKG_VERSION"),
        }))?;
        Ok(delivered)
    }
}

/// On a host the project is pushed to, `deploy-from` is held against the
/// branches the delivering client stated: the host has no repository to
/// look the commit up in.
fn check_stated_branches(commit: &str, stated: &[String], allowed: Option<&[String]>) -> Result<(), MmError> {
    let Some(allowed) = allowed else {
        return Ok(());
    };
    if stated.iter().any(|branch| crate::domain::branch_pattern::allows(allowed, branch)) {
        return Ok(());
    }
    let found = if stated.is_empty() {
        "the client that sends it stated no branch of the origin that contains it. Send it from a checkout that has the branch (`git fetch origin`, then plan again)".to_owned()
    } else {
        format!("the client that sends it stated it is on {}", stated.join(", "))
    };
    Err(MmError::Security {
        message: format!("commit {commit} is not on an allowed branch ({}); {found}", allowed.join(", ")),
    })
}
