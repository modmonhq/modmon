use std::fs;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use mm::adapters::config::ConfigLoader;
use mm::app::ports::BuildSourceInspector;
use mm::app::{BuildManager, BuildRequest, WorkspaceSelection};
use mm::domain::build::{BuildSource, ImageBuildPlan};
use mm::domain::error::MmError;
use tempfile::TempDir;

mod temp_dirs;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

const CONFIG: &str = r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de5
  name: shop
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  base:
    path: base
    build:
      image: ghcr.io/example/base
  tools:
    path: tools
    build:
      image: ghcr.io/example/tools
  api:
    path: api
    services:
      api:
        stack: app
    build:
      image: ghcr.io/example/api
  devtool:
    path: devtool
    services:
      devtool:
        stack: app
        environments: [local]
    build:
      image: ghcr.io/example/devtool
      depends-on:
        tools: TOOLS_IMAGE
  worker:
    path: worker
    services:
      worker:
        stack: app
        environments: [staging]
    build:
      image: ghcr.io/example/worker
environments:
  local:
    bindings: {}
  staging:
    bindings: {}
"#;

#[derive(Debug)]
struct CleanSource;

#[async_trait]
impl BuildSourceInspector for CleanSource {
    async fn inspect(&self, _project_root: &Path) -> Result<BuildSource, MmError> {
        Ok(BuildSource {
            commit: COMMIT.to_owned(),
            branch: Some("main".to_owned()),
            tags_at_head: Vec::new(),
            repository_tags: Vec::new(),
            tags_complete: true,
            default_branch: true,
            dirty: false,
        })
    }
}

fn project() -> Result<TempDir, Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    let root = project.path();
    fs::create_dir(root.join(".config"))?;
    fs::write(root.join(".config/mm.yaml"), CONFIG)?;
    fs::write(
        root.join("compose.yaml"),
        "services:\n  api: {expose: ['80']}\n  devtool: {expose: ['80']}\n  worker: {expose: ['80']}\n",
    )?;
    for (module, dockerfile) in [
        ("base", "FROM scratch\n"),
        ("tools", "FROM scratch\n"),
        ("api", "FROM ghcr.io/example/base:edge\nCOPY . /app\n"),
        ("devtool", "ARG TOOLS_IMAGE\nFROM ${TOOLS_IMAGE}\n"),
        ("worker", "FROM scratch\n"),
    ] {
        fs::create_dir(root.join(module))?;
        fs::write(root.join(module).join("Dockerfile"), dockerfile)?;
        fs::write(root.join(module).join("main.txt"), module)?;
    }
    Ok(project)
}

fn planner() -> BuildManager {
    BuildManager::new(Arc::new(ConfigLoader), Arc::new(CleanSource))
}

fn modules(plan: &ImageBuildPlan) -> Vec<&str> {
    let mut modules = plan
        .units
        .iter()
        .map(|unit| unit.module.as_str())
        .collect::<Vec<_>>();
    modules.sort_unstable();
    modules
}

#[tokio::test]
async fn a_build_plans_the_modules_its_environment_runs_and_their_bases()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let local = WorkspaceSelection::new(project.path(), Some("local".to_owned()));
    let plan = planner().plan(&local, BuildRequest::new(false, false)).await?;
    assert_eq!(modules(&plan), ["api", "base", "devtool", "tools"]);
    let api = plan
        .units
        .iter()
        .find(|unit| unit.module == "api")
        .ok_or("api is not planned")?;
    assert_eq!(api.dependencies[0].producer_module, "base");

    let staging = WorkspaceSelection::new(project.path(), Some("staging".to_owned()));
    let published = planner().plan(&staging, BuildRequest::new(true, false)).await?;
    assert_eq!(modules(&published), ["api", "base", "worker"]);

    let every = planner().plan(&staging, BuildRequest::new(true, true)).await?;
    assert_eq!(
        modules(&every),
        ["api", "base", "devtool", "tools", "worker"]
    );
    Ok(())
}

#[tokio::test]
async fn a_module_outside_the_environment_cannot_break_its_build()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    fs::write(
        project.path().join("worker/Dockerfile"),
        "ARG UNDECLARED\nFROM ${UNDECLARED}\n",
    )?;
    let local = WorkspaceSelection::new(project.path(), Some("local".to_owned()));
    planner().plan(&local, BuildRequest::new(false, false)).await?;
    let staging = WorkspaceSelection::new(project.path(), Some("staging".to_owned()));
    let error = planner()
        .plan(&staging, BuildRequest::new(true, false))
        .await
        .err()
        .ok_or("a broken module the environment runs was planned")?;
    assert!(error.to_string().contains("UNDECLARED"), "{error}");
    Ok(())
}

#[tokio::test]
async fn an_environment_without_built_modules_names_the_all_flag()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let config = project.path().join(".config/mm.yaml");
    fs::write(
        &config,
        CONFIG.replace(
            "  staging:\n    bindings: {}\n",
            "  staging:\n    bindings: {}\n  docs:\n    bindings: {}\n",
        )
        .replace("      api:\n        stack: app\n", "      api:\n        stack: app\n        environments: [local, staging]\n"),
    )?;
    let docs = WorkspaceSelection::new(project.path(), Some("docs".to_owned()));
    let error = planner()
        .plan(&docs, BuildRequest::new(false, false))
        .await
        .err()
        .ok_or("an environment with no built module was planned")?;
    assert!(
        error.to_string().contains("`mm -e docs build --all`"),
        "{error}"
    );
    Ok(())
}
