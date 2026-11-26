use std::path::PathBuf;

use super::*;
use crate::app::environment::secret_defaults::secrets_covered_by_defaults;
use crate::domain::config::{MmConfig, SecretBinding};
use crate::domain::environment::{ModuleEnvironmentIndex, SourceLocation};

fn metadata(status: SecretStatus) -> SecretMetadata {
    SecretMetadata {
        status,
        revision: None,
        created_at_unix_ms: None,
        candidate_revision: None,
        set_by: None,
    }
}

fn bundle(services: Vec<CompiledServiceEnvironment>) -> CompiledEnvironmentBundle {
    CompiledEnvironmentBundle {
        manifest: CompiledEnvironmentManifest {
            project_name: "project".to_owned(),
            environment: "development".to_owned(),
            input_hash: "input".to_owned(),
            services: Vec::new(),
        },
        services,
    }
}

fn document() -> Result<ConfigDocument, serde_yaml_ng::Error> {
    let config = serde_yaml_ng::from_str::<MmConfig>(
        "schema-version: 1\n\
             project:\n  id: 00000000-0000-0000-0000-000000000001\n  name: project\n\
             target: local\n\
             stacks:\n  app:\n    files: [compose.yaml]\n\
             modules:\n  api:\n    path: api\n    services:\n      web:\n        stack: app\n        secret-files:\n          /run/secrets/database:\n            secret: database.password\n\
             secrets:\n  database.password:\n    storage:\n      provider: age\n      location: local/database-password\n\
             environments:\n  development:\n    bindings:\n      DB.PASSWORD:\n        secret: database.password\n      CONFIGURED:\n        value: configured\n      URL:\n        cast: url\n        value-ref: domains.missing\n",
    )?;
    Ok(ConfigDocument {
        config,
        root: PathBuf::from("/project"),
        path: PathBuf::from("/project/.config/mm.yaml"),
        sources: Default::default(),
        source_hash: "source".to_owned(),
        unknown_keys: Vec::new(),
    })
}

fn declaration(global_key: &str, required: bool, default: Option<&str>) -> VariableDeclaration {
    VariableDeclaration {
        name: global_key.replace('.', "_"),
        global_key: Some(global_key.to_owned()),
        required,
        default_value: default.map(str::to_owned),
        literal_value: None,
        location: SourceLocation {
            path: PathBuf::from("/project/api/.env.example"),
            line: 1,
            column: 1,
        },
    }
}

fn index(global_key: &str) -> EnvironmentIndex {
    EnvironmentIndex {
        environment: "development".to_owned(),
        input_hash: "input".to_owned(),
        modules: vec![ModuleEnvironmentIndex {
            module: "api".to_owned(),
            template_path: PathBuf::from("api/.env.example"),
            services: Vec::new(),
            variables: vec![declaration(global_key, false, None)],
        }],
        diagnostics: Vec::new(),
    }
}

#[test]
fn provider_diagnostics_distinguish_each_actionable_status() {
    let mut diagnostics = Vec::new();
    for status in [
        SecretStatus::ProviderUnavailable,
        SecretStatus::AccessDenied,
        SecretStatus::Invalid,
        SecretStatus::Set,
        SecretStatus::Unset,
        SecretStatus::NotChecked,
    ] {
        append_secret_diagnostic("provider", &metadata(status), None, &mut diagnostics);
    }
    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        [
            "MM_SECRET_PROVIDER_UNAVAILABLE",
            "MM_SECRET_ACCESS_DENIED",
            "MM_SECRET_INVALID",
        ]
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.message.contains("provider"))
    );
}

#[test]
fn compiled_service_selection_requires_an_unambiguous_module() -> Result<(), MmError> {
    let same = bundle(vec![
        CompiledServiceEnvironment::new(
            "app".to_owned(),
            "stack".to_owned(),
            "web".to_owned(),
            b"VALUE=one".to_vec(),
        ),
        CompiledServiceEnvironment::new(
            "app".to_owned(),
            "stack".to_owned(),
            "worker".to_owned(),
            b"VALUE=one".to_vec(),
        ),
    ]);
    assert_eq!(select_compiled_service(&same, "app", None)?.service, "web");
    assert_eq!(
        select_compiled_service(&same, "app", Some("worker"))?.service,
        "worker"
    );
    assert!(select_compiled_service(&same, "app", Some("missing")).is_err());
    assert!(select_compiled_service(&same, "missing", None).is_err());

    let different = bundle(vec![
        CompiledServiceEnvironment::new(
            "app".to_owned(),
            "stack".to_owned(),
            "web".to_owned(),
            b"VALUE=one".to_vec(),
        ),
        CompiledServiceEnvironment::new(
            "app".to_owned(),
            "stack".to_owned(),
            "worker".to_owned(),
            b"VALUE=two".to_vec(),
        ),
    ]);
    assert!(select_compiled_service(&different, "app", None).is_err());
    Ok(())
}

#[test]
fn index_validation_counts_only_errors() {
    let warning = ValidationReport {
        diagnostics: vec![Diagnostic::warning("MM_WARNING", "warning")],
    };
    assert!(ensure_index_valid(&warning).is_ok());
    let invalid = ValidationReport {
        diagnostics: vec![
            Diagnostic::error("MM_FIRST", "first"),
            Diagnostic::warning("MM_WARNING", "warning"),
            Diagnostic::error("MM_SECOND", "second"),
        ],
    };
    assert!(matches!(
        ensure_index_valid(&invalid),
        Err(MmError::Validation { errors: 2, .. })
    ));
}

#[test]
fn used_secret_discovery_is_deduplicated_and_rejects_missing_declarations()
-> Result<(), Box<dyn std::error::Error>> {
    let mut document = document()?;
    let references = used_secret_references(&document, "development", &index("DB.PASSWORD"))?;
    assert_eq!(references.len(), 1);
    assert!(
        references
            .keys()
            .next()
            .is_some_and(|key| key.starts_with("age/"))
    );

    let environment = document.config.environments.get_mut("development");
    assert!(environment.is_some());
    if let Some(environment) = environment {
        environment.bindings.insert(
            "MISSING".to_owned(),
            EnvironmentBinding::Secret(SecretBinding {
                secret: "undeclared".to_owned(),
            }),
        );
    }
    let error = used_secret_references(&document, "development", &index("MISSING"))
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default()
        .to_string();
    assert!(error.contains("undeclared secret 'undeclared'"));
    Ok(())
}

#[test]
fn variable_status_preserves_precedence_and_failure_semantics() -> Result<(), serde_yaml_ng::Error>
{
    let mut document = document()?;
    let service_overrides = BTreeMap::new();
    let secret_metadata = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let empty = BTreeMap::new();
    {
        let mut evaluate = |declaration: &VariableDeclaration,
                            overrides: &BTreeMap<String, EnvironmentValueOverride>,
                            check_secrets| {
            variable_status(VariableStatusRequest {
                document: &document,
                environment: "development",
                service: "web",
                declaration,
                service_overrides: &service_overrides,
                overrides,
                secret_metadata: &secret_metadata,
                check_secrets,
                diagnostics: &mut diagnostics,
            })
        };

        let configured = evaluate(&declaration("CONFIGURED", false, None), &empty, false);
        assert_eq!(configured.source, ValueSource::ConfigValue);
        assert_eq!(configured.value.as_deref(), Some("configured"));

        let invalid_cast = evaluate(&declaration("URL", false, None), &empty, false);
        assert_eq!(invalid_cast.status, ResolutionStatus::Invalid);
        assert_eq!(invalid_cast.source, ValueSource::Missing);

        let defaulted = evaluate(
            &declaration("DEFAULT", false, Some("fallback")),
            &empty,
            false,
        );
        assert_eq!(defaulted.status, ResolutionStatus::Defaulted);
        assert_eq!(defaulted.value.as_deref(), Some("fallback"));

        let required = evaluate(&declaration("REQUIRED", true, None), &empty, false);
        assert_eq!(required.status, ResolutionStatus::Unset);
    }
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "MM_ENV_REQUIRED_UNRESOLVED" && diagnostic.message.contains("REQUIRED")
    }));

    let secret = document.config.secrets.get_mut("database.password");
    assert!(secret.is_some());
    if let Some(secret) = secret {
        secret.storage.location = "../invalid".to_owned();
    }
    let overrides = BTreeMap::from([(
        "DB.PASSWORD".to_owned(),
        EnvironmentValueOverride {
            key: "DB.PASSWORD".to_owned(),
            value: "override".to_owned(),
            revision: 1,
            updated_at_unix_ms: 1,
        },
    )]);
    let invalid_reference = variable_status(VariableStatusRequest {
        document: &document,
        environment: "development",
        service: "web",
        declaration: &declaration("DB.PASSWORD", false, None),
        service_overrides: &service_overrides,
        overrides: &overrides,
        secret_metadata: &secret_metadata,
        check_secrets: true,
        diagnostics: &mut diagnostics,
    });
    assert_eq!(invalid_reference.source, ValueSource::SqliteOverride);
    assert_eq!(invalid_reference.value.as_deref(), Some("override"));
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_SECRET_REFERENCE_INVALID")
    );
    Ok(())
}

fn defaults_document() -> Result<ConfigDocument, serde_yaml_ng::Error> {
    let config = serde_yaml_ng::from_str::<MmConfig>(
        "schema-version: 1\n\
             project:\n  id: 00000000-0000-0000-0000-000000000001\n  name: project\n\
             target: local\n\
             stacks:\n  app:\n    files: [compose.yaml]\n\
             modules:\n  api:\n    path: api\n    services:\n      web:\n        stack: app\n  worker:\n    path: worker\n    services:\n      jobs:\n        stack: app\n        secret-files:\n          /run/secrets/key:\n            secret: files.key\n\
             secrets:\n  cache.password:\n    storage:\n      provider: age\n      location: local/cache\n  files.key:\n    storage:\n      provider: age\n      location: local/files\n  unused.token:\n    storage:\n      provider: age\n      location: local/unused\n\
             environments:\n  development:\n    bindings:\n      CACHE.PASSWORD:\n        secret: cache.password\n      FILES.KEY:\n        secret: files.key\n",
    )?;
    Ok(ConfigDocument {
        config,
        root: PathBuf::from("/project"),
        path: PathBuf::from("/project/.config/mm.yaml"),
        sources: Default::default(),
        source_hash: "source".to_owned(),
        unknown_keys: Vec::new(),
    })
}

fn defaults_index(
    api: Vec<VariableDeclaration>,
    worker: Vec<VariableDeclaration>,
) -> EnvironmentIndex {
    let module = |module: &str, service: &str, variables| ModuleEnvironmentIndex {
        module: module.to_owned(),
        template_path: PathBuf::from(format!("{module}/.env.example")),
        services: vec![crate::domain::environment::StackService {
            stack: "app".to_owned(),
            name: service.to_owned(),
        }],
        variables,
    };
    EnvironmentIndex {
        environment: "development".to_owned(),
        input_hash: "input".to_owned(),
        modules: vec![module("api", "web", api), module("worker", "jobs", worker)],
        diagnostics: Vec::new(),
    }
}

fn covered(
    index: &EnvironmentIndex,
    status: SecretStatus,
) -> Result<BTreeSet<String>, serde_yaml_ng::Error> {
    let metadata = ["cache.password", "files.key", "unused.token"]
        .into_iter()
        .map(|name| (name.to_owned(), metadata(status)))
        .collect();
    Ok(secrets_covered_by_defaults(
        &defaults_document()?,
        "development",
        index,
        &metadata,
    ))
}

#[test]
fn an_unset_secret_is_defaulted_only_when_every_reader_has_a_default()
-> Result<(), serde_yaml_ng::Error> {
    let cache = |default| declaration("CACHE.PASSWORD", false, default);
    let both = defaults_index(vec![cache(Some("devpass"))], vec![cache(Some("other"))]);
    assert_eq!(
        covered(&both, SecretStatus::Unset)?,
        BTreeSet::from(["cache.password".to_owned()])
    );

    let one_missing = defaults_index(vec![cache(Some("devpass"))], vec![cache(None)]);
    assert!(covered(&one_missing, SecretStatus::Unset)?.is_empty());

    let empty = defaults_index(vec![cache(Some(""))], Vec::new());
    assert!(covered(&empty, SecretStatus::Unset)?.is_empty());

    let required_empty = defaults_index(
        vec![declaration("CACHE.PASSWORD", true, Some(""))],
        Vec::new(),
    );
    assert!(covered(&required_empty, SecretStatus::Unset)?.is_empty());

    let unused = defaults_index(Vec::new(), Vec::new());
    assert!(covered(&unused, SecretStatus::Unset)?.is_empty());

    let file = defaults_index(
        vec![declaration("FILES.KEY", false, Some("devkey"))],
        Vec::new(),
    );
    assert!(covered(&file, SecretStatus::Unset)?.is_empty());
    Ok(())
}

#[test]
fn a_stored_or_unknown_secret_is_never_defaulted() -> Result<(), serde_yaml_ng::Error> {
    let cache = |default| declaration("CACHE.PASSWORD", false, default);
    let index = defaults_index(vec![cache(Some("devpass"))], vec![cache(Some("other"))]);
    for status in [
        SecretStatus::Set,
        SecretStatus::NotChecked,
        SecretStatus::ProviderUnavailable,
        SecretStatus::AccessDenied,
        SecretStatus::Invalid,
    ] {
        assert!(covered(&index, status)?.is_empty(), "{status:?}");
    }
    Ok(())
}
