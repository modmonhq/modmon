impl SshRemoteTransport {
    /// One relayed SSH session per forwarded connection: the request opens
    /// the daemon's connection to the container, then bytes flow as data
    /// frames until either side closes.
    async fn forward_connection(
        &self,
        scope: RemoteScope,
        service: String,
        port: u16,
        local: tokio::net::TcpStream,
    ) -> Result<crate::domain::forward::ForwardTotals, MmError> {
        use base64::Engine;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        scope
            .validate()
            .map_err(|message| MmError::Security { message })?;
        let mut connection = self.connection().await?;
        let request_id = connection
            .ask(
                scope,
                RemoteMethod::Forward { service, port },
                Duration::from_millis(MAX_REQUEST_TIMEOUT_MS),
                MutationState::NotStarted,
            )
            .await?;
        await_forward_start(&mut connection, request_id).await?;
        let engine = base64::engine::general_purpose::STANDARD;
        let (mut local_read, mut local_write) = local.into_split();
        let mut totals = crate::domain::forward::ForwardTotals::default();
        let version = connection.protocol;
        let Connection { stdin, stdout, .. } = &mut connection;
        let outcome = {
            let upstream = async {
                let mut buffer = vec![0_u8; 32 * 1024];
                loop {
                    let count = local_read.read(&mut buffer).await.map_err(|source| MmError::Io {
                        context: "reading the forwarded local connection".to_owned(),
                        source,
                    })?;
                    if count == 0 {
                        write_frame(
                            stdin,
                            &ClientFrame::Cancel {
                                version,
                                request_id,
                            },
                        )
                        .await?;
                        return Ok::<_, MmError>(());
                    }
                    totals.to_service = totals.to_service.saturating_add(count as u64);
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
            let downstream = async {
                loop {
                    match stdout.next::<ServerFrame>().await? {
                        Some(ServerFrame::Stream { request_id: received, item, .. })
                            if received == request_id =>
                        {
                            let RemoteStreamItem::Data(bytes) = *item else {
                                return Err(protocol("unexpected stream item during a forward"));
                            };
                            let bytes = engine
                                .decode(bytes)
                                .map_err(|_| protocol("forward data is not base64"))?;
                            totals.from_service =
                                totals.from_service.saturating_add(bytes.len() as u64);
                            local_write.write_all(&bytes).await.map_err(|source| MmError::Io {
                                context: "writing the forwarded local connection".to_owned(),
                                source,
                            })?;
                        }
                        Some(ServerFrame::End { request_id: received, .. })
                            if received == request_id =>
                        {
                            return Ok(());
                        }
                        Some(ServerFrame::Error { error, .. }) => return Err(server_error(error)),
                        None => return Ok(()),
                        Some(_) => return Err(protocol("unexpected frame during a forward")),
                    }
                }
            };
            tokio::select! {
                result = upstream => result,
                result = downstream => result,
            }
        };
        if let Err(error) = local_write.shutdown().await {
            tracing::debug!(%error, "forwarded local connection was already closed");
        }
        outcome.map(|()| totals)
    }
}

/// Waits for the daemon to confirm it connected to the service.
async fn await_forward_start(connection: &mut Connection, request_id: uuid::Uuid) -> Result<(), MmError> {
        let start = tokio::time::timeout(
            Duration::from_secs(60),
            connection.stdout.next(),
        )
        .await
        .map_err(|_| late(&connection.subject, "forward", Duration::from_secs(60), MutationState::NotStarted))??
        .ok_or_else(|| disconnect(&connection.subject, MutationState::NotStarted))?;
        let agreed = connection.protocol;
        match start {
            ServerFrame::Result {
                version,
                request_id: received,
                result,
            } if version == agreed
                && received == request_id
                && *result == RemoteResult::StreamStarted => {}
            ServerFrame::Error {
                version,
                request_id: received,
                error,
            } if version == agreed && (received.is_none() || received == Some(request_id)) => {
                return Err(server_error(error));
            }
            _ => return Err(protocol("remote forward start response is invalid")),
        }
        Ok(())
}
