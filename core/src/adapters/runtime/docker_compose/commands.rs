use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::adapters::runtime::inspect::{
    ownership_label_args, parse_inspected_resource, parse_resource_ids,
    require_inspected_project_ownership,
};
use crate::adapters::runtime::observe::{
    normalize_observation, normalized_rendered_semantics,
    parse_compose_observation, parse_compose_service_names,
};
use crate::app::ports::{
    CommandOutput, CommandSpec, LocalDeploymentRuntime, OwnedVolumeFile, ProcessRunner,
    RuntimeDriver, VolumeOwnership,
};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::error::MmError;
use crate::domain::ownership::{
    ResourceKind, ResourceOwnership, validate_resource_label, validate_resource_name,
};
use crate::domain::runtime::{
    ConvergenceCancellation, ConvergenceOutcome, ConvergencePolicy,
    DOCKER_INHERITED_ENVIRONMENT, RenderedProject, ResourceEnsureResult, RuntimeKind, RuntimeProbe,
    RuntimeResource, RuntimeTarget, runtime_descriptor,
};
use crate::domain::target::{NamedTarget, TargetTransport};

const VOLUME_HELPER_IMAGE: &str = "docker.io/library/busybox@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0";

#[derive(Clone)]
pub struct DockerComposeRuntime {
    process: Arc<dyn ProcessRunner>,
    /// The engine's API socket, when reads go there instead of to the CLI.
    pub(super) engine: Option<crate::adapters::engine_api::EngineApi>,
}

impl std::fmt::Debug for DockerComposeRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DockerComposeRuntime")
            .finish_non_exhaustive()
    }
}

impl DockerComposeRuntime {
    #[must_use]
    pub fn new(process: Arc<dyn ProcessRunner>) -> Self {
        Self {
            process,
            engine: None,
        }
    }

    /// Reads what the engine's API can answer from its socket at `engine`
    /// instead of starting the CLI: a status's containers, the engine's
    /// version, a network's addresses. `None` keeps the CLI.
    #[must_use]
    pub fn reading_engine_api(
        mut self,
        engine: Option<crate::adapters::engine_api::EngineApi>,
    ) -> Self {
        self.engine = engine;
        self
    }

    fn command_spec(&self, args: impl IntoIterator<Item = impl Into<String>>) -> CommandSpec {
        CommandSpec::new("docker")
            .args(args)
            .inherit_environment(DOCKER_INHERITED_ENVIRONMENT)
    }

    async fn command(&self, args: Vec<String>) -> Result<CommandOutput, MmError> {
        self.process
            .run(self.command_spec(args).timeout(Duration::from_secs(30)))
            .await
    }

    /// Every container of the project, running or not.
    pub(super) async fn containers(
        &self,
        target: &RuntimeTarget,
    ) -> Result<Vec<crate::domain::runtime::ContainerObservation>, MmError> {
        let mut args = self.compose_args(target)?;
        args.extend([
            "ps".to_owned(),
            "--all".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ]);
        let output = self
            .process
            .run(
                self.command_spec(args)
                    .interpolation_environment(&target.interpolation_environment)
                    .current_dir(&target.root)
                    .timeout(Duration::from_secs(30))
                    .internal_stdout(),
            )
            .await?;
        let output = self.ensure_success(output)?;
        parse_compose_observation(&output, "Docker")
    }

    fn ensure_success(&self, output: CommandOutput) -> Result<CommandOutput, MmError> {
        if output.success() {
            return Ok(output);
        }
        Err(MmError::ProcessFailed {
            program: "docker".to_owned(),
            code: output.code,
            stderr: output.stderr.trim().to_owned(),
        })
    }

    fn compose_args(&self, target: &RuntimeTarget) -> Result<Vec<String>, MmError> {
        if target.compose_files.is_empty() {
            return Err(MmError::Security {
                message: "refusing to mutate a Compose project without explicit Compose files"
                    .to_owned(),
            });
        }
        let mut args = vec!["compose".to_owned()];
        for compose_file in &target.compose_files {
            args.push("--file".to_owned());
            args.push(compose_file.to_string_lossy().into_owned());
        }
        args.extend([
            "--project-directory".to_owned(),
            target.project_directory.to_string_lossy().into_owned(),
            "--project-name".to_owned(),
            target.project_name.clone(),
        ]);
        Ok(args)
    }

    fn ownership(
        target: &NamedTarget,
        kind: ResourceKind,
        name: &str,
        resource: &str,
    ) -> Result<ResourceOwnership, MmError> {
        require_local_named_target(target)?;
        validate_resource_name(name)?;
        validate_resource_label(resource)?;
        Ok(ResourceOwnership::for_named_target(
            target, kind, name, resource,
        ))
    }

    async fn inspect_owned(
        &self,
        ownership: &ResourceOwnership,
    ) -> Result<Option<serde_json::Value>, MmError> {
        let output = self
            .command(vec![
                ownership.kind.as_str().to_owned(),
                "inspect".to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
                ownership.logical_name.clone(),
            ])
            .await?;
        if !output.success() {
            if is_missing_resource(&output) {
                return Ok(None);
            }
            self.ensure_success(output)?;
            return Err(MmError::RuntimeResponse {
                message: "Docker inspect returned an invalid result".to_owned(),
            });
        }
        Ok(Some(parse_inspected_resource(output, ownership, "Docker")?))
    }

    async fn preflight_project(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        let ownership = ResourceOwnership::for_runtime_target(
            target,
            ResourceKind::Stack,
            target.project_name.clone(),
            target.stack_name.clone(),
        )
        .ok_or_else(|| MmError::Security {
            message: "runtime project has no stable target identity".to_owned(),
        })?;
        let output = self
            .command(vec![
                "ps".to_owned(),
                "--all".to_owned(),
                "--filter".to_owned(),
                format!("label=com.docker.compose.project={}", target.project_name),
                "--format".to_owned(),
                "json".to_owned(),
            ])
            .await?;
        let output = self.ensure_success(output)?;
        for id in parse_resource_ids(&output, "Docker", "container")? {
            let inspected = self
                .command(vec![
                    "container".to_owned(),
                    "inspect".to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                    id,
                ])
                .await?;
            require_inspected_project_ownership(
                self.ensure_success(inspected)?,
                &ownership,
                "Docker",
            )?;
        }
        Ok(())
    }

    async fn run_volume_helper(
        &self,
        volume_name: &str,
        source: Option<&Path>,
        file_name: &str,
        mode: u32,
        script: &str,
    ) -> Result<(), MmError> {
        validate_volume_file(file_name)?;
        if let Some(source) = source {
            return self
                .copy_volume_file(volume_name, source, file_name, mode, 0, 0)
                .await;
        }
        let mut args = vec!["run".to_owned(), "--rm".to_owned()];
        args.extend(Self::volume_helper_args(volume_name));
        args.extend([
            VOLUME_HELPER_IMAGE.to_owned(),
            "sh".to_owned(),
            "-c".to_owned(),
            script.to_owned(),
            "mm-volume-helper".to_owned(),
            file_name.to_owned(),
            format!("{mode:o}"),
        ]);
        let output = self
            .process
            .run(self.command_spec(args).timeout(Duration::from_secs(90)))
            .await?;
        self.ensure_success(output)?;
        Ok(())
    }

    fn volume_helper_args(volume_name: &str) -> Vec<String> {
        vec![
            "--network".to_owned(),
            "none".to_owned(),
            "--cpus".to_owned(),
            "0.25".to_owned(),
            "--memory".to_owned(),
            "64m".to_owned(),
            "--pids-limit".to_owned(),
            "32".to_owned(),
            "--read-only".to_owned(),
            "--cap-drop".to_owned(),
            "ALL".to_owned(),
            "--security-opt".to_owned(),
            "no-new-privileges".to_owned(),
            "--volume".to_owned(),
            format!("{volume_name}:/volume"),
        ]
    }

    async fn copy_volume_file(
        &self,
        volume_name: &str,
        source: &Path,
        file_name: &str,
        mode: u32,
        uid: u32,
        gid: u32,
    ) -> Result<(), MmError> {
        let source = canonical_volume_source(source)?;
        let staging_name = format!(
            ".mm-source-{}",
            &blake3::hash(file_name.as_bytes()).to_hex()[..16]
        );
        let mut create = vec!["create".to_owned()];
        create.extend(Self::volume_helper_args(volume_name));
        create.extend([
            "--cap-add".to_owned(),
            "CHOWN".to_owned(),
            "--cap-add".to_owned(),
            "FOWNER".to_owned(),
        ]);
        create.extend([
            VOLUME_HELPER_IMAGE.to_owned(),
            "sh".to_owned(),
            "-c".to_owned(),
            "set -eu; trap 'rm -f -- \"/volume/$1\"' EXIT; chown \"$4:$5\" \"/volume/$1\"; chmod \"$2\" \"/volume/$1\"; mv -f \"/volume/$1\" \"/volume/$3\"".to_owned(),
            "mm-volume-helper".to_owned(),
            staging_name.clone(),
            format!("{mode:o}"),
            file_name.to_owned(),
            uid.to_string(),
            gid.to_string(),
        ]);
        let created = self
            .process
            .run(self.command_spec(create).timeout(Duration::from_secs(90)))
            .await?;
        let created = self.ensure_success(created)?;
        let container_id = created.stdout.trim();
        if container_id.is_empty()
            || container_id.len() > 64
            || !container_id
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err(MmError::RuntimeResponse {
                message: "Docker helper create returned an invalid container ID".to_owned(),
            });
        }
        let copied = self
            .command(vec![
                "cp".to_owned(),
                source.display().to_string(),
                format!("{container_id}:/volume/{staging_name}"),
            ])
            .await;
        if let Err(error) = copied.and_then(|output| self.ensure_success(output).map(|_| ())) {
            let cleanup = self.remove_volume_helper(container_id).await;
            return Err(match cleanup {
                Ok(()) => error,
                Err(cleanup) => MmError::operation_failure(
                    container_id,
                    crate::domain::error::MutationStatus::NotStarted,
                    error,
                    vec![cleanup.to_string()],
                    "remove the owned Docker helper container before retrying",
                ),
            });
        }
        let started = self
            .process
            .run(
                self.command_spec(vec![
                    "start".to_owned(),
                    "--attach".to_owned(),
                    container_id.to_owned(),
                ])
                .timeout(Duration::from_secs(90)),
            )
            .await;
        let removed = self.remove_volume_helper(container_id).await;
        let started = started.and_then(|output| self.ensure_success(output).map(|_| ()));
        started?;
        removed?;
        Ok(())
    }

    async fn remove_volume_helper(&self, container_id: &str) -> Result<(), MmError> {
        self.command(vec![
            "rm".to_owned(),
            "--force".to_owned(),
            container_id.to_owned(),
        ])
        .await
        .and_then(|output| self.ensure_success(output).map(|_| ()))
    }
}

fn canonical_volume_source(source: &Path) -> Result<std::path::PathBuf, MmError> {
    let source = dunce::canonicalize(source).map_err(|source_error| MmError::Io {
        context: format!("resolving volume file source {}", source.display()),
        source: source_error,
    })?;
    if !source.is_file() {
        return Err(MmError::Security {
            message: format!(
                "volume file source '{}' is not a regular file",
                source.display()
            ),
        });
    }
    Ok(source)
}
