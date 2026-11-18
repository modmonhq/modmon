use super::{
    build_dependency_cycle, is_build_identifier, is_oci_platform, looks_like_secret_name,
    looks_sensitive_filename, validate_name, visit_build_dependency,
};
use crate::domain::config::{ConfigDocument, EnvScalar, MmConfig};
use crate::domain::config::{ModuleBuildConfig, ModuleConfig};
use crate::domain::diagnostic::ValidationReport;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn module(dependencies: &[(&str, &str)], enabled: bool) -> ModuleConfig {
    ModuleConfig {
        path: PathBuf::from("module"),
        services: BTreeMap::new(),
        networks: Vec::new(),
        env: None,
        build: Some(ModuleBuildConfig {
            enabled,
            image: Some("example/module:latest".to_owned()),
            depends_on: dependencies
                .iter()
                .map(|(name, argument)| ((*name).to_owned(), (*argument).to_owned()))
                .collect(),
            ..ModuleBuildConfig::default()
        }),
        development_mount: None,
        development_mail: None,
        development: None,
    }
}

#[test]
fn lexical_validators_accept_only_portable_public_identifiers() {
    for value in ["linux/amd64", "linux/arm64/v8", "wasi/wasm32"] {
        assert!(is_oci_platform(value), "platform: {value}");
    }
    for value in [
        "linux",
        "Linux/amd64",
        "linux//amd64",
        "linux/amd64/v8/extra",
        "linux/this-architecture-name-is-far-too-long-for-oci",
    ] {
        assert!(!is_oci_platform(value), "platform: {value}");
    }

    for value in ["release", "release.1", "x86_64", "feature-test"] {
        assert!(is_build_identifier(value), "identifier: {value}");
    }
    for value in ["", "contains/slash", "contains space", &"x".repeat(65)] {
        assert!(!is_build_identifier(value), "identifier: {value}");
    }

    for value in [
        "PASSWORD",
        "DB_PASSPHRASE",
        "AUTH_TOKEN",
        "SERVICE_API_KEY",
        "SSH_PRIVATE_KEY",
    ] {
        assert!(looks_like_secret_name(value), "secret name: {value}");
    }
    for value in ["KEYBOARD", "PUBLIC_KEY", "TOKENIZER", "SECRETARY"] {
        assert!(!looks_like_secret_name(value), "public name: {value}");
    }

    for value in [
        ".env",
        ".env.local",
        "id_rsa",
        "id_ed25519",
        "id_ecdsa",
        "id_dsa",
        "age-identity.txt",
    ] {
        assert!(looks_sensitive_filename(value), "sensitive file: {value}");
    }
    for value in ["env", ".environment", "id_rsa.pub", "compose.yaml"] {
        assert!(!looks_sensitive_filename(value), "public file: {value}");
    }
}

#[test]
fn name_validation_reports_invalid_values_without_rejecting_supported_names() {
    let path = Path::new("mm.yaml");
    let mut report = ValidationReport::default();
    for value in ["app", "App_1", "api-worker"] {
        validate_name(value, "module name", path, &mut report);
    }
    assert!(report.diagnostics.is_empty());

    for value in ["", "contains space", "path/name", "unicode-name-ß"] {
        validate_name(value, "module name", path, &mut report);
    }
    assert_eq!(report.diagnostics.len(), 4);
    assert!(
        report
            .diagnostics
            .iter()
            .all(|item| item.code == "MM_NAME_INVALID" && item.path.as_deref() == Some(path))
    );
}

#[test]
fn dependency_cycle_detection_is_deterministic_and_ignores_disabled_builds() {
    let acyclic = BTreeMap::from([
        ("api".to_owned(), module(&[("base", "BASE_IMAGE")], true)),
        ("base".to_owned(), module(&[], true)),
        (
            "disabled".to_owned(),
            module(&[("disabled", "SELF")], false),
        ),
    ]);
    assert_eq!(build_dependency_cycle(&acyclic), None);

    let cyclic = BTreeMap::from([
        (
            "api".to_owned(),
            module(&[("worker", "WORKER_IMAGE")], true),
        ),
        ("base".to_owned(), module(&[("api", "API_IMAGE")], true)),
        ("worker".to_owned(), module(&[("base", "BASE_IMAGE")], true)),
    ]);
    assert_eq!(
        build_dependency_cycle(&cyclic),
        Some(vec![
            "api".to_owned(),
            "worker".to_owned(),
            "base".to_owned(),
            "api".to_owned(),
        ])
    );

    let graph = BTreeMap::new();
    let mut visiting = BTreeSet::from(["detached"]);
    let mut visited = BTreeSet::new();
    let mut stack = Vec::new();
    assert_eq!(
        visit_build_dependency("detached", &graph, &mut visiting, &mut visited, &mut stack,),
        None
    );
    assert_eq!(
        visit_build_dependency("missing", &graph, &mut visiting, &mut visited, &mut stack,),
        None
    );
}

#[test]
fn malformed_module_contract_reports_each_unsafe_build_service_and_template_shape()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    std::fs::create_dir(temporary.path().join("directory-input"))?;
    let source = r#"
schema-version: 1
default-environment: local
project:
  id: 00000000-0000-0000-0000-000000000001
  name: validation
  development-environment: local
target: local
build:
  platforms: [Linux/AMD64]
  tag-strategy: auto
stacks:
  app:
    files: [missing-compose.yaml]
modules:
  bad module:
    path: missing-module
    networks: [missing-network]
    development-mount:
      source: .
      target: relative
      services: ["!!!"]
    build:
      enabled: true
      image: ""
      context: ../outside
      dockerfile: /absolute-Dockerfile
      default-variant: absent
      targets:
        bad/target: bad/suffix
        second: bad/suffix
      args:
        1INVALID: public
        VERSION: public
        DB_PASSWORD: public
        PUBLIC_IMAGE: public
      secrets:
        bad/id: missing-secret
      credential-only-secrets: [bad/id, bad/id, absent]
      depends-on:
        bad module: SHARED_IMAGE
        disabled: VERSION
        unknown-invalid: 1INVALID
        unknown-conflict: PUBLIC_IMAGE
        unknown-secret: DB_TOKEN
        unknown-shared: SHARED_IMAGE
      extra-inputs: [.env, directory-input, missing-input, missing-input]
    services:
      "!!!":
        stack: missing-stack
        http-port: 0
        local-https: false
        networks: [missing-network]
        env:
          1INVALID: public
        secret-files:
          ../escape:
            secret: missing-secret
            uid: user
            gid: ""
            mode: 0
      "@@@":
        stack: missing-stack
        automatic-local-ingress: false
  duplicate:
    path: missing-duplicate
    services:
      "!!!":
        stack: missing-stack
  disabled:
    path: missing-disabled
    build:
      enabled: false
  template-only:
    path: missing-template
    env:
      template: missing.env
environments:
  local:
    bindings: {}
"#;
    let mut config = serde_yaml_ng::from_str::<MmConfig>(source)?;
    let invalid_module = config
        .modules
        .get_mut("bad module")
        .ok_or("invalid module missing")?;
    invalid_module
        .build
        .as_mut()
        .ok_or("invalid build missing")?
        .args
        .insert(
            "CONTROL".to_owned(),
            EnvScalar::String("public\nvalue".to_owned()),
        );
    invalid_module
        .services
        .get_mut("!!!")
        .ok_or("invalid service missing")?
        .env
        .insert(
            "CONTROL".to_owned(),
            EnvScalar::String("public\nvalue".to_owned()),
        );
    let document = ConfigDocument {
        config,
        root: temporary.path().to_path_buf(),
        path: temporary.path().join("mm.yaml"),
        sources: Default::default(),
        source_hash: "a".repeat(64),
    };
    let report = crate::app::config_validation::validate(&document, "local");
    let codes = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for expected in [
        "MM_BUILD_PLATFORM_INVALID",
        "MM_NAME_INVALID",
        "MM_MODULE_PATH_MISSING",
        "MM_DEVELOPMENT_MOUNT_INVALID",
        "MM_BUILD_IMAGE_REQUIRED",
        "MM_BUILD_DEFAULT_VARIANT_UNKNOWN",
        "MM_BUILD_TARGET_INVALID",
        "MM_BUILD_TAG_SUFFIX_INVALID",
        "MM_BUILD_TAG_SUFFIX_DUPLICATE",
        "MM_BUILD_ARG_INVALID",
        "MM_BUILD_ARG_RESERVED",
        "MM_BUILD_SECRET_ARG_FORBIDDEN",
        "MM_BUILD_ARG_VALUE_INVALID",
        "MM_BUILD_SECRET_ID_INVALID",
        "MM_BUILD_SECRET_UNKNOWN",
        "MM_BUILD_CREDENTIAL_SECRET_UNKNOWN",
        "MM_BUILD_CREDENTIAL_SECRET_DUPLICATE",
        "MM_BUILD_DEPENDENCY_SELF",
        "MM_BUILD_DEPENDENCY_DISABLED",
        "MM_BUILD_DEPENDENCY_UNKNOWN",
        "MM_BUILD_DEPENDENCY_ARG_INVALID",
        "MM_BUILD_DEPENDENCY_ARG_CONFLICT",
        "MM_BUILD_DEPENDENCY_ARG_DUPLICATE",
        "MM_BUILD_EXTRA_INPUT_DUPLICATE",
        "MM_BUILD_EXTRA_INPUT_INVALID",
        "MM_BUILD_EXTRA_INPUT_MISSING",
        "MM_BUILD_EXTRA_INPUT_SENSITIVE",
        "MM_SERVICE_SLUG_INVALID",
        "MM_SERVICE_HTTP_PORT_INVALID",
        "MM_INGRESS_HTTP_OPT_OUT",
        "MM_SERVICE_STACK_UNKNOWN",
        "MM_SERVICE_DUPLICATE",
        "MM_ENV_VARIABLE_INVALID",
        "MM_ENV_VALUE_INVALID",
        "MM_SECRET_FILE_TARGET_INVALID",
        "MM_SECRET_FILE_UNKNOWN",
        "MM_SECRET_FILE_PERMISSIONS_INVALID",
        "MM_ENV_WITHOUT_TEMPLATE",
        "MM_MODULE_SERVICES_EMPTY",
        "MM_ENV_TEMPLATE_MISSING",
    ] {
        assert!(
            codes.contains(expected),
            "missing diagnostic {expected}: {codes:?}"
        );
    }
    Ok(())
}
