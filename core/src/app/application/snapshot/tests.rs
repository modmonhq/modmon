use super::*;
use crate::domain::runtime::RuntimeTarget;

fn container(state: &str, health: Option<&str>) -> ContainerObservation {
    ContainerObservation {
        id: state.to_owned(),
        name: state.to_owned(),
        service: "web".to_owned(),
        image: "example/web".to_owned(),
        state: state.to_owned(),
        status: state.to_owned(),
        health: health.map(str::to_owned),
        restart_count: None,
        exit_code: None,
        created_at: None,
        started_at: None,
        finished_at: None,
        revision: None,
        digest: None,
        diagnostic_code: None,
        error: None,
        labels: BTreeMap::new(),
        node: None,
        history: false,
    }
}

#[test]
fn service_classification_compose_boundaries_and_capability_diagnostics_are_exact()
-> Result<(), MmError> {
    assert_eq!(
        classify_service(&[container("running", None), container("exited", None)]),
        ServiceState::Degraded
    );
    assert_eq!(
        classify_service(&[container("running", Some("unhealthy"))]),
        ServiceState::Degraded
    );
    assert_eq!(
        classify_service(&[
            container("stopped", None),
            container("exited", None),
            container("created", None),
        ]),
        ServiceState::Stopped
    );
    assert_eq!(
        classify_service(&[container("paused", None)]),
        ServiceState::Unknown
    );
    assert_eq!(
        classify_service(&[container("paused", Some("unhealthy"))]),
        ServiceState::Degraded
    );

    let mut target = RuntimeTarget {
        root: PathBuf::from("/workspace"),
        project_directory: PathBuf::from("/workspace"),
        target: None,
        project_id: crate::domain::workspace::ProjectId(uuid::Uuid::from_u128(1)),
        environment: "staging".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "example".to_owned(),
        artifact_hash: "a".repeat(64),
        compose_files: vec![PathBuf::from("compose.yaml"), PathBuf::from("overlay.yaml")],
        user_compose_file_count: 0,
        interpolation_environment: crate::domain::runtime::RuntimeEnvironment::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    };
    assert!(user_runtime_target(&target).is_err());
    target.user_compose_file_count = 3;
    assert!(user_runtime_target(&target).is_err());
    target.user_compose_file_count = 1;
    assert_eq!(
        user_runtime_target(&target)?.compose_files,
        [PathBuf::from("compose.yaml")]
    );

    let mut probe = RuntimeProbe {
        kind: crate::domain::runtime::RuntimeKind::DockerCompose,
        available: true,
        engine_version: Some("1".to_owned()),
        compose_provider: None,
        rootless: None,
        socket_path: None,
        capabilities: vec![RuntimeCapability::RenderCompose],
        diagnostics: Vec::new(),
    };
    assert!(!require_capabilities(
        &mut probe,
        &[
            RuntimeCapability::RenderCompose,
            RuntimeCapability::ApplyCompose
        ]
    ));
    assert_eq!(probe.diagnostics.len(), 1);
    assert_eq!(probe.diagnostics[0].code, "MM_RUNTIME_CAPABILITY_MISSING");
    assert!(require_capabilities(
        &mut probe,
        &[RuntimeCapability::RenderCompose]
    ));
    Ok(())
}
