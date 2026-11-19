use super::*;
use crate::domain::config::MmConfig;
use crate::domain::target::TargetDefinition;
use std::path::PathBuf;
use uuid::Uuid;

fn document() -> Result<ConfigDocument, serde_yaml_ng::Error> {
    Ok(ConfigDocument {
        config: serde_yaml_ng::from_str::<MmConfig>(
            "schema-version: 1\ndefault-environment: local\nproject:\n  id: 00000000-0000-0000-0000-000000000001\n  name: project\n  development-environment: local\ntarget: local\nstacks:\n  app:\n    project-name: demo-{environment}-{stack}-{project-id}\n    files: [deploy/compose.yaml]\n  empty:\n    files: []\nenvironments:\n  local:\n    bindings: {}\n",
        )?,
        root: PathBuf::from("/workspace"),
        path: PathBuf::from("/workspace/mm.yaml"),
        sources: Default::default(),
        source_hash: "a".repeat(64),
        unknown_keys: Vec::new(),
    })
}

#[test]
fn project_validation_reports_invalid_owner_defaults_and_environment_keys()
-> Result<(), serde_yaml_ng::Error> {
    let mut invalid_slug = document()?;
    invalid_slug.config.project.name = "!!!".to_owned();
    let mut slug_report = ValidationReport::default();
    validate_project(
        &ValidationContext::new(&invalid_slug, "local"),
        &mut slug_report,
    );
    assert!(
        slug_report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_PROJECT_SLUG_INVALID")
    );

    let mut document = document()?;
    document.config.project.name = "!!!".to_owned();
    document.config.default_environment = Some("absent".to_owned());
    document.config.project.development_environment = Some("missing".to_owned());
    document.config.target = "Bad Target".to_owned();
    document.config.development_mail.bindings.insert(
        "1INVALID".to_owned(),
        crate::domain::development_mail::DevelopmentMailValue::SmtpHost,
    );
    let mut report = ValidationReport::default();
    validate_project(&ValidationContext::new(&document, "local"), &mut report);
    let codes = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<BTreeSet<_>>();
    for expected in [
        "MM_NAME_INVALID",
        "MM_DEFAULT_ENVIRONMENT_UNKNOWN",
        "MM_DEVELOPMENT_ENVIRONMENT_UNKNOWN",
        "MM_DEVELOPMENT_MAIL_KEY_INVALID",
        "MM_TARGET_NAME_INVALID",
    ] {
        assert!(codes.contains(expected), "missing {expected}: {codes:?}");
    }
    Ok(())
}

#[test]
fn compose_targets_bind_directory_templates_and_remote_identity() -> Result<(), serde_yaml_ng::Error>
{
    let document = document()?;
    assert_eq!(
        compose_project_directory(&document, "app"),
        PathBuf::from("/workspace/deploy")
    );
    assert_eq!(
        compose_project_directory(&document, "empty"),
        PathBuf::from("/workspace")
    );
    let base = compose_project_name(&document, "local", "app");
    assert_eq!(base, "demo-local-app-00000000-0000-0000-0000-000000000001");
    assert!(is_compose_project_name("project_1-local"));
    assert!(!is_compose_project_name("Project"));
    assert!(!is_compose_project_name("-project"));
    assert!(!is_compose_project_name("project!"));

    let target_id = TargetId(Uuid::from_u128(9));
    let target = NamedTarget::new("remote".to_owned(), TargetDefinition::local(target_id));
    let targets = runtime_targets(&document, "local", Some(&target));
    assert_eq!(targets.len(), 2);
    assert!(
        targets
            .iter()
            .all(|item| item.target.as_ref() == Some(&target))
    );
    let app = targets.iter().find(|item| item.stack_name == "app");
    assert!(app.is_some());
    let Some(app) = app else {
        return Ok(());
    };
    assert!(app.project_name.starts_with(&format!("{base}-mm-")));
    assert_eq!(app.compose_files, [PathBuf::from("deploy/compose.yaml")]);
    assert_eq!(app.user_compose_file_count, 1);
    assert_eq!(app.environment, "local");

    let local = runtime_targets(&document, "local", None);
    assert_eq!(local[0].target, None);
    assert!(local.iter().any(|item| item.project_name == base));
    let changed = target_scoped_compose_project_name(
        &base,
        target_id,
        document.config.project.id,
        "production",
        "app",
    );
    assert_ne!(changed, app.project_name);
    assert_eq!(targets[0].artifact_hash, "");
    assert_eq!(targets[0].interpolation_environment.len(), 0);
    assert!(!targets[0].installation_scoped);
    Ok(())
}
