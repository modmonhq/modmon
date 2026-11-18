use super::*;
use crate::app::ports::CommandOutput;
use crate::domain::runtime::{RuntimeEnvironment, RuntimeTarget, ServiceState};
use crate::domain::workspace::ProjectId;

fn output(stdout: &str) -> CommandOutput {
    CommandOutput {
        code: Some(0),
        stdout: stdout.to_owned().into(),
        stderr: String::new().into(),
        truncated: false,
    }
}

fn target() -> RuntimeTarget {
    RuntimeTarget {
        root: "/tmp".into(),
        project_directory: "/tmp".into(),
        target: None,
        project_id: ProjectId(uuid::Uuid::from_u128(80)),
        environment: "local".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "demo".to_owned(),
        artifact_hash: "hash".to_owned(),
        compose_files: vec!["compose.yaml".into()],
        user_compose_file_count: 1,
        interpolation_environment: RuntimeEnvironment::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

#[test]
fn parses_compose_array_ndjson_and_rejects_malformed_records()
-> Result<(), crate::domain::error::MmError> {
    let array = output(
        r#"[{"ID":"abc","Name":"demo-web-1","Image":"image@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","State":"running","Status":"Up (healthy)","Labels":{"com.docker.compose.service":"web","com.docker.compose.project":"demo","private":"hidden"},"RestartCount":"2","ExitCode":0}]"#,
    );
    let containers = parse_compose_observation(&array, "Docker")?;
    assert_eq!(containers[0].health.as_deref(), Some("healthy"));
    assert!(!containers[0].labels.contains_key("private"));
    assert_eq!(containers[0].restart_count, Some(2));
    assert!(containers[0].digest.is_some());
    let sorted = parse_compose_observation(
        &output(
            r#"[{"ID":"2","Name":"z","Service":"z","State":"running","Error":""},{"ID":"1","Name":"a","Service":"a","State":"running","ExitCode":2147483648}]"#,
        ),
        "Docker",
    )?;
    assert_eq!(sorted[0].service, "a");
    assert_eq!(sorted[0].exit_code, None);

    let ndjson = output(
        "{\"ID\":\"def\",\"Names\":[\"demo-api-1\"],\"State\":\"exited\",\"Status\":\"Exited (1)\",\"Service\":\"api\"}\n",
    );
    assert_eq!(
        parse_compose_observation(&ndjson, "Docker")?[0].service,
        "api"
    );
    assert!(parse_compose_observation(&output(r#"[{"ID":"abc"}]"#), "Docker").is_err());
    assert!(parse_compose_observation(&output("not-json"), "Docker").is_err());
    let mut truncated = output("[]");
    truncated.truncated = true;
    assert!(parse_compose_observation(&truncated, "Docker").is_err());
    assert!(parse_compose_service_names("services: {}\n").is_err());
    assert!(compose_service_names("not: [valid").is_err());
    assert!(normalized_rendered_semantics("demo", "not: [valid").is_err());
    Ok(())
}

#[test]
fn rendered_semantics_normalize_services_networks_volumes_and_deploy_settings()
-> Result<(), crate::domain::error::MmError> {
    let rendered = r#"
services:
  web:
    image: nginx:latest
    environment: [A=1, B]
    networks:
      front:
        aliases: [www, web]
    healthcheck: {test: [CMD, true]}
    deploy:
      replicas: 2
      update_config: {order: start-first}
      restart_policy: {condition: on-failure}
      placement: {constraints: [node.role == worker]}
networks:
  front:
    name: shared-front
    external: true
volumes:
  data: {}
  cache:
    labels:
      io.mm.owner: mm
      unrelated: ignored
"#;
    let semantics = normalized_rendered_semantics("demo", rendered)?;
    let web = &semantics.services["web"];
    assert_eq!(web.environment_names, vec!["A", "B"]);
    assert_eq!(web.aliases["front"], vec!["web", "www"]);
    assert!(web.healthcheck);
    assert_eq!(web.replicas, Some(2));
    assert_eq!(web.update_order.as_deref(), Some("start-first"));
    assert_eq!(web.restart_condition.as_deref(), Some("on-failure"));
    assert_eq!(semantics.resources.len(), 3);
    assert_ne!(
        hash_rendered_config("demo", rendered),
        hash_rendered_config("other", rendered)
    );
    let sequence = normalized_rendered_semantics(
        "demo",
        "services:\n  api:\n    networks: [back, front]\n    deploy:\n      replicas: 4294967296\nnetworks:\n  back: {}\n  front: {}\n",
    )?;
    assert_eq!(sequence.services["api"].networks, ["back", "front"]);
    assert_eq!(sequence.services["api"].replicas, None);
    Ok(())
}

#[test]
fn observations_distinguish_running_missing_stopped_degraded_and_failed_services()
-> Result<(), crate::domain::error::MmError> {
    let containers = parse_compose_observation(
        &output(
            r#"[
{"ID":"1","Name":"demo-web-1","Service":"web","State":"running","Status":"Up (healthy)"},
{"ID":"4","Name":"demo-web-2","Service":"web","State":"running","Status":"Up (healthy)"},
{"ID":"2","Name":"demo-stopped-1","Service":"stopped","State":"exited","Status":"Exited (0)"},
{"ID":"3","Name":"demo-bad-1","Service":"bad","State":"running","Status":"Up (unhealthy)"}
]"#,
        ),
        "Docker",
    )?;
    let services = normalize_observation(
        &target(),
        &[
            "bad".to_owned(),
            "missing".to_owned(),
            "stopped".to_owned(),
            "web".to_owned(),
        ],
        &containers,
    );
    assert_eq!(
        services
            .iter()
            .find(|item| item.identity.service == "web")
            .map(|item| item.state),
        Some(ServiceState::Running)
    );
    assert_eq!(
        services
            .iter()
            .find(|item| item.identity.service == "missing")
            .map(|item| item.state),
        Some(ServiceState::Missing)
    );
    assert_eq!(
        services
            .iter()
            .find(|item| item.identity.service == "stopped")
            .map(|item| item.state),
        Some(ServiceState::Stopped)
    );
    // A service wants as many copies as it has containers; one when it has none.
    assert_eq!(
        services
            .iter()
            .map(|item| (
                item.identity.service.as_str(),
                item.desired_count,
                item.running_count
            ))
            .collect::<Vec<_>>(),
        [
            ("bad", 1, 1),
            ("missing", 1, 0),
            ("stopped", 1, 0),
            ("web", 2, 2)
        ]
    );
    assert_eq!(
        health_from_status("Up (starting)"),
        Some("starting".to_owned())
    );
    assert_eq!(health_from_status("plain"), None);
    Ok(())
}

#[test]
fn rendered_yaml_that_does_not_parse_names_its_line_with_values_masked() {
    let rendered = "configs:\n  [REDACTED]_init:\n    file: /srv/init.sql\nservices:\n  db:\n    environment:\n      PASSWORD: s3cret\n";
    let message = compose_service_names(rendered)
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(message.contains("not valid YAML"), "{message}");
    assert!(message.contains("values masked"), "{message}");
    assert!(!message.contains("s3cret"), "{message}");
    assert_eq!(
        mask_yaml_line("      PASSWORD: s3cret"),
        "      PASSWORD: …"
    );
    assert_eq!(mask_yaml_line("  postgres_init:"), "  postgres_init:");
    assert_eq!(mask_yaml_line("MIIEvQIBADANBgkqhkiG9w0BAQEFAASC"), "…");
}
