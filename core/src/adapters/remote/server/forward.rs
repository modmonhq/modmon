impl RemoteDaemon {
    /// Carries one forwarded TCP connection between the client's data frames
    /// and the service container's port. Either side closing ends it.
    async fn forward<R, W>(
        &self,
        reader: &mut FrameReader<&mut R>,
        writer: &mut W,
        context: &RemoteRequestContext,
        (service, port): (&str, u16),
        timeout: Duration,
    ) -> Result<(), MmError>
    where
        R: AsyncRead + Unpin + ?Sized,
        W: AsyncWrite + Unpin + ?Sized,
    {
        use base64::Engine;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let address = timed(
            timeout.min(Duration::from_secs(30)),
            self.use_cases.forward_address(context, service, port),
        )
        .await?;
        let tcp = tokio::time::timeout(Duration::from_secs(10), tokio::net::TcpStream::connect(address))
            .await
            .map_err(|_| MmError::RuntimeResponse {
                message: format!("service '{service}' did not accept a connection on port {port}"),
            })?
            .map_err(|source| MmError::Io {
                context: format!("connecting to service '{service}' port {port}"),
                source,
            })?;
        send_result(writer, context, RemoteResult::StreamStarted).await?;
        let (mut tcp_read, mut tcp_write) = tcp.into_split();
        let request_id = context.request_id;
        let mut sequence = 0_u64;
        let mut to_client = 0_u64;
        let mut to_service = 0_u64;
        let engine = base64::engine::general_purpose::STANDARD;
        // Both directions run until one finishes; dropping the other is safe
        // because the session ends either way.
        let outcome = {
            let upstream = async {
                let mut buffer = vec![0_u8; 32 * 1024];
                loop {
                    let count = tcp_read.read(&mut buffer).await.map_err(|source| MmError::Io {
                        context: format!("reading from service '{service}'"),
                        source,
                    })?;
                    if count == 0 {
                        return Ok::<_, MmError>(());
                    }
                    sequence += 1;
                    to_client = to_client.saturating_add(count as u64);
                    write_frame(
                        writer,
                        &ServerFrame::Stream {
                            version: context.protocol,
                            request_id,
                            sequence,
                            item: Box::new(RemoteStreamItem::Data(engine.encode(&buffer[..count]))),
                        },
                    )
                    .await?;
                }
            };
            let downstream = async {
                loop {
                    match reader.next::<ClientFrame>().await? {
                        Some(ClientFrame::Data { request_id: received, bytes, .. }) if received == request_id => {
                            let bytes = engine.decode(bytes).map_err(|_| protocol("forward data is not base64"))?;
                            to_service = to_service.saturating_add(bytes.len() as u64);
                            tcp_write.write_all(&bytes).await.map_err(|source| MmError::Io {
                                context: format!("writing to service '{service}'"),
                                source,
                            })?;
                        }
                        Some(ClientFrame::Cancel { request_id: received, .. }) if received == request_id => {
                            return Ok::<_, MmError>(());
                        }
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
        if let Err(error) = tcp_write.shutdown().await {
            tracing::debug!(%error, "forwarded connection was already closed");
        }
        self.audit_forward(context, (service, port), (to_client, to_service), outcome.is_ok())?;
        outcome?;
        send_end(writer, context, sequence, to_client).await
    }

    fn audit_forward(
        &self,
        context: &RemoteRequestContext,
        (service, port): (&str, u16),
        (to_client, to_service): (u64, u64),
        closed: bool,
    ) -> Result<(), MmError> {
        self.journal.audit(&serde_json::json!({
            "timestamp_unix_ms": now_unix_ms()?, "request_id": context.request_id,
            "principal": context.principal, "principal_id": context.principal_id,
            "key_fingerprint": context.key_fingerprint, "session_id": context.session_id,
            "project_id": context.scope.project_id, "environment": context.scope.environment,
            "method": "forward", "service": service, "port": port,
            "bytes_to_client": to_client, "bytes_to_service": to_service,
            "result": if closed { "closed" } else { "failed" },
            "daemon_version": env!("CARGO_PKG_VERSION"),
        }))
    }
}
