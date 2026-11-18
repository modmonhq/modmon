use super::*;

fn request(
    action: ConfigMutationAction,
    resource: ConfigResourceKind,
    name: &str,
    parent: Option<&str>,
    definition: Option<serde_json::Value>,
) -> ConfigMutationRequest {
    ConfigMutationRequest {
        action,
        resource,
        name: name.to_owned(),
        parent: parent.map(str::to_owned),
        definition,
    }
}

/// Edits the anchor file, where every top-level key lives unless split.
fn edit(
    source: &str,
    parents: &[String],
    key: &str,
    request: &ConfigMutationRequest,
) -> Result<String, MmError> {
    edit_resource(Path::new(".config/mm.yaml"), source, parents, key, request)
}

#[test]
fn every_resource_kind_maps_to_a_lossless_configuration_location()
-> Result<(), Box<dyn std::error::Error>> {
    for (resource, parent) in [
        (ConfigResourceKind::Module, None),
        (ConfigResourceKind::Service, Some("api")),
        (ConfigResourceKind::Stack, None),
        (ConfigResourceKind::Build, None),
        (ConfigResourceKind::Environment, None),
        (ConfigResourceKind::Binding, None),
        (ConfigResourceKind::Binding, Some("local")),
        (ConfigResourceKind::Secret, None),
        (ConfigResourceKind::Ingress, Some("app")),
        (ConfigResourceKind::LiveMount, None),
        (ConfigResourceKind::Job, None),
        (ConfigResourceKind::Schedule, None),
        (ConfigResourceKind::Gate, None),
    ] {
        let location = resource_location(&request(
            ConfigMutationAction::Add,
            resource,
            "entry",
            parent,
            Some(serde_json::json!({})),
        ))?;
        assert!(!location.key.is_empty());
        assert!(!location.parents.is_empty());
    }
    assert!(
        resource_location(&request(
            ConfigMutationAction::Add,
            ConfigResourceKind::Service,
            "web",
            None,
            Some(serde_json::json!({})),
        ))
        .is_err()
    );
    for key in ["safe-key", "safe.key", "safe_key"] {
        validate_yaml_key(key)?;
    }
    for key in ["", "bad key", "bad/key"] {
        assert!(validate_yaml_key(key).is_err());
    }
    Ok(())
}

#[test]
fn block_and_flow_edits_add_replace_remove_and_create_missing_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "modules: {}\nstacks:\n  app:\n    files: [compose.yaml]\nenvironments:\n  local:\n    bindings: {}\n";
    let add_module = request(
        ConfigMutationAction::Add,
        ConfigResourceKind::Module,
        "api",
        None,
        Some(serde_json::json!({"path": "api", "services": {}})),
    );
    let location = resource_location(&add_module)?;
    let added = edit(source, &location.parents, &location.key, &add_module)?;
    assert!(added.contains("api:"));
    assert!(edit(&added, &location.parents, &location.key, &add_module).is_err());

    let edit_module = request(
        ConfigMutationAction::Edit,
        ConfigResourceKind::Module,
        "api",
        None,
        Some(serde_json::json!({"path": "renamed", "services": {}})),
    );
    let edited = edit(&added, &location.parents, &location.key, &edit_module)?;
    assert!(edited.contains("path: renamed"));
    let remove_module = request(
        ConfigMutationAction::Remove,
        ConfigResourceKind::Module,
        "api",
        None,
        None,
    );
    let removed = edit(&edited, &location.parents, &location.key, &remove_module)?;
    assert!(!removed.contains("api:"));
    assert!(edit(&removed, &location.parents, &location.key, &remove_module).is_err());

    let service = request(
        ConfigMutationAction::Add,
        ConfigResourceKind::Service,
        "web",
        Some("missing-module"),
        Some(serde_json::json!({"stack": "app"})),
    );
    let location = resource_location(&service)?;
    let nested = edit(source, &location.parents, &location.key, &service)?;
    assert!(nested.contains("missing-module:"));
    assert!(nested.contains("web:"));
    assert!(
        edit(
            source,
            &location.parents,
            &location.key,
            &request(
                ConfigMutationAction::Edit,
                ConfigResourceKind::Service,
                "web",
                Some("missing-module"),
                Some(serde_json::json!({})),
            ),
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn reviewed_commit_rejects_stale_source_and_result_hashes() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = crate::test_support::tempdir()?;
    std::fs::write(temp.path().join("compose.yaml"), "services: {}\n")?;
    ConfigLoader.initialize(temp.path(), Some("guided"))?;
    let request = request(
        ConfigMutationAction::Add,
        ConfigResourceKind::Module,
        "api",
        None,
        Some(serde_json::json!({"path": "api", "services": {}})),
    );
    let preview = ConfigLoader.preview(temp.path(), &request)?;
    let result_hash = blake3::hash(preview.after.as_bytes()).to_hex().to_string();
    assert!(
        ConfigLoader
            .commit(&preview, "stale", &result_hash)
            .is_err()
    );
    assert!(
        ConfigLoader
            .commit(&preview, &preview.source_hash, "stale")
            .is_err()
    );
    let document = ConfigLoader.commit(&preview, &preview.source_hash, &result_hash)?;
    assert!(document.config.modules.contains_key("api"));
    Ok(())
}

#[test]
fn discovery_reports_compose_services_modules_mounts_and_templates()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    std::fs::write(
        temp.path().join("compose.yaml"),
        "services:\n  api:\n    build: ./api\n    ports: ['8080:80']\n    volumes: ['./api:/app']\n    env_file: api/.env\n",
    )?;
    let module = temp.path().join("api");
    std::fs::create_dir(&module)?;
    std::fs::write(module.join("Dockerfile"), "FROM scratch\n")?;
    std::fs::write(
        module.join("Cargo.toml"),
        "[package]\nname='api'\nversion='0.1.0'\n",
    )?;
    std::fs::write(module.join(".env.example"), "PUBLIC=value\n")?;
    let discovery = discover_project(temp.path())?;
    for subject in [
        "compose-file",
        "service:api",
        "module:api",
        "development-mount:api",
        "environment-template:api",
    ] {
        assert!(
            discovery
                .proposals
                .iter()
                .any(|proposal| proposal.subject == subject)
        );
    }
    std::fs::write(temp.path().join("compose.yaml"), "not: [valid")?;
    assert!(discover_project(temp.path()).is_err());
    Ok(())
}

#[test]
fn guided_error_helpers_reject_missing_definitions_scalar_paths_and_non_directories()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "modules: {}\nstacks:\n  app: scalar\n";
    let missing_definition = request(
        ConfigMutationAction::Add,
        ConfigResourceKind::Service,
        "web",
        Some("api"),
        None,
    );
    let location = resource_location(&missing_definition)?;
    assert!(
        edit_resource(
            Path::new(".config/mm.yaml"),
            source,
            &location.parents,
            &location.key,
            &missing_definition
        )
        .is_err()
    );
    let missing_edit = request(
        ConfigMutationAction::Edit,
        ConfigResourceKind::Module,
        "api",
        None,
        None,
    );
    let location = resource_location(&missing_edit)?;
    assert!(
        edit_resource(
            Path::new(".config/mm.yaml"),
            source,
            &location.parents,
            &location.key,
            &missing_edit
        )
        .is_err()
    );

    let scalar = request(
        ConfigMutationAction::Add,
        ConfigResourceKind::Ingress,
        "ignored",
        Some("app"),
        Some(serde_json::json!({})),
    );
    let location = resource_location(&scalar)?;
    assert!(
        edit_resource(
            Path::new(".config/mm.yaml"),
            source,
            &location.parents,
            &location.key,
            &scalar
        )
        .is_err()
    );
    assert!(
        insert_missing_path(
            source,
            MappingRange {
                start: 0,
                end: source.len(),
                indent: 0,
                flow_empty: false
            },
            &[],
            "entry",
            &serde_json::json!({})
        )
        .is_err()
    );

    let temp = crate::test_support::tempdir()?;
    let file = temp.path().join("not-a-directory");
    std::fs::write(&file, b"plain")?;
    assert!(discover_project(&file).is_err());
    assert!(discover_compose(&temp.path().join("missing.yaml"), &mut Vec::new()).is_err());

    let preview = ConfigMutationPreview {
        config_path: std::path::PathBuf::from("/proc/mm-guided/config.yaml"),
        source_hash: blake3::hash(b"").to_hex().to_string(),
        before: String::new(),
        after: String::new(),
    };
    assert!(
        ConfigLoader
            .commit(&preview, &preview.source_hash, &preview.source_hash)
            .is_err()
    );
    Ok(())
}
