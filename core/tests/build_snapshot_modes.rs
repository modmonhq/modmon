#![cfg(unix)]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::adapters::build::DockerBuildxBuilder;
use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::state::SqliteStateStore;
use mm::app::ports::{
    BuildOperationLock, BuildSourceInspector, CommandOutput, CommandSpec, ProcessRunner,
    SecretMaterialResolver, SecretMetadataInspector, StateStore, TargetOperationLockGuard,
};
use mm::app::{BuildExecutionPorts, BuildManager, BuildService, WorkspaceSelection};
use mm::domain::build::{BuildSource, LocalImageEngine};
use mm::domain::error::MmError;
use mm::domain::runtime::RuntimeKind;
use mm::domain::secret::{SecretMetadata, SecretReference, SecretResolution, SecretScope};
use mm::domain::workspace::ProjectId;
use rustix::fs::Mode;
use zeroize::Zeroizing;

mod temp_dirs;

const CONFIG: &str = r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de7
  name: shop
  development-environment: local
default-environment: local
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
      image: ghcr.io/example/web
environments:
  local:
    bindings: {}
"#;

const IMAGE_ID: &str = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

#[derive(Debug)]
struct CleanSource;

#[async_trait]
impl BuildSourceInspector for CleanSource {
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

#[derive(Debug)]
struct NoSecrets;

#[async_trait]
impl SecretMetadataInspector for NoSecrets {
    async fn inspect(
        &self,
        _scope: &SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        Err(MmError::Secret {
            message: "this project has no build secrets".to_owned(),
        })
    }
}

#[async_trait]
impl SecretMaterialResolver for NoSecrets {
    async fn resolve(
        &self,
        _scope: &SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        Err(MmError::Secret {
            message: "this project has no build secrets".to_owned(),
        })
    }
}

#[derive(Debug)]
struct NoLock;

struct NoGuard;

impl TargetOperationLockGuard for NoGuard {}

impl BuildOperationLock for NoLock {
    fn acquire(
        &self,
        _project_id: ProjectId,
    ) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Ok(Box::new(NoGuard))
    }
}

/// A `docker` that records the mode of every entry in the context it is
/// asked to build, by path relative to the context.
#[derive(Debug, Default)]
struct ModeRecordingEngine {
    contexts: Mutex<Vec<BTreeMap<PathBuf, u32>>>,
}

fn modes_under(root: &Path) -> std::io::Result<BTreeMap<PathBuf, u32>> {
    let mut modes = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        modes.insert(relative, metadata.permissions().mode() & 0o7777);
        if metadata.is_dir() {
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
        }
    }
    Ok(modes)
}

#[async_trait]
impl ProcessRunner for ModeRecordingEngine {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let line = specification.redacted_display();
        if line.starts_with("docker build ") {
            let context = line.rsplit(' ').next().unwrap_or_default();
            let modes = modes_under(Path::new(context)).map_err(|source| MmError::Io {
                context: format!("reading the build context {context}"),
                source,
            })?;
            if let Ok(mut contexts) = self.contexts.lock() {
                contexts.push(modes);
            }
        }
        let stdout = if line.starts_with("docker image inspect") {
            IMAGE_ID
        } else {
            ""
        };
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(stdout.to_owned()),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

fn project() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    let root = project.path();
    fs::create_dir(root.join(".config"))?;
    fs::write(root.join(".config/mm.yaml"), CONFIG)?;
    fs::write(
        root.join("compose.yaml"),
        "services:\n  web:\n    image: ghcr.io/example/web:old\n    expose: ['80']\n",
    )?;
    fs::create_dir_all(root.join("web/spec/nested"))?;
    fs::create_dir(root.join("web/bin"))?;
    fs::write(root.join("web/Dockerfile"), "FROM scratch\nCOPY . /app\n")?;
    fs::write(root.join("web/spec/nested/api.yaml"), "openapi: 3.1.0\n")?;
    fs::write(root.join("web/bin/start"), "#!/bin/sh\n")?;
    fs::set_permissions(
        root.join("web/bin/start"),
        fs::Permissions::from_mode(0o700),
    )?;
    Ok(project)
}

fn builds(home: &Path, engine: Arc<ModeRecordingEngine>) -> Result<BuildService, MmError> {
    let state: Arc<dyn StateStore> = Arc::new(SqliteStateStore::open(&home.join("state.sqlite3"))?);
    Ok(BuildService::new(
        BuildManager::new(Arc::new(ConfigLoader), Arc::new(CleanSource))
            .with_local_engine(Some(LocalImageEngine::Docker)),
        BuildExecutionPorts {
            documents: Arc::new(ConfigLoader),
            state,
            builder: Arc::new(DockerBuildxBuilder::new(engine, home.join("builds"))),
            secret_inspector: Arc::new(NoSecrets),
            secret_resolver: Arc::new(NoSecrets),
            locks: Arc::new(NoLock),
            clock: Arc::new(FixedClock { unix_ms: 1_000 }),
        },
    ))
}

#[tokio::test]
async fn a_build_context_snapshot_has_the_same_modes_under_any_umask()
-> Result<(), Box<dyn std::error::Error>> {
    rustix::process::umask(Mode::from_bits_truncate(0o077));
    let project = project()?;
    let home = temp_dirs::tempdir()?;
    let engine = Arc::new(ModeRecordingEngine::default());
    let builds = builds(home.path(), engine.clone())?;
    let mut selection = WorkspaceSelection::new(project.path(), Some("local".to_owned()));
    selection.runtime = Some(RuntimeKind::DockerCompose);

    let plan = builds.plan(&selection, false).await?;
    builds
        .apply_reviewed(&selection, false, &plan.plan_hash)
        .await?;

    let contexts = engine.contexts.lock().map_err(|_| "poisoned")?.clone();
    let expected = [
        ("", 0o755),
        ("Dockerfile", 0o644),
        ("bin", 0o755),
        ("bin/start", 0o755),
        ("spec", 0o755),
        ("spec/nested", 0o755),
        ("spec/nested/api.yaml", 0o644),
    ]
    .into_iter()
    .map(|(path, mode)| (PathBuf::from(path), mode))
    .collect::<BTreeMap<_, _>>();
    assert_eq!(contexts, vec![expected]);

    fs::set_permissions(
        project.path().join("web/bin/start"),
        fs::Permissions::from_mode(0o600),
    )?;
    let without_executable = builds.plan(&selection, false).await?;
    assert_ne!(
        without_executable.units[0].context_hash,
        plan.units[0].context_hash
    );
    fs::set_permissions(
        project.path().join("web/bin/start"),
        fs::Permissions::from_mode(0o654),
    )?;
    let executable_by_others_only = builds.plan(&selection, false).await?;
    assert_eq!(
        executable_by_others_only.units[0].context_hash,
        without_executable.units[0].context_hash
    );
    fs::set_permissions(
        project.path().join("web/bin/start"),
        fs::Permissions::from_mode(0o755),
    )?;
    fs::set_permissions(
        project.path().join("web/spec/nested/api.yaml"),
        fs::Permissions::from_mode(0o644),
    )?;
    fs::set_permissions(
        project.path().join("web/spec"),
        fs::Permissions::from_mode(0o755),
    )?;
    let under_another_umask = builds.plan(&selection, false).await?;
    assert_eq!(
        under_another_umask.units[0].context_hash,
        plan.units[0].context_hash
    );
    Ok(())
}
