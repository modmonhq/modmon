//! Pulling the images a Compose project's services need before any of them
//! starts, under the deployment's pull limit instead of a start's.

use std::time::Duration;

use futures_util::StreamExt as _;
use tokio::time::Instant;

use crate::app::ports::{CommandSpec, Doing, ProcessRunner};
use crate::domain::error::MmError;
use crate::domain::runtime::{RuntimeKind, RuntimeTarget};

const LOOK_TIMEOUT: Duration = Duration::from_secs(45);
/// How many images a node looks at or pulls at once.
const PULLS_AT_ONCE: usize = 3;

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
    /// `pull_policy: always`, or one that refreshes an image by its age:
    /// pulled even when the engine has it.
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
        let wanted = self
            .wanted(target, compose, services, deadline, limit)
            .await?;
        let missing = self.missing(target, wanted, deadline, limit).await?;
        if missing.is_empty() {
            return Ok(());
        }
        let auth = super::registry_auth::RegistryAuth::prepare(&target.registry_credentials)?;
        let total = missing.len();
        let doing = Doing::now(format!(
            "pulling {} for stack '{}': {}",
            if total == 1 {
                "1 image".to_owned()
            } else {
                format!("{total} images")
            },
            target.stack_name,
            missing
                .iter()
                .map(|wanted| wanted.image.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        let mut pulling = Vec::with_capacity(missing.len());
        for wanted in &missing {
            pulling.push(self.pull_one(target, wanted, &auth, (deadline, limit)));
        }
        let pulls = futures_util::stream::iter(pulling)
            .buffered(PULLS_AT_ONCE)
            .collect::<Vec<_>>()
            .await;
        drop(doing);
        pulls
            .into_iter()
            .collect::<Result<Vec<()>, MmError>>()
            .map(drop)
    }

    async fn pull_one(
        &self,
        target: &RuntimeTarget,
        wanted: &WantedImage,
        auth: &super::registry_auth::RegistryAuth,
        (deadline, limit): (Instant, Duration),
    ) -> Result<(), MmError> {
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
            Ok(output) if output.success() => Ok(()),
            Ok(output) => Err(pull_failed(target, wanted, &output.stderr)),
            Err(MmError::ProcessTimeout { .. }) => Err(pull_timeout(target, wanted, limit)),
            Err(error) => Err(error),
        }
    }

    /// The images of `wanted` to pull: the ones pulled always, and the
    /// ones the engine lacks. One look tells whether the engine has every
    /// image; only when it lacks one is each looked at alone.
    async fn missing(
        &self,
        target: &RuntimeTarget,
        wanted: Vec<WantedImage>,
        deadline: Instant,
        limit: Duration,
    ) -> Result<Vec<WantedImage>, MmError> {
        let looked = wanted
            .iter()
            .filter(|wanted| !wanted.always)
            .collect::<Vec<_>>();
        let all_present = match looked.as_slice() {
            [] => true,
            [_] => false,
            many => self.present(target, many, deadline, limit).await?,
        };
        let mut absent = std::collections::BTreeSet::new();
        if !all_present {
            let mut looking = Vec::with_capacity(looked.len());
            for wanted in &looked {
                looking.push(self.present(target, std::slice::from_ref(wanted), deadline, limit));
            }
            let presence = futures_util::stream::iter(looking)
                .buffered(PULLS_AT_ONCE)
                .collect::<Vec<_>>()
                .await;
            for (wanted, present) in looked.iter().zip(presence) {
                if !present? {
                    absent.insert(wanted.image.clone());
                }
            }
        }
        Ok(wanted
            .into_iter()
            .filter(|wanted| wanted.always || absent.contains(&wanted.image))
            .collect())
    }

    async fn wanted(
        &self,
        target: &RuntimeTarget,
        mut compose: Vec<String>,
        services: Option<&[String]>,
        deadline: Instant,
        limit: Duration,
    ) -> Result<Vec<WantedImage>, MmError> {
        compose.push("config".to_owned());
        let look = look_within(deadline).ok_or_else(|| look_timeout(target, limit))?;
        let output = self
            .process
            .run(
                (self.base)(compose)
                    .interpolation_environment(&target.interpolation_environment)
                    .current_dir(&target.root)
                    .timeout(look)
                    .internal_stdout(),
            )
            .await
            .map_err(|error| past_deadline(error, look, || look_timeout(target, limit)))?;
        if !output.success() {
            return Err(MmError::ProcessFailed {
                program: self.program.to_owned(),
                code: output.code,
                stderr: output.stderr.trim().to_owned(),
            });
        }
        wanted_images(&output.stdout, services)
    }

    /// Whether the engine has every image of `wanted`.
    async fn present(
        &self,
        target: &RuntimeTarget,
        wanted: &[&WantedImage],
        deadline: Instant,
        limit: Duration,
    ) -> Result<bool, MmError> {
        let Some(first) = wanted.first() else {
            return Ok(true);
        };
        let look = look_within(deadline).ok_or_else(|| pull_timeout(target, first, limit))?;
        let mut arguments = vec![
            "image".to_owned(),
            "inspect".to_owned(),
            "--format".to_owned(),
            "{{.Id}}".to_owned(),
        ];
        arguments.extend(wanted.iter().map(|wanted| wanted.image.clone()));
        let output = self
            .process
            .run(
                (self.base)(arguments)
                    .timeout(look)
                    .with_output_limit(16 * 1024),
            )
            .await
            .map_err(|error| past_deadline(error, look, || pull_timeout(target, first, limit)))?;
        Ok(output.success())
    }
}

/// How long a look may take: `LOOK_TIMEOUT`, or less when the pull's
/// deadline comes first. `None` once the deadline passed.
fn look_within(deadline: Instant) -> Option<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    (!remaining.is_zero()).then(|| remaining.min(LOOK_TIMEOUT))
}

/// `late()` for a look that ran out of the pull's time rather than its own.
fn past_deadline(error: MmError, look: Duration, late: impl FnOnce() -> MmError) -> MmError {
    match error {
        MmError::ProcessTimeout { .. } if look < LOOK_TIMEOUT => late(),
        error => error,
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
        let always = !matches!(policy, "missing" | "if_not_present");
        let Some(image) = service
            .get("image")
            .and_then(serde_yaml_ng::Value::as_str)
            .filter(|image| !crate::domain::build::is_local_image_reference(image))
            .map(str::to_owned)
        else {
            continue;
        };
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

fn look_timeout(target: &RuntimeTarget, limit: Duration) -> MmError {
    MmError::Timeout {
        message: format!(
            "finding which images stack '{}' lacks did not finish within {} (deploy.pull-timeout); nothing of the stack was started. Raise `environments.{}.deploy.pull-timeout` in mm.yaml, or check that the container engine on this host answers",
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
