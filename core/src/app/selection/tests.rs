use super::{
    LocalSelectionPreference, SelectionSource, resolve_operation_context,
    resolve_runtime_and_target, resolve_target_name,
};
use crate::app::ports::TargetStore;
use crate::domain::config::RuntimeConfig;
use crate::domain::error::MmError;
use crate::domain::runtime::RuntimeKind;
use crate::domain::target::{
    NamedTarget, TargetDefinition, TargetId, TargetIngress, TargetTransport,
};
use uuid::Uuid;

struct StaticTargetStore(NamedTarget);

impl TargetStore for StaticTargetStore {
    fn list_targets(&self) -> Result<Vec<NamedTarget>, MmError> {
        Ok(vec![self.0.clone()])
    }

    fn resolve_target(&self, name: &str) -> Result<NamedTarget, MmError> {
        if name == self.0.name {
            Ok(self.0.clone())
        } else {
            Err(MmError::Target {
                message: format!("target '{name}' was not found"),
            })
        }
    }

    fn insert_target(
        &self,
        _name: &str,
        _definition: TargetDefinition,
    ) -> Result<NamedTarget, MmError> {
        Err(unexpected_target_mutation())
    }

    fn ensure_target(
        &self,
        _name: &str,
        _definition: TargetDefinition,
    ) -> Result<NamedTarget, MmError> {
        Err(unexpected_target_mutation())
    }

    fn remove_target(
        &self,
        _name: &str,
        _expected_fingerprint: &str,
    ) -> Result<NamedTarget, MmError> {
        Err(unexpected_target_mutation())
    }

    fn update_target_transport(
        &self,
        _name: &str,
        _expected_fingerprint: &str,
        _transport: TargetTransport,
    ) -> Result<NamedTarget, MmError> {
        Err(unexpected_target_mutation())
    }

    fn update_target_ingress(
        &self,
        _name: &str,
        _expected_fingerprint: &str,
        _ingress: TargetIngress,
    ) -> Result<NamedTarget, MmError> {
        Err(unexpected_target_mutation())
    }
}

fn unexpected_target_mutation() -> MmError {
    MmError::Target {
        message: "selection must not mutate the target catalog".to_owned(),
    }
}

#[test]
fn cli_runtime_wins_over_local_and_project_defaults() -> Result<(), crate::domain::error::MmError> {
    let document = crate::test_project_documents().load_from(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal"),
    )?;
    let target = NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(Uuid::nil())),
    );
    let env_pref = LocalSelectionPreference {
        runtime: Some(RuntimeKind::PodmanCompose),
        target: None,
    };
    let resolved = resolve_runtime_and_target(
        &document,
        "local",
        Some(RuntimeKind::DockerCompose),
        None,
        Some(&env_pref),
        None,
        &target,
    )?;
    assert_eq!(resolved.runtime, RuntimeKind::DockerCompose);
    assert_eq!(resolved.runtime_source, SelectionSource::Cli);
    assert_eq!(resolved.target, "local");
    assert_eq!(resolved.target_source, SelectionSource::ProjectConfig);
    assert_eq!(
        [
            SelectionSource::Cli,
            SelectionSource::LocalEnvironmentOverride,
            SelectionSource::LocalProjectOverride,
            SelectionSource::EnvironmentConfig,
            SelectionSource::ProjectConfig,
            SelectionSource::TargetDefault,
        ]
        .map(SelectionSource::as_str),
        [
            "cli",
            "local-environment",
            "local-project",
            "environment-config",
            "project-config",
            "target-default",
        ]
    );
    let _ = RuntimeConfig::default();
    Ok(())
}

#[test]
fn local_project_preferences_precede_configured_defaults()
-> Result<(), crate::domain::error::MmError> {
    let document = crate::test_project_documents().load_from(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal"),
    )?;
    let target = NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(Uuid::nil())),
    );
    let preference = LocalSelectionPreference {
        runtime: Some(RuntimeKind::PodmanCompose),
        target: Some("preferred".to_owned()),
    };
    let resolved = resolve_runtime_and_target(
        &document,
        "local",
        None,
        None,
        None,
        Some(&preference),
        &target,
    )?;
    assert_eq!(resolved.target, "preferred");
    assert_eq!(
        resolved.target_source,
        SelectionSource::LocalProjectOverride
    );
    assert_eq!(resolved.runtime, RuntimeKind::PodmanCompose);
    assert_eq!(
        resolved.runtime_source,
        SelectionSource::LocalProjectOverride
    );
    Ok(())
}

#[test]
fn arbitrary_environment_keys_use_the_same_resolver() -> Result<(), crate::domain::error::MmError> {
    let document = crate::test_project_documents().load_from(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal"),
    )?;
    for environment in ["integration", "uat", "preview-a", "qa-west"] {
        let (name, source) = resolve_target_name(&document, environment, None, None, None)?;
        assert_eq!(name, "local");
        assert_eq!(source, SelectionSource::ProjectConfig);
    }
    Ok(())
}

#[test]
fn missing_runtime_is_a_hard_error() -> Result<(), crate::domain::error::MmError> {
    let mut document = crate::test_project_documents().load_from(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal"),
    )?;
    document.config.runtime.driver = None;
    let target = NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(Uuid::nil())),
    );
    let error = resolve_runtime_and_target(&document, "local", None, None, None, None, &target)
        .err()
        .ok_or_else(|| crate::domain::error::MmError::Environment {
            message: "missing runtime must fail closed".to_owned(),
        })?;
    assert!(error.to_string().contains("no runtime was selected"));
    Ok(())
}

#[test]
fn unavailable_runtime_is_a_hard_error() -> Result<(), crate::domain::error::MmError> {
    let document = crate::test_project_documents().load_from(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal"),
    )?;
    let mut target = NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(Uuid::nil())),
    );
    target.definition.available_runtimes = vec![RuntimeKind::DockerCompose];
    let error = resolve_runtime_and_target(&document, "local", None, None, None, None, &target)
        .err()
        .ok_or_else(|| crate::domain::error::MmError::Environment {
            message: "unavailable runtime must fail closed".to_owned(),
        })?;
    assert!(error.to_string().contains("does not offer runtime"));
    Ok(())
}

#[test]
fn operation_context_binds_the_resolved_target_and_document_identity() -> Result<(), MmError> {
    let document = crate::test_project_documents().load_from(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal"),
    )?;
    let target = NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(Uuid::from_u128(7))),
    );
    let store = StaticTargetStore(target.clone());

    let context = resolve_operation_context(
        &document,
        "local",
        Some(RuntimeKind::DockerCompose),
        Some("local"),
        None,
        None,
        &store,
    )?;

    assert_eq!(context.project_root, document.root);
    assert_eq!(context.project_id, document.config.project.id);
    assert_eq!(context.config_hash, document.source_hash);
    assert_eq!(context.target, target);
    assert_eq!(context.runtime, RuntimeKind::DockerCompose);
    assert_eq!(context.selection.target_source, SelectionSource::Cli);
    assert_eq!(context.selection.runtime_source, SelectionSource::Cli);
    assert_eq!(context.transport, TargetTransport::Local);
    Ok(())
}

/// A catalog without the target, as on a fresh clone.
struct EmptyTargetStore;

impl TargetStore for EmptyTargetStore {
    fn list_targets(&self) -> Result<Vec<NamedTarget>, MmError> {
        Ok(Vec::new())
    }

    fn resolve_target(&self, name: &str) -> Result<NamedTarget, MmError> {
        Err(MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::MissingTarget {
                name: name.to_owned(),
                path: std::path::PathBuf::from("targets.yaml"),
            },
        })
    }

    fn insert_target(
        &self,
        _name: &str,
        _definition: TargetDefinition,
    ) -> Result<NamedTarget, MmError> {
        Err(unexpected_target_mutation())
    }

    fn ensure_target(
        &self,
        _name: &str,
        _definition: TargetDefinition,
    ) -> Result<NamedTarget, MmError> {
        Err(unexpected_target_mutation())
    }

    fn remove_target(
        &self,
        _name: &str,
        _expected_fingerprint: &str,
    ) -> Result<NamedTarget, MmError> {
        Err(unexpected_target_mutation())
    }

    fn update_target_transport(
        &self,
        _name: &str,
        _expected_fingerprint: &str,
        _transport: TargetTransport,
    ) -> Result<NamedTarget, MmError> {
        Err(unexpected_target_mutation())
    }

    fn update_target_ingress(
        &self,
        _name: &str,
        _expected_fingerprint: &str,
        _ingress: TargetIngress,
    ) -> Result<NamedTarget, MmError> {
        Err(unexpected_target_mutation())
    }
}

const REPO_HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w";

fn repo_target(yaml: &str) -> Result<crate::domain::repo_target::RepoTargetConfig, MmError> {
    serde_yaml_ng::from_str(yaml).map_err(|error| MmError::Target {
        message: error.to_string(),
    })
}

#[test]
fn a_flat_repo_target_resolves_from_its_declared_facts() -> Result<(), MmError> {
    let mut document = crate::test_project_documents().load_from(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal"),
    )?;
    document.config.targets.insert(
        "production".to_owned(),
        repo_target(&format!(
            "host: app.example.test\nport: 2222\nhost-key: {REPO_HOST_KEY}\ndaemon-fingerprint: {}\n",
            "a".repeat(64)
        ))?,
    );
    let context = resolve_operation_context(
        &document,
        "local",
        Some(RuntimeKind::DockerCompose),
        Some("production"),
        None,
        None,
        &EmptyTargetStore,
    )?;
    assert_eq!(context.target.name, "production");
    assert_eq!(context.selection.target, "production");
    assert_eq!(context.runtime, RuntimeKind::DockerCompose);
    assert!(context.target.definition.nodes.is_empty());
    assert!(matches!(
        &context.transport,
        TargetTransport::Ssh { host, port: 2222, .. } if host == "app.example.test"
    ));
    Ok(())
}

#[test]
fn a_repo_target_that_lists_nodes_resolves_to_a_target_that_names_every_node() -> Result<(), MmError>
{
    let mut document = crate::test_project_documents().load_from(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal"),
    )?;
    let recorded = |name: &str, host: &str, digit: &str| {
        format!(
            "  - name: {name}\n    host: {host}\n    host-key: {REPO_HOST_KEY}\n    daemon-fingerprint: {}\n",
            digit.repeat(64)
        )
    };
    let cluster = repo_target(&format!(
        "nodes:\n{}{}  - name: db-1\n    host: 203.0.113.13\n",
        recorded("app-1", "203.0.113.11", "1"),
        recorded("app-2", "203.0.113.12", "2"),
    ))?;
    cluster
        .validate("production")
        .map_err(|message| MmError::Target { message })?;
    document
        .config
        .targets
        .insert("production".to_owned(), cluster);

    // No runtime is configured anywhere: a cluster target runs Docker
    // Compose unless the environment says otherwise.
    document.config.runtime.driver = None;
    let context = resolve_operation_context(
        &document,
        "local",
        None,
        Some("production"),
        None,
        None,
        &EmptyTargetStore,
    )?;
    assert_eq!(context.runtime, RuntimeKind::DockerCompose);
    assert_eq!(
        context.selection.runtime_source,
        SelectionSource::TargetDefault
    );
    let nodes = &context.target.definition.nodes;
    assert_eq!(
        nodes
            .iter()
            .map(|node| (node.name.as_str(), node.host.as_str(), node.is_recorded()))
            .collect::<Vec<_>>(),
        [
            ("app-1", "203.0.113.11", true),
            ("app-2", "203.0.113.12", true),
            // Not set up yet: listed, but mm cannot connect to it.
            ("db-1", "203.0.113.13", false),
        ]
    );
    assert_eq!(
        nodes
            .iter()
            .map(|node| node.private_address.map(|address| address.to_string()))
            .collect::<Vec<_>>(),
        [
            Some("10.90.0.1".to_owned()),
            Some("10.90.0.2".to_owned()),
            Some("10.90.0.3".to_owned())
        ]
    );
    assert!(matches!(
        &context.transport,
        TargetTransport::Ssh { host, .. } if host == "203.0.113.11"
    ));

    // A target nobody declared is still reported as missing.
    let missing = resolve_operation_context(
        &document,
        "local",
        None,
        Some("staging"),
        None,
        None,
        &EmptyTargetStore,
    );
    assert!(matches!(missing, Err(MmError::Setup { .. })));
    Ok(())
}
