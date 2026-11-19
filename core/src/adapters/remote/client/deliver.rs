// Sending a commit's bundle to a host that takes the project's files from
// the client.

/// How long a delivery may take: a bundle of the largest size over a slow
/// link, and the host's check of every file.
const DELIVER_REQUEST_TIMEOUT_MS: u64 = 60 * 60 * 1_000;

impl SshRemoteTransport {
    /// Offers the commit and, when the host does not hold it, sends the
    /// bundle file in data frames. The host answers once it has checked
    /// that the files are the commit's.
    async fn deliver_bundle(
        &self,
        scope: RemoteScope,
        offer: crate::domain::delivery::BundleOffer,
        bundle: Option<PathBuf>,
    ) -> Result<crate::domain::delivery::Delivered, MmError> {
        scope
            .validate()
            .map_err(|message| MmError::Security { message })?;
        let timeout = Duration::from_millis(DELIVER_REQUEST_TIMEOUT_MS);
        let deadline = tokio::time::Instant::now() + timeout;
        let method = RemoteMethod::Deliver(Box::new(offer.clone()));
        let (mut connection, request_id) =
            self.open(scope, method, timeout, MutationState::Unknown).await?;
        let delivered = match delivery_answer(&mut connection, request_id, deadline).await? {
            RemoteResult::Delivered(delivered) => *delivered,
            RemoteResult::StreamStarted => {
                let path = bundle.ok_or_else(|| {
                    protocol("the host asked for a bundle this client did not offer")
                })?;
                send_bundle(&mut connection, request_id, &path).await?;
                match delivery_answer(&mut connection, request_id, deadline).await? {
                    RemoteResult::Delivered(delivered) => *delivered,
                    _ => return Err(protocol("remote delivery returned another result type")),
                }
            }
            _ => return Err(protocol("remote delivery returned another result type")),
        };
        let other_bundle = offer
            .bundle
            .as_ref()
            .is_some_and(|offered| offered.hash != delivered.bundle_hash);
        if delivered.commit != offer.commit || other_bundle {
            return Err(MmError::Security {
                message: format!(
                    "{} answered the delivery of commit {} with another commit or bundle; nothing was planned",
                    self.endpoint.subject, offer.commit
                ),
            });
        }
        Ok(delivered)
    }
}

/// The host's next answer to a delivery.
async fn delivery_answer(
    connection: &mut Connection,
    request_id: uuid::Uuid,
    deadline: tokio::time::Instant,
) -> Result<RemoteResult, MmError> {
    let state = MutationState::Unknown;
    let waited = deadline.saturating_duration_since(tokio::time::Instant::now());
    let frame = tokio::time::timeout_at(deadline, connection.stdout.next())
        .await
        .map_err(|_| late(&connection.subject, "deliver", waited, state))??
        .ok_or_else(|| disconnect(&connection.subject, state))?;
    let agreed = connection.protocol;
    match frame {
        ServerFrame::Result {
            version,
            request_id: received,
            result,
        } if version == agreed && received == request_id => Ok(*result),
        ServerFrame::Error {
            version,
            request_id: received,
            error,
        } if version == agreed && (received.is_none() || received == Some(request_id)) => {
            Err(server_error(error))
        }
        _ => Err(protocol("remote delivery response did not match the request")),
    }
}

/// Sends the bundle file, a piece at a time. A host that refuses the
/// upload part way says why; that is reported instead of the closed pipe.
async fn send_bundle(
    connection: &mut Connection,
    request_id: uuid::Uuid,
    path: &std::path::Path,
) -> Result<(), MmError> {
    use base64::Engine;
    use std::io::Read;

    // A local file, read a piece at a time between the frames.
    let mut file = std::fs::File::open(path).map_err(|source| MmError::Io {
        context: format!("opening the bundle {}", path.display()),
        source,
    })?;
    let engine = base64::engine::general_purpose::STANDARD;
    let version = connection.protocol;
    let Connection { stdin, stdout, subject, .. } = connection;
    let upload = async {
        let mut buffer = vec![0_u8; crate::domain::delivery::BUNDLE_CHUNK_BYTES];
        loop {
            let count = file.read(&mut buffer).map_err(|source| MmError::Io {
                context: format!("reading the bundle {}", path.display()),
                source,
            })?;
            if count == 0 {
                return Ok::<_, MmError>(());
            }
            write_frame(
                stdin,
                &ClientFrame::Data {
                    version,
                    request_id,
                    bytes: engine.encode(&buffer[..count]),
                },
            )
            .await?;
        }
    };
    // The host says nothing during an upload unless it gives it up.
    let refused = |frame: Result<Option<ServerFrame>, MmError>| match frame {
        Ok(Some(ServerFrame::Error { error, .. })) => server_error(error),
        Ok(Some(_)) => protocol("unexpected frame during a delivery"),
        Ok(None) => disconnect(subject, MutationState::Unknown),
        Err(error) => error,
    };
    let sent = tokio::select! {
        sent = upload => sent,
        frame = stdout.next::<ServerFrame>() => Err(refused(frame)),
    };
    let Err(error) = sent else {
        return Ok(());
    };
    let said = tokio::time::timeout(Duration::from_secs(2), stdout.next::<ServerFrame>()).await;
    Err(match said {
        Ok(Ok(Some(ServerFrame::Error { error, .. }))) => server_error(error),
        _ => error,
    })
}
