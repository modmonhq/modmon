use super::failure::{Asked, Builder};
use super::*;
use crate::domain::build::LocalImageEngine;
use crate::domain::runtime::PODMAN_INHERITED_ENVIRONMENT;

const IMAGE_MISSING: [&str; 4] = [
    "no such image",
    "image not known",
    "failed to find image",
    "no such object",
];

pub(super) fn command(engine: LocalImageEngine) -> CommandSpec {
    match engine {
        LocalImageEngine::Docker => docker_command("docker"),
        LocalImageEngine::Podman => {
            CommandSpec::new("podman").inherit_environment(PODMAN_INHERITED_ENVIRONMENT)
        }
    }
}

/// Builds into the engine's own image store: `docker build` runs on the
/// current Docker context's own builder, `podman build` on Podman's
/// storage.
pub(super) async fn build_variant(
    process: &dyn ProcessRunner,
    artifacts: &Path,
    engine: LocalImageEngine,
    request: BuildVariantRequest<'_>,
) -> Result<BuiltImage, MmError> {
    let staging = prepare_build_staging(artifacts, &request)?;
    let BuildVariantRequest {
        unit,
        variant,
        platforms,
        ..
    } = request;
    let tags = variant
        .tags
        .iter()
        .map(|tag| format!("{}:{tag}", unit.image))
        .collect::<Vec<_>>();
    let reference = tags.first().cloned().ok_or_else(|| MmError::Build {
        message: format!("module '{}' has no local image tag", unit.module),
    })?;
    let builder = match engine {
        LocalImageEngine::Docker => context_builder(process).await,
        LocalImageEngine::Podman => None,
    };
    let output = process
        .run(build_command(
            engine,
            &request,
            &staging,
            builder.as_deref(),
        ))
        .await
        .map_err(|error| super::failure::not_started(error, Builder::Engine(engine)))?;
    if !output.success() {
        let asked = Asked {
            platforms,
            publish: false,
        };
        return Err(super::failure::explain(
            process,
            Builder::Engine(engine),
            &output,
            Some(asked),
        )
        .await);
    }
    let digest = image_id(process, engine, &reference)
        .await?
        .ok_or_else(|| MmError::Build {
            message: format!(
                "{} built module '{}' but has no image {reference}",
                engine.name(),
                unit.module
            ),
        })?;
    Ok(BuiltImage {
        module: unit.module.clone(),
        target: variant.target.clone(),
        repository: unit.image.clone(),
        tags,
        digest,
        execution_fingerprint: String::new(),
        secret_revisions: BTreeMap::new(),
        archive: None,
        reused: false,
    })
}

/// The builder of the current Docker context's own engine, which has the
/// context's name, whatever `docker buildx use` selected. `None` when
/// Docker does not name its context.
async fn context_builder(process: &dyn ProcessRunner) -> Option<String> {
    let output = process
        .run(
            command(LocalImageEngine::Docker)
                .args(["context", "show"])
                .timeout(Duration::from_secs(30)),
        )
        .await
        .ok()?;
    let name = output.stdout.trim();
    let named = output.success()
        && name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_.+-".contains(character));
    named.then(|| name.to_owned())
}

fn build_command(
    engine: LocalImageEngine,
    request: &BuildVariantRequest<'_>,
    staging: &BuildStaging,
    builder: Option<&str>,
) -> CommandSpec {
    let mut command = command(engine)
        .arg("build")
        .arg("--file")
        .arg(staging.dockerfile.to_string_lossy())
        .arg("--platform")
        .arg(request.platforms.join(","))
        .current_dir(request.project_root)
        .timeout(Duration::from_secs(2 * 60 * 60));
    if engine == LocalImageEngine::Docker {
        command = command.environment("DOCKER_BUILDKIT", "1");
    }
    if let Some(builder) = builder {
        command = command.environment("BUILDX_BUILDER", builder);
    }
    if let Some(stage) = request.variant.dockerfile_stage() {
        command = command.arg("--target").arg(stage);
    }
    for (name, value) in request.unit.variant_args(request.variant) {
        command = command.arg("--build-arg").arg(format!("{name}={value}"));
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
    command.arg(staging.snapshot.to_string_lossy())
}

/// The engine's ID of `reference`, or `None` when the engine has no such
/// image.
pub(super) async fn image_id(
    process: &dyn ProcessRunner,
    engine: LocalImageEngine,
    reference: &str,
) -> Result<Option<String>, MmError> {
    let output = process
        .run(
            command(engine)
                .args(["image", "inspect", "--format", "{{.Id}}"])
                .arg(reference)
                .timeout(Duration::from_secs(60)),
        )
        .await
        .map_err(|error| super::failure::not_started(error, Builder::Engine(engine)))?;
    if !output.success() {
        let stderr = output.stderr.to_ascii_lowercase();
        if IMAGE_MISSING.iter().any(|missing| stderr.contains(missing)) {
            return Ok(None);
        }
        return Err(super::failure::explain(process, Builder::Engine(engine), &output, None).await);
    }
    let id = output.stdout.trim();
    let id = if id.starts_with("sha256:") {
        id.to_owned()
    } else {
        format!("sha256:{id}")
    };
    if !is_sha256_digest(&id) {
        return Err(MmError::Build {
            message: format!(
                "{} answered an image ID for {reference} that is not a sha256 digest",
                engine.name()
            ),
        });
    }
    Ok(Some(id))
}
