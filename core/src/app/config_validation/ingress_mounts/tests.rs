use super::*;
use crate::domain::config::MmConfig;
use std::path::PathBuf;

fn document(root: PathBuf) -> Result<ConfigDocument, serde_yaml_ng::Error> {
    Ok(ConfigDocument {
        config: serde_yaml_ng::from_str::<MmConfig>(
            r#"
schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: middleware
target: local
stacks: {}
ingress:
  middlewares:
    Bad_Name:
      environments: [absent]
      file: missing.yaml
    empty:
      file: empty.yaml
    invalid:
      file: invalid.yaml
    inactive:
      environments: [production]
      file: valid.yaml
environments:
  local:
    bindings: {}
  production:
    bindings: {}
"#,
        )?,
        root: root.clone(),
        path: root.join("mm.yaml"),
        sources: Default::default(),
        source_hash: "a".repeat(64),
    })
}

#[test]
fn middleware_validation_reports_name_allowlist_file_and_schema_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    std::fs::write(
        temporary.path().join("empty.yaml"),
        "http:\n  middlewares: {}\n",
    )?;
    std::fs::write(temporary.path().join("invalid.yaml"), "not: supported\n")?;
    std::fs::write(
        temporary.path().join("valid.yaml"),
        "http:\n  middlewares:\n    strip:\n      replacePathRegex:\n        regex: ^/old/(.*)\n        replacement: /new/$1\n",
    )?;
    let document = document(temporary.path().to_path_buf())?;
    let mut report = ValidationReport::default();
    validate_project_middlewares(&document, "local", &mut report);
    let codes = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<BTreeSet<_>>();
    for expected in [
        "MM_MIDDLEWARE_NAME_INVALID",
        "MM_MIDDLEWARE_ENVIRONMENT_UNKNOWN",
        "MM_MIDDLEWARE_FILE_MISSING",
        "MM_MIDDLEWARE_EMPTY",
        "MM_MIDDLEWARE_INVALID",
    ] {
        assert!(codes.contains(expected), "missing {expected}: {codes:?}");
    }
    assert!(!codes.contains("MM_MIDDLEWARE_UNREADABLE"));
    Ok(())
}

#[test]
fn conflicting_canonical_middleware_definition_is_rejected() -> Result<(), serde_yaml_ng::Error> {
    let document = document(PathBuf::from("/workspace"))?;
    let path = Path::new("/workspace/middleware.yaml");
    let name = namespaced_middleware_name(document.config.project.id, "bundle", "rewrite");
    let mut namespaced = BTreeMap::from([(
        name,
        (
            "owner".to_owned(),
            crate::domain::ingress::CanonicalMiddleware::ReplacePathRegex {
                regex: "^/first/(.*)".to_owned(),
                replacement: "/$1".to_owned(),
            },
        ),
    )]);
    let bytes = b"http:\n  middlewares:\n    rewrite:\n      replacePathRegex:\n        regex: ^/second/(.*)\n        replacement: /$1\n";
    let mut report = ValidationReport::default();
    validate_middleware_definitions(
        &document,
        "bundle",
        path,
        bytes,
        &mut namespaced,
        &mut report,
    );
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "MM_MIDDLEWARE_CONFLICT");
    Ok(())
}
