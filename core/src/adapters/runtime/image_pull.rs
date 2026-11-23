//! Pulling the images a Compose project's services need before any of them
//! starts, under the deployment's pull limit instead of a start's.

use std::time::Duration;

use tokio::time::Instant;

use crate::app::ports::{CommandSpec, Doing, ProcessRunner};
use crate::domain::error::MmError;
use crate::domain::runtime::{RuntimeKind, RuntimeTarget};

const LOOK_TIMEOUT: Duration = Duration::from_secs(45);

/// One engine's way to run itself: `podman` or `docker` with the
/// environment it inherits.
pub(crate) struct ImagePuller<'a> {
    pub process: &'a dyn ProcessRunner,
    pub kind: RuntimeKind,
    pub program: &'a str,
    pub base: &'a (dyn Fn(Vec<String>) -> CommandSpec + Send + Sync),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WantedImage {
    pub image: String,
    pub services: Vec<String>,
    /// `pull_policy: always`: pulled even when the engine has it.
    pub always: bool,
}

impl ImagePuller<'_> {
    /// Pulls the images of `services` (every service when `None`) of the
    /// project that `compose` (the `compose --file ...` arguments) names,
    /// that the engine lacks, within `limit` for all of them.
    pub(crate) async fn pull(
        &self,
        target: &RuntimeTarget,
        compose: Vec<String>,
        services: Option<&[String]>,
        limit: Duration,
    ) -> Result<(), MmError> {
        let deadline = Instant::now() + limit;
        let mut missing = Vec::new();
        for wanted in self.wanted(target, compose, services).await? {
            if wanted.always || !self.present(&wanted.image).await? {
                missing.push(wanted);
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        let auth = super::registry_auth::RegistryAuth::prepare(&target.registry_credentials)?;
        let total = missing.len();
        let line = |index: usize| {
            format!(
                "pulling image {} of {total} for stack '{}': {}",
                index + 1,
                target.stack_name,
                missing[index].image
            )
        };
        let doing = Doing::now(line(0));
        for (index, wanted) in missing.iter().enumerate() {
            doing.update(line(index));
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(pull_timeout(target, wanted, limit));
            }
            let pulled = self
                .process
                .run(
                    auth.for_runtime(
                        self.kind,
                        (self.base)(vec![
                            "pull".to_owned(),
                            "--quiet".to_owned(),
                            qualified(&wanted.image),
                        ]),
                    )
                    .timeout(remaining)
                    .with_output_limit(16 * 1024),
                )
                .await;
            match pulled {
                Ok(output) if output.success() => {}
                Ok(output) => return Err(pull_failed(target, wanted, &output.stderr)),
                Err(MmError::ProcessTimeout { .. }) => {
                    return Err(pull_timeout(target, wanted, limit));
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    async fn wanted(
        &self,
        target: &RuntimeTarget,
        mut compose: Vec<String>,
        services: Option<&[String]>,
    ) -> Result<Vec<WantedImage>, MmError> {
        compose.push("config".to_owned());
        let output = self
            .process
            .run(
                (self.base)(compose)
                    .interpolation_environment(&target.interpolation_environment)
                    .current_dir(&target.root)
                    .timeout(LOOK_TIMEOUT)
                    .internal_stdout(),
            )
            .await?;
        if !output.success() {
            return Err(MmError::ProcessFailed {
                program: self.program.to_owned(),
                code: output.code,
                stderr: output.stderr.trim().to_owned(),
            });
        }
        wanted_images(&output.stdout, services)
    }

    async fn present(&self, image: &str) -> Result<bool, MmError> {
        let output = self
            .process
            .run(
                (self.base)(vec![
                    "image".to_owned(),
                    "inspect".to_owned(),
                    "--format".to_owned(),
                    "{{.Id}}".to_owned(),
                    image.to_owned(),
                ])
                .timeout(LOOK_TIMEOUT)
                .with_output_limit(16 * 1024),
            )
            .await?;
        Ok(output.success())
    }
}

/// The images that the rendered Compose configuration `config` gives
/// `services` (every service when `None`), each once. A service with a
/// `build` section or an image mm built, or whose `pull_policy` is `never`
/// or `build`, pulls nothing.
pub(crate) fn wanted_images(
    config: &str,
    services: Option<&[String]>,
) -> Result<Vec<WantedImage>, MmError> {
    let parsed: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(config).map_err(|error| MmError::RuntimeResponse {
            message: format!("rendered Compose configuration is not YAML: {error}"),
        })?;
    let mut wanted = Vec::<WantedImage>::new();
    let Some(mapping) = parsed
        .get("services")
        .and_then(serde_yaml_ng::Value::as_mapping)
    else {
        return Ok(wanted);
    };
    for (name, service) in mapping {
        let Some(name) = name.as_str() else {
            continue;
        };
        if services.is_some_and(|services| !services.iter().any(|wanted| wanted == name)) {
            continue;
        }
        let policy = service
            .get("pull_policy")
            .and_then(serde_yaml_ng::Value::as_str)
            .unwrap_or("missing");
        if service.get("build").is_some() || matches!(policy, "never" | "build") {
            continue;
        }
        let Some(image) = service
            .get("image")
            .and_then(serde_yaml_ng::Value::as_str)
            .filter(|image| !crate::domain::build::is_local_image_reference(image))
            .map(str::to_owned)
        else {
            continue;
        };
        let always = policy == "always";
        match wanted.iter_mut().find(|known| known.image == image) {
            Some(known) => {
                known.services.push(name.to_owned());
                known.always |= always;
            }
            None => wanted.push(WantedImage {
                image,
                services: vec![name.to_owned()],
                always,
            }),
        }
    }
    Ok(wanted)
}

/// `image` as Docker's API reads it: a name without a registry is on
/// `docker.io`, and one without a namespace there is in `library`. Podman's
/// CLI would otherwise ask which registry a short name means.
#[must_use]
pub(crate) fn qualified(image: &str) -> String {
    match image.split_once('/') {
        None => format!("docker.io/library/{image}"),
        Some((first, _)) if first.contains('.') || first.contains(':') || first == "localhost" => {
            image.to_owned()
        }
        Some(_) => format!("docker.io/{image}"),
    }
}

fn pull_failed(target: &RuntimeTarget, wanted: &WantedImage, stderr: &str) -> MmError {
    let why = stderr
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or("the engine gave no reason");
    MmError::Launch {
        message: format!(
            "could not pull image {} for {} of stack '{}': {why}. Check the image name and tag, and for a private registry its login under `registries` in mm.yaml; nothing of the stack was started",
            wanted.image,
            services_phrase(&wanted.services),
            target.stack_name
        ),
    }
}

fn pull_timeout(target: &RuntimeTarget, wanted: &WantedImage, limit: Duration) -> MmError {
    MmError::Timeout {
        message: format!(
            "pulling image {} for {} of stack '{}' did not finish within {} (deploy.pull-timeout); nothing of the stack was started. The layers that arrived stay, so a new deployment carries on from them. Raise `environments.{}.deploy.pull-timeout` in mm.yaml, or check how fast this host reaches the registry",
            wanted.image,
            services_phrase(&wanted.services),
            target.stack_name,
            limit_phrase(limit),
            target.environment
        ),
    }
}

fn services_phrase(services: &[String]) -> String {
    match services {
        [one] => format!("service '{one}'"),
        many => format!("services '{}'", many.join("', '")),
    }
}

fn limit_phrase(limit: Duration) -> String {
    let seconds = limit.as_secs();
    if seconds.is_multiple_of(60) {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}
