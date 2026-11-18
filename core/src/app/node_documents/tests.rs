use std::fs;
use std::sync::Arc;

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn project() -> Result<tempfile::TempDir, std::io::Error> {
    let root = crate::test_support::tempdir()?;
    fs::create_dir_all(root.path().join(".config"))?;
    fs::write(
        root.path().join(".config/mm.yaml"),
        "schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: shop
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  shop:
    path: .
    services:
      web: {stack: app, placement: all}
      db: {stack: app, placement: app-2}
targets:
  cluster:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
environments:
  production:
    target: cluster
",
    )?;
    Ok(root)
}

fn documents(node: &str) -> NodeDocuments {
    NodeDocuments::new(
        crate::test_project_documents(),
        NodeScope {
            target: "cluster".to_owned(),
            node: node.to_owned(),
            environment: "production".to_owned(),
            resolver_files: None,
            mesh_addresses: Default::default(),
        },
    )
}

#[test]
fn every_load_is_the_configuration_as_the_node_sees_it() -> TestResult {
    let root = project()?;
    let first = documents("app-1");
    let from_start = first.load_from(root.path())?;
    let from_path = first.load(&first.discover(root.path())?)?;
    for document in [from_start, from_path] {
        let services = &document.config.modules["shop"].services;
        assert!(services["web"].is_active("production"));
        assert!(!services["db"].is_active("production"));
    }
    let second = documents("app-2").load_from(root.path())?;
    assert!(second.config.modules["shop"].services["db"].is_active("production"));
    Ok(())
}

#[test]
fn a_node_the_commit_does_not_list_is_refused_and_configuration_is_never_edited() -> TestResult {
    let root = project()?;
    let error = documents("app-9")
        .load_from(root.path())
        .err()
        .map(|error| (error.code(), error.to_string()));
    let (code, message) = error.unwrap_or_default();
    assert_eq!(code, "MM_PLACEMENT");
    assert!(
        message.contains("'app-9' is not one of the target's nodes"),
        "{message}"
    );

    let store: Arc<dyn ProjectDocumentStore> = Arc::new(documents("app-1"));
    assert!(store.set_default_environment(root.path(), None).is_err());
    Ok(())
}
