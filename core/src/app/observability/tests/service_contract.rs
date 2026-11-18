use super::*;

#[tokio::test]
async fn logs_authorize_select_redact_and_keep_raw_mode_explicit()
-> Result<(), Box<dyn std::error::Error>> {
    let redacted = setup(vec![RuntimeCapability::StreamLogs])?;
    lock(&redacted.secrets.resolutions)?.push_back(Ok(secret(SecretStatus::Set, Some(SENTINEL))?));
    redacted.operations.queue_log(vec![Ok(log_record())])?;
    let cancellation = OperationCancellation::default();
    let mut stream = redacted
        .service
        .logs(
            &redacted.selection,
            log_request(LogMode::Redacted),
            cancellation.clone(),
        )
        .await?;
    assert_eq!(
        stream
            .recv()
            .await
            .transpose()?
            .ok_or("missing log record")?
            .message,
        "public"
    );
    assert!(stream.recv().await.is_none());
    let calls = redacted.operations.log_calls()?;
    assert_eq!(calls[0].target, "app");
    assert_eq!(
        calls[0]
            .request
            .selector
            .as_ref()
            .and_then(|selector| selector.module.as_deref()),
        Some("api")
    );
    assert_eq!(calls[0].redactions, [SENTINEL]);
    assert_eq!(
        lock(&redacted.authorizer.requests)?[0].operation,
        RuntimeOperation::Logs
    );

    let raw = setup(vec![RuntimeCapability::StreamLogs])?;
    raw.operations.queue_log(Vec::new())?;
    let mut stream = raw
        .service
        .logs(
            &raw.selection,
            log_request(LogMode::Raw),
            OperationCancellation::default(),
        )
        .await?;
    assert!(stream.recv().await.is_none());
    assert_eq!(raw.secrets.calls.load(Ordering::SeqCst), 0);
    assert!(raw.operations.log_calls()?[0].redactions.is_empty());
    assert_eq!(
        lock(&raw.authorizer.requests)?[0].operation,
        RuntimeOperation::RawLogs
    );
    Ok(())
}

#[tokio::test]
async fn log_failures_stop_before_runtime_effects_and_invalid_secret_states_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let invalid = setup(vec![RuntimeCapability::StreamLogs])?;
    let mut request = log_request(LogMode::Redacted);
    request.tail = 1_000_001;
    assert!(
        invalid
            .service
            .logs(
                &invalid.selection,
                request,
                OperationCancellation::default()
            )
            .await
            .is_err()
    );
    assert!(lock(&invalid.authorizer.requests)?.is_empty());

    let denied = setup(vec![RuntimeCapability::StreamLogs])?;
    denied.authorizer.denied.store(true, Ordering::SeqCst);
    assert!(
        denied
            .service
            .logs(
                &denied.selection,
                log_request(LogMode::Redacted),
                OperationCancellation::default(),
            )
            .await
            .is_err()
    );
    assert!(denied.operations.log_calls()?.is_empty());
    assert_eq!(denied.secrets.calls.load(Ordering::SeqCst), 0);

    let unsupported = setup(Vec::new())?;
    assert!(
        unsupported
            .service
            .logs(
                &unsupported.selection,
                log_request(LogMode::Redacted),
                OperationCancellation::default(),
            )
            .await
            .is_err()
    );
    assert!(unsupported.operations.log_calls()?.is_empty());

    for resolution in [
        secret(SecretStatus::Unset, None)?,
        secret(SecretStatus::ProviderUnavailable, None)?,
        secret(SecretStatus::Unset, Some(SENTINEL))?,
    ] {
        let setup = setup(vec![RuntimeCapability::StreamLogs])?;
        lock(&setup.secrets.resolutions)?.push_back(Ok(resolution));
        let result = setup
            .service
            .logs(
                &setup.selection,
                log_request(LogMode::Redacted),
                OperationCancellation::default(),
            )
            .await;
        if setup.operations.log_calls()?.is_empty() {
            assert!(result.is_err());
        } else {
            assert!(result.is_ok());
        }
    }
    Ok(())
}

#[tokio::test]
async fn exec_resolves_secret_environment_and_audits_each_terminal_transition()
-> Result<(), Box<dyn std::error::Error>> {
    let setup = setup(vec![RuntimeCapability::Exec])?;
    lock(&setup.secrets.resolutions)?.push_back(Ok(secret(SecretStatus::Set, Some(SENTINEL))?));
    setup.operations.queue_exec(vec![
        Ok(ExecEvent::Selected {
            runtime: RuntimeKind::DockerCompose,
            identity: identity(),
            container_id: "container-1".to_owned(),
        }),
        Ok(ExecEvent::Output {
            source: ExecOutputSource::Stdout,
            value: "public".to_owned(),
            lossy_utf8: false,
        }),
        Ok(finished()),
    ])?;
    let cancellation = OperationCancellation::default();
    let mut stream = setup
        .service
        .exec(&setup.selection, exec_request()?, cancellation.clone())
        .await?;
    let mut events = Vec::new();
    while let Some(event) = stream.recv().await {
        events.push(event?);
    }
    assert_eq!(events.len(), 3);
    assert!(cancellation.is_cancelled());
    let calls = setup.operations.exec_calls()?;
    assert_eq!(calls[0].target, "app");
    assert_eq!(calls[0].request.selector.module.as_deref(), Some("api"));
    assert!(calls[0].environment_debug.contains("TOKEN"));
    assert!(!calls[0].environment_debug.contains(SENTINEL));
    let records = lock(&setup.audit.records)?;
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].executable, "printf");
    assert_eq!(records[1].container_id.as_deref(), Some("container-1"));
    assert_eq!(records[2].exit_code, Some(7));
    assert_eq!(records[2].finished_at_unix_ms, Some(NOW + 10));
    assert_eq!(
        lock(&setup.authorizer.requests)?[0].operation,
        RuntimeOperation::Exec
    );
    Ok(())
}

#[tokio::test]
async fn exec_validation_authorization_capability_and_resolution_fail_before_driver_effects()
-> Result<(), Box<dyn std::error::Error>> {
    let invalid = setup(vec![RuntimeCapability::Exec])?;
    let mut request = exec_request()?;
    request.argv.clear();
    assert!(
        invalid
            .service
            .exec(
                &invalid.selection,
                request,
                OperationCancellation::default()
            )
            .await
            .is_err()
    );
    assert!(lock(&invalid.authorizer.requests)?.is_empty());

    let denied = setup(vec![RuntimeCapability::Exec])?;
    denied.authorizer.denied.store(true, Ordering::SeqCst);
    assert!(
        denied
            .service
            .exec(
                &denied.selection,
                exec_request()?,
                OperationCancellation::default(),
            )
            .await
            .is_err()
    );
    assert!(denied.operations.exec_calls()?.is_empty());

    let unsupported = setup(Vec::new())?;
    assert!(
        unsupported
            .service
            .exec(
                &unsupported.selection,
                exec_request()?,
                OperationCancellation::default(),
            )
            .await
            .is_err()
    );
    assert!(unsupported.operations.exec_calls()?.is_empty());

    let unavailable = setup(vec![RuntimeCapability::Exec])?;
    lock(&unavailable.secrets.resolutions)?.push_back(Ok(secret(SecretStatus::Unset, None)?));
    assert!(
        unavailable
            .service
            .exec(
                &unavailable.selection,
                exec_request()?,
                OperationCancellation::default(),
            )
            .await
            .is_err()
    );
    assert!(unavailable.operations.exec_calls()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn status_and_explicit_module_failures_stop_at_their_exact_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let invalid_environment = setup(vec![RuntimeCapability::StreamLogs])?;
    let config_path = invalid_environment.selection.start.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        config_path,
        config.replace("default-environment: local\n", ""),
    )?;
    let selection = WorkspaceSelection::new(&invalid_environment.selection.start, None);
    assert!(
        invalid_environment
            .service
            .status(&selection, &approval(false))
            .await
            .is_err()
    );
    assert!(lock(&invalid_environment.authorizer.requests)?.is_empty());

    let denied = setup(vec![RuntimeCapability::StreamLogs])?;
    denied.authorizer.denied.store(true, Ordering::SeqCst);
    assert!(
        denied
            .service
            .status(&denied.selection, &approval(false))
            .await
            .is_err()
    );
    assert_eq!(
        lock(&denied.authorizer.requests)?[0].operation,
        RuntimeOperation::Status
    );

    let module_mismatch = setup(vec![RuntimeCapability::StreamLogs, RuntimeCapability::Exec])?;
    let mut logs = log_request(LogMode::Raw);
    if let Some(selector) = &mut logs.selector {
        selector.module = Some("missing".to_owned());
    }
    assert!(
        module_mismatch
            .service
            .logs(
                &module_mismatch.selection,
                logs,
                OperationCancellation::default(),
            )
            .await
            .is_err()
    );
    let mut exec = exec_request()?;
    exec.selector.module = Some("missing".to_owned());
    assert!(
        module_mismatch
            .service
            .exec(
                &module_mismatch.selection,
                exec,
                OperationCancellation::default(),
            )
            .await
            .is_err()
    );
    assert!(module_mismatch.operations.log_calls()?.is_empty());
    assert!(module_mismatch.operations.exec_calls()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn immediate_driver_and_initial_audit_failures_are_terminal_and_ordered()
-> Result<(), Box<dyn std::error::Error>> {
    let driver_failure = setup(vec![RuntimeCapability::Exec])?;
    lock(&driver_failure.secrets.resolutions)?
        .push_back(Ok(secret(SecretStatus::Set, Some(SENTINEL))?));
    *lock(&driver_failure.operations.exec_error)? = Some(MmError::RuntimeResponse {
        message: "injected driver failure".to_owned(),
    });
    let mut request = exec_request()?;
    request.shell_wrapper = true;
    request.approval.approved_high_risk = true;
    let result = driver_failure
        .service
        .exec(
            &driver_failure.selection,
            request,
            OperationCancellation::default(),
        )
        .await;
    let error = match result {
        Err(error) => error,
        Ok(_) => return Err("driver failure unexpectedly returned a stream".into()),
    };
    assert_eq!(error.code(), "MM_RUNTIME_RESPONSE");
    {
        let records = lock(&driver_failure.audit.records)?;
        assert_eq!(records.len(), 2);
        assert_eq!(
            records[1].error_code.as_deref(),
            Some("MM_RUNTIME_RESPONSE")
        );
    }
    assert_eq!(
        lock(&driver_failure.authorizer.requests)?[0].operation,
        RuntimeOperation::Shell
    );

    let audit_failure = setup(vec![RuntimeCapability::Exec])?;
    lock(&audit_failure.secrets.resolutions)?
        .push_back(Ok(secret(SecretStatus::Set, Some(SENTINEL))?));
    audit_failure.audit.fail_at.store(1, Ordering::SeqCst);
    assert!(
        audit_failure
            .service
            .exec(
                &audit_failure.selection,
                exec_request()?,
                OperationCancellation::default(),
            )
            .await
            .is_err()
    );
    assert!(audit_failure.operations.exec_calls()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn audited_stream_propagates_upstream_and_selected_audit_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let upstream = setup(vec![RuntimeCapability::Exec])?;
    lock(&upstream.secrets.resolutions)?.push_back(Ok(secret(SecretStatus::Set, Some(SENTINEL))?));
    upstream
        .operations
        .queue_exec(vec![Err(MmError::RuntimeResponse {
            message: "stream failed".to_owned(),
        })])?;
    let mut stream = upstream
        .service
        .exec(
            &upstream.selection,
            exec_request()?,
            OperationCancellation::default(),
        )
        .await?;
    let event = stream.recv().await.ok_or("missing upstream event")?;
    let error = event.err().ok_or("upstream event was not an error")?;
    assert_eq!(error.code(), "MM_RUNTIME_RESPONSE");
    assert!(stream.recv().await.is_none());
    assert_eq!(
        lock(&upstream.audit.records)?[1].error_code.as_deref(),
        Some("MM_RUNTIME_RESPONSE")
    );

    let selected_audit = setup(vec![RuntimeCapability::Exec])?;
    lock(&selected_audit.secrets.resolutions)?
        .push_back(Ok(secret(SecretStatus::Set, Some(SENTINEL))?));
    selected_audit.audit.fail_at.store(2, Ordering::SeqCst);
    selected_audit
        .operations
        .queue_exec(vec![Ok(ExecEvent::Selected {
            runtime: RuntimeKind::DockerCompose,
            identity: identity(),
            container_id: "container-2".to_owned(),
        })])?;
    let cancellation = OperationCancellation::default();
    let mut stream = selected_audit
        .service
        .exec(
            &selected_audit.selection,
            exec_request()?,
            cancellation.clone(),
        )
        .await?;
    let event = stream.recv().await.ok_or("missing audit event")?;
    let error = event.err().ok_or("audit event was not an error")?;
    assert_eq!(error.code(), "MM_STATE");
    assert!(stream.recv().await.is_none());
    assert!(cancellation.is_cancelled());
    Ok(())
}

#[tokio::test]
async fn audited_stream_records_cancellation_and_propagates_its_audit_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let cancelled = setup(vec![RuntimeCapability::Exec])?;
    lock(&cancelled.secrets.resolutions)?.push_back(Ok(secret(SecretStatus::Set, Some(SENTINEL))?));
    cancelled.operations.queue_exec(vec![Ok(ExecEvent::Output {
        source: ExecOutputSource::Stdout,
        value: "partial".to_owned(),
        lossy_utf8: false,
    })])?;
    let mut stream = cancelled
        .service
        .exec(
            &cancelled.selection,
            exec_request()?,
            OperationCancellation::default(),
        )
        .await?;
    assert!(stream.recv().await.is_some_and(|event| event.is_ok()));
    assert!(stream.recv().await.is_none());
    assert_eq!(
        lock(&cancelled.audit.records)?[1].error_code.as_deref(),
        Some("MM_CANCELLED")
    );

    let cancellation_audit = setup(vec![RuntimeCapability::Exec])?;
    lock(&cancellation_audit.secrets.resolutions)?
        .push_back(Ok(secret(SecretStatus::Set, Some(SENTINEL))?));
    cancellation_audit.audit.fail_at.store(2, Ordering::SeqCst);
    cancellation_audit.operations.queue_exec(Vec::new())?;
    let mut stream = cancellation_audit
        .service
        .exec(
            &cancellation_audit.selection,
            exec_request()?,
            OperationCancellation::default(),
        )
        .await?;
    let event = stream.recv().await.ok_or("missing cancellation audit")?;
    let error = event.err().ok_or("cancellation audit was not an error")?;
    assert_eq!(error.code(), "MM_STATE");
    assert!(stream.recv().await.is_none());
    Ok(())
}

async fn wait_for_audit_calls(audit: &FakeAudit, expected: usize) {
    for _ in 0..100 {
        if audit.calls.load(Ordering::SeqCst) >= expected {
            return;
        }
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn closed_exec_receiver_cancels_upstream_for_each_audit_outcome()
-> Result<(), Box<dyn std::error::Error>> {
    for (events, fail_audit) in [
        (
            vec![Ok(ExecEvent::Selected {
                runtime: RuntimeKind::DockerCompose,
                identity: identity(),
                container_id: "container-3".to_owned(),
            })],
            true,
        ),
        (
            vec![Ok(ExecEvent::Output {
                source: ExecOutputSource::Stdout,
                value: "unread".to_owned(),
                lossy_utf8: false,
            })],
            false,
        ),
        (Vec::new(), true),
    ] {
        let setup = setup(vec![RuntimeCapability::Exec])?;
        lock(&setup.secrets.resolutions)?.push_back(Ok(secret(SecretStatus::Set, Some(SENTINEL))?));
        if fail_audit {
            setup.audit.fail_at.store(2, Ordering::SeqCst);
        }
        let gate = setup.operations.gate_exec()?;
        setup.operations.queue_exec(events)?;
        let cancellation = OperationCancellation::default();
        let stream = setup
            .service
            .exec(&setup.selection, exec_request()?, cancellation.clone())
            .await?;
        drop(stream);
        gate.notify_one();
        wait_for_audit_calls(&setup.audit, 2).await;
        assert!(setup.audit.calls.load(Ordering::SeqCst) >= 2);
        assert!(cancellation.is_cancelled());
    }
    Ok(())
}
