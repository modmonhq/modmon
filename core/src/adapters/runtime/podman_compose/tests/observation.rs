    use std::collections::VecDeque;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use serde_json::json;
    use zeroize::Zeroizing;

    use super::{
        PodmanComposeRuntime, container_from_json, parse_provider_executable, provider_name,
    };
    use crate::app::ports::{
        CommandOutput, CommandSpec, LocalDeploymentRuntime, OwnedVolumeFile, ProcessRunner,
        RuntimeDriver, VolumeOwnership,
    };
    use crate::domain::error::MmError;
    use crate::domain::runtime::{ResourceEnsureResult, RuntimeCapability, RuntimeTarget};
    use crate::domain::target::{
        NamedTarget, TargetBootstrap, TargetDefinition, TargetId, TargetTransport,
    };
    use uuid::Uuid;

    const TARGET_UUID: u128 = 0x319c_527b_6242_7b43_bfd4_bd60_e27c_1de3;

    #[derive(Debug, PartialEq, Eq)]
    struct RecordedCommand {
        program: String,
        args: Vec<String>,
        current_dir: Option<PathBuf>,
        internal_stdout: bool,
    }

    #[derive(Debug)]
    struct RecordingRunner {
        outputs: Mutex<VecDeque<CommandOutput>>,
        commands: Mutex<Vec<RecordedCommand>>,
    }

    #[async_trait]
    impl ProcessRunner for RecordingRunner {
        async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
            let command = RecordedCommand {
                program: specification.program().to_owned(),
                args: specification.arguments().map(str::to_owned).collect(),
                current_dir: specification.current_directory().cloned(),
                internal_stdout: specification.keeps_internal_stdout(),
            };
            self.commands
                .lock()
                .map_err(|_| MmError::State {
                    message: "recorded command lock was poisoned".to_owned(),
                })?
                .push(command);
            self.outputs
                .lock()
                .map_err(|_| MmError::State {
                    message: "recorded output lock was poisoned".to_owned(),
                })?
                .pop_front()
                .ok_or_else(|| MmError::State {
                    message: "recording runner has no output".to_owned(),
                })
        }
    }

    #[test]
    fn identifies_the_external_compose_provider() {
        let stderr = r#">>>> Executing external compose provider "/usr/libexec/docker/cli-plugins/docker-compose". <<<<"#;
        assert_eq!(
            parse_provider_executable(stderr).as_deref(),
            Some(Path::new("/usr/libexec/docker/cli-plugins/docker-compose"))
        );
        assert_eq!(
            provider_name("Docker Compose version v5.4.0"),
            "docker-compose"
        );
    }

    #[test]
    fn reads_compose_identity_from_podman_json() {
        let value = json!({
            "Id": "abc123",
            "Names": ["minimal-api-1"],
            "Image": "alpine:3.22",
            "State": "running",
            "Status": "Up 10 seconds",
            "Labels": {
                "com.docker.compose.project": "minimal",
                "com.docker.compose.service": "api",
                "example.secret": "must-not-escape"
            }
        });
        let container = container_from_json(&value);
        assert!(container.is_some());
        if let Some(container) = container {
            assert_eq!(container.service, "api");
            assert_eq!(container.name, "minimal-api-1");
            assert!(!container.labels.contains_key("example.secret"));
        }
    }

    #[test]
    fn reads_health_from_podman_status() {
        let value = json!({
            "Id": "abc123",
            "Names": ["minimal-api-1"],
            "Image": "alpine:3.22",
            "State": "running",
            "Status": "Up 10 seconds (unhealthy)",
            "Labels": {
                "com.docker.compose.project": "minimal",
                "com.docker.compose.service": "api"
            }
        });
        let container = container_from_json(&value);
        assert_eq!(
            container.and_then(|container| container.health),
            Some("unhealthy".to_owned())
        );
    }
    #[tokio::test]
    async fn renders_and_observes_one_stack_with_overlay_last()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from("/workspace");
        let runner = Arc::new(RecordingRunner {
            outputs: Mutex::new(
                [
                    command_output("services:\n  api:\n    image: alpine:3.22\n"),
                    command_output("services:\n  api:\n    image: alpine:3.22\n"),
                    command_output("[]"),
                ]
                .into_iter()
                .collect(),
            ),
            commands: Mutex::new(Vec::new()),
        });
        let runtime = PodmanComposeRuntime::with_executable(runner.clone(), "podman");
        let target = RuntimeTarget {
            root: root.clone(),
            project_directory: root.join("deploy"),
            target: Some(NamedTarget::new(
                "local".to_owned(),
                TargetDefinition {
                    id: TargetId(Uuid::parse_str("319c527b-6242-7b43-bfd4-bd60e27c1de3")?),
                    transport: TargetTransport::Local,
                    bootstrap: TargetBootstrap::Ansible,
                    ingress: None,
                    default_runtime: None,
                    available_runtimes: Vec::new(),
                    runtime: None,
                    nodes: Vec::new(),
                },
            )),
            project_id: crate::domain::workspace::ProjectId(Uuid::parse_str(
                "019c527b-6242-7b43-bfd4-bd60e27c1de3",
            )?),
            environment: "local".to_owned(),
            stack_name: "app".to_owned(),
            project_name: "demo-local-app".to_owned(),
            artifact_hash: "artifact-hash".to_owned(),
            compose_files: vec![
                root.join("compose.yaml"),
                root.join("compose.override.yaml"),
                std::path::PathBuf::from("/mm/compose/mm.overlay.yaml"),
            ],
            user_compose_file_count: 2,
            interpolation_environment: Default::default(),
            installation_scoped: false,
            registry_credentials: Vec::new(),
        };

        let rendered = runtime.render(&target).await?;
        let observed = runtime.observe(&target).await?;
        assert_eq!(rendered.stack, "app");
        assert_eq!(observed.stack, "app");
        let commands = runner
            .commands
            .lock()
            .map_err(|_| "command lock poisoned")?;
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[0].args,
            vec![
                "compose",
                "--file",
                "/workspace/compose.yaml",
                "--file",
                "/workspace/compose.override.yaml",
                "--file",
                "/mm/compose/mm.overlay.yaml",
                "--project-directory",
                "/workspace/deploy",
                "--project-name",
                "demo-local-app",
                "config",
                "--no-interpolate",
            ]
        );
        assert_eq!(commands[0].current_dir.as_deref(), Some(root.as_path()));
        assert!(commands[0].internal_stdout);
        assert!(commands[2].internal_stdout);
        assert_eq!(
            commands[2].args,
            vec![
                "ps",
                "--all",
                "--filter",
                "label=com.docker.compose.project=demo-local-app",
                "--format",
                "json",
            ]
        );
        Ok(())
    }
    #[tokio::test]
    async fn advertises_apply_when_local_compose_is_available()
    -> Result<(), Box<dyn std::error::Error>> {
        let runner = recording_runner([
            command_output("podman version 5.5.0\n"),
            command_output(
                r#"{"host":{"security":{"rootless":true},"remoteSocket":{"path":"/run/user/1000/podman.sock"}},"version":{"Version":"5.5.0"}}"#,
            ),
            command_output("Docker Compose version v2.39.2\n"),
        ]);
        let runtime =
            PodmanComposeRuntime::with_environment(runner, "podman-test", empty_environment);

        let probe = runtime.probe().await;

        assert!(probe.available);
        assert_eq!(
            probe.capabilities,
            vec![
                RuntimeCapability::OneShotJobs,
                RuntimeCapability::StreamLogs,
                RuntimeCapability::Exec,
                RuntimeCapability::RuntimeSecrets,
                RuntimeCapability::BindMounts,
                RuntimeCapability::DirectEnvironment,
                RuntimeCapability::RenderCompose,
                RuntimeCapability::ApplyCompose,
                RuntimeCapability::ObserveServices,
                RuntimeCapability::ContainerHealth,
                RuntimeCapability::ManageNetworks,
                RuntimeCapability::ManageVolumes,
                RuntimeCapability::MutateVolumeFiles,
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn creates_owned_resources_with_exact_labels_and_reports_unchanged()
    -> Result<(), Box<dyn std::error::Error>> {
        let target = local_target();
        let target_id = target.definition.id.0.to_string();
        let runner = recording_runner([
            command_output_with_code(1, ""),
            command_output("mm-ingress\n"),
            command_output(""),
            owned_volume_output(
                "mm-ingress-acme",
                "/storage/mm-ingress-acme/_data",
                "ingress-acme",
            ),
        ]);
        let runtime = PodmanComposeRuntime::with_environment(
            runner.clone(),
            "podman-test",
            empty_environment,
        );

        let network = runtime
            .ensure_network(&target, "mm-ingress", "ingress-network")
            .await?;
        let volume = runtime
            .ensure_volume(&target, "mm-ingress-acme", "ingress-acme")
            .await?;

        assert_eq!(network, ResourceEnsureResult::Created);
        assert_eq!(volume, ResourceEnsureResult::Unchanged);
        let commands = runner
            .commands
            .lock()
            .map_err(|_| std::io::Error::other("command lock poisoned"))?;
        let expected = vec![
            recorded(arguments(&["network", "exists", "mm-ingress"]), None),
            recorded(
                vec![
                    "network".to_owned(),
                    "create".to_owned(),
                    "--label".to_owned(),
                    "io.mm.generation=1".to_owned(),
                    "--label".to_owned(),
                    "io.mm.resource=ingress-network".to_owned(),
                    "--label".to_owned(),
                    "io.mm.resource-kind=network".to_owned(),
                    "--label".to_owned(),
                    format!("io.mm.target-id={target_id}"),
                    "mm-ingress".to_owned(),
                ],
                None,
            ),
            recorded(arguments(&["volume", "exists", "mm-ingress-acme"]), None),
            recorded(
                arguments(&["volume", "inspect", "--format", "json", "mm-ingress-acme"]),
                None,
            ),
        ];
        assert_eq!(commands.as_slice(), expected.as_slice());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_an_unmanaged_resource_name_collision() -> Result<(), Box<dyn std::error::Error>>
    {
        let runner = recording_runner([
            command_output(""),
            command_output(
                &json!([{
                    "name": "mm-ingress",
                    "labels": {"io.mm.target-id": Uuid::nil().to_string()}
                }])
                .to_string(),
            ),
        ]);
        let runtime = PodmanComposeRuntime::with_environment(
            runner.clone(),
            "podman-test",
            empty_environment,
        );

        let result = runtime
            .ensure_network(&local_target(), "mm-ingress", "ingress-network")
            .await;

        assert!(matches!(result, Err(MmError::Security { .. })));
        let commands = runner
            .commands
            .lock()
            .map_err(|_| std::io::Error::other("command lock poisoned"))?;
        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[1].args,
            arguments(&["network", "inspect", "--format", "json", "mm-ingress"])
        );
        Ok(())
    }

    #[tokio::test]
    async fn rejects_every_remote_engine_environment_without_running_podman()
    -> Result<(), Box<dyn std::error::Error>> {
        let lookups: [fn(&str) -> Option<OsString>; 4] = [
            container_host_environment,
            container_connection_environment,
            docker_host_environment,
            docker_context_environment,
        ];
        for lookup in lookups {
            let runner = recording_runner(Vec::<CommandOutput>::new());
            let runtime =
                PodmanComposeRuntime::with_environment(runner.clone(), "podman-test", lookup);

            let result = runtime
                .ensure_network(&local_target(), "mm-ingress", "ingress-network")
                .await;

            assert!(matches!(result, Err(MmError::Security { .. })));
            let commands = runner
                .commands
                .lock()
                .map_err(|_| std::io::Error::other("command lock poisoned"))?;
            assert!(commands.is_empty());
        }
        Ok(())
    }

    #[tokio::test]
    async fn applies_ordered_compose_files_and_removes_without_volumes()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = PathBuf::from("/workspace");
        let target = deployment_target(&root);
        let runner = recording_runner([
            command_output("[]"),
            command_output(""),
            command_output("[]"),
            command_output(""),
        ]);
        let runtime = PodmanComposeRuntime::with_environment(
            runner.clone(),
            "podman-test",
            empty_environment,
        );

        runtime.apply(&target).await?;
        runtime.remove(&target).await?;

        let commands = runner
            .commands
            .lock()
            .map_err(|_| std::io::Error::other("command lock poisoned"))?;
        let compose_prefix = [
            "compose",
            "--file",
            "/workspace/compose.yaml",
            "--file",
            "/workspace/compose.override.yaml",
            "--file",
            "/mm/compose/mm.overlay.yaml",
            "--project-directory",
            "/workspace/deploy",
            "--project-name",
            "demo-local-app",
        ];
        let expected = vec![
            recorded(
                arguments(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=com.docker.compose.project=demo-local-app",
                    "--format",
                    "json",
                ]),
                None,
            ),
            recorded(
                compose_prefix
                    .iter()
                    .copied()
                    .chain(["up", "-d", "--remove-orphans", "--no-build"])
                    .map(str::to_owned)
                    .collect(),
                Some(root.clone()),
            ),
            recorded(
                arguments(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=com.docker.compose.project=demo-local-app",
                    "--format",
                    "json",
                ]),
                None,
            ),
            recorded(
                compose_prefix
                    .iter()
                    .copied()
                    .chain(["down", "--remove-orphans"])
                    .map(str::to_owned)
                    .collect(),
                Some(root),
            ),
        ];
        assert_eq!(commands.as_slice(), expected.as_slice());
        assert!(
            !commands[3]
                .args
                .iter()
                .any(|argument| matches!(argument.as_str(), "--volumes" | "-v"))
        );
        Ok(())
    }
