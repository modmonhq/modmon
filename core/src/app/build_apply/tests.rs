use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{
    BuildExecutionPorts, BuildService, inject_upstream_digests, require_plan_hash,
    select_upstream_image, validated_build_result,
};
use crate::adapters::clock::FixedClock;
use crate::adapters::state::SqliteStateStore;
use crate::app::BuildManager;
use crate::app::ports::{
    BuildOperationGuard, BuildOperationLock, BuildSourceInspector, BuildVariantRequest,
    ImageBuilder, SecretMaterialResolver, SecretMetadataInspector, TargetOperationLockGuard,
    UpstreamBuildContext,
};
use crate::domain::build::{
    BuildDependencyReference, BuildReusePolicy, BuildSource, BuildTagStrategy, BuiltImage,
    FirstPartyBuildEdge, ImageBuildPlan, ImageBuildUnitPlan, ImageBuildVariantPlan,
};
use crate::domain::config::{MmConfig, ModuleBuildConfig};
use crate::domain::error::MmError;
use crate::domain::operation::OperationId;
use crate::domain::secret::{
    SecretMaterial, SecretMetadata, SecretReference, SecretResolution, SecretScope, SecretStatus,
};
use crate::domain::workspace::ProjectId;

struct Guard;

impl BuildOperationGuard for Guard {
    fn preserve(self: Box<Self>) {}
}

impl TargetOperationLockGuard for Guard {}

struct NoSource;

#[async_trait::async_trait]
impl BuildSourceInspector for NoSource {
    async fn inspect(&self, _project_root: &Path) -> Result<BuildSource, MmError> {
        Err(MmError::Build {
            message: "source inspection is not used by secret helper tests".to_owned(),
        })
    }
}

struct NoBuilder;

#[async_trait::async_trait]
impl ImageBuilder for NoBuilder {
    async fn build_variant(
        &self,
        _request: BuildVariantRequest<'_>,
    ) -> Result<BuiltImage, MmError> {
        Err(MmError::Build {
            message: "building is not used by secret helper tests".to_owned(),
        })
    }

    async fn retag_variant(
        &self,
        _source_repository: &str,
        _digest: &str,
        _destination_repository: &str,
        _tags: &[String],
    ) -> Result<String, MmError> {
        Err(MmError::Build {
            message: "retagging is not used by secret helper tests".to_owned(),
        })
    }
}

struct NoLock;

impl BuildOperationLock for NoLock {
    fn acquire(
        &self,
        _project_id: ProjectId,
    ) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Ok(Box::new(Guard))
    }
}

#[derive(Default)]
struct ControlledSecrets {
    inspect_failure: AtomicBool,
    resolve_failure: AtomicBool,
    missing_revision: AtomicBool,
    missing_material: AtomicBool,
}

#[async_trait::async_trait]
impl SecretMetadataInspector for ControlledSecrets {
    async fn inspect(
        &self,
        _scope: &SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        if self.inspect_failure.load(Ordering::SeqCst) {
            return Err(MmError::Secret {
                message: "injected metadata failure".to_owned(),
            });
        }
        Ok(SecretMetadata {
            status: SecretStatus::Set,
            revision: (!self.missing_revision.load(Ordering::SeqCst))
                .then(|| "revision-1".to_owned()),
            created_at_unix_ms: Some(1),
            candidate_revision: None,
            set_by: None,
        })
    }
}

#[async_trait::async_trait]
impl SecretMaterialResolver for ControlledSecrets {
    async fn resolve(
        &self,
        _scope: &SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        if self.resolve_failure.load(Ordering::SeqCst) {
            return Err(MmError::Secret {
                message: "injected resolution failure".to_owned(),
            });
        }
        Ok(SecretResolution {
            metadata: SecretMetadata {
                status: SecretStatus::Set,
                revision: Some("revision-1".to_owned()),
                created_at_unix_ms: Some(1),
                candidate_revision: None,
                set_by: None,
            },
            material: if self.missing_material.load(Ordering::SeqCst) {
                None
            } else {
                Some(
                    SecretMaterial::new("build-secret".to_owned())
                        .map_err(|message| MmError::Secret { message })?,
                )
            },
        })
    }
}

fn build_service(
    secrets: Arc<ControlledSecrets>,
) -> Result<(BuildService, tempfile::TempDir), Box<dyn std::error::Error>> {
    let state_directory = crate::test_support::tempdir()?;
    let state = Arc::new(SqliteStateStore::open(
        &state_directory.path().join("state.db"),
    )?);
    let ports = BuildExecutionPorts {
        documents: crate::test_project_documents(),
        state,
        builder: Arc::new(NoBuilder),
        secret_inspector: secrets.clone(),
        secret_resolver: secrets,
        locks: Arc::new(NoLock),
        clock: Arc::new(FixedClock { unix_ms: 1 }),
    };
    assert!(format!("{ports:?}").contains("BuildExecutionPorts"));
    let service = BuildService::new(
        BuildManager::new(crate::test_project_documents(), Arc::new(NoSource)),
        ports,
    );
    assert!(format!("{service:?}").contains("BuildService"));
    Ok((service, state_directory))
}

fn secret_config() -> Result<MmConfig, MmError> {
    let document = crate::test_project_documents()
        .load_from(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal"))?;
    let mut config = document.config;
    config
        .modules
        .get_mut("api")
        .ok_or_else(|| MmError::Build {
            message: "minimal fixture has no api module".to_owned(),
        })?
        .build = Some(ModuleBuildConfig {
        secrets: BTreeMap::from([("gh_pat".to_owned(), "database.password".to_owned())]),
        ..ModuleBuildConfig::default()
    });
    Ok(config)
}

fn variant(target: Option<&str>) -> ImageBuildVariantPlan {
    ImageBuildVariantPlan {
        target: target.map(str::to_owned),
        tag_suffix: String::new(),
        tags: vec!["latest".to_owned()],
        stage: None,
        args: BTreeMap::new(),
    }
}

fn unit(module: &str, image: &str) -> ImageBuildUnitPlan {
    ImageBuildUnitPlan {
        module: module.to_owned(),
        image: image.to_owned(),
        context: PathBuf::from(module),
        dockerfile: PathBuf::from(format!("{module}/Dockerfile")),
        context_hash: "a".repeat(64),
        dockerfile_hash: "b".repeat(64),
        args: BTreeMap::new(),
        secret_ids: Vec::new(),
        credential_secret_ids: Vec::new(),
        dependencies: Vec::new(),
        upstream_node_ids: Vec::new(),
        extra_inputs: Vec::new(),
        version_embedded: false,
        reuse_policy: BuildReusePolicy::Allowed,
        input_fingerprint: "c".repeat(64),
        variants: vec![variant(None)],
    }
}

fn image(module: &str, target: Option<&str>, digest_byte: char) -> BuiltImage {
    BuiltImage {
        module: module.to_owned(),
        target: target.map(str::to_owned),
        repository: format!("registry.example/{module}"),
        tags: vec![format!("registry.example/{module}:latest")],
        digest: format!("sha256:{}", digest_byte.to_string().repeat(64)),
        execution_fingerprint: "d".repeat(64),
        secret_revisions: BTreeMap::new(),
        archive: Some(PathBuf::from(format!("/tmp/{module}.tar"))),
        reused: false,
    }
}

fn plan(units: Vec<ImageBuildUnitPlan>) -> ImageBuildPlan {
    ImageBuildPlan {
        schema: crate::domain::build::PLAN_SCHEMA.to_owned(),
        project_id: ProjectId(uuid::Uuid::nil()),
        config_hash: "e".repeat(64),
        environment: "local".to_owned(),
        source: BuildSource {
            commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            branch: Some("main".to_owned()),
            tags_at_head: Vec::new(),
            repository_tags: Vec::new(),
            tags_complete: true,
            default_branch: true,
            dirty: false,
        },
        publish: false,
        engine: None,
        tag_strategy: BuildTagStrategy::Auto,
        platforms: vec!["linux/amd64".to_owned()],
        units,
        plan_hash: "reviewed".to_owned(),
    }
}

fn edge(reference: BuildDependencyReference) -> FirstPartyBuildEdge {
    FirstPartyBuildEdge {
        producer_module: "base".to_owned(),
        dependent_module: "web".to_owned(),
        stage: None,
        reference,
    }
}

#[test]
fn upstream_selection_requires_a_built_matching_variant() -> Result<(), Box<dyn std::error::Error>>
{
    let missing = select_upstream_image(&[], "base", None);
    assert!(matches!(missing, Err(MmError::Build { .. })));

    let sole = image("base", Some("arm64"), 'a');
    assert_eq!(
        select_upstream_image(std::slice::from_ref(&sole), "base", None)?,
        &sole
    );

    let exact = image("base", Some("amd64"), 'b');
    let default = image("base", None, 'c');
    let images = vec![sole, exact.clone(), default.clone()];
    assert_eq!(
        select_upstream_image(&images, "base", Some("amd64"))?,
        &exact
    );
    assert_eq!(
        select_upstream_image(&images, "base", Some("riscv64"))?,
        &default
    );

    let no_default = vec![image("base", Some("arm64"), 'a'), exact];
    let mismatch = select_upstream_image(&no_default, "base", Some("riscv64"));
    assert!(matches!(mismatch, Err(MmError::Build { .. })));
    Ok(())
}

#[test]
fn dependency_injection_is_immutable_deduplicated_and_auditable()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unit("base", "registry.example/base");
    let mut web = unit("web", "registry.example/web");
    web.dependencies = vec![
        edge(BuildDependencyReference::BuildArgument {
            arg: "BASE_IMAGE".to_owned(),
            instruction_index: 1,
            source_line: 2,
        }),
        edge(BuildDependencyReference::Dockerfile {
            original_reference: "registry.example/base:latest".to_owned(),
            instruction_index: 2,
            source_line: 3,
        }),
    ];
    let reviewed = plan(vec![base, web.clone()]);
    let upstream = image("base", None, 'a');

    let prepared = inject_upstream_digests(
        &reviewed,
        &web,
        &web.variants[0],
        std::slice::from_ref(&upstream),
    )?;

    let immutable = format!("registry.example/base@{}", upstream.digest);
    assert_eq!(prepared.executable.args.get("BASE_IMAGE"), Some(&immutable));
    assert_eq!(prepared.dependencies.len(), 2);
    assert_eq!(
        prepared.upstream_contexts,
        vec![UpstreamBuildContext {
            immutable_reference: immutable,
            digest: upstream.digest,
            local_archive: upstream.archive,
        }]
    );
    assert_eq!(prepared.upstream_digests.len(), 2);
    assert!(
        web.args.is_empty(),
        "the reviewed unit must remain unchanged"
    );
    Ok(())
}

#[test]
fn dependency_injection_rejects_unknown_or_unbuilt_producers() {
    let mut web = unit("web", "registry.example/web");
    web.dependencies
        .push(edge(BuildDependencyReference::Dockerfile {
            original_reference: "registry.example/base:latest".to_owned(),
            instruction_index: 1,
            source_line: 1,
        }));

    let unknown = inject_upstream_digests(&plan(vec![web.clone()]), &web, &web.variants[0], &[]);
    assert!(matches!(unknown, Err(MmError::Build { .. })));

    let unbuilt = inject_upstream_digests(
        &plan(vec![unit("base", "registry.example/base"), web.clone()]),
        &web,
        &web.variants[0],
        &[],
    );
    assert!(matches!(unbuilt, Err(MmError::Build { .. })));
}

#[test]
fn result_and_review_validation_fail_closed() {
    let reviewed = plan(vec![unit("web", "registry.example/web")]);
    assert!(require_plan_hash(&reviewed, "reviewed").is_ok());
    assert!(matches!(
        require_plan_hash(&reviewed, "stale"),
        Err(MmError::State { .. })
    ));

    let empty = validated_build_result(
        OperationId::new(),
        &reviewed,
        Vec::new(),
        Vec::new(),
        Box::new(Guard),
    );
    assert!(matches!(empty, Err(MmError::Build { .. })));

    let invalid = validated_build_result(
        OperationId::new(),
        &reviewed,
        vec![image("unexpected", None, 'a')],
        Vec::new(),
        Box::new(Guard),
    );
    assert!(matches!(invalid, Err(MmError::Security { .. })));
}

#[tokio::test]
async fn secret_gated_reuse_and_resolution_fail_closed_at_each_review_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    assert!(NoSource.inspect(Path::new(".")).await.is_err());
    assert!(
        NoBuilder
            .retag_variant("source", "digest", "destination", &[])
            .await
            .is_err()
    );
    let no_build_unit = unit("api", "registry.example/api");
    let no_build_secrets = BTreeMap::new();
    assert!(
        NoBuilder
            .build_variant(BuildVariantRequest {
                operation_id: OperationId::new(),
                project_root: Path::new("."),
                plan_hash: "reviewed",
                platforms: &[],
                publish: false,
                unit: &no_build_unit,
                variant: &no_build_unit.variants[0],
                secrets: &no_build_secrets,
                dependencies: &[],
                upstream_contexts: &[],
                engine: None,
            })
            .await
            .is_err()
    );
    let _guard = NoLock.acquire(ProjectId(uuid::Uuid::nil()))?;

    let secrets = Arc::new(ControlledSecrets::default());
    let (service, _state_directory) = build_service(secrets.clone())?;
    let config = secret_config()?;
    let mut reviewed = unit("api", "registry.example/api");

    assert!(
        service
            .secret_revisions_for_reuse(&reviewed, &config, "local")
            .await?
            .is_empty()
    );

    reviewed.reuse_policy = BuildReusePolicy::RevisionGated;
    reviewed.secret_ids = vec!["gh_pat".to_owned()];
    assert_eq!(
        service
            .secret_revisions_for_reuse(&reviewed, &config, "local")
            .await?,
        BTreeMap::from([("gh_pat".to_owned(), "revision-1".to_owned())])
    );
    let material = service
        .resolve_unit_secrets(&reviewed, &config, "local")
        .await?;
    assert!(
        material
            .get("gh_pat")
            .is_some_and(|value| value.expose(|value| value == "build-secret"))
    );

    let mut missing_mapping = config.clone();
    missing_mapping
        .modules
        .get_mut("api")
        .ok_or("api module")?
        .build = None;
    assert!(
        service
            .secret_revisions_for_reuse(&reviewed, &missing_mapping, "local")
            .await
            .is_err()
    );

    let mut missing_declaration = config.clone();
    missing_declaration
        .modules
        .get_mut("api")
        .and_then(|module| module.build.as_mut())
        .ok_or("api build")?
        .secrets
        .insert("gh_pat".to_owned(), "undeclared".to_owned());
    assert!(
        service
            .secret_revisions_for_reuse(&reviewed, &missing_declaration, "local")
            .await
            .is_err()
    );
    assert!(
        service
            .resolve_unit_secrets(&reviewed, &missing_declaration, "local")
            .await
            .is_err()
    );

    let mut invalid_reference = config.clone();
    invalid_reference
        .secrets
        .get_mut("database.password")
        .ok_or("database secret")?
        .storage
        .location = "../invalid".to_owned();
    assert!(
        service
            .secret_revisions_for_reuse(&reviewed, &invalid_reference, "local")
            .await
            .is_err()
    );
    assert!(
        service
            .resolve_unit_secrets(&reviewed, &invalid_reference, "local")
            .await
            .is_err()
    );

    secrets.inspect_failure.store(true, Ordering::SeqCst);
    assert!(
        service
            .secret_revisions_for_reuse(&reviewed, &config, "local")
            .await
            .is_err()
    );
    secrets.inspect_failure.store(false, Ordering::SeqCst);
    secrets.missing_revision.store(true, Ordering::SeqCst);
    assert!(
        service
            .secret_revisions_for_reuse(&reviewed, &config, "local")
            .await
            .is_err()
    );
    secrets.missing_revision.store(false, Ordering::SeqCst);

    let mut changed_ids = reviewed.clone();
    changed_ids.secret_ids.clear();
    assert!(
        service
            .resolve_unit_secrets(&changed_ids, &config, "local")
            .await
            .is_err()
    );
    secrets.resolve_failure.store(true, Ordering::SeqCst);
    assert!(
        service
            .resolve_unit_secrets(&reviewed, &config, "local")
            .await
            .is_err()
    );
    secrets.resolve_failure.store(false, Ordering::SeqCst);
    secrets.missing_material.store(true, Ordering::SeqCst);
    assert!(
        service
            .resolve_unit_secrets(&reviewed, &config, "local")
            .await
            .is_err()
    );
    Ok(())
}
