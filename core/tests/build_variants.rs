use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::state::SqliteStateStore;
use mm::app::ports::{
    BuildOperationGuard, BuildOperationLock, BuildSourceInspector, BuildVariantRequest,
    ImageBuilder, SecretMaterialResolver, SecretMetadataInspector, StateStore,
    TargetOperationLockGuard,
};
use mm::app::{BuildExecutionPorts, BuildManager, BuildService, WorkspaceSelection};
use mm::domain::build::{BuildSource, BuiltImage};
use mm::domain::error::MmError;
use mm::domain::operation::OperationId;
use mm::domain::secret::{SecretMetadata, SecretReference, SecretResolution, SecretScope};
use mm::domain::workspace::ProjectId;
use tempfile::TempDir;

mod temp_dirs;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

const CONFIG: &str = r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de4
  name: build-variants
default-environment: production
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
      image: registry.example.com/web
      targets:
        production: ""
        production-staging:
          target: production
          suffix: staging
          args:
            SITE_URL: https://staging.example.com
            WS_HOST: staging.example.com
      args:
        SITE_URL: https://example.com
        WS_HOST: example.com
        THEME: dark
environments:
  production:
    build-variant: production
  staging:
    build-variant: production-staging
"#;

#[derive(Debug)]
struct Source;

#[async_trait]
impl BuildSourceInspector for Source {
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
            message: "no build secrets are configured".to_owned(),
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
            message: "no build secrets are configured".to_owned(),
        })
    }
}

struct Guard;

impl TargetOperationLockGuard for Guard {}

#[derive(Debug)]
struct Lock;

impl BuildOperationLock for Lock {
    fn acquire(
        &self,
        _project_id: ProjectId,
    ) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Ok(Box::new(Guard))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Built {
    variant: Option<String>,
    stage: Option<String>,
    args: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
struct Builder {
    built: Mutex<Vec<Built>>,
}

impl Builder {
    fn built(&self) -> Result<Vec<Built>, MmError> {
        self.built
            .lock()
            .map(|built| built.clone())
            .map_err(|_| MmError::State {
                message: "fake builder lock was poisoned".to_owned(),
            })
    }
}

#[async_trait]
impl ImageBuilder for Builder {
    fn begin_operation(
        &self,
        _operation_id: OperationId,
    ) -> Result<Box<dyn BuildOperationGuard>, MmError> {
        struct Kept;
        impl BuildOperationGuard for Kept {
            fn preserve(self: Box<Self>) {}
        }
        Ok(Box::new(Kept))
    }

    async fn build_variant(&self, request: BuildVariantRequest<'_>) -> Result<BuiltImage, MmError> {
        let BuildVariantRequest { unit, variant, .. } = request;
        let built = Built {
            variant: variant.target.clone(),
            stage: variant.dockerfile_stage().map(str::to_owned),
            args: unit.variant_args(variant),
        };
        let identity =
            serde_json::to_vec(&(&built.stage, &built.args)).map_err(|error| MmError::Build {
                message: format!("serializing fake build identity: {error}"),
            })?;
        self.built
            .lock()
            .map_err(|_| MmError::State {
                message: "fake builder lock was poisoned".to_owned(),
            })?
            .push(built);
        Ok(BuiltImage {
            module: unit.module.clone(),
            target: variant.target.clone(),
            repository: unit.image.clone(),
            tags: variant
                .tags
                .iter()
                .map(|tag| format!("{}:{tag}", unit.image))
                .collect(),
            digest: format!("sha256:{}", blake3::hash(&identity).to_hex()),
            execution_fingerprint: String::new(),
            secret_revisions: BTreeMap::new(),
            archive: None,
            reused: false,
        })
    }

    async fn retag_variant(
        &self,
        _source_repository: &str,
        digest: &str,
        _destination_repository: &str,
        _tags: &[String],
    ) -> Result<String, MmError> {
        Ok(digest.to_owned())
    }
}

struct Project {
    root: TempDir,
    _state_directory: TempDir,
    state: Arc<SqliteStateStore>,
}

impl Project {
    fn new(config: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let root = temp_dirs::tempdir()?;
        fs::create_dir(root.path().join(".config"))?;
        fs::create_dir(root.path().join("web"))?;
        fs::write(root.path().join(".config/mm.yaml"), config)?;
        fs::write(
            root.path().join("compose.yaml"),
            "services:\n  web:\n    image: registry.example.com/web:old\n",
        )?;
        fs::write(
            root.path().join("web/Dockerfile"),
            "FROM scratch AS production\nARG SITE_URL\nARG WS_HOST\n",
        )?;
        let state_directory = temp_dirs::tempdir()?;
        let state = Arc::new(SqliteStateStore::open(
            &state_directory.path().join("state.sqlite3"),
        )?);
        Ok(Self {
            root,
            _state_directory: state_directory,
            state,
        })
    }

    fn service(&self, builder: Arc<Builder>) -> BuildService {
        let state: Arc<dyn StateStore> = self.state.clone();
        BuildService::new(
            BuildManager::new(Arc::new(ConfigLoader), Arc::new(Source)),
            BuildExecutionPorts {
                documents: Arc::new(ConfigLoader),
                state,
                builder,
                secret_inspector: Arc::new(NoSecrets),
                secret_resolver: Arc::new(NoSecrets),
                locks: Arc::new(Lock),
                clock: Arc::new(FixedClock { unix_ms: 1000 }),
            },
        )
    }

    fn select(&self, environment: &str) -> WorkspaceSelection {
        WorkspaceSelection::new(self.root.path(), Some(environment.to_owned()))
    }
}

fn args(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[tokio::test]
async fn each_environment_builds_its_variant_from_one_stage_with_its_own_arguments()
-> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new(CONFIG)?;
    let builder = Arc::new(Builder::default());
    let service = project.service(builder.clone());

    let staging = service.plan(&project.select("staging"), true).await?;
    let [unit] = staging.units.as_slice() else {
        return Err("staging plans one unit".into());
    };
    let [variant] = unit.variants.as_slice() else {
        return Err("staging builds only the variant it selects".into());
    };
    assert_eq!(variant.target.as_deref(), Some("production-staging"));
    assert_eq!(variant.stage.as_deref(), Some("production"));
    assert_eq!(variant.tag_suffix, "staging");
    assert_eq!(
        variant.args,
        args(&[
            ("SITE_URL", "https://staging.example.com"),
            ("WS_HOST", "staging.example.com"),
        ])
    );
    let staged = service
        .apply_reviewed(&project.select("staging"), true, &staging.plan_hash)
        .await?;

    let production = service.plan(&project.select("production"), true).await?;
    let [variant] = production.units[0].variants.as_slice() else {
        return Err("production builds only the variant it selects".into());
    };
    assert_eq!(variant.target.as_deref(), Some("production"));
    assert_eq!(variant.stage, None);
    assert!(variant.args.is_empty());
    let produced = service
        .apply_reviewed(&project.select("production"), true, &production.plan_hash)
        .await?;

    let version = staging.units[0].args["VERSION"].clone();
    assert_eq!(
        builder.built()?,
        vec![
            Built {
                variant: Some("production-staging".to_owned()),
                stage: Some("production".to_owned()),
                args: args(&[
                    ("SITE_URL", "https://staging.example.com"),
                    ("THEME", "dark"),
                    ("VERSION", &version),
                    ("WS_HOST", "staging.example.com"),
                ]),
            },
            Built {
                variant: Some("production".to_owned()),
                stage: Some("production".to_owned()),
                args: args(&[
                    ("SITE_URL", "https://example.com"),
                    ("THEME", "dark"),
                    ("VERSION", &version),
                    ("WS_HOST", "example.com"),
                ]),
            },
        ]
    );
    assert_ne!(staged.images[0].digest, produced.images[0].digest);
    assert!(
        staged.images[0]
            .tags
            .iter()
            .all(|tag| tag.ends_with("-staging"))
    );
    assert!(
        produced.images[0]
            .tags
            .iter()
            .all(|tag| !tag.ends_with("-staging"))
    );
    Ok(())
}

#[tokio::test]
async fn a_receipt_built_for_one_variant_is_refused_for_another()
-> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new(&format!(
        "{CONFIG}  preview:\n    build-variant: production-staging\n"
    ))?;
    let service = project.service(Arc::new(Builder::default()));
    let staging = project.select("staging");
    let plan = service.plan(&staging, true).await?;
    service
        .apply_reviewed(&staging, true, &plan.plan_hash)
        .await?;
    let receipt = service.receipt(&staging)?;

    let refused = service
        .review_import(&project.select("production"), &receipt, false)
        .await;
    assert!(
        matches!(&refused, Err(MmError::Build { message })
            if message.contains("module 'web'")
                && message.contains("build variant production here, production-staging in the receipt")
                && message.contains("mm -e production build --publish")),
        "{refused:?}"
    );

    let review = service
        .review_import(&project.select("preview"), &receipt, false)
        .await?;
    assert_eq!(review.source_environment, "staging");
    assert_eq!(review.environment, "preview");
    Ok(())
}

#[tokio::test]
async fn a_receipt_of_every_variant_promotes_to_each_environment()
-> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new(&format!("{CONFIG}  ci: {{}}\n"))?;
    let builder = Arc::new(Builder::default());
    let service = project.service(builder.clone());
    let ci = project.select("ci");
    let plan = service.plan(&ci, true).await?;
    assert_eq!(plan.units[0].variants.len(), 2);
    service.apply_reviewed(&ci, true, &plan.plan_hash).await?;
    assert_eq!(builder.built()?.len(), 2);
    let receipt = service.receipt(&ci)?;
    for environment in ["production", "staging"] {
        let review = service
            .review_import(&project.select(environment), &receipt, false)
            .await?;
        assert_eq!(review.environment, environment);
    }
    Ok(())
}

#[tokio::test]
async fn changed_variant_arguments_refuse_the_old_receipt() -> Result<(), Box<dyn std::error::Error>>
{
    let project = Project::new(&format!(
        "{CONFIG}  preview:\n    build-variant: production-staging\n"
    ))?;
    let service = project.service(Arc::new(Builder::default()));
    let staging = project.select("staging");
    let plan = service.plan(&staging, true).await?;
    service
        .apply_reviewed(&staging, true, &plan.plan_hash)
        .await?;
    let receipt = service.receipt(&staging)?;

    let config_path = project.root.path().join(".config/mm.yaml");
    fs::write(
        &config_path,
        fs::read_to_string(&config_path)?.replace(
            "WS_HOST: staging.example.com",
            "WS_HOST: preview.example.com",
        ),
    )?;
    let refused = service
        .review_import(&project.select("preview"), &receipt, false)
        .await;
    assert!(
        matches!(&refused, Err(MmError::Build { message })
            if message.contains("build argument WS_HOST of build variant production-staging")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn variant_arguments_follow_the_module_argument_rules() -> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new(&CONFIG.replace(
        "            WS_HOST: staging.example.com\n",
        "            WS_HOST: staging.example.com\n            API_TOKEN: x\n            VERSION: \"1\"\n",
    ))?;
    let document = ConfigLoader.load_from(project.root.path())?;
    let codes = mm::adapters::config::validate(&document, "staging")
        .diagnostics
        .into_iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>();
    for (code, argument) in [
        ("MM_BUILD_SECRET_ARG_FORBIDDEN", "API_TOKEN"),
        ("MM_BUILD_ARG_RESERVED", "VERSION"),
    ] {
        assert!(
            codes.iter().any(|(found, message)| found == code
                && message.contains("build target 'production-staging'")
                && message.contains(argument)),
            "{codes:?}"
        );
    }
    Ok(())
}

#[test]
fn the_string_form_and_an_unknown_variant_key_read_clearly()
-> Result<(), Box<dyn std::error::Error>> {
    let project =
        Project::new(&CONFIG.replace("          suffix: staging\n", "          sufix: staging\n"))?;
    let error = ConfigLoader
        .load_from(project.root.path())
        .err()
        .ok_or("an unknown key in a build target must be refused")?;
    assert!(error.to_string().contains("sufix"), "{error}");

    let project = Project::new(&CONFIG.replace(
        "        production-staging:\n          target: production\n          suffix: staging\n",
        "        production-staging:\n",
    ))?;
    let document = ConfigLoader.load_from(project.root.path())?;
    let build = document.config.modules["web"]
        .build
        .as_ref()
        .ok_or("web builds")?;
    let target = &build.targets["production-staging"];
    assert_eq!(target.stage("production-staging"), "production-staging");
    assert_eq!(target.suffix("production-staging"), "production-staging");
    assert_eq!(build.targets["production"].suffix("production"), "");
    Ok(())
}
