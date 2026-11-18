mod helper_tests {
use std::collections::BTreeMap;

use serde_json::json;
use zeroize::Zeroizing;

use super::super::{
    PodmanResourceKind, apply_health, first_nonempty_line, parse_owned_resource,
    parse_provider_executable, path_argument, provider_name, require_local_named_target,
    unix_mode, validate_mountpoint, validate_resource_label, validate_resource_name,
    validate_volume_file_name, verified_local_source,
};
use crate::app::ports::CommandOutput;
use crate::domain::ownership::{ResourceKind, ResourceOwnership};
use crate::domain::runtime::{ContainerObservation, RuntimeKind};
use crate::domain::target::{
    NamedTarget, TargetBootstrap, TargetDefinition, TargetId, TargetTransport,
};

fn output(stdout: &str) -> CommandOutput {
    CommandOutput {
        code: Some(0),
        stdout: Zeroizing::new(stdout.to_owned()),
        stderr: Zeroizing::new(String::new()),
        truncated: false,
    }
}

#[test]
fn owned_resource_inspection_accepts_exact_labels_and_rejects_ambiguity()
-> Result<(), Box<dyn std::error::Error>> {
    let target_id = TargetId(uuid::Uuid::from_u128(71));
    for (kind, name, resource) in [
        (PodmanResourceKind::Network, "demo-net", "frontend"),
        (PodmanResourceKind::Volume, "demo-data", "data"),
    ] {
        let labels = ResourceOwnership {
            project_id: None,
            environment: None,
            target_id,
            kind: match kind {
                PodmanResourceKind::Network => ResourceKind::Network,
                PodmanResourceKind::Volume => ResourceKind::Volume,
            },
            logical_name: name.to_owned(),
            resource: resource.to_owned(),
            generation: 1,
        }
        .labels();
        let row = json!([{"Name": name, "Labels": labels}]);
        assert_eq!(
            parse_owned_resource(output(&row.to_string()), kind, name, target_id, resource)?["Name"],
            name
        );
    }

    let mut truncated = output("[]");
    truncated.truncated = true;
    assert!(parse_owned_resource(truncated, PodmanResourceKind::Network, "name", target_id, "resource").is_err());
    for invalid in ["not-json", "[]", "[{},{}]", "[{}]", r#"[{"Name":"wrong"}]"#] {
        assert!(parse_owned_resource(output(invalid), PodmanResourceKind::Network, "name", target_id, "resource").is_err());
    }
    let no_labels = json!([{"Name": "name"}]);
    assert!(parse_owned_resource(output(&no_labels.to_string()), PodmanResourceKind::Network, "name", target_id, "resource").is_err());
    Ok(())
}

#[test]
fn resource_volume_mode_and_mountpoint_validation_is_strict()
-> Result<(), Box<dyn std::error::Error>> {
    for name in ["resource", "r_1.2-name"] {
        validate_resource_name(name)?;
    }
    for name in ["", "-leading", "bad/name", "bad name", &"x".repeat(256)] {
        assert!(validate_resource_name(name).is_err());
    }
    validate_resource_label("module.data")?;
    for label in ["", "bad\nlabel", &"x".repeat(256)] {
        assert!(validate_resource_label(label).is_err());
    }
    assert_eq!(validate_volume_file_name("config.json")?, std::path::PathBuf::from("config.json"));
    for name in ["", ".", "..", "../escape", "nested/file"] {
        assert!(validate_volume_file_name(name).is_err());
    }
    assert_eq!(unix_mode(0o640)?, "0640");
    assert!(unix_mode(0o1000).is_err());
    assert_eq!(validate_mountpoint("/var/lib/data", "data")?, std::path::PathBuf::from("/var/lib/data"));
    for mountpoint in ["", "/", "relative", "/bad/../path", "/bad\npath"] {
        assert!(validate_mountpoint(mountpoint, "data").is_err());
    }
    Ok(())
}

#[test]
fn local_source_target_path_health_and_provider_helpers_cover_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let source = temporary.path().join("source");
    std::fs::write(&source, b"data")?;
    assert_eq!(verified_local_source(&source)?, std::fs::canonicalize(&source)?);
    assert!(verified_local_source(temporary.path()).is_err());
    assert!(verified_local_source(&temporary.path().join("missing")).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&source, temporary.path().join("link"))?;
        assert!(verified_local_source(&temporary.path().join("link")).is_err());
    }
    assert_eq!(path_argument(&source, "source")?, source.to_string_lossy());

    let local = NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(uuid::Uuid::from_u128(72))),
    );
    assert_eq!(require_local_named_target(&local)?, local.definition.id);
    let remote = NamedTarget::new(
        "remote".to_owned(),
        TargetDefinition {
            id: TargetId(uuid::Uuid::from_u128(73)),
            transport: TargetTransport::Ssh {
                host: "host".to_owned(),
                port: 22,
                user: "user".to_owned(),
                host_key_fingerprint: "SHA256:test".to_owned(),
                host_key: None,
                identity_file: None,
                remote_root: Some("/srv/project".into()),
                daemon_fingerprint: None,
            },
            bootstrap: TargetBootstrap::Ansible,
            ingress: None,
            default_runtime: Some(RuntimeKind::PodmanCompose),
            available_runtimes: vec![RuntimeKind::PodmanCompose],
            runtime: None,
            nodes: Vec::new(),
        },
    );
    assert!(require_local_named_target(&remote).is_err());

    let mut containers = vec![ContainerObservation {
        id: "abcdef".to_owned(),
        name: "web".to_owned(),
        service: "web".to_owned(),
        image: "image".to_owned(),
        state: "running".to_owned(),
        status: "up".to_owned(),
        health: Some("starting".to_owned()),
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
    }];
    apply_health(&mut containers, vec![("abc".to_owned(), Some("healthy".to_owned()))]);
    assert_eq!(containers[0].health.as_deref(), Some("healthy"));
    apply_health(&mut containers, vec![("abc".to_owned(), Some("unhealthy".to_owned()))]);
    assert_eq!(containers[0].diagnostic_code.as_deref(), Some("MM_RUNTIME_UNHEALTHY"));
    apply_health(&mut containers, vec![("abc".to_owned(), Some("healthy".to_owned()))]);
    assert!(containers[0].diagnostic_code.is_none());
    apply_health(&mut containers, vec![("missing".to_owned(), None)]);
    assert_eq!(first_nonempty_line("\n first \nsecond"), Some("first".to_owned()));
    assert_eq!(first_nonempty_line("\n "), None);
    assert_eq!(provider_name("podman-compose 1.0"), "podman-compose");
    assert_eq!(provider_name("unknown"), "compose-provider");
    assert_eq!(
        parse_provider_executable("compose provider \"/usr/bin/provider\" selected"),
        Some("/usr/bin/provider".into())
    );
    assert_eq!(parse_provider_executable("compose provider unquoted"), None);
    assert_eq!(parse_provider_executable("missing marker"), None);
    Ok(())
}

fn container(id: &str) -> ContainerObservation {
    ContainerObservation {
        id: id.to_owned(),
        name: "web".to_owned(),
        service: "web".to_owned(),
        image: "image".to_owned(),
        state: "running".to_owned(),
        status: "up".to_owned(),
        health: None,
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

#[tokio::test]
async fn health_queries_handle_success_disappearance_malformed_and_truncated_output()
-> Result<(), Box<dyn std::error::Error>> {
    let mut truncated = super::command_output("abc\thealthy\n");
    truncated.truncated = true;
    let runner = std::sync::Arc::new(super::RecordingRunner {
        outputs: std::sync::Mutex::new(
            [
                super::command_output("abc\thealthy\ndef\tnone\n"),
                super::command_output_with_code(1, ""),
                super::command_output("malformed"),
                truncated,
            ]
            .into_iter()
            .collect(),
        ),
        commands: std::sync::Mutex::new(Vec::new()),
    });
    let runtime = super::PodmanComposeRuntime::with_executable(runner, "podman");
    assert_eq!(
        runtime
            .query_health(std::path::Path::new("/tmp"), &["abc".to_owned(), "def".to_owned()])
            .await?,
        Some(vec![("abc".to_owned(), Some("healthy".to_owned())), ("def".to_owned(), None)])
    );
    assert_eq!(
        runtime
            .query_health(std::path::Path::new("/tmp"), &["gone".to_owned()])
            .await?,
        None
    );
    assert!(runtime.query_health(std::path::Path::new("/tmp"), &["bad".to_owned()]).await.is_err());
    assert!(runtime.query_health(std::path::Path::new("/tmp"), &["large".to_owned()]).await.is_err());
    Ok(())
}

#[tokio::test]
async fn health_population_skips_empty_ids_and_falls_back_to_individual_inspection()
-> Result<(), Box<dyn std::error::Error>> {
    let runner = std::sync::Arc::new(super::RecordingRunner {
        outputs: std::sync::Mutex::new(
            [
                super::command_output_with_code(1, ""),
                super::command_output("abcdef\thealthy\n"),
            ]
            .into_iter()
            .collect(),
        ),
        commands: std::sync::Mutex::new(Vec::new()),
    });
    let runtime = super::PodmanComposeRuntime::with_executable(runner, "podman");
    let mut empty = vec![container("")];
    runtime.populate_health(std::path::Path::new("/tmp"), &mut empty).await?;
    let mut containers = vec![container("abcdef")];
    runtime.populate_health(std::path::Path::new("/tmp"), &mut containers).await?;
    assert_eq!(containers[0].health.as_deref(), Some("healthy"));
    Ok(())
}
}
