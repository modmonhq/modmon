use super::*;
use crate::domain::config::MmConfig;
use std::path::PathBuf;

fn document() -> Result<ConfigDocument, serde_yaml_ng::Error> {
    Ok(ConfigDocument {
        config: serde_yaml_ng::from_str::<MmConfig>(
            "schema-version: 1\nproject:\n  id: 00000000-0000-0000-0000-000000000001\n  name: environments\ntarget: local\nstacks: {}\nenvironments:\n  local:\n    bindings: {}\n  production:\n    bindings: {}\n",
        )?,
        root: PathBuf::from("/workspace"),
        path: PathBuf::from("/workspace/mm.yaml"),
        sources: Default::default(),
        source_hash: "a".repeat(64),
        unknown_keys: Vec::new(),
    })
}

#[test]
fn selected_environment_reports_empty_and_unknown_catalogs_with_context()
-> Result<(), serde_yaml_ng::Error> {
    let configured = document()?;
    let mut report = ValidationReport::default();
    validate_selected_environment(&ValidationContext::new(&configured, "absent"), &mut report);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "MM_ENVIRONMENT_UNKNOWN");
    assert_eq!(
        report.diagnostics[0].help.as_deref(),
        Some("available environments: local, production")
    );

    let mut empty = document()?;
    empty.config.environments.clear();
    let mut report = ValidationReport::default();
    validate_selected_environment(&ValidationContext::new(&empty, "local"), &mut report);
    assert_eq!(report.diagnostics[0].code, "MM_ENVIRONMENTS_EMPTY");
    assert_eq!(
        report.diagnostics[0].path.as_deref(),
        Some(empty.path.as_path())
    );
    Ok(())
}
