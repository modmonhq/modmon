// Receiving the frames of a log or exec stream. Included into `client.rs`.

#[derive(Clone, Copy)]
enum StreamKind {
    Logs,
    Exec,
}

enum StreamValue {
    Log(crate::domain::observability::LogRecord),
    Exec(crate::domain::observability::ExecEvent),
}

struct StreamReceiver {
    receiver: tokio::sync::mpsc::Receiver<Result<StreamValue, MmError>>,
    cancellation: OperationCancellation,
}

struct StreamReceiveContext {
    connection: Connection,
    request_id: uuid::Uuid,
    timeout: Duration,
    disconnect_state: MutationState,
    kind: StreamKind,
    cancellation: OperationCancellation,
    sender: tokio::sync::mpsc::Sender<Result<StreamValue, MmError>>,
}

async fn receive_stream(context: StreamReceiveContext) {
    let StreamReceiveContext {
        mut connection,
        request_id,
        timeout,
        disconnect_state,
        kind,
        cancellation,
        sender,
    } = context;
    let deadline = tokio::time::Instant::now() + timeout;
    let mut sequence = 0_u64;
    let mut bytes = 0_u64;
    let mut cancel_sent = false;
    loop {
        tokio::select! {
            frame = connection.stdout.next::<ServerFrame>() => {
                let Some(frame) = receive_frame(frame, (&connection.subject, disconnect_state), &sender).await else {
                    break;
                };
                let encoded = match encode_frame(&frame) {
                    Ok(value) => value.len(),
                    Err(error) => {
                        send_stream_error(&sender, &cancellation, error).await;
                        break;
                    }
                };
                match frame {
                    ServerFrame::Stream {
                        version: PROTOCOL_VERSION,
                        request_id: received,
                        sequence: received_sequence,
                        item,
                    } if received == request_id && received_sequence == sequence.saturating_add(1) => {
                        sequence = received_sequence;
                        bytes = bytes.saturating_add(u64::try_from(encoded).unwrap_or(u64::MAX));
                        if sequence > MAX_STREAM_FRAMES || bytes > MAX_STREAM_BYTES {
                            cancellation.cancel();
                            send_stream_error(&sender, &cancellation, protocol("remote stream exceeded its client-side frame or byte limit")).await;
                            break;
                        }
                        let item = match (kind, *item) {
                            (StreamKind::Logs, RemoteStreamItem::Log(record)) => StreamValue::Log(record),
                            (StreamKind::Exec, RemoteStreamItem::Exec(event)) => StreamValue::Exec(event),
                            _ => {
                                send_stream_error(&sender, &cancellation, protocol("remote stream item type changed")).await;
                                break;
                            }
                        };
                        if sender.send(Ok(item)).await.is_err() {
                            cancellation.cancel();
                        }
                    }
                    ServerFrame::End {
                        version: PROTOCOL_VERSION,
                        request_id: received,
                        frames,
                        bytes: reported_bytes,
                    } if received == request_id && frames == sequence && reported_bytes == bytes => break,
                    ServerFrame::Error { request_id: received, error, .. }
                        if received.is_none() || received == Some(request_id) => {
                            send_stream_error(&sender, &cancellation, server_error(error)).await;
                            break;
                        }
                    _ => {
                        send_stream_error(&sender, &cancellation, protocol("remote stream frame is out of order or mismatched")).await;
                        break;
                    }
                }
            }
            () = tokio::time::sleep(Duration::from_millis(25)), if !cancel_sent => {
                if cancellation.is_cancelled() {
                    let cancel = ClientFrame::Cancel { version: PROTOCOL_VERSION, request_id };
                    if write_frame(&mut connection.stdin, &cancel).await.is_err() {
                        break;
                    }
                    cancel_sent = true;
                }
            }
            () = tokio::time::sleep_until(deadline) => {
                cancellation.cancel();
                if write_frame(
                    &mut connection.stdin,
                    &ClientFrame::Cancel { version: PROTOCOL_VERSION, request_id },
                ).await.is_err() {
                    tracing::debug!(%request_id, "remote cancellation frame could not be delivered");
                }
                send_stream_error(&sender, &cancellation, stream_late(&connection.subject, timeout)).await;
                break;
            }
        }
        if cancellation.is_cancelled() && !cancel_sent {
            continue;
        }
    }
    if let Err(error) = connection.stdin.shutdown().await {
        tracing::debug!(%request_id, %error, "remote stream stdin shutdown failed");
    }
    cancellation.cancel();
}

/// A stream the host did not end within its deadline.
fn stream_late(subject: &str, timeout: Duration) -> MmError {
    MmError::Timeout {
        message: format!(
            "{subject} did not end the stream within its deadline of {} s",
            timeout.as_secs()
        ),
    }
}

async fn send_stream_error(
    sender: &tokio::sync::mpsc::Sender<Result<StreamValue, MmError>>,
    cancellation: &OperationCancellation,
    error: MmError,
) {
    if sender.send(Err(error)).await.is_err() {
        cancellation.cancel();
    }
}

async fn receive_frame(
    frame: Result<Option<ServerFrame>, MmError>,
    (subject, disconnect_state): (&str, MutationState),
    sender: &tokio::sync::mpsc::Sender<Result<StreamValue, MmError>>,
) -> Option<ServerFrame> {
    match frame {
        Ok(Some(frame)) => Some(frame),
        Ok(None) => {
            if sender
                .send(Err(disconnect(subject, disconnect_state)))
                .await
                .is_err()
            {
                tracing::debug!("remote stream receiver closed before disconnect diagnostics");
            }
            None
        }
        Err(error) => {
            if sender.send(Err(error)).await.is_err() {
                tracing::debug!("remote stream receiver closed before transport diagnostics");
            }
            None
        }
    }
}
