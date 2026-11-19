use super::validate_relative_path;
use crate::app::config_validation::validate;
use crate::domain::config::{
    ConfigDocument, EnvScalar, EnvironmentBinding, MmConfig, ValueBinding,
};
use crate::domain::diagnostic::ValidationReport;
use std::path::{Path, PathBuf};

#[test]
fn malformed_secret_and_binding_contract_reports_ownership_reference_and_cast_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: secret-validation
target: local
stacks: {}
secrets:
  "bad name!":
    storage:
      provider: undeclared
      location: shared/location
    generate:
      preset: password
      length: 1
    input:
      prompt: " "
    rotation:
      interval-days: 0
  duplicate:
    storage:
      provider: undeclared
      location: shared/location
    input:
      prompt: "unsafe\tcontrol"
  bad-env-location:
    storage:
      provider: env
      location: bad-name
  reserved-env-location:
    storage:
      provider: env
      location: PATH
  unused:
    storage:
      provider: age
      location: local/unused
bindings:
  lower.key:
    secret: absent
environments:
  "bad env":
    target: ../bad
    build-variant: bad/variant
    domains:
      "bad key": Upper_Host
    bindings:
      UNKNOWN.DOMAIN:
        cast: url
        value-ref: domains.missing
      MALFORMED.REF:
        cast: url
        value-ref: not-a-domain-reference
"#;
    let mut config = serde_yaml_ng::from_str::<MmConfig>(source)?;
    config.bindings.insert(
        "CONTROL.VALUE".to_owned(),
        EnvironmentBinding::Value(ValueBinding {
            value: EnvScalar::String("line one\nline two".to_owned()),
        }),
    );
    let document = ConfigDocument {
        config,
        root: PathBuf::from("/workspace"),
        path: PathBuf::from("/workspace/mm.yaml"),
        sources: Default::default(),
        source_hash: "a".repeat(64),
        unknown_keys: Vec::new(),
    };
    let report = validate(&document, "bad env");
    let codes = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for expected in [
        "MM_SECRET_NAME_INVALID",
        "MM_SECRET_REF_INVALID",
        "MM_SECRET_PROVIDER_DUPLICATE",
        "MM_SECRET_SOURCE_CONFLICT",
        "MM_SECRET_GENERATOR_INVALID",
        "MM_SECRET_PROMPT_INVALID",
        "MM_SECRET_ROTATION_INVALID",
        "MM_SECRET_UNUSED",
        "MM_NAME_INVALID",
        "MM_INGRESS_HOST_INVALID",
        "MM_TARGET_NAME_INVALID",
        "MM_BUILD_VARIANT_INVALID",
        "MM_ENV_KEY_INVALID",
        "MM_SECRET_UNKNOWN",
        "MM_ENV_VALUE_INVALID",
        "MM_ENV_VALUE_REF_INVALID",
    ] {
        assert!(
            codes.contains(expected),
            "missing diagnostic {expected}: {codes:?}"
        );
    }
    Ok(())
}

#[test]
fn relative_paths_reject_absolute_and_parent_components_but_allow_normalized_project_paths() {
    let config_path = Path::new("mm.yaml");
    let mut report = ValidationReport::default();
    validate_relative_path(Path::new("modules/api"), "module", config_path, &mut report);
    assert!(report.diagnostics.is_empty());
    validate_relative_path(Path::new("../outside"), "module", config_path, &mut report);
    validate_relative_path(Path::new("/absolute"), "module", config_path, &mut report);
    assert_eq!(report.diagnostics.len(), 2);
    assert!(
        report
            .diagnostics
            .iter()
            .all(|item| item.code == "MM_PATH_INVALID")
    );
}

#[test]
fn an_environment_certificate_names_declared_secrets() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000002
  name: provided-tls
target: local
stacks: {}
secrets:
  tls.certificate:
    storage: {provider: age, location: local/tls-certificate}
environments:
  integration:
    tls: {certificate: tls.certificate, key: tls.key}
";
    let document = |config: MmConfig| ConfigDocument {
        config,
        root: PathBuf::from("/workspace"),
        path: PathBuf::from("/workspace/mm.yaml"),
        sources: Default::default(),
        source_hash: "a".repeat(64),
        unknown_keys: Vec::new(),
    };
    let report = validate(&document(serde_yaml_ng::from_str(source)?), "integration");
    let refused = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "MM_ENVIRONMENT_TLS_INVALID")
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(
        refused[0].contains("environments.integration.tls.key")
            && refused[0].contains("'tls.key'")
            && refused[0].contains("mm secret set tls.key --stdin"),
        "{refused:?}"
    );
    // A secret the certificate names is used.
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_SECRET_UNUSED"),
        "{:?}",
        report.diagnostics
    );
    Ok(())
}
