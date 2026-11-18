use super::*;
use crate::domain::config::MmConfig;
use std::path::PathBuf;

const CONFIG: &str = r"schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: shop
target: local
stacks:
  app:
    files: [compose.yaml]
environments:
  local:
    bindings: {}
  production:
    target: production
    bindings: {}
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
      - {name: db-1, host: 203.0.113.13}
    cluster:
      groups: {web: [app-1, app-2]}
modules:
  shop:
    path: .
    services:
      web: {stack: app, placement: web}
      worker: {stack: app, placement: [db-1, all]}
      database: {stack: app, placement: PLACEMENT}
      preview: {stack: app, environments: [local], placement: nowhere}
";

fn problems(placement: &str, environment: &str) -> Result<Vec<String>, serde_yaml_ng::Error> {
    let document = ConfigDocument {
        config: serde_yaml_ng::from_str::<MmConfig>(&CONFIG.replace("PLACEMENT", placement))?,
        root: PathBuf::from("/workspace"),
        path: PathBuf::from("/workspace/mm.yaml"),
        sources: Default::default(),
        source_hash: "a".repeat(64),
    };
    let mut report = ValidationReport::default();
    validate_placement(&ValidationContext::new(&document, environment), &mut report);
    Ok(report
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            assert_eq!(diagnostic.code, "MM_PLACEMENT");
            diagnostic.message
        })
        .collect())
}

#[test]
fn nodes_groups_all_and_lists_resolve_on_the_cluster_target() -> Result<(), serde_yaml_ng::Error> {
    assert_eq!(problems("db-1", "production")?, Vec::<String>::new());
    Ok(())
}

#[test]
fn an_unknown_word_names_the_service_and_the_choices() -> Result<(), serde_yaml_ng::Error> {
    let problems = problems("managers", "production")?;
    assert_eq!(problems.len(), 1, "{problems:?}");
    let problem = &problems[0];
    assert!(problem.contains("service 'shop.database'"), "{problem}");
    assert!(
        problem.contains("'managers' is not a node or group"),
        "{problem}"
    );
    assert!(problem.contains("nodes: app-1, app-2, db-1"), "{problem}");
    assert!(problem.contains("groups: web"), "{problem}");
    Ok(())
}

#[test]
fn a_single_host_target_does_not_check_placement() -> Result<(), serde_yaml_ng::Error> {
    assert_eq!(problems("managers", "local")?, Vec::<String>::new());
    Ok(())
}
