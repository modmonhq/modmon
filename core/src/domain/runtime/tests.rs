use super::{
    ConvergenceCancellation, ConvergencePolicy, DOCKER_INHERITED_ENVIRONMENT,
    PODMAN_INHERITED_ENVIRONMENT, RuntimeCapability, RuntimeEnvironment, RuntimeKind, RuntimeProbe,
    evidence, reserved_runtime_environment_names, runtime_descriptor,
};
use crate::domain::diagnostic::Diagnostic;

#[test]
fn driver_allowlists_do_not_import_unrelated_variables() {
    assert!(PODMAN_INHERITED_ENVIRONMENT.contains(&"CONTAINERS_CONF"));
    assert!(PODMAN_INHERITED_ENVIRONMENT.contains(&"PODMAN_COMPOSE_PROVIDER"));
    assert!(!PODMAN_INHERITED_ENVIRONMENT.contains(&"DOCKER_HOST"));
    assert!(!PODMAN_INHERITED_ENVIRONMENT.contains(&"DOCKER_CONTEXT"));
    assert!(!PODMAN_INHERITED_ENVIRONMENT.contains(&"DOCKER_TLS_VERIFY"));
    assert!(DOCKER_INHERITED_ENVIRONMENT.contains(&"DOCKER_HOST"));
    assert!(DOCKER_INHERITED_ENVIRONMENT.contains(&"DOCKER_TLS_VERIFY"));
    assert!(!DOCKER_INHERITED_ENVIRONMENT.contains(&"CONTAINER_HOST"));
    assert!(!DOCKER_INHERITED_ENVIRONMENT.contains(&"PODMAN_COMPOSE_PROVIDER"));
    assert!(!DOCKER_INHERITED_ENVIRONMENT.contains(&"CONTAINERS_CONF"));
}

#[test]
fn unavailable_probe_cannot_satisfy_an_operation() {
    let probe = RuntimeProbe::unavailable(
        RuntimeKind::DockerCompose,
        Diagnostic::error("MM_DOCKER_UNAVAILABLE", "missing"),
    );
    assert!(matches!(
        probe.require_capabilities("observe", &[RuntimeCapability::ObserveServices], None),
        Err(error) if error.code() == "MM_DOCKER_UNAVAILABLE"
    ));
    assert!(matches!(
        probe.require_capabilities("observe", &[], None),
        Err(error) if error.code() == "MM_DOCKER_UNAVAILABLE"
    ));
}

/// An engine that did not answer in time lacks no capability: the
/// operation fails with a timeout that names the engine and the host's
/// check, not the capability it asked for.
#[test]
fn a_late_engine_is_a_timeout_not_a_missing_capability() {
    let probe = RuntimeProbe::unavailable(
        RuntimeKind::DockerCompose,
        Diagnostic::error(
            super::PROBE_TIMEOUT_CODE,
            "docker timed out after 30 seconds",
        ),
    );
    let error = probe
        .require_capabilities(
            "direct_environment_delivery",
            &[RuntimeCapability::ObserveServices],
            Some("local"),
        )
        .err();
    let error = error
        .as_ref()
        .map(|error| (error.code(), error.to_string()));
    assert_eq!(
        error,
        Some((
            "MM_TIMEOUT",
            "the Docker engine of target 'local' did not answer in time, and nothing was changed: docker timed out after 30 seconds. Check the engine on the host (`systemctl status docker`) and run the command again".to_owned()
        ))
    );
}

#[test]
fn runtime_kinds_and_capabilities_publish_complete_stable_behavior_descriptors() {
    let direct_evidence = evidence(RuntimeCapability::Exec, "fixture", "live-job");
    assert_eq!(direct_evidence.capability, RuntimeCapability::Exec);
    assert_eq!(direct_evidence.fixture, "fixture");
    assert_eq!(direct_evidence.live_job, "live-job");

    assert_eq!(
        RuntimeKind::PodmanCompose.inherited_environment(),
        PODMAN_INHERITED_ENVIRONMENT
    );
    assert_eq!(
        RuntimeKind::DockerCompose.inherited_environment(),
        DOCKER_INHERITED_ENVIRONMENT
    );
    assert_eq!(
        RuntimeKind::PodmanCompose.apply_capability(),
        RuntimeCapability::ApplyCompose
    );
    assert_eq!(
        RuntimeKind::DockerCompose.apply_capability(),
        RuntimeCapability::ApplyCompose
    );

    let capabilities = [
        RuntimeCapability::RenderCompose,
        RuntimeCapability::ApplyCompose,
        RuntimeCapability::ObserveServices,
        RuntimeCapability::ContainerHealth,
        RuntimeCapability::StreamLogs,
        RuntimeCapability::Exec,
        RuntimeCapability::ResolveImageDigest,
        RuntimeCapability::Rollback,
        RuntimeCapability::ManageNetworks,
        RuntimeCapability::ManageVolumes,
        RuntimeCapability::MutateVolumeFiles,
        RuntimeCapability::RemoteTransport,
        RuntimeCapability::DirectEnvironment,
        RuntimeCapability::BindMounts,
        RuntimeCapability::RuntimeSecrets,
        RuntimeCapability::OneShotJobs,
    ];
    let names = capabilities.map(RuntimeCapability::as_str);
    assert_eq!(
        names
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        capabilities.len()
    );
    assert!(
        names
            .iter()
            .all(|name| !name.is_empty() && !name.contains(' '))
    );
    assert!(
        capabilities
            .iter()
            .all(|capability| !capability.behavior().is_empty())
    );

    for kind in RuntimeKind::ALL {
        let descriptor = runtime_descriptor(kind);
        assert_eq!(descriptor.kind, kind);
        assert!(!descriptor.capabilities.is_empty());
        assert!(descriptor.capabilities.iter().all(|evidence| {
            !evidence.fixture.is_empty()
                && !evidence.live_job.is_empty()
                && capabilities.contains(&evidence.capability)
        }));
    }
}

#[test]
fn available_probe_reports_only_the_exact_missing_capability()
-> Result<(), Box<dyn std::error::Error>> {
    let probe = RuntimeProbe {
        kind: RuntimeKind::DockerCompose,
        available: true,
        engine_version: Some("27.0".to_owned()),
        compose_provider: None,
        rootless: Some(false),
        socket_path: None,
        capabilities: vec![RuntimeCapability::StreamLogs],
        diagnostics: Vec::new(),
    };
    assert!(
        probe
            .require_capabilities("logs", &[RuntimeCapability::StreamLogs], Some("local"))
            .is_ok()
    );
    let error = probe
        .require_capabilities(
            "observe",
            &[
                RuntimeCapability::StreamLogs,
                RuntimeCapability::ObserveServices,
            ],
            Some("local"),
        )
        .err()
        .ok_or("missing capability was accepted")?;
    assert_eq!(error.code(), "MM_RUNTIME_UNSUPPORTED");
    assert!(error.to_string().contains("observe"));
    Ok(())
}

#[test]
fn sensitive_environment_and_convergence_controls_are_bounded_and_redacted() {
    let mut environment = RuntimeEnvironment::default();
    assert!(environment.is_empty());
    assert_eq!(
        environment.insert("TOKEN".to_owned(), "sentinel-one".to_owned()),
        None
    );
    assert_eq!(environment.len(), 1);
    let replaced = environment.insert("TOKEN".to_owned(), "sentinel-two".to_owned());
    assert_eq!(
        replaced.as_deref().map(String::as_str),
        Some("sentinel-one")
    );
    assert_eq!(
        environment.values().collect::<Vec<_>>(),
        vec![("TOKEN", "sentinel-two")]
    );
    let debug = format!("{environment:?}");
    assert!(debug.contains("TOKEN"));
    assert!(!debug.contains("sentinel"));

    let policy = ConvergencePolicy::default();
    assert!(policy.timeout >= policy.maximum_backoff);
    assert!(policy.maximum_backoff >= policy.initial_backoff);
    let cancellation = ConvergenceCancellation::default();
    assert!(!cancellation.is_cancelled());
    cancellation.cancel();
    assert!(cancellation.is_cancelled());

    let names = reserved_runtime_environment_names().collect::<std::collections::BTreeSet<_>>();
    assert!(names.contains("PATH"));
    assert!(names.contains("DOCKER_HOST"));
    assert!(names.contains("CONTAINER_HOST"));
}

#[test]
fn driver_names_parse_and_the_removed_driver_names_its_replacement()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        RuntimeKind::ALL,
        [RuntimeKind::DockerCompose, RuntimeKind::PodmanCompose]
    );
    for kind in RuntimeKind::ALL {
        assert_eq!(kind.name().parse::<RuntimeKind>()?, kind);
        assert_eq!(serde_yaml_ng::to_string(&kind)?.trim(), kind.name());
        assert_eq!(serde_yaml_ng::from_str::<RuntimeKind>(kind.name())?, kind);
        assert_eq!(
            serde_json::from_value::<RuntimeKind>(serde_json::json!(kind.name()))?,
            kind
        );
    }

    let removed = [
        "docker-swarm"
            .parse::<RuntimeKind>()
            .err()
            .unwrap_or_default(),
        serde_yaml_ng::from_str::<RuntimeKind>("docker-swarm")
            .map(drop)
            .map_err(|error| error.to_string())
            .err()
            .unwrap_or_default(),
        serde_yaml_ng::from_str::<crate::domain::config::RuntimeConfig>("driver: docker-swarm\n")
            .map(drop)
            .map_err(|error| error.to_string())
            .err()
            .unwrap_or_default(),
        serde_json::from_str::<RuntimeKind>("\"docker-swarm\"")
            .map(drop)
            .map_err(|error| error.to_string())
            .err()
            .unwrap_or_default(),
    ];
    for error in removed {
        assert!(
            error.contains("the docker-swarm runtime driver was removed"),
            "{error}"
        );
        assert!(
            error.contains("docker-compose") && error.contains("nodes"),
            "{error}"
        );
        assert!(!error.contains("unknown variant"), "{error}");
    }

    let unknown = "kubernetes"
        .parse::<RuntimeKind>()
        .err()
        .unwrap_or_default();
    assert!(
        unknown.contains("unknown runtime driver 'kubernetes'")
            && unknown.contains("docker-compose or podman-compose"),
        "{unknown}"
    );
    Ok(())
}

#[test]
fn an_engine_that_cannot_be_used_says_why_instead_of_a_missing_capability() {
    let probe = RuntimeProbe::unavailable(
        RuntimeKind::PodmanCompose,
        Diagnostic::error(
            "MM_PODMAN_UNAVAILABLE",
            "Podman returned exit code Some(125): Error: configure storage: read-only file system",
        )
        .with_help("check that Podman's API socket is running: `systemctl status podman.socket`"),
    );
    let error = probe
        .require_capabilities(
            "direct_environment_delivery",
            &[RuntimeCapability::DirectEnvironment],
            Some("app-1"),
        )
        .err();
    assert_eq!(
        error.as_ref().map(crate::domain::error::MmError::code),
        Some("MM_PODMAN_UNAVAILABLE")
    );
    let message = error.map(|error| error.to_string()).unwrap_or_default();
    assert!(
        message.contains("Podman engine of target 'app-1' cannot be used"),
        "{message}"
    );
    assert!(message.contains("read-only file system"), "{message}");
    assert!(
        message.contains("systemctl status podman.socket"),
        "{message}"
    );
}
