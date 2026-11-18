//! Helpers shared by unit tests.

use std::io;
use std::path::PathBuf;

/// The system temporary directory at its canonical path. On macOS it lives
/// under the `/var` symlink, and mm's path checks refuse symlinked ancestors.
pub(crate) fn temp_root() -> io::Result<PathBuf> {
    std::env::temp_dir().canonicalize()
}

/// A temporary directory whose path has no symlink components.
pub(crate) fn tempdir() -> io::Result<tempfile::TempDir> {
    tempfile::tempdir_in(temp_root()?)
}

/// Whether the filesystem accepts file names that are not UTF-8. APFS on
/// macOS rejects them, so tests of non-UTF-8 handling skip that part there.
pub(crate) const NON_UTF8_FILE_NAMES: bool = !cfg!(target_os = "macos");

/// A published one-image build of `commit` for `environment`, as a receipt.
pub(crate) fn build_receipt(
    project_id: crate::domain::workspace::ProjectId,
    environment: &str,
    commit: &str,
) -> Result<crate::domain::build::BuildReceipt, String> {
    use std::collections::BTreeMap;

    use crate::domain::build::{
        BuildReceipt, BuildReusePolicy, BuildSource, BuildTagStrategy, BuiltImage, ImageBuildPlan,
        ImageBuildResult, ImageBuildScope, ImageBuildUnitPlan, ImageBuildVariantPlan,
        unit_execution_fingerprint, unit_input_fingerprint,
    };
    let platforms = vec!["linux/amd64".to_owned()];
    let source = BuildSource {
        commit: commit.to_owned(),
        branch: Some("main".to_owned()),
        tags_at_head: Vec::new(),
        repository_tags: Vec::new(),
        tags_complete: true,
        default_branch: true,
        dirty: false,
    };
    let version = crate::domain::build::build_version(&source)?;
    let tags = crate::domain::build::automatic_tags(&source, BuildTagStrategy::Auto)?;
    let mut unit = ImageBuildUnitPlan {
        module: "api".to_owned(),
        image: "registry.example/api".to_owned(),
        context: PathBuf::from("api"),
        dockerfile: PathBuf::from("api/Dockerfile"),
        context_hash: "a".repeat(64),
        dockerfile_hash: "b".repeat(64),
        args: BTreeMap::from([("VERSION".to_owned(), version)]),
        secret_ids: Vec::new(),
        credential_secret_ids: Vec::new(),
        dependencies: Vec::new(),
        upstream_node_ids: Vec::new(),
        extra_inputs: Vec::new(),
        version_embedded: false,
        reuse_policy: BuildReusePolicy::Allowed,
        input_fingerprint: String::new(),
        variants: vec![ImageBuildVariantPlan {
            target: None,
            tag_suffix: String::new(),
            tags: tags.clone(),
            stage: None,
            args: BTreeMap::new(),
        }],
    };
    unit.input_fingerprint = unit_input_fingerprint(&unit, &platforms)?;
    let plan = ImageBuildPlan::new(
        ImageBuildScope {
            project_id,
            config_hash: "c".repeat(64),
            environment: environment.to_owned(),
        },
        source,
        true,
        BuildTagStrategy::Auto,
        platforms,
        vec![unit],
    )?;
    let unit = &plan.units[0];
    let result = ImageBuildResult {
        operation_id: crate::domain::operation::OperationId::new(),
        plan_hash: plan.plan_hash.clone(),
        images: vec![BuiltImage {
            module: unit.module.clone(),
            target: None,
            repository: unit.image.clone(),
            tags: tags
                .iter()
                .map(|tag| format!("{}:{tag}", unit.image))
                .collect(),
            digest: format!("sha256:{}", "d".repeat(64)),
            execution_fingerprint: unit_execution_fingerprint(
                &unit.input_fingerprint,
                environment,
                None,
                &BTreeMap::new(),
                &BTreeMap::new(),
            )?,
            secret_revisions: BTreeMap::new(),
            archive: None,
            reused: false,
        }],
        dependencies: Vec::new(),
    };
    BuildReceipt::new(plan, result)
}
