use super::*;
use crate::domain::runtime::RuntimeEnvironment;
use crate::domain::workspace::ProjectId;

fn target(compose_files: usize, user_compose_file_count: usize) -> RuntimeTarget {
    RuntimeTarget {
        root: "/tmp".into(),
        project_directory: "/tmp".into(),
        target: None,
        project_id: ProjectId(uuid::Uuid::from_u128(91)),
        environment: "dev".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "demo".to_owned(),
        artifact_hash: "hash".to_owned(),
        compose_files: (0..compose_files)
            .map(|index| format!("{index}.yaml").into())
            .collect(),
        user_compose_file_count,
        interpolation_environment: RuntimeEnvironment::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

fn rendered(hash: &str) -> RenderedProject {
    RenderedProject {
        stack: "app".to_owned(),
        project_name: "demo".to_owned(),
        service_names: vec!["web".to_owned()],
        services: BTreeMap::new(),
        resources: Vec::new(),
        config_hash: hash.to_owned(),
    }
}

fn runtime() -> FakeRuntime {
    FakeRuntime {
        probe: RuntimeProbe {
            kind: RuntimeKind::DockerCompose,
            available: true,
            engine_version: Some("1".to_owned()),
            compose_provider: None,
            rootless: None,
            socket_path: None,
            capabilities: Vec::new(),
            diagnostics: Vec::new(),
        },
        user_rendered: Some(BTreeMap::from([("app".to_owned(), rendered("user"))])),
        rendered: BTreeMap::from([("app".to_owned(), rendered("full"))]),
        observed: BTreeMap::from([(
            "app".to_owned(),
            ObservedProject {
                stack: "app".to_owned(),
                ..ObservedProject::default()
            },
        )]),
    }
}

#[tokio::test]
async fn selects_user_or_full_render_and_reports_missing_stacks()
-> Result<(), crate::domain::error::MmError> {
    let runtime = runtime();
    assert_eq!(runtime.kind(), RuntimeKind::DockerCompose);
    assert_eq!(runtime.probe().await.kind, RuntimeKind::DockerCompose);
    assert_eq!(runtime.render(&target(1, 1)).await?.config_hash, "user");
    assert_eq!(runtime.render(&target(2, 1)).await?.config_hash, "full");

    let mut fallback = runtime.clone();
    fallback.user_rendered = None;
    assert_eq!(fallback.render(&target(1, 1)).await?.config_hash, "full");
    let mut missing = target(1, 1);
    missing.stack_name = "missing".to_owned();
    assert!(runtime.render(&missing).await.is_err());
    assert!(runtime.observe(&missing).await.is_err());
    Ok(())
}

#[tokio::test]
async fn observes_and_returns_cancelled_or_succeeded_convergence()
-> Result<(), crate::domain::error::MmError> {
    let runtime = runtime();
    assert_eq!(runtime.observe(&target(1, 1)).await?.stack, "app");

    let cancellation = ConvergenceCancellation::default();
    cancellation.cancel();
    let cancelled = runtime
        .wait_for_convergence(&target(1, 1), ConvergencePolicy::default(), cancellation)
        .await?;
    assert_eq!(cancelled.state, ConvergenceState::Cancelled);
    assert_eq!(cancelled.polls, 0);

    let succeeded = runtime
        .wait_for_convergence(
            &target(1, 1),
            ConvergencePolicy::default(),
            ConvergenceCancellation::default(),
        )
        .await?;
    assert_eq!(succeeded.state, ConvergenceState::Succeeded);
    assert_eq!(succeeded.polls, 1);
    Ok(())
}
