//! The build manager against a scripted source inspector: which modules a
//! plan builds, in which order, with which inputs, and what it refuses.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tempfile::TempDir;

use mm::app::ports::BuildSourceInspector;
use mm::app::{BuildManager, WorkspaceSelection};
use mm::domain::build::{BuildDependencyReference, BuildSource};
use mm::domain::error::MmError;

#[derive(Debug)]
struct FakeSource {
    source: BuildSource,
}

#[derive(Debug)]
struct FailingSource;

#[async_trait]
impl BuildSourceInspector for FailingSource {
    async fn inspect(&self, _project_root: &Path) -> Result<BuildSource, MmError> {
        Err(MmError::Build {
            message: "injected source inspection failure".to_owned(),
        })
    }
}

#[async_trait]
impl BuildSourceInspector for FakeSource {
    async fn inspect(&self, _project_root: &Path) -> Result<BuildSource, MmError> {
        Ok(self.source.clone())
    }
}

fn source(dirty: bool) -> BuildSource {
    BuildSource {
        commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        branch: Some("main".to_owned()),
        tags_at_head: vec!["v26.5.0".to_owned()],
        repository_tags: vec!["v26.4.0".to_owned(), "v26.5.0".to_owned()],
        tags_complete: true,
        default_branch: true,
        dirty,
    }
}

fn build_manager(source: BuildSource) -> BuildManager {
    BuildManager::new(
        Arc::new(mm::adapters::config::ConfigLoader),
        Arc::new(FakeSource { source }),
    )
}

fn project() -> Result<TempDir, Box<dyn std::error::Error>> {
    let project = crate::temp_dirs::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir(project.path().join("website"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-test
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  website:
    path: website
    services:
      app:
        stack: app
    build:
      image: ghcr.io/acme/web
      targets:
        production: ""
        development: dev
      args:
        SENTRY_ORG: acme
        SENTRY_PROJECT: web
      secrets:
        SENTRY_AUTH_TOKEN: build.sentry-auth-token
        composer_auth: build.composer-auth
        gh_pat: build.github-token
secrets:
  build.sentry-auth-token:
    storage:
      provider: env
      location: ACME_SENTRY_AUTH_TOKEN
  build.composer-auth:
    storage:
      provider: env
      location: ACME_COMPOSER_AUTH
  build.github-token:
    storage:
      provider: env
      location: ACME_GITHUB_TOKEN
environments:
  local:
    bindings: {}
"#,
    )?;
    fs::write(
        project.path().join("compose.yaml"),
        "services:\n  app:\n    image: ghcr.io/acme/web:old\n",
    )?;
    fs::write(
        project.path().join("website/Dockerfile"),
        "FROM scratch AS production\nFROM scratch AS development\n",
    )?;
    fs::write(project.path().join("website/app.txt"), "content\n")?;
    Ok(project)
}

#[tokio::test]
async fn plans_build_only_modules_with_authoritative_images()
-> Result<(), Box<dyn std::error::Error>> {
    let project = crate::temp_dirs::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir(project.path().join("php"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-only
default-environment: integration
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  php:
    path: php
    build:
      image: ghcr.io/acme/base-runtime
environments:
  integration:
    bindings: {}
  uat:
    bindings: {}
  preview-a:
    bindings: {}
  qa-west:
    bindings: {}
"#,
    )?;
    fs::write(project.path().join("compose.yaml"), "services: {}\n")?;
    fs::write(project.path().join("php/Dockerfile"), "FROM scratch\n")?;
    let manager = build_manager(source(false));
    let plan = manager
        .plan(
            &WorkspaceSelection::new(project.path(), Some("uat".to_owned())),
            true,
        )
        .await?;
    assert_eq!(plan.units.len(), 1);
    assert_eq!(plan.units[0].module, "php");
    assert_eq!(plan.units[0].image, "ghcr.io/acme/base-runtime");
    assert!(plan.units[0].dependencies.is_empty());
    assert!(!plan.units[0].version_embedded);
    Ok(())
}

#[tokio::test]
async fn plans_topologically_sorted_dependencies_and_extra_inputs()
-> Result<(), Box<dyn std::error::Error>> {
    let project = crate::temp_dirs::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir(project.path().join("php"))?;
    fs::create_dir(project.path().join("web"))?;
    fs::create_dir(project.path().join("shared"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-graph
default-environment: integration
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  web:
    path: web
    services:
      web:
        stack: app
    build:
      image: ghcr.io/acme/web
      depends-on:
        php: MM_PHP_IMAGE
      extra-inputs:
        - shared/extensions.txt
  php:
    path: php
    build:
      image: ghcr.io/acme/base-runtime
environments:
  integration:
    bindings: {}
"#,
    )?;
    fs::write(
        project.path().join("compose.yaml"),
        "services:\n  web:\n    expose: ['80']\n",
    )?;
    fs::write(project.path().join("php/Dockerfile"), "FROM scratch\n")?;
    fs::write(
        project.path().join("web/Dockerfile"),
        "ARG VERSION=dev\nARG MM_PHP_IMAGE\nFROM ${MM_PHP_IMAGE}\nARG VERSION\nENV IMAGE_VERSION=${VERSION}\n",
    )?;
    fs::write(project.path().join("shared/extensions.txt"), "gd\n")?;
    let manager = build_manager(source(false));
    let plan = manager
        .plan(&WorkspaceSelection::new(project.path(), None), true)
        .await?;
    assert_eq!(
        plan.units
            .iter()
            .map(|unit| unit.module.as_str())
            .collect::<Vec<_>>(),
        vec!["php", "web"]
    );
    assert_eq!(plan.units[1].dependencies[0].producer_module, "php");
    assert!(matches!(
        &plan.units[1].dependencies[0].reference,
        BuildDependencyReference::BuildArgument { arg, .. } if arg == "MM_PHP_IMAGE"
    ));
    assert_eq!(
        plan.units[1].extra_inputs[0].path,
        PathBuf::from("shared/extensions.txt")
    );
    assert!(plan.units[1].version_embedded);
    assert!(!plan.units[0].version_embedded);
    assert_ne!(
        plan.units[0].input_fingerprint,
        plan.units[1].input_fingerprint
    );
    plan.validate().map_err(std::io::Error::other)?;
    Ok(())
}

#[tokio::test]
async fn rejects_missing_dependency_arguments_and_cycles() -> Result<(), Box<dyn std::error::Error>>
{
    let project = crate::temp_dirs::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir(project.path().join("php"))?;
    fs::create_dir(project.path().join("web"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-cycle
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  php:
    path: php
    build:
      image: ghcr.io/acme/base-runtime
      depends-on:
        web: MM_WEB_IMAGE
  web:
    path: web
    build:
      image: ghcr.io/acme/web
      depends-on:
        php: MM_PHP_IMAGE
environments:
  local:
    bindings: {}
"#,
    )?;
    fs::write(project.path().join("compose.yaml"), "services: {}\n")?;
    fs::write(
        project.path().join("php/Dockerfile"),
        "ARG MM_WEB_IMAGE\nFROM ${MM_WEB_IMAGE}\n",
    )?;
    fs::write(
        project.path().join("web/Dockerfile"),
        "ARG MM_PHP_IMAGE\nFROM ${MM_PHP_IMAGE}\n",
    )?;
    let manager = build_manager(source(false));
    assert!(matches!(
        manager
            .plan(&WorkspaceSelection::new(project.path(), None), false)
            .await,
        Err(MmError::Validation { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn plans_multi_platform_release_and_development_images()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let manager = build_manager(source(false));

    let plan = manager
        .plan(&WorkspaceSelection::new(project.path(), None), true)
        .await?;

    assert_eq!(
        plan.platforms,
        vec!["linux/amd64".to_owned(), "linux/arm64".to_owned()]
    );
    assert_eq!(plan.units[0].image, "ghcr.io/acme/web");
    assert_eq!(plan.units[0].args["VERSION"], "v26.5.0");
    assert_eq!(plan.units[0].args["SENTRY_ORG"], "acme");
    assert_eq!(plan.units[0].args["SENTRY_PROJECT"], "web");
    assert_eq!(
        plan.units[0].secret_ids,
        vec![
            "SENTRY_AUTH_TOKEN".to_owned(),
            "composer_auth".to_owned(),
            "gh_pat".to_owned(),
        ]
    );
    assert_eq!(
        plan.units[0].variants[0].tags,
        vec![
            "v26.5.0-dev".to_owned(),
            "26.5.0-dev".to_owned(),
            "26.5-dev".to_owned(),
            "latest-dev".to_owned(),
            "sha-0123456789abcdef0123456789abcdef01234567-dev".to_owned(),
        ]
    );
    assert_eq!(
        plan.units[0].variants[1].target.as_deref(),
        Some("production")
    );
    let plan_json = serde_json::to_string(&plan)?;
    assert!(!plan_json.contains("ACME_SENTRY_AUTH_TOKEN"));
    assert!(!plan_json.contains("ACME_COMPOSER_AUTH"));
    assert!(!plan_json.contains("ACME_GITHUB_TOKEN"));
    plan.validate().map_err(std::io::Error::other)?;
    Ok(())
}

#[tokio::test]
async fn ignored_environment_file_never_changes_the_context_hash()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    fs::write(project.path().join("website/.dockerignore"), ".env\n")?;
    fs::write(project.path().join("website/.env"), "SECRET=first\n")?;
    let manager = build_manager(source(false));
    let selection = WorkspaceSelection::new(project.path(), None);

    let before = manager.plan(&selection, false).await?;
    fs::write(project.path().join("website/.env"), "SECRET=second\n")?;
    let after = manager.plan(&selection, false).await?;

    assert_eq!(before.units[0].context_hash, after.units[0].context_hash);
    assert_eq!(before.plan_hash, after.plan_hash);
    Ok(())
}

#[tokio::test]
async fn included_environment_file_and_dirty_publish_are_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    fs::write(project.path().join("website/.env"), "SECRET=value\n")?;
    let clean = build_manager(source(false));

    assert!(matches!(
        clean
            .plan(&WorkspaceSelection::new(project.path(), None), false)
            .await,
        Err(MmError::Security { .. })
    ));

    fs::remove_file(project.path().join("website/.env"))?;
    let dirty = build_manager(source(true));
    assert!(matches!(
        dirty
            .plan(&WorkspaceSelection::new(project.path(), None), true)
            .await,
        Err(MmError::Build { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn identical_first_party_inputs_share_a_canonical_node()
-> Result<(), Box<dyn std::error::Error>> {
    let project = crate::temp_dirs::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir(project.path().join("shared"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-dedup
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  web:
    path: shared
    build:
      image: ghcr.io/example/web
  api:
    path: shared
    build:
      image: ghcr.io/example/api
environments:
  local:
    bindings: {}
"#,
    )?;
    fs::write(project.path().join("compose.yaml"), "services: {}\n")?;
    fs::write(project.path().join("shared/Dockerfile"), "FROM scratch\n")?;
    let manager = build_manager(source(false));
    let plan = manager
        .plan(&WorkspaceSelection::new(project.path(), None), false)
        .await?;
    assert_eq!(plan.units.len(), 2);
    assert_eq!(
        plan.units[0].input_fingerprint,
        plan.units[1].input_fingerprint
    );
    assert_ne!(plan.units[0].image, plan.units[1].image);
    plan.validate().map_err(std::io::Error::other)?;
    Ok(())
}

#[tokio::test]
async fn first_party_from_creates_build_order_and_ambiguous_refs_fail()
-> Result<(), Box<dyn std::error::Error>> {
    let project = crate::temp_dirs::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir(project.path().join("php"))?;
    fs::create_dir(project.path().join("web"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-from
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  web:
    path: web
    build:
      image: ghcr.io/example/web
  php:
    path: php
    build:
      image: ghcr.io/example/php
environments:
  local:
    bindings: {}
"#,
    )?;
    fs::write(project.path().join("compose.yaml"), "services: {}\n")?;
    fs::write(project.path().join("php/Dockerfile"), "FROM scratch\n")?;
    fs::write(
        project.path().join("web/Dockerfile"),
        "FROM ghcr.io/example/php\n",
    )?;
    let manager = build_manager(source(false));
    let plan = manager
        .plan(&WorkspaceSelection::new(project.path(), None), false)
        .await?;
    assert_eq!(
        plan.units
            .iter()
            .map(|unit| unit.module.as_str())
            .collect::<Vec<_>>(),
        vec!["php", "web"]
    );
    assert_eq!(plan.units[1].dependencies[0].producer_module, "php");
    assert!(matches!(
        plan.units[1].dependencies[0].reference,
        BuildDependencyReference::Dockerfile { .. }
    ));

    fs::create_dir(project.path().join("php-b"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-from
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  web:
    path: web
    build:
      image: ghcr.io/example/web
  php:
    path: php
    build:
      image: ghcr.io/example/php
  php-b:
    path: php-b
    build:
      image: ghcr.io/example/php
environments:
  local:
    bindings: {}
"#,
    )?;
    fs::write(project.path().join("php-b/Dockerfile"), "FROM scratch\n")?;
    let error = manager
        .plan(&WorkspaceSelection::new(project.path(), None), false)
        .await
        .err()
        .ok_or("ambiguous first-party image")?;
    let message = error.to_string();
    assert!(message.contains("web/Dockerfile:1"), "{message}");
    assert!(message.contains("php, php-b"), "{message}");

    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-from
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  web:
    path: web
    build:
      image: ghcr.io/example/web
      depends-on:
        php: BASE_IMAGE
        php-b: BASE_IMAGE
  php:
    path: php
    build:
      image: ghcr.io/example/php
  php-b:
    path: php-b
    build:
      image: ghcr.io/example/php-b
environments:
  local:
    bindings: {}
"#,
    )?;
    fs::write(
        project.path().join("web/Dockerfile"),
        "ARG BASE_IMAGE\nFROM ${BASE_IMAGE}\n",
    )?;
    assert!(matches!(
        manager
            .plan(&WorkspaceSelection::new(project.path(), None), false)
            .await,
        Err(MmError::Validation { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn inferred_first_party_cycle_reports_a_path() -> Result<(), Box<dyn std::error::Error>> {
    let project = crate::temp_dirs::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir(project.path().join("php"))?;
    fs::create_dir(project.path().join("web"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-cycle-from
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  php:
    path: php
    build:
      image: ghcr.io/example/php
  web:
    path: web
    build:
      image: ghcr.io/example/web
environments:
  local:
    bindings: {}
"#,
    )?;
    fs::write(project.path().join("compose.yaml"), "services: {}\n")?;
    fs::write(
        project.path().join("php/Dockerfile"),
        "FROM ghcr.io/example/web\n",
    )?;
    fs::write(
        project.path().join("web/Dockerfile"),
        "FROM ghcr.io/example/php\n",
    )?;
    let manager = build_manager(source(false));
    let error = manager
        .plan(&WorkspaceSelection::new(project.path(), None), false)
        .await
        .err()
        .ok_or("cycle")?;
    assert!(error.to_string().contains("php -> web -> php"), "{error}");
    Ok(())
}

#[tokio::test]
async fn planning_propagates_source_policy_and_empty_workspace_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let selection = WorkspaceSelection::new(project.path(), None);
    let empty = crate::temp_dirs::tempdir()?;
    assert!(
        build_manager(source(false))
            .plan(&WorkspaceSelection::new(empty.path(), None), false)
            .await
            .is_err()
    );
    assert!(
        build_manager(source(false))
            .plan(
                &WorkspaceSelection::new(project.path(), Some("missing".to_owned())),
                false
            )
            .await
            .is_err()
    );
    let failing = BuildManager::new(
        Arc::new(mm::adapters::config::ConfigLoader),
        Arc::new(FailingSource),
    );
    assert!(format!("{failing:?}").contains("BuildManager"));
    assert!(matches!(
        failing.plan(&selection, false).await,
        Err(MmError::Build { .. })
    ));

    let mut incomplete = source(false);
    incomplete.tags_complete = false;
    assert!(matches!(
        build_manager(incomplete).plan(&selection, true).await,
        Err(MmError::Build { .. })
    ));

    let mut conflicting = source(false);
    conflicting.tags_at_head = vec!["v1.0.0".to_owned(), "v2.0.0".to_owned()];
    assert!(matches!(
        build_manager(conflicting.clone())
            .plan(&selection, false)
            .await,
        Err(MmError::Build { .. })
    ));
    conflicting.dirty = true;
    assert!(matches!(
        build_manager(conflicting).plan(&selection, false).await,
        Err(MmError::Build { .. })
    ));

    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        config_path,
        config.replace("    build:\n", "    build:\n      enabled: false\n"),
    )?;
    assert!(matches!(
        build_manager(source(false)).plan(&selection, false).await,
        Err(MmError::Build { .. })
    ));
    Ok(())
}
