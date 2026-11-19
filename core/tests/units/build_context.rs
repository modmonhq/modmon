//! Which files a build context sends: the ignore file BuildKit would read,
//! its patterns, and the refusals for what would be sent.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tempfile::TempDir;
use zeroize::Zeroizing;

use mm::adapters::build::DockerBuildxBuilder;
use mm::app::ports::{
    BuildSourceInspector, BuildVariantRequest, CommandOutput, CommandSpec, ImageBuilder,
    ProcessRunner,
};
use mm::app::{BuildManager, WorkspaceSelection};
use mm::domain::build::{BuildSource, ContextIgnore, ImageBuildPlan};
use mm::domain::error::MmError;
use mm::domain::operation::OperationId;

const CONFIG: &str = r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de5
  name: monorepo
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  web:
    path: .
    services:
      web:
        stack: app
    build:
      image: registry.example.com/web
      dockerfile: web/Dockerfile
  api:
    path: .
    services:
      api:
        stack: app
    build:
      image: registry.example.com/api
      dockerfile: api/Dockerfile
environments:
  local:
    bindings: {}
"#;

const WEB_ALLOWLIST: &str = "*\n!web\n!ui\n";

#[derive(Debug)]
struct Source;

#[async_trait]
impl BuildSourceInspector for Source {
    async fn inspect(&self, _project_root: &Path) -> Result<BuildSource, MmError> {
        Ok(BuildSource {
            commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            branch: Some("main".to_owned()),
            tags_at_head: Vec::new(),
            repository_tags: Vec::new(),
            tags_complete: true,
            default_branch: true,
            dirty: false,
        })
    }
}

fn monorepo() -> Result<TempDir, Box<dyn std::error::Error>> {
    let project = crate::temp_dirs::tempdir()?;
    let root = project.path();
    for directory in [".config", "web", "ui", "api"] {
        fs::create_dir(root.join(directory))?;
    }
    fs::write(root.join(".config/mm.yaml"), CONFIG)?;
    fs::write(
        root.join("compose.yaml"),
        "services:\n  web:\n    image: registry.example.com/web:old\n  api:\n    image: registry.example.com/api:old\n",
    )?;
    fs::write(root.join(".dockerignore"), "web\nui\n")?;
    fs::write(root.join("web/Dockerfile"), "FROM scratch\nCOPY web ui /\n")?;
    fs::write(root.join("web/Dockerfile.dockerignore"), WEB_ALLOWLIST)?;
    fs::write(root.join("web/index.ts"), "export {};\n")?;
    fs::write(root.join("ui/button.ts"), "export {};\n")?;
    fs::write(root.join("api/Dockerfile"), "FROM scratch\nCOPY api /\n")?;
    fs::write(root.join("api/main.go"), "package main\n")?;
    Ok(project)
}

async fn plan(project: &TempDir) -> Result<ImageBuildPlan, MmError> {
    BuildManager::new(
        Arc::new(mm::adapters::config::ConfigLoader),
        Arc::new(Source),
    )
    .plan(&WorkspaceSelection::new(project.path(), None), true)
    .await
}

async fn hashes(project: &TempDir) -> Result<BTreeMap<String, String>, MmError> {
    Ok(plan(project)
        .await?
        .units
        .into_iter()
        .map(|unit| (unit.module, unit.context_hash))
        .collect())
}

async fn refusal(project: &TempDir) -> Result<String, Box<dyn std::error::Error>> {
    let Err(error) = plan(project).await else {
        return Err("the build plan was not refused".into());
    };
    assert_eq!(error.code(), "MM_SECURITY");
    let refusal = error
        .refusal()
        .ok_or("the refusal is not in mm's own words")?;
    assert!(error.to_string().ends_with(&refusal));
    Ok(refusal)
}

fn excluded(ignore: &ContextIgnore, path: &str) -> bool {
    let mut verdict = None;
    let mut end = 0;
    for part in path.split('/') {
        end += part.len();
        verdict = Some(ignore.verdict(&path[..end], verdict.as_ref()));
        end += 1;
    }
    verdict.is_some_and(|verdict| verdict.excluded())
}

#[test]
fn ignore_patterns_are_anchored_at_the_context_and_matched_as_buildkit_does()
-> Result<(), Box<dyn std::error::Error>> {
    let allowlist = ContextIgnore::parse("# web image\n*\n!web\n!/ui/\n")?;
    assert!(!excluded(&allowlist, "web"));
    assert!(!excluded(&allowlist, "web/src/index.ts"));
    assert!(!excluded(&allowlist, "ui/button.ts"));
    assert!(excluded(&allowlist, "api/main.go"));
    assert!(excluded(&allowlist, ".dockerignore"));
    assert!(allowlist.skips_excluded_directory("api"));

    let anchored = ContextIgnore::parse("node_modules\n*.log\n")?;
    assert!(excluded(&anchored, "node_modules/left-pad/index.js"));
    assert!(!excluded(&anchored, "web/node_modules/left-pad/index.js"));
    assert!(excluded(&anchored, "debug.log"));
    assert!(!excluded(&anchored, "web/debug.log"));

    let anywhere = ContextIgnore::parse("**/node_modules\nweb/**/*.map\nweb/dist/**\n")?;
    assert!(excluded(&anywhere, "node_modules"));
    assert!(excluded(&anywhere, "web/node_modules/left-pad/index.js"));
    assert!(excluded(&anywhere, "web/app.js.map"));
    assert!(excluded(&anywhere, "web/assets/app.js.map"));
    assert!(excluded(&anywhere, "web/dist/index.html"));
    assert!(!excluded(&anywhere, "web/dist"));

    let reincluded = ContextIgnore::parse("web\n!web/public/robots.txt\n")?;
    assert!(excluded(&reincluded, "web/index.ts"));
    assert!(!excluded(&reincluded, "web/public/robots.txt"));
    assert!(!reincluded.skips_excluded_directory("web"));
    assert!(!reincluded.skips_excluded_directory("web/public"));
    assert!(reincluded.skips_excluded_directory("web/src"));

    let wildcard = ContextIgnore::parse("web\n!**/*.md\n")?;
    assert!(!wildcard.skips_excluded_directory("web/src"));
    assert!(!excluded(&wildcard, "web/README.md"));

    let inherited = ContextIgnore::parse("web/*.log\n!web\n")?;
    assert!(excluded(&inherited, "web/debug.log"));
    assert!(!excluded(&inherited, "web/index.ts"));

    for (text, line) in [("web\n!\n", 2), ("web\\\n", 1), ("ok\n\n[a\n", 3)] {
        let Err(invalid) = ContextIgnore::parse(text) else {
            return Err(format!("{text:?} was accepted").into());
        };
        assert_eq!(invalid.line, line, "{text:?}");
    }
    Ok(())
}

#[tokio::test]
async fn a_dockerfile_ignore_file_keeps_other_units_out_of_the_context_hash()
-> Result<(), Box<dyn std::error::Error>> {
    let project = monorepo()?;
    let root = project.path();
    let before = hashes(&project).await?;

    fs::write(root.join("api/main.go"), "package main\n\nfunc main() {}\n")?;
    fs::write(root.join(".dockerignore"), "web\nui\n*.md\n")?;
    let api_changed = hashes(&project).await?;
    assert_eq!(api_changed["web"], before["web"]);
    assert_ne!(api_changed["api"], before["api"]);

    fs::write(root.join("web/index.ts"), "export const ready = true;\n")?;
    let web_changed = hashes(&project).await?;
    assert_ne!(web_changed["web"], api_changed["web"]);
    assert_eq!(web_changed["api"], api_changed["api"]);

    fs::write(root.join("ui/button.ts"), "export const size = 2;\n")?;
    assert_ne!(hashes(&project).await?["web"], web_changed["web"]);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn only_a_symlink_that_would_be_sent_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let project = monorepo()?;
    let root = project.path();
    fs::create_dir_all(root.join("web/.output/public"))?;
    fs::write(root.join("web/.output/public/index.html"), "<p>hi</p>\n")?;
    std::os::unix::fs::symlink(".output/public", root.join("web/dist"))?;

    fs::write(
        root.join("web/Dockerfile.dockerignore"),
        format!("{WEB_ALLOWLIST}web/dist\n"),
    )?;
    plan(&project).await?;

    fs::write(root.join("web/Dockerfile.dockerignore"), WEB_ALLOWLIST)?;
    assert_eq!(
        refusal(&project).await?,
        "symlink web/dist -> .output/public would be sent in build context . (exclude it in web/Dockerfile.dockerignore)"
    );

    fs::remove_file(root.join("web/Dockerfile.dockerignore"))?;
    fs::write(root.join(".dockerignore"), "ui\n")?;
    assert_eq!(
        refusal(&project).await?,
        "symlink web/dist -> .output/public would be sent in build context . (exclude it in api/Dockerfile.dockerignore or .dockerignore)"
    );
    Ok(())
}

#[tokio::test]
async fn a_sensitive_file_that_would_be_sent_is_named_with_its_rule()
-> Result<(), Box<dyn std::error::Error>> {
    let project = monorepo()?;
    let root = project.path();
    fs::write(root.join("web/.env"), "TOKEN=first\n")?;
    assert_eq!(
        refusal(&project).await?,
        "sensitive file web/.env would be sent in build context . (exclude it in web/Dockerfile.dockerignore)"
    );

    fs::write(
        root.join("web/Dockerfile.dockerignore"),
        format!("{WEB_ALLOWLIST}**/.env\n"),
    )?;
    let before = hashes(&project).await?;
    fs::write(root.join("web/.env"), "TOKEN=second\n")?;
    assert_eq!(hashes(&project).await?, before);
    Ok(())
}

#[tokio::test]
async fn an_invalid_ignore_pattern_names_the_file_and_line()
-> Result<(), Box<dyn std::error::Error>> {
    let project = monorepo()?;
    fs::write(
        project.path().join("web/Dockerfile.dockerignore"),
        format!("{WEB_ALLOWLIST}web/[dist\n"),
    )?;
    let Err(error) = plan(&project).await else {
        return Err("an invalid pattern was accepted".into());
    };
    assert_eq!(error.code(), "MM_BUILD");
    assert_eq!(
        error.refusal().as_deref(),
        Some("web/Dockerfile.dockerignore line 4: `web/[dist` is not a valid pattern")
    );
    Ok(())
}

#[derive(Debug, Default)]
struct SnapshotListing {
    files: Mutex<Vec<String>>,
}

#[async_trait]
impl ProcessRunner for SnapshotListing {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let arguments = specification.redacted_record().arguments;
        let failed = |message: &str| MmError::State {
            message: message.to_owned(),
        };
        let metadata = arguments
            .iter()
            .position(|argument| argument == "--metadata-file")
            .and_then(|index| arguments.get(index + 1))
            .ok_or_else(|| failed("the build command has no metadata file"))?;
        fs::write(
            metadata,
            format!(r#"{{"containerimage.digest":"sha256:{}"}}"#, "0".repeat(64)),
        )
        .map_err(|source| MmError::Io {
            context: "writing the fake build metadata".to_owned(),
            source,
        })?;
        let snapshot = PathBuf::from(
            arguments
                .last()
                .ok_or_else(|| failed("the build command has no context"))?,
        );
        let mut files = Vec::new();
        list_files(&snapshot, &snapshot, &mut files)?;
        files.sort();
        *self
            .files
            .lock()
            .map_err(|_| failed("the snapshot listing lock was poisoned"))? = files;
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(String::new()),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

fn list_files(root: &Path, directory: &Path, files: &mut Vec<String>) -> Result<(), MmError> {
    let io = |source| MmError::Io {
        context: format!("listing the build snapshot {}", directory.display()),
        source,
    };
    for entry in fs::read_dir(directory).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if path.is_dir() {
            list_files(root, &path, files)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            files.push(relative.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

async fn snapshot_of(
    project: &TempDir,
    plan: &ImageBuildPlan,
    module: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let unit = plan
        .units
        .iter()
        .find(|unit| unit.module == module)
        .ok_or("the module was not planned")?;
    let artifacts = crate::temp_dirs::tempdir()?;
    let listing = Arc::new(SnapshotListing::default());
    let builder = DockerBuildxBuilder::new(listing.clone(), artifacts.path().join("builds"));
    let operation_id = OperationId::new();
    let _operation = builder.begin_operation(operation_id)?;
    builder
        .build_variant(BuildVariantRequest {
            operation_id,
            project_root: project.path(),
            plan_hash: &plan.plan_hash,
            platforms: &plan.platforms,
            publish: true,
            unit,
            variant: unit.variants.first().ok_or("the unit has no variant")?,
            secrets: &BTreeMap::new(),
            dependencies: &[],
            upstream_contexts: &[],
        })
        .await?;
    let files = listing
        .files
        .lock()
        .map_err(|_| "the snapshot listing lock was poisoned")?
        .clone();
    Ok(files)
}

#[tokio::test]
async fn the_build_sends_what_its_ignore_file_leaves_in() -> Result<(), Box<dyn std::error::Error>>
{
    let project = monorepo()?;
    let plan = plan(&project).await?;
    assert_eq!(
        snapshot_of(&project, &plan, "web").await?,
        [
            "ui/button.ts",
            "web/Dockerfile",
            "web/Dockerfile.dockerignore",
            "web/index.ts"
        ]
    );
    assert_eq!(
        snapshot_of(&project, &plan, "api").await?,
        [
            ".config/mm.yaml",
            ".dockerignore",
            "api/Dockerfile",
            "api/main.go",
            "compose.yaml"
        ]
    );
    Ok(())
}
