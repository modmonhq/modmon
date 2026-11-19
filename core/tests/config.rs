use std::fs;
use std::path::{Path, PathBuf};

use mm::adapters::config::{
    ConfigLoader, MmConfig, SecretGeneratorConfig, resolve_environment, validate,
};
use mm::domain::environment_delivery::{
    EnvironmentDeliveryMode, EnvironmentDeliveryProvenance, resolve_environment_delivery,
};
use mm::domain::error::MmError;
use tempfile::TempDir;

mod temp_dirs;

#[test]
fn discovers_and_validates_a_project_from_a_module_directory()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture_root();
    let loader = ConfigLoader;
    let document = loader.load_from(&fixture.join("api"))?;
    let environment = resolve_environment(&document, None)?;
    let report = validate(&document, &environment);

    assert_eq!(document.config.project.name, "minimal");
    assert_eq!(environment, "local");
    assert!(matches!(
        document
            .config
            .secrets
            .get("database.password")
            .and_then(|declaration| declaration.generate.as_ref()),
        Some(SecretGeneratorConfig::Password { length: 32 })
    ));
    assert_eq!(
        document.config.environments["local"].bindings["DB.PASSWORD"].secret_name(),
        Some("database.password")
    );
    assert!(report.is_valid(), "diagnostics: {:?}", report.diagnostics);
    Ok(())
}

#[test]
fn rejects_provider_instance_names_reserved_by_builtin_storage()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    fs::create_dir_all(temporary.path().join(".config"))?;
    let source = fs::read_to_string(fixture_root().join(".config/mm.yaml"))?.replacen(
        "secrets:\n",
        &format!(
            "providers:\n  age:\n    driver: openbao\n    scope: project\n    mode: external\n    address: https://provider.example.test\n    storage-mount: secrets\n    automation-auth-mount: automation\n    trust-bundle: ca.pem\n    expected-trust-sha256: sha256:{}\nsecrets:\n",
            "a".repeat(64)
        ),
        1,
    );
    fs::write(temporary.path().join(".config/mm.yaml"), source)?;
    fs::write(temporary.path().join("compose.yaml"), "services: {}\n")?;

    let document = ConfigLoader.load_from(temporary.path())?;
    let report = validate(&document, "local");
    let repeated = validate(&document, "local");
    assert_eq!(repeated.diagnostics, report.diagnostics);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "MM_PROVIDER_INSTANCE_RESERVED"
            && diagnostic.message.contains("reserved")
    }));
    Ok(())
}

#[test]
fn requires_exact_schema_one_without_rewriting_invalid_sources()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture_root();
    let current = fs::read_to_string(fixture.join(".config/mm.yaml"))?;
    assert_eq!(ConfigLoader.load_from(&fixture)?.config.schema_version, 1);

    for invalid in [None, Some(0), Some(2), Some(4), Some(5)] {
        let temporary = temp_dirs::tempdir()?;
        let config_directory = temporary.path().join(".config");
        fs::create_dir_all(&config_directory)?;
        let source = invalid.map_or_else(
            || current.replacen("schema-version: 1\n", "", 1),
            |version| {
                current.replacen(
                    "schema-version: 1",
                    &format!("schema-version: {version}"),
                    1,
                )
            },
        );
        let path = config_directory.join("mm.yaml");
        fs::write(&path, &source)?;

        let error = ConfigLoader
            .load_from(temporary.path())
            .err()
            .ok_or("invalid schema loaded")?;
        let message = error.to_string();
        if let Some(version) = invalid {
            assert!(message.contains(&format!("unsupported schema-version {version}")));
            assert!(message.contains("expected schema-version: 1"));
        } else {
            assert!(message.contains("missing field `schema-version`"));
        }
        assert_eq!(fs::read_to_string(path)?, source);
    }
    Ok(())
}

#[test]
fn rejects_uri_secret_declarations_without_rewriting_the_source()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let config_directory = temporary.path().join(".config");
    fs::create_dir_all(&config_directory)?;
    let source = fs::read_to_string(fixture_root().join(".config/mm.yaml"))?.replacen(
        "    storage:\n      provider: age\n      location: local/db-password\n",
        "    provider: age://local/db-password\n",
        1,
    );
    let path = config_directory.join("mm.yaml");
    fs::write(&path, &source)?;

    let error = ConfigLoader
        .load_from(temporary.path())
        .err()
        .ok_or("URI secret declaration loaded")?;
    let message = error.to_string();
    assert!(message.contains("missing field `storage`"), "{message}");
    assert!(message.contains("unknown key `secrets."), "{message}");
    assert!(message.contains(".provider`"), "{message}");
    assert_eq!(fs::read_to_string(path)?, source);
    Ok(())
}

#[test]
fn ignores_unknown_configuration_fields() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let config_directory = temporary.path().join(".config");
    fs::create_dir_all(&config_directory)?;
    fs::write(
        config_directory.join("mm.yaml"),
        r#"
schema-version: 1
default-environment: local
unknown: true
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: invalid
target: local
stacks:
  app:
    files: [compose.yaml]
environments:
  local:
    bindings: {}
"#,
    )?;

    let document = ConfigLoader.load_from(temporary.path())?;
    assert_eq!(document.unknown_keys.len(), 1);
    assert_eq!(document.unknown_keys[0].key, "unknown");
    assert_eq!(document.unknown_keys[0].file, ".config/mm.yaml");
    Ok(())
}

#[test]
fn ingress_accepts_only_domain_mappings() -> Result<(), Box<dyn std::error::Error>> {
    let mapping: MmConfig =
        serde_yaml_ng::from_str(&config_with_ingress("    ingress:\n      domain: web\n"))?;
    let mapping = mapping.stacks["app"].ingress.routes();
    assert_eq!(mapping.len(), 1);
    assert_eq!(mapping[0].domain, "web");
    assert_eq!(mapping[0].route, "/");
    assert_eq!(mapping[0].service, None);
    assert_eq!(mapping[0].port, None);
    assert!(mapping[0].https);
    assert!(mapping[0].environments.is_empty());

    let list: MmConfig = serde_yaml_ng::from_str(&config_with_ingress(
        "    ingress:\n      - domain: web\n        service: web\n        port: 8080\n      - domain: api\n        route: /api\n        https: false\n        environments: [local]\n",
    ))?;
    let list = list.stacks["app"].ingress.routes();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].service.as_deref(), Some("web"));
    assert_eq!(list[0].port, Some(8080));
    assert_eq!(list[1].route, "/api");
    assert!(!list[1].https);
    assert_eq!(list[1].environments, vec!["local".to_owned()]);

    for invalid in [
        "    ingress: app.example.com\n",
        "    ingress: http://app.localhost/api\n",
        "    ingress:\n      host: app.example.com\n",
        "    ingress:\n      - app.example.com\n",
        "    ingress:\n      public:\n        domain: web\n",
        "    ingress: null\n",
    ] {
        assert!(
            serde_yaml_ng::from_str::<MmConfig>(&config_with_ingress(invalid)).is_err(),
            "unexpectedly accepted ingress shape:\n{invalid}"
        );
    }
    Ok(())
}

#[test]
fn validates_project_global_middleware_bundles() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    fs::create_dir_all(temporary.path().join(".config"))?;
    fs::create_dir_all(temporary.path().join("middleware"))?;
    fs::write(temporary.path().join("compose.yaml"), "services: {}\n")?;
    fs::write(
        temporary.path().join("middleware/block-metrics.yml"),
        "http:\n  middlewares:\n    block-metrics:\n      replacePathRegex:\n        regex: ^/(prometheus|metrics).*\n        replacement: /heartbeat\n",
    )?;
    fs::write(
        temporary.path().join(".config/mm.yaml"),
        r#"
schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: middleware
target: local
runtime:
  driver: podman-compose
stacks:
  app:
    files: [compose.yaml]
ingress:
  middlewares:
    block-metrics:
      environments: [production]
      file: middleware/block-metrics.yml
environments:
  local:
    bindings: {}
  production:
    bindings: {}
"#,
    )?;

    let document = ConfigLoader.load_from(temporary.path())?;
    let local = validate(&document, "local");
    let production = validate(&document, "production");
    assert!(local.is_valid(), "{:?}", local.diagnostics);
    assert!(production.is_valid(), "{:?}", production.diagnostics);
    assert_eq!(document.config.ingress.middlewares.len(), 1);

    fs::write(
        temporary.path().join("middleware/block-metrics.yml"),
        "http:\n  routers: {}\n  middlewares:\n    block-metrics:\n      replacePathRegex:\n        regex: ^/x\n        replacement: /y\n",
    )?;
    let ignored = validate(&ConfigLoader.load_from(temporary.path())?, "production");
    assert!(ignored.is_valid(), "{:?}", ignored.diagnostics);
    assert!(ignored.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "MM_CONFIG_UNKNOWN_KEY"
            && diagnostic.message.starts_with(
                "unknown key `http.routers` in middleware/block-metrics.yml; this mm ignores it",
            )
    }));
    fs::write(
        temporary.path().join("middleware/block-metrics.yml"),
        "http:\n  middlewares:\n    block-metrics:\n      headers: {}\n",
    )?;
    let rejected = validate(&ConfigLoader.load_from(temporary.path())?, "production");
    assert!(!rejected.is_valid());
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_MIDDLEWARE_INVALID")
    );
    Ok(())
}

#[test]
fn generated_schema_exposes_ingress_instead_of_routes() -> Result<(), Box<dyn std::error::Error>> {
    let schema = serde_json::to_value(ConfigLoader.schema())?;
    assert!(schema.to_string().contains("\"ingress\""));
    assert!(!schema.to_string().contains("\"routes\""));
    assert!(
        schema["required"]
            .as_array()
            .is_some_and(|required| required.iter().any(|field| field == "schema-version"))
    );
    assert_eq!(schema["properties"]["schema-version"]["minimum"], 1);
    assert_eq!(schema["properties"]["schema-version"]["maximum"], 1);
    Ok(())
}

#[test]
fn published_project_schema_has_no_drift() -> Result<(), Box<dyn std::error::Error>> {
    let mut generated = serde_json::to_string_pretty(&ConfigLoader.schema())?;
    generated.push('\n');
    let published =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../site/public/schema/mm.schema.json");
    assert_eq!(generated.as_bytes(), fs::read(published)?);
    Ok(())
}

#[test]
fn rejects_removed_singular_compose_shape() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    fs::create_dir_all(temporary.path().join(".config"))?;
    fs::write(
        temporary.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: old-shape
target: local
compose:
  files: [compose.yaml]
environments:
  local:
    bindings: {}
"#,
    )?;
    assert!(matches!(
        ConfigLoader.load_from(temporary.path()),
        Err(MmError::ConfigParse { .. })
    ));
    Ok(())
}

#[test]
fn initialized_projects_use_approved_delivery_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = initialized_project_without_compose()?;
    let document = ConfigLoader.load_from(temporary.path())?;
    assert_eq!(
        document.config.project.development_environment.as_deref(),
        Some("local")
    );
    assert_eq!(
        document.config.environment_delivery.default,
        Some(EnvironmentDeliveryMode::Direct)
    );
    assert_eq!(
        document.config.environment_delivery.development_default,
        Some(EnvironmentDeliveryMode::EnvFile)
    );
    let local = resolve_environment_delivery(&document.config, "local");
    assert_eq!(local.mode, EnvironmentDeliveryMode::EnvFile);
    assert_eq!(
        local.provenance,
        EnvironmentDeliveryProvenance::DevelopmentDefault
    );
    Ok(())
}

#[test]
fn configured_development_environment_uses_built_in_env_file_default()
-> Result<(), Box<dyn std::error::Error>> {
    let document = ConfigLoader.load_from(&fixture_root())?;
    let local = resolve_environment_delivery(&document.config, "local");
    assert_eq!(local.mode, EnvironmentDeliveryMode::EnvFile);
    assert_eq!(
        local.provenance,
        EnvironmentDeliveryProvenance::BuiltInDefault
    );
    Ok(())
}

#[test]
fn rejects_unknown_environment_delivery_modes() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    fs::create_dir_all(temporary.path().join(".config"))?;
    fs::write(
        temporary.path().join(".config/mm.yaml"),
        r#"
schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: invalid
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
environments:
  local:
    environment-delivery: sidecar
    bindings: {}
"#,
    )?;
    assert!(matches!(
        ConfigLoader.load_from(temporary.path()),
        Err(MmError::ConfigParse { .. })
    ));
    Ok(())
}

#[test]
fn edits_environment_delivery_without_reformatting_the_file()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    fs::create_dir_all(temporary.path().join(".config"))?;
    fs::create_dir_all(temporary.path().join("api"))?;
    let fixture = fixture_root();
    let original = fs::read_to_string(fixture.join(".config/mm.yaml"))?;
    let source = format!("# retained header\n{original}# retained footer\n");
    fs::write(temporary.path().join(".config/mm.yaml"), source)?;
    fs::copy(
        fixture.join("compose.yaml"),
        temporary.path().join("compose.yaml"),
    )?;
    fs::copy(
        fixture.join("api/.env.example"),
        temporary.path().join("api/.env.example"),
    )?;

    let loader = ConfigLoader;
    loader.set_environment_delivery(temporary.path(), "local", EnvironmentDeliveryMode::Direct)?;
    let edited = fs::read_to_string(temporary.path().join(".config/mm.yaml"))?;
    assert!(edited.contains("# retained header"));
    assert!(edited.contains("# retained footer"));
    assert!(edited.contains("environment-delivery: direct"));
    let document = loader.load_from(temporary.path())?;
    assert_eq!(
        resolve_environment_delivery(&document.config, "local").mode,
        EnvironmentDeliveryMode::Direct
    );

    loader.set_environment_delivery(temporary.path(), "local", EnvironmentDeliveryMode::EnvFile)?;
    let edited = fs::read_to_string(temporary.path().join(".config/mm.yaml"))?;
    assert!(edited.contains("environment-delivery: env-file"));
    let document = loader.load_from(temporary.path())?;
    assert_eq!(
        resolve_environment_delivery(&document.config, "local").mode,
        EnvironmentDeliveryMode::EnvFile
    );
    Ok(())
}

#[test]
fn reports_missing_compose_files_without_panicking() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = initialized_project_without_compose()?;
    let document = ConfigLoader.load_from(temporary.path())?;
    let report = validate(&document, "local");

    assert_eq!(document.config.target, "local");
    assert!(document.config.stacks.contains_key("app"));
    assert!(!report.is_valid());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_COMPOSE_FILE_MISSING")
    );
    Ok(())
}

#[test]
fn rejects_runtime_connection_and_invalid_compose_names() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = temp_dirs::tempdir()?;
    let config_directory = temporary.path().join(".config");
    fs::create_dir_all(&config_directory)?;
    fs::write(temporary.path().join("compose.yaml"), "services: {}\n")?;
    fs::write(
        config_directory.join("mm.yaml"),
        r#"
schema-version: 1
default-environment: local
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: invalid
target: local
runtime:
  driver: podman-compose
  connection: production
stacks:
  app:
    project-name: Invalid-{environment}
    files: [compose.yaml]
environments:
  local:
    bindings: {}
"#,
    )?;

    let connection = ConfigLoader.load_from(temporary.path())?.unknown_keys;
    assert_eq!(connection.len(), 1, "{connection:?}");
    assert_eq!(connection[0].key, "runtime.connection");
    fs::write(
        config_directory.join("mm.yaml"),
        r#"
schema-version: 1
default-environment: local
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: invalid
target: local
runtime:
  driver: podman-compose
stacks:
  app:
    project-name: Invalid-{environment}
    files: [compose.yaml]
environments:
  local:
    bindings: {}
"#,
    )?;
    let document = ConfigLoader.load_from(temporary.path())?;
    let report = validate(&document, "local");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_COMPOSE_PROJECT_NAME_INVALID")
    );
    Ok(())
}

#[test]
fn rejects_invalid_provider_locators_and_multiline_environment_values()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    fs::create_dir_all(temporary.path().join(".config"))?;
    fs::write(temporary.path().join("compose.yaml"), "services: {}\n")?;
    fs::write(
        temporary.path().join(".config/mm.yaml"),
        r#"schema-version: 1
default-environment: local
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: invalid-values
target: local
stacks:
  app:
    files: [compose.yaml]
secrets:
  bad-secret:
    storage:
      provider: env
      location: BAD-NAME
environments:
  local:
    bindings:
      BAD.SECRET:
        secret: bad-secret
      BAD.VALUE:
        value: |
          line-one
          line-two
"#,
    )?;

    let document = ConfigLoader.load_from(temporary.path())?;
    let report = validate(&document, "local");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_SECRET_REF_INVALID")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_ENV_VALUE_INVALID")
    );
    Ok(())
}

#[test]
fn edits_default_environment_without_reformatting_the_file()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    fs::create_dir_all(temporary.path().join(".config"))?;
    fs::create_dir_all(temporary.path().join("api"))?;
    let fixture = fixture_root();
    let original = fs::read_to_string(fixture.join(".config/mm.yaml"))?;
    let source = format!("# retained header\n{original}# retained footer\n");
    fs::write(temporary.path().join(".config/mm.yaml"), source)?;
    fs::copy(
        fixture.join("compose.yaml"),
        temporary.path().join("compose.yaml"),
    )?;
    fs::copy(
        fixture.join("api/.env.example"),
        temporary.path().join("api/.env.example"),
    )?;

    let loader = ConfigLoader;
    loader.set_default_environment(temporary.path(), None)?;
    let without_default = fs::read_to_string(temporary.path().join(".config/mm.yaml"))?;
    assert!(without_default.contains("# retained header"));
    assert!(without_default.contains("# retained footer"));
    assert!(!without_default.contains("default-environment:"));

    loader.set_default_environment(temporary.path(), Some("local"))?;
    let restored = fs::read_to_string(temporary.path().join(".config/mm.yaml"))?;
    assert!(restored.contains("default-environment: local"));
    assert!(restored.contains("# retained header"));
    assert!(restored.contains("# retained footer"));
    Ok(())
}

#[test]
fn inherits_top_level_bindings_and_resolves_url_casts() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    fs::create_dir_all(temporary.path().join(".config"))?;
    fs::create_dir_all(temporary.path().join("app"))?;
    fs::write(
        temporary.path().join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n    expose: [8080]\n",
    )?;
    fs::write(
        temporary.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: bindings
default-environment: preview
target: local
stacks:
  app:
    files: [compose.yaml]
    ingress:
      - domain: web
        service: web
      - domain: mail
        service: web
        https: false
        environments: [preview]
modules:
  app:
    path: app
    services:
      web:
        stack: app
bindings:
  DB.HOST:
    value: database
environments:
  preview:
    domains:
      web: app.preview.test
      mail: mail.preview.test
    bindings:
      APP.DEBUG:
        value: true
      APP.URL:
        cast: url
        value-ref: domains.web
      MAIL.URL:
        cast: url
        value-ref: domains.mail
  integration:
    domains:
      web: app.integration.test
    bindings:
      APP.DEBUG:
        value: false
      APP.URL:
        cast: url
        value-ref: domains.web
"#,
    )?;
    let document = ConfigLoader.load_from(temporary.path())?;
    let preview = validate(&document, "preview");
    assert!(preview.is_valid(), "diagnostics: {:?}", preview.diagnostics);
    assert_eq!(
        document.config.environments["preview"].domains["web"],
        "app.preview.test"
    );
    let integration = validate(&document, "integration");
    assert!(
        integration.is_valid(),
        "diagnostics: {:?}",
        integration.diagnostics
    );
    Ok(())
}

#[test]
fn rejects_unknown_domains_casts_allowlists_and_https_conflicts()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    fs::create_dir_all(temporary.path().join(".config"))?;
    fs::write(
        temporary.path().join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n",
    )?;
    fs::write(
        temporary.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: invalid-ingress
target: local
stacks:
  app:
    files: [compose.yaml]
    ingress:
      - domain: missing
        service: web
      - domain: web
        service: web
      - domain: web
        route: /cp
        service: web
        https: false
      - domain: web
        route: /mail
        service: web
        environments: [unknown]
modules:
  app:
    path: app
    services:
      web:
        stack: app
environments:
  preview:
    domains:
      web: app.preview.test
    bindings:
      APP.URL:
        cast: url
        value-ref: bindings.web
  qa:
    domains:
      web: app.qa.test
    bindings: {}
"#,
    )?;
    let document = ConfigLoader.load_from(temporary.path())?;
    let report = validate(&document, "preview");
    let codes = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"MM_INGRESS_DOMAIN_UNKNOWN"));
    assert!(codes.contains(&"MM_INGRESS_ENVIRONMENT_UNKNOWN"));
    assert!(codes.contains(&"MM_ENV_VALUE_REF_INVALID"));
    assert!(codes.contains(&"MM_INGRESS_HTTPS_CONFLICT"));
    Ok(())
}

#[test]
fn validates_local_ingress_slugs_collisions_and_reserved_hosts()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    fs::create_dir_all(temporary.path().join(".config"))?;
    fs::create_dir_all(temporary.path().join("app"))?;
    fs::write(
        temporary.path().join("compose.yaml"),
        "services:\n  web_api:\n    image: example/web\n    expose: [8080]\n  web-api:\n    image: example/web\n    expose: [8081]\n",
    )?;
    fs::write(
        temporary.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: collisions
  development-environment: development
target: local
stacks:
  app:
    files: [compose.yaml]
    ingress:
      domain: reserved
      service: web_api
modules:
  app:
    path: app
    services:
      web_api:
        stack: app
      web-api:
        stack: app
environments:
  development:
    domains:
      reserved: mail.mm.test
    bindings: {}
"#,
    )?;

    let document = ConfigLoader.load_from(temporary.path())?;
    let report = validate(&document, "development");
    let codes = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"MM_SERVICE_SLUG_COLLISION"));
    assert!(codes.contains(&"MM_INGRESS_HOST_RESERVED"));
    Ok(())
}

#[test]
fn allows_same_owner_nested_prefixes_and_rejects_duplicates()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    fs::create_dir_all(temporary.path().join(".config"))?;
    fs::create_dir_all(temporary.path().join("app"))?;
    fs::write(
        temporary.path().join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n    expose: [8080]\n  cp:\n    image: example/cp\n    expose: [8081]\n",
    )?;
    fs::write(
        temporary.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: nested
target: local
stacks:
  app:
    files: [compose.yaml]
    ingress:
      - domain: web
        service: web
      - domain: web
        route: /cp
        service: cp
      - domain: web
        route: /proxy-image
        service: web
modules:
  app:
    path: app
    services:
      web:
        stack: app
      cp:
        stack: app
environments:
  local:
    domains:
      web: web.example.test
    bindings: {}
"#,
    )?;
    let document = ConfigLoader.load_from(temporary.path())?;
    let report = validate(&document, "local");
    assert!(report.is_valid(), "diagnostics: {:?}", report.diagnostics);

    let duplicate = fs::read_to_string(temporary.path().join(".config/mm.yaml"))? + "";
    let _ = duplicate;
    let mut config = fs::read_to_string(temporary.path().join(".config/mm.yaml"))?;
    config = config.replacen(
        "        service: web\n",
        "        service: web\n      - domain: web\n        service: cp\n",
        1,
    );
    fs::write(temporary.path().join(".config/mm.yaml"), config)?;
    let document = ConfigLoader.load_from(temporary.path())?;
    let report = validate(&document, "local");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_INGRESS_ROUTE_CONFLICT")
    );
    Ok(())
}

fn initialized_project_without_compose() -> Result<TempDir, Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    ConfigLoader.initialize(temporary.path(), Some("test-project"))?;
    Ok(temporary)
}

#[test]
fn initialization_recovers_a_fully_published_project_and_rejects_a_tampered_marker()
-> Result<(), Box<dyn std::error::Error>> {
    const MARKER_CONTENT: &[u8] = b"mm Ansible starter publication is incomplete\n";

    let project = temp_dirs::tempdir()?;
    let initialized = ConfigLoader.initialize(project.path(), Some("recoverable"))?;
    let marker = project.path().join(".config/ansible/.mm-initializing");
    fs::write(&marker, MARKER_CONTENT)?;
    let recovered = ConfigLoader.initialize(project.path(), Some("ignored"))?;
    assert_eq!(recovered.document.config.project.name, "recoverable");
    assert!(!marker.exists());
    for starter in initialized.starter_files {
        assert!(starter.is_file());
    }

    fs::write(&marker, b"tampered\n")?;
    assert!(ConfigLoader.initialize(project.path(), None).is_err());
    assert!(marker.exists());
    Ok(())
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal")
}

fn config_with_ingress(ingress: &str) -> String {
    format!(
        "schema-version: 1\nproject:\n  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3\n  name: ingress-shapes\ntarget: local\nstacks:\n  app:\n    files: [compose.yaml]\n{ingress}environments:\n  local:\n    bindings: {{}}\n"
    )
}
