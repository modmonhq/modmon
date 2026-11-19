use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::*;
use crate::domain::config::{ConfigDocument, MiddlewareBundleConfig, MmConfig};
use crate::domain::workspace::ProjectId;

fn document(root: &Path) -> Result<ConfigDocument, serde_yaml_ng::Error> {
    let config = serde_yaml_ng::from_str::<MmConfig>(
        "schema-version: 1\n\
         project:\n  id: 00000000-0000-0000-0000-000000000001\n  name: project\n\
         target: local\n\
         stacks: {}\n\
         modules: {}\n\
         environments:\n  development: {}\n",
    )?;
    Ok(ConfigDocument {
        config,
        root: root.to_path_buf(),
        path: root.join(".config/mm.yaml"),
        sources: Default::default(),
        source_hash: "source".to_owned(),
        unknown_keys: Vec::new(),
    })
}

fn bundle(environments: &[&str], file: &str) -> MiddlewareBundleConfig {
    MiddlewareBundleConfig {
        environments: environments
            .iter()
            .map(|environment| (*environment).to_owned())
            .collect(),
        file: PathBuf::from(file),
    }
}

fn middleware_file(name: &str, regex: &str, replacement: &str) -> String {
    format!(
        "http:\n  middlewares:\n    '{name}':\n      replacePathRegex:\n        regex: '{regex}'\n        replacement: '{replacement}'\n"
    )
}

fn environment_message(error: MmError) -> String {
    match error {
        MmError::Environment { message } => message,
        other => other.to_string(),
    }
}

#[test]
fn parser_accepts_only_bounded_named_replace_path_regex_definitions() -> Result<(), MmError> {
    let path = Path::new("middlewares.yaml");
    let parsed =
        parse_middleware_file(path, middleware_file("strip-api", "^/api", "/").as_bytes())?;
    assert_eq!(
        parsed.get("strip-api"),
        Some(&CanonicalMiddleware::ReplacePathRegex {
            regex: "^/api".to_owned(),
            replacement: "/".to_owned(),
        })
    );

    for name in [
        "",
        "-leading",
        "trailing-",
        "Uppercase",
        "under_score",
        &"a".repeat(65),
    ] {
        let source = middleware_file(name, "^/api", "/");
        let error = parse_middleware_file(path, source.as_bytes())
            .err()
            .map(environment_message)
            .unwrap_or_default();
        assert!(error.contains("invalid middleware name"));
    }

    for source in [
        "not: [valid",
        "http:\n  middlewares:\n    auth:\n      forwardAuth: {}\n",
        "http:\n  middlewares:\n    empty: {}\n",
    ] {
        assert!(parse_middleware_file(path, source.as_bytes()).is_err());
    }
    let (definitions, ignored) = read_middleware_file(
        path,
        b"http:\n  middlewares: {}\n  routers: {}\nunknown: true\n",
    )?;
    assert!(definitions.is_empty());
    let unknown = middleware_unknown_keys(Path::new("ingress/middlewares.yaml"), &ignored);
    let keys = unknown
        .iter()
        .map(|key| key.key.as_str())
        .collect::<Vec<_>>();
    assert_eq!(keys, ["http.routers", "unknown"]);
    assert!(
        unknown
            .iter()
            .all(|key| key.file == "ingress/middlewares.yaml")
    );

    let long = "a".repeat(4097);
    for value in ["", "\n", long.as_str()] {
        for field in ["regex", "replacement"] {
            let error = validate_middleware_string(value, path, "rewrite", field)
                .err()
                .map(environment_message)
                .unwrap_or_default();
            assert!(error.contains(&format!("invalid {field}")));
        }
    }

    assert!(parse_middleware_file(path, middleware_file("rewrite", "", "/").as_bytes()).is_err());
    assert!(
        parse_middleware_file(path, middleware_file("rewrite", "^/api", "").as_bytes()).is_err()
    );
    Ok(())
}

#[test]
fn bundle_activation_and_namespacing_are_exact() {
    assert!(bundle_is_active(&[], "development"));
    assert!(bundle_is_active(
        &["production".to_owned(), "development".to_owned()],
        "development"
    ));
    assert!(!bundle_is_active(&["production".to_owned()], "development"));
    assert_eq!(
        namespaced_middleware_name(ProjectId(Uuid::from_u128(1)), "routing", "strip-api"),
        "mm-00000000000000000000000000000001-routing-strip-api"
    );
}

#[test]
fn loader_reads_only_active_regular_files_and_binds_their_digest()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let active_source = middleware_file("strip", "^/api", "/");
    fs::write(temporary.path().join("active.yaml"), &active_source)?;
    let mut document = document(temporary.path())?;
    document.config.ingress.middlewares = BTreeMap::from([
        ("active".to_owned(), bundle(&[], "active.yaml")),
        (
            "inactive".to_owned(),
            bundle(&["production"], "does-not-exist.yaml"),
        ),
    ]);

    let loaded = load_active_middlewares(&document, "development")?;
    let name = namespaced_middleware_name(document.config.project.id, "active", "strip");
    assert_eq!(loaded.names.as_slice(), std::slice::from_ref(&name));
    assert!(loaded.definitions.contains_key(&name));
    assert_eq!(loaded.inputs[0].bundle, "active");
    assert_eq!(loaded.inputs[0].path, PathBuf::from("active.yaml"));
    assert_eq!(
        loaded.inputs[0].digest,
        blake3::hash(active_source.as_bytes()).to_hex().to_string()
    );
    Ok(())
}

#[test]
fn loader_rejects_missing_empty_oversized_and_symlinked_active_files()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let mut document = document(temporary.path())?;
    document
        .config
        .ingress
        .middlewares
        .insert("bundle".to_owned(), bundle(&[], "middleware.yaml"));

    let error = load_active_middlewares(&document, "development")
        .err()
        .map(environment_message)
        .unwrap_or_default();
    assert!(error.contains("file does not exist"));

    fs::write(
        temporary.path().join("middleware.yaml"),
        "http:\n  middlewares: {}\n",
    )?;
    let error = load_active_middlewares(&document, "development")
        .err()
        .map(environment_message)
        .unwrap_or_default();
    assert!(error.contains("defines no middlewares"));

    fs::write(
        temporary.path().join("middleware.yaml"),
        vec![b'a'; MAX_MIDDLEWARE_FILE_BYTES + 1],
    )?;
    assert!(load_active_middlewares(&document, "development").is_err());

    fs::write(temporary.path().join("middleware.yaml"), "http: [malformed")?;
    assert!(load_active_middlewares(&document, "development").is_err());

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        fs::remove_file(temporary.path().join("middleware.yaml"))?;
        fs::write(
            temporary.path().join("target.yaml"),
            middleware_file("strip", "^/api", "/"),
        )?;
        symlink("target.yaml", temporary.path().join("middleware.yaml"))?;
        assert!(load_active_middlewares(&document, "development").is_err());
    }
    Ok(())
}
