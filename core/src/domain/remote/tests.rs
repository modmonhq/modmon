use super::*;

fn scope() -> RemoteScope {
    RemoteScope {
        project_id: ProjectId(Uuid::from_u128(1)),
        environment: "production".to_owned(),
        target: "remote".to_owned(),
        target_fingerprint: "a".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: None,
    }
}

fn binding(runtime: RuntimeKind) -> RemotePlanBinding {
    RemotePlanBinding {
        protocol_version: PROTOCOL_VERSION,
        daemon_version: "1.2.3".to_owned(),
        daemon_fingerprint: "a".repeat(64),
        target_fingerprint: "b".repeat(64),
        target: "remote".to_owned(),
        node: None,
        kind: RemotePlanKind::Deployment,
        project_id: ProjectId(Uuid::from_u128(1)),
        environment: "production".to_owned(),
        runtime,
        commit: "e".repeat(40),
        deployment_plan_hash: "c".repeat(64),
        access_change_hash: "c".repeat(64),
        principal_id: PrincipalId(Uuid::from_u128(2)),
        authorization_revision: 1,
        authorization_hash: "d".repeat(64),
        plan_nonce: Uuid::from_u128(3),
        expires_at_unix_ms: 1_000_000,
        bundle_hash: None,
        plan_hash: String::new(),
    }
}

#[test]
fn scope_rejects_ambiguous_or_unbound_identity() {
    assert!(scope().validate().is_ok());
    let mutations: [fn(&mut RemoteScope); 5] = [
        |value| value.environment.clear(),
        |value| value.environment = "x".repeat(256),
        |value| value.environment = "bad\nvalue".to_owned(),
        |value| value.target = "Bad Target".to_owned(),
        |value| value.target_fingerprint = "A".repeat(64),
    ];
    for mutate in mutations {
        let mut candidate = scope();
        mutate(&mut candidate);
        assert!(candidate.validate().is_err());
    }
}

#[test]
fn plan_binding_hashes_every_runtime_and_rejects_stale_or_invalid_metadata() -> Result<(), String> {
    for runtime in RuntimeKind::ALL {
        let mut candidate = binding(runtime);
        candidate.plan_hash = candidate.canonical_hash()?;
        assert!(candidate.validate().is_ok());
        candidate.expires_at_unix_ms += 1;
        assert!(candidate.validate().is_err());
    }

    let mutations: [fn(&mut RemotePlanBinding); 12] = [
        |value| value.protocol_version += 1,
        |value| value.daemon_version.clear(),
        |value| value.daemon_fingerprint = "A".repeat(64),
        |value| value.target_fingerprint.clear(),
        |value| value.deployment_plan_hash = "0".repeat(63),
        |value| value.authorization_hash.push('g'),
        |value| value.authorization_revision = 0,
        |value| value.environment.clear(),
        |value| value.plan_hash = "f".repeat(64),
        |value| value.commit = "main".to_owned(),
        |value| value.commit.clear(),
        |value| value.access_change_hash.clear(),
    ];
    for mutate in mutations {
        let mut candidate = binding(RuntimeKind::DockerCompose);
        mutate(&mut candidate);
        assert!(candidate.validate().is_err());
    }
    Ok(())
}

#[test]
fn protocol_method_and_frame_discriminants_are_stable() {
    let methods = [
        RemoteMethod::Status,
        RemoteMethod::Exec(ExecRequest {
            selector: crate::domain::observability::ServiceSelector {
                stack: None,
                module: None,
                service: "web".to_owned(),
                container: None,
            },
            argv: vec!["true".to_owned()],
            user: None,
            workdir: None,
            environment: Vec::new(),
            tty: crate::domain::observability::ExecTtyMode::Disabled,
            stdin: crate::domain::observability::ExecStdinMode::Closed,
            timeout: std::time::Duration::from_secs(1),
            approval: crate::domain::observability::ApprovalContext {
                principal: "operator".to_owned(),
                source: "test".to_owned(),
                approved_high_risk: true,
            },
            shell_wrapper: false,
        }),
        RemoteMethod::PlanDeployment {
            commit: "a".repeat(40),
            receipt: None,
            release: Default::default(),
        },
        RemoteMethod::ApplyDeployment {
            remote_plan_hash: "a".repeat(64),
        },
    ];
    assert_eq!(
        methods.each_ref().map(|method| method.name()),
        ["status", "exec", "plan_deployment", "apply_deployment"]
    );
    assert!(!methods[0].is_mutation());
    assert!(methods[1].is_mutation());
    assert!(!methods[2].is_mutation());
    assert!(methods[3].is_mutation());

    let request_id = Uuid::from_u128(4);
    let client_frames = [
        ClientFrame::RelaySession {
            version: 1,
            key_fingerprint: "SHA256:key".to_owned(),
            source: "ssh".to_owned(),
            session_id: request_id,
        },
        ClientFrame::Hello {
            version: 2,
            hello: ClientHello {
                client_nonce: request_id,
                expected_daemon_fingerprint: "a".repeat(64),
            },
        },
        ClientFrame::Request {
            version: 3,
            request_id,
            operation_nonce: Uuid::from_u128(5),
            deadline_unix_ms: 10,
            scope: scope(),
            method: Box::new(RemoteMethod::Status),
        },
        ClientFrame::Cancel {
            version: 4,
            request_id,
        },
    ];
    assert_eq!(
        client_frames.each_ref().map(|frame| frame.version()),
        [1, 2, 3, 4]
    );

    let error = RemoteError {
        code: "failed".to_owned(),
        message: "failure".to_owned(),
        retryable: false,
        mutation_state: MutationState::NotStarted,
    };
    let server_frames = [
        ServerFrame::Hello {
            version: 1,
            hello: ServerHello {
                client_nonce: request_id,
                server_nonce: Uuid::from_u128(6),
                daemon_version: "1".to_owned(),
                daemon_fingerprint: "a".repeat(64),
                instance_id: Uuid::from_u128(7),
            },
        },
        ServerFrame::Result {
            version: 2,
            request_id,
            result: Box::new(RemoteResult::StreamStarted),
        },
        ServerFrame::Stream {
            version: 3,
            request_id,
            sequence: 0,
            item: Box::new(RemoteStreamItem::Exec(ExecEvent::Output {
                source: crate::domain::observability::ExecOutputSource::Stdout,
                value: "ok".to_owned(),
                lossy_utf8: false,
            })),
        },
        ServerFrame::End {
            version: 4,
            request_id,
            frames: 1,
            bytes: 2,
        },
        ServerFrame::Error {
            version: 5,
            request_id: Some(request_id),
            error,
        },
    ];
    assert_eq!(
        server_frames.each_ref().map(|frame| frame.version()),
        [1, 2, 3, 4, 5]
    );
}
