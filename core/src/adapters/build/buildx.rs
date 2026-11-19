use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_user_controlled_directory, read_regular_file,
};
use crate::app::build::snapshot_build_context;
use crate::app::dockerfile::rewrite_dependencies;
use crate::app::ports::{
    BuildOperationGuard, BuildVariantRequest, CommandSpec, ImageBuilder, ProcessRunner,
    UpstreamBuildContext,
};
use crate::domain::build::{BuiltImage, ImageBuildUnitPlan, ImageBuildVariantPlan};
use crate::domain::error::MmError;
use crate::domain::runtime::DOCKER_INHERITED_ENVIRONMENT;
use crate::domain::secret::SecretMaterial;

const MAX_METADATA_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
pub struct DockerBuildxBuilder {
    process: Arc<dyn ProcessRunner>,
    artifacts: PathBuf,
    executable: String,
}

impl std::fmt::Debug for DockerBuildxBuilder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DockerBuildxBuilder")
            .field("artifacts", &self.artifacts)
            .field("executable", &self.executable)
            .finish_non_exhaustive()
    }
}

impl DockerBuildxBuilder {
    #[must_use]
    pub fn new(process: Arc<dyn ProcessRunner>, artifacts: PathBuf) -> Self {
        Self {
            process,
            artifacts,
            executable: "docker".to_owned(),
        }
    }
}

#[async_trait]
impl ImageBuilder for DockerBuildxBuilder {
    fn begin_operation(
        &self,
        operation_id: crate::domain::operation::OperationId,
    ) -> Result<Box<dyn BuildOperationGuard>, MmError> {
        prepare_private_directory(&self.artifacts)?;
        let path = operation_directory(&self.artifacts, operation_id);
        if path.exists() {
            return Err(MmError::Security {
                message: "image build operation directory already exists".to_owned(),
            });
        }
        fs::create_dir(&path).map_err(|source| MmError::Io {
            context: format!(
                "creating image build operation directory {}",
                path.display()
            ),
            source,
        })?;
        set_private_directory_permissions(&path)?;
        Ok(Box::new(LocalBuildOperationGuard {
            path,
            preserve: false,
        }))
    }

    fn cleanup_operation(
        &self,
        operation_id: crate::domain::operation::OperationId,
    ) -> Result<(), MmError> {
        remove_operation_directory(&operation_directory(&self.artifacts, operation_id))
    }

    async fn build_variant(&self, request: BuildVariantRequest<'_>) -> Result<BuiltImage, MmError> {
        let staging = prepare_build_staging(&self.artifacts, &request)?;
        let command = build_command(&self.executable, &request, &staging)?;
        let BuildVariantRequest {
            operation_id,
            plan_hash: _,
            publish,
            unit,
            variant,
            ..
        } = request;
        let output = self.process.run(command).await?;
        if !output.success() {
            return Err(MmError::ProcessFailed {
                program: self.executable.clone(),
                code: output.code,
                stderr: output.stderr.trim().to_owned(),
            });
        }
        let digest = read_digest(&staging.metadata_path)?;
        let archive = if publish {
            None
        } else {
            Some(publish_archive(
                &self.artifacts,
                operation_id,
                unit,
                variant,
                &staging.archive,
            )?)
        };

        Ok(BuiltImage {
            module: unit.module.clone(),
            target: variant.target.clone(),
            repository: unit.image.clone(),
            tags: variant
                .tags
                .iter()
                .map(|tag| format!("{}:{tag}", unit.image))
                .collect(),
            digest,
            execution_fingerprint: String::new(),
            secret_revisions: BTreeMap::new(),
            archive,
            reused: false,
        })
    }

    async fn retag_variant(
        &self,
        source_repository: &str,
        digest: &str,
        destination_repository: &str,
        tags: &[String],
    ) -> Result<String, MmError> {
        if !is_sha256_digest(digest) {
            return Err(MmError::Security {
                message: "cannot retag an invalid image digest".to_owned(),
            });
        }
        if tags.is_empty() {
            return Err(MmError::Build {
                message: "cannot retag an image without tags".to_owned(),
            });
        }
        let source = format!("{source_repository}@{digest}");
        let mut command = docker_command(&self.executable).args(["buildx", "imagetools", "create"]);
        for tag in tags {
            command = command
                .arg("--tag")
                .arg(format!("{destination_repository}:{tag}"));
        }
        command = command.arg(&source).timeout(Duration::from_secs(10 * 60));
        let output = self.process.run(command).await?;
        if !output.success() {
            return Err(MmError::ProcessFailed {
                program: self.executable.clone(),
                code: output.code,
                stderr: output.stderr.trim().to_owned(),
            });
        }

        for tag in tags {
            let destination = format!("{destination_repository}:{tag}");
            let returned =
                inspect_remote_digest(self.process.as_ref(), &self.executable, &destination)
                    .await?;
            if returned != digest {
                return Err(MmError::Security {
                    message: format!(
                        "immutable retag for '{destination}' did not preserve the source digest"
                    ),
                });
            }
        }
        Ok(digest.to_owned())
    }
}

struct BuildStaging {
    temporary: tempfile::TempDir,
    snapshot: PathBuf,
    dockerfile: PathBuf,
    metadata_path: PathBuf,
    archive: PathBuf,
    secret_paths: BTreeMap<String, PathBuf>,
}

fn prepare_build_staging(
    artifacts: &Path,
    request: &BuildVariantRequest<'_>,
) -> Result<BuildStaging, MmError> {
    let operation_directory = operation_directory(artifacts, request.operation_id);
    ensure_user_controlled_directory(&operation_directory)?;
    let temporary = tempfile::Builder::new()
        .prefix(".build-")
        .tempdir_in(&operation_directory)
        .map_err(|source| MmError::Io {
            context: format!(
                "creating build staging directory in {}",
                artifacts.display()
            ),
            source,
        })?;
    let snapshot = temporary.path().join("context");
    let dockerfile = snapshot_build_context(
        request.project_root,
        &request.project_root.join(&request.unit.context),
        &request.project_root.join(&request.unit.dockerfile),
        &snapshot,
        &request.unit.context_hash,
    )?;
    let source = fs::read_to_string(&dockerfile).map_err(|source| MmError::Io {
        context: format!(
            "reading immutable Dockerfile snapshot {}",
            dockerfile.display()
        ),
        source,
    })?;
    let rewritten = rewrite_dependencies(
        &source,
        &dockerfile.display().to_string(),
        request.dependencies,
    )?;
    fs::write(&dockerfile, rewritten).map_err(|source| MmError::Io {
        context: format!(
            "writing immutable Dockerfile snapshot {}",
            dockerfile.display()
        ),
        source,
    })?;
    let secret_directory = temporary.path().join("secrets");
    if !request.secrets.is_empty() {
        fs::create_dir(&secret_directory).map_err(|source| MmError::Io {
            context: "creating private build secret directory".to_owned(),
            source,
        })?;
        set_private_directory_permissions(&secret_directory)?;
    }
    let mut secret_paths = BTreeMap::new();
    for (id, material) in request.secrets {
        let path = secret_directory.join(id);
        write_secret(&path, material)?;
        secret_paths.insert(id.clone(), path);
    }
    Ok(BuildStaging {
        metadata_path: temporary.path().join("metadata.json"),
        archive: temporary.path().join("image.oci.tar"),
        temporary,
        snapshot,
        dockerfile,
        secret_paths,
    })
}

fn docker_command(executable: &str) -> CommandSpec {
    CommandSpec::new(executable).inherit_environment(DOCKER_INHERITED_ENVIRONMENT)
}

fn build_command(
    executable: &str,
    request: &BuildVariantRequest<'_>,
    staging: &BuildStaging,
) -> Result<CommandSpec, MmError> {
    let mut command = docker_command(executable)
        .args(["buildx", "build", "--file"])
        .arg(staging.dockerfile.to_string_lossy())
        .arg("--platform")
        .arg(request.platforms.join(","))
        .arg("--metadata-file")
        .arg(staging.metadata_path.to_string_lossy())
        .arg("--provenance=mode=max")
        .arg("--sbom=true")
        .current_dir(request.project_root)
        .environment("DOCKER_BUILDKIT", "1")
        .timeout(Duration::from_secs(2 * 60 * 60));
    command = if request.publish {
        command.arg("--push")
    } else {
        command
            .arg("--output")
            .arg(format!("type=oci,dest={}", staging.archive.display()))
    };
    if let Some(stage) = request.variant.dockerfile_stage() {
        command = command.arg("--target").arg(stage);
    }
    for (name, value) in request.unit.variant_args(request.variant) {
        command = command.arg("--build-arg").arg(format!("{name}={value}"));
    }
    for (index, upstream) in request.upstream_contexts.iter().enumerate() {
        let source = build_context_source(staging.temporary.path(), index, upstream)?;
        command = command
            .arg("--build-context")
            .arg(format!("{}={source}", upstream.immutable_reference));
    }
    for tag in &request.variant.tags {
        command = command
            .arg("--tag")
            .arg(format!("{}:{tag}", request.unit.image));
    }
    for (id, path) in &staging.secret_paths {
        command = command
            .arg("--secret")
            .sensitive_arg(format!("id={id},src={}", path.to_string_lossy()));
    }
    for material in request.secrets.values() {
        command = material.expose(|value| command.redact_value(value.to_owned()));
    }
    Ok(command.arg(staging.snapshot.to_string_lossy()))
}

async fn inspect_remote_digest(
    process: &dyn ProcessRunner,
    executable: &str,
    reference: &str,
) -> Result<String, MmError> {
    let inspect = docker_command(executable)
        .args(["buildx", "imagetools", "inspect", "--format", "{{json .}}"])
        .arg(reference)
        .timeout(Duration::from_secs(2 * 60));
    let output = process.run(inspect).await?;
    if !output.success() {
        return Err(MmError::ProcessFailed {
            program: executable.to_owned(),
            code: output.code,
            stderr: output.stderr.trim().to_owned(),
        });
    }
    let metadata: serde_json::Value =
        serde_json::from_str(output.stdout.trim()).map_err(|error| MmError::Build {
            message: format!("Docker imagetools inspect returned invalid metadata: {error}"),
        })?;
    metadata
        .get("digest")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            metadata
                .pointer("/manifest/digest")
                .and_then(serde_json::Value::as_str)
        })
        .map(str::to_owned)
        .ok_or_else(|| MmError::Build {
            message: "Docker imagetools inspect did not contain an image digest".to_owned(),
        })
}

struct LocalBuildOperationGuard {
    path: PathBuf,
    preserve: bool,
}

impl BuildOperationGuard for LocalBuildOperationGuard {
    fn preserve(mut self: Box<Self>) {
        self.preserve = true;
    }
}

impl Drop for LocalBuildOperationGuard {
    fn drop(&mut self) {
        if !self.preserve
            && let Err(error) = fs::remove_dir_all(&self.path)
        {
            tracing::warn!(path = %self.path.display(), %error, "image build cleanup failed");
        }
    }
}

fn operation_directory(
    artifacts: &Path,
    operation_id: crate::domain::operation::OperationId,
) -> PathBuf {
    artifacts.join(operation_id.0.to_string())
}

fn remove_operation_directory(path: &Path) -> Result<(), MmError> {
    if !path.exists() {
        return Ok(());
    }
    ensure_no_symlink_components(path)?;
    fs::remove_dir_all(path).map_err(|source| MmError::Io {
        context: format!(
            "cleaning image build operation directory {}",
            path.display()
        ),
        source,
    })
}

fn build_context_source(
    temporary: &Path,
    index: usize,
    upstream: &UpstreamBuildContext,
) -> Result<String, MmError> {
    if !is_sha256_digest(&upstream.digest)
        || !upstream
            .immutable_reference
            .ends_with(&format!("@{}", upstream.digest))
    {
        return Err(MmError::Security {
            message: "upstream build context is not pinned to its reviewed digest".to_owned(),
        });
    }
    let Some(archive) = &upstream.local_archive else {
        return Ok(format!("docker-image://{}", upstream.immutable_reference));
    };
    let layout = temporary.join(format!("upstream-{index}"));
    fs::create_dir(&layout).map_err(|source| MmError::Io {
        context: "creating operation-scoped OCI layout".to_owned(),
        source,
    })?;
    set_private_directory_permissions(&layout)?;
    extract_oci_layout(archive, &layout)?;
    Ok(format!(
        "oci-layout://{}@{}",
        layout.display(),
        upstream.digest
    ))
}

fn extract_oci_layout(archive_path: &Path, destination: &Path) -> Result<(), MmError> {
    ensure_no_symlink_components(archive_path)?;
    let file = File::open(archive_path).map_err(|source| MmError::Io {
        context: format!("opening upstream OCI archive {}", archive_path.display()),
        source,
    })?;
    let mut archive = tar::Archive::new(file);
    let entries = archive.entries().map_err(|source| MmError::Io {
        context: "reading upstream OCI archive".to_owned(),
        source,
    })?;
    for entry in entries {
        let mut entry = entry.map_err(|source| MmError::Io {
            context: "reading upstream OCI archive entry".to_owned(),
            source,
        })?;
        let path = entry.path().map_err(|source| MmError::Io {
            context: "reading upstream OCI archive path".to_owned(),
            source,
        })?;
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
            || !(entry.header().entry_type().is_file() || entry.header().entry_type().is_dir())
        {
            return Err(MmError::Security {
                message: "upstream OCI archive contains an unsafe entry".to_owned(),
            });
        }
        entry.unpack_in(destination).map_err(|source| MmError::Io {
            context: "extracting operation-scoped OCI layout".to_owned(),
            source,
        })?;
    }
    if !destination.join("oci-layout").is_file() || !destination.join("index.json").is_file() {
        return Err(MmError::Build {
            message: "upstream build output is not an OCI image layout".to_owned(),
        });
    }
    Ok(())
}

fn prepare_private_directory(path: &Path) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    fs::create_dir_all(path).map_err(|source| MmError::Io {
        context: format!("creating build artifact directory {}", path.display()),
        source,
    })?;
    set_private_directory_permissions(path)?;
    ensure_user_controlled_directory(path)
}

fn set_private_directory_permissions(path: &Path) -> Result<(), MmError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            MmError::Io {
                context: format!(
                    "setting private directory permissions on {}",
                    path.display()
                ),
                source,
            }
        })?;
    }
    Ok(())
}

fn write_secret(path: &Path, material: &SecretMaterial) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: "creating ephemeral build secret".to_owned(),
        source,
    })?;
    material
        .expose(|value| file.write_all(value.as_bytes()))
        .map_err(|source| MmError::Io {
            context: "writing ephemeral build secret".to_owned(),
            source,
        })?;
    file.sync_all().map_err(|source| MmError::Io {
        context: "synchronizing ephemeral build secret".to_owned(),
        source,
    })
}

fn read_digest(path: &Path) -> Result<String, MmError> {
    let bytes = read_regular_file(path, MAX_METADATA_BYTES)?.ok_or_else(|| MmError::Build {
        message: "Docker Buildx did not produce metadata".to_owned(),
    })?;
    let metadata: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| MmError::Build {
            message: format!("Docker Buildx metadata was invalid: {error}"),
        })?;
    let digest = metadata
        .get("containerimage.digest")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            metadata
                .get("containerimage.descriptor")
                .and_then(|descriptor| descriptor.get("digest"))
                .and_then(serde_json::Value::as_str)
        })
        .ok_or_else(|| MmError::Build {
            message: "Docker Buildx metadata did not contain an image digest".to_owned(),
        })?;
    if !is_sha256_digest(digest) {
        return Err(MmError::Build {
            message: "Docker Buildx returned an invalid image digest".to_owned(),
        });
    }
    Ok(digest.to_owned())
}

fn publish_archive(
    artifacts: &Path,
    operation_id: crate::domain::operation::OperationId,
    unit: &ImageBuildUnitPlan,
    variant: &ImageBuildVariantPlan,
    staged: &Path,
) -> Result<PathBuf, MmError> {
    if !staged.is_file() {
        return Err(MmError::Build {
            message: "Docker Buildx did not produce the requested OCI archive".to_owned(),
        });
    }
    let parent = operation_directory(artifacts, operation_id).join(&unit.module);
    prepare_private_directory(&parent)?;
    let name = if variant.tag_suffix.is_empty() {
        "default"
    } else {
        &variant.tag_suffix
    };
    let destination = parent.join(format!("{name}.oci.tar"));
    ensure_no_symlink_components(&destination)?;
    if destination.exists() {
        let metadata = fs::symlink_metadata(&destination).map_err(|source| MmError::Io {
            context: format!("inspecting OCI archive {}", destination.display()),
            source,
        })?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(MmError::Security {
                message: "managed OCI archive path is not a regular file".to_owned(),
            });
        }
        fs::remove_file(&destination).map_err(|source| MmError::Io {
            context: format!("replacing OCI archive {}", destination.display()),
            source,
        })?;
    }
    fs::rename(staged, &destination).map_err(|source| MmError::Io {
        context: format!("publishing OCI archive {}", destination.display()),
        source,
    })?;
    Ok(destination)
}

fn is_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

#[cfg(test)]
#[path = "buildx/tests.rs"]
mod tests;
