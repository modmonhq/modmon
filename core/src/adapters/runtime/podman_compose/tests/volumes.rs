    #[tokio::test]
    async fn creates_a_volume_file_with_a_no_clobber_rename()
    -> Result<(), Box<dyn std::error::Error>> {
        let mountpoint = "/storage/mm-ingress-acme/_data";
        let mut outputs = vec![owned_volume_output(
            "mm-ingress-acme",
            mountpoint,
            "ingress-acme",
        )];
        outputs.extend((0..6).map(|_| command_output("")));
        let runner = recording_runner(outputs);
        let runtime = PodmanComposeRuntime::with_environment(
            runner.clone(),
            "podman-test",
            empty_environment,
        );

        runtime
            .create_volume_file_if_absent(
                &local_target(),
                "mm-ingress-acme",
                "ingress-acme",
                "acme.json",
                0o600,
            )
            .await?;

        let commands = runner
            .commands
            .lock()
            .map_err(|_| std::io::Error::other("command lock poisoned"))?;
        assert_eq!(commands.len(), 7);
        let temporary = commands[2]
            .args
            .get(7)
            .cloned()
            .ok_or_else(|| std::io::Error::other("install command omitted its destination"))?;
        let expected = vec![
            recorded(
                arguments(&["volume", "inspect", "--format", "json", "mm-ingress-acme"]),
                None,
            ),
            recorded(
                arguments(&["unshare", "mkdir", "--parents", "--", mountpoint]),
                None,
            ),
            recorded(
                vec![
                    "unshare".to_owned(),
                    "install".to_owned(),
                    "--mode".to_owned(),
                    "0600".to_owned(),
                    "--no-target-directory".to_owned(),
                    "--".to_owned(),
                    "/dev/null".to_owned(),
                    temporary.clone(),
                ],
                None,
            ),
            recorded(
                vec![
                    "unshare".to_owned(),
                    "sync".to_owned(),
                    "--file-system".to_owned(),
                    "--".to_owned(),
                    temporary.clone(),
                ],
                None,
            ),
            recorded(
                vec![
                    "unshare".to_owned(),
                    "mv".to_owned(),
                    "--no-clobber".to_owned(),
                    "--no-target-directory".to_owned(),
                    "--".to_owned(),
                    temporary.clone(),
                    format!("{mountpoint}/acme.json"),
                ],
                None,
            ),
            recorded(
                vec![
                    "unshare".to_owned(),
                    "rm".to_owned(),
                    "--force".to_owned(),
                    "--".to_owned(),
                    temporary,
                ],
                None,
            ),
            recorded(
                arguments(&["unshare", "sync", "--file-system", "--", mountpoint]),
                None,
            ),
        ];
        assert_eq!(commands.as_slice(), expected.as_slice());
        Ok(())
    }

    #[tokio::test]
    async fn atomically_replaces_a_volume_file_from_a_verified_source()
    -> Result<(), Box<dyn std::error::Error>> {
        let source_directory = crate::test_support::tempdir()?;
        let source = source_directory.path().join("dynamic.yaml");
        fs::write(&source, "http:\n  routers: {}\n")?;
        let canonical_source = dunce::canonicalize(&source)?;
        let mountpoint = "/storage/mm-ingress-config/_data";
        let mut outputs = vec![owned_volume_output(
            "mm-ingress-config",
            mountpoint,
            "ingress-dynamic-config",
        )];
        outputs.extend((0..5).map(|_| command_output("")));
        let runner = recording_runner(outputs);
        let runtime = PodmanComposeRuntime::with_environment(
            runner.clone(),
            "podman-test",
            empty_environment,
        );

        runtime
            .replace_volume_file(
                &local_target(),
                "mm-ingress-config",
                "ingress-dynamic-config",
                "dynamic.yaml",
                &source,
                0o644,
            )
            .await?;

        let commands = runner
            .commands
            .lock()
            .map_err(|_| std::io::Error::other("command lock poisoned"))?;
        assert_eq!(commands.len(), 6);
        let temporary = commands[2]
            .args
            .get(7)
            .cloned()
            .ok_or_else(|| std::io::Error::other("install command omitted its destination"))?;
        let source_argument = canonical_source
            .to_str()
            .ok_or_else(|| std::io::Error::other("temporary source path is not Unicode"))?;
        let expected = vec![
            recorded(
                arguments(&["volume", "inspect", "--format", "json", "mm-ingress-config"]),
                None,
            ),
            recorded(
                arguments(&["unshare", "mkdir", "--parents", "--", mountpoint]),
                None,
            ),
            recorded(
                vec![
                    "unshare".to_owned(),
                    "install".to_owned(),
                    "--mode".to_owned(),
                    "0644".to_owned(),
                    "--no-target-directory".to_owned(),
                    "--".to_owned(),
                    source_argument.to_owned(),
                    temporary.clone(),
                ],
                None,
            ),
            recorded(
                vec![
                    "unshare".to_owned(),
                    "sync".to_owned(),
                    "--file-system".to_owned(),
                    "--".to_owned(),
                    temporary.clone(),
                ],
                None,
            ),
            recorded(
                vec![
                    "unshare".to_owned(),
                    "mv".to_owned(),
                    "--force".to_owned(),
                    "--no-target-directory".to_owned(),
                    "--".to_owned(),
                    temporary,
                    format!("{mountpoint}/dynamic.yaml"),
                ],
                None,
            ),
            recorded(
                arguments(&["unshare", "sync", "--file-system", "--", mountpoint]),
                None,
            ),
        ];
        assert_eq!(commands.as_slice(), expected.as_slice());
        Ok(())
    }

    #[tokio::test]
    async fn validates_volume_ownership_before_unshare() -> Result<(), Box<dyn std::error::Error>> {
        let runner = recording_runner([command_output(
            &json!([{
                "Name": "mm-ingress-acme",
                "Mountpoint": "/storage/mm-ingress-acme/_data",
                "Labels": {}
            }])
            .to_string(),
        )]);
        let runtime = PodmanComposeRuntime::with_environment(
            runner.clone(),
            "podman-test",
            empty_environment,
        );

        let result = runtime
            .create_volume_file_if_absent(
                &local_target(),
                "mm-ingress-acme",
                "ingress-acme",
                "acme.json",
                0o600,
            )
            .await;

        assert!(matches!(result, Err(MmError::Security { .. })));
        let commands = runner
            .commands
            .lock()
            .map_err(|_| std::io::Error::other("command lock poisoned"))?;
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].args.first().map(String::as_str), Some("volume"));
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_symlink_sources_and_traversal_file_names_before_podman()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::symlink;

        let source_directory = crate::test_support::tempdir()?;
        let source = source_directory.path().join("source.yaml");
        let symlink_source = source_directory.path().join("linked.yaml");
        fs::write(&source, "http: {}\n")?;
        symlink(&source, &symlink_source)?;
        let runner = recording_runner(Vec::<CommandOutput>::new());
        let runtime = PodmanComposeRuntime::with_environment(
            runner.clone(),
            "podman-test",
            empty_environment,
        );

        let linked_result = runtime
            .replace_volume_file(
                &local_target(),
                "mm-ingress-config",
                "ingress-dynamic-config",
                "dynamic.yaml",
                &symlink_source,
                0o644,
            )
            .await;
        let traversal_result = runtime
            .create_volume_file_if_absent(
                &local_target(),
                "mm-ingress-acme",
                "ingress-acme",
                "../acme.json",
                0o600,
            )
            .await;

        assert!(matches!(linked_result, Err(MmError::Security { .. })));
        assert!(matches!(traversal_result, Err(MmError::Security { .. })));
        let commands = runner
            .commands
            .lock()
            .map_err(|_| std::io::Error::other("command lock poisoned"))?;
        assert!(commands.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn remaining_volume_mutations_validate_inputs_before_podman()
    -> Result<(), Box<dyn std::error::Error>> {
        let runtime = PodmanComposeRuntime::with_environment(
            recording_runner(Vec::<CommandOutput>::new()),
            "podman-test",
            empty_environment,
        );
        let target = local_target();
        let ownership = VolumeOwnership { uid: 1, gid: 2, mode: 0o600 };

        assert!(runtime.remove_network(&target, "../bad", "resource").await.is_err());
        assert!(runtime.remove_volume(&target, "../bad", "resource").await.is_err());
        assert!(
            runtime
                .set_volume_owner(&target, "../bad", "resource", ownership)
                .await
                .is_err()
        );
        assert!(
            runtime
                .replace_volume_file_owned(
                    &target,
                    "../bad",
                    "resource",
                    OwnedVolumeFile { name: "file", ownership },
                    Path::new("/dev/null"),
                )
                .await
                .is_err()
        );
        assert!(
            runtime
                .remove_volume_file(&target, "../bad", "resource", "file")
                .await
                .is_err()
        );
        let primary = MmError::State { message: "primary".to_owned() };
        assert!(matches!(
            super::volume_cleanup_error(primary, Ok(())),
            MmError::State { .. }
        ));
        assert!(matches!(
            super::volume_cleanup_error(
                MmError::State { message: "primary".to_owned() },
                Err(MmError::State { message: "cleanup".to_owned() })
            ),
            MmError::RuntimeResponse { .. }
        ));
        Ok(())
    }

    fn recording_runner(outputs: impl IntoIterator<Item = CommandOutput>) -> Arc<RecordingRunner> {
        Arc::new(RecordingRunner {
            outputs: Mutex::new(outputs.into_iter().collect()),
            commands: Mutex::new(Vec::new()),
        })
    }

    fn recorded(args: Vec<String>, current_dir: Option<PathBuf>) -> RecordedCommand {
        RecordedCommand {
            program: "podman-test".to_owned(),
            args,
            current_dir,
            internal_stdout: false,
        }
    }

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn local_target() -> NamedTarget {
        NamedTarget::new(
            "local".to_owned(),
            TargetDefinition::local(TargetId(Uuid::from_u128(TARGET_UUID))),
        )
    }

    fn deployment_target(root: &Path) -> RuntimeTarget {
        RuntimeTarget {
            root: root.to_path_buf(),
            project_directory: root.join("deploy"),
            target: Some(local_target()),
            project_id: crate::domain::workspace::ProjectId(Uuid::from_u128(
                0x019c_527b_6242_7b43_bfd4_bd60_e27c_1de3,
            )),
            environment: "local".to_owned(),
            stack_name: "app".to_owned(),
            project_name: "demo-local-app".to_owned(),
            artifact_hash: "artifact-hash".to_owned(),
            compose_files: vec![
                root.join("compose.yaml"),
                root.join("compose.override.yaml"),
                PathBuf::from("/mm/compose/mm.overlay.yaml"),
            ],
            user_compose_file_count: 2,
            interpolation_environment: Default::default(),
            installation_scoped: false,
            registry_credentials: Vec::new(),
        }
    }

    fn owned_volume_output(name: &str, mountpoint: &str, resource: &str) -> CommandOutput {
        command_output(
            &json!([{
                "Name": name,
                "Mountpoint": mountpoint,
                "Labels": {
                    "io.mm.target-id": Uuid::from_u128(TARGET_UUID).to_string(),
                    "io.mm.resource": resource,
                    "io.mm.resource-kind": "volume",
                    "io.mm.generation": "1"
                }
            }])
            .to_string(),
        )
    }

    fn empty_environment(_name: &str) -> Option<OsString> {
        None
    }

    fn redirected_environment(name: &str, redirected: &str) -> Option<OsString> {
        (name == redirected).then(|| OsString::from("ssh://remote.example.test/run/podman.sock"))
    }

    fn container_host_environment(name: &str) -> Option<OsString> {
        redirected_environment(name, "CONTAINER_HOST")
    }

    fn container_connection_environment(name: &str) -> Option<OsString> {
        redirected_environment(name, "CONTAINER_CONNECTION")
    }

    fn docker_host_environment(name: &str) -> Option<OsString> {
        redirected_environment(name, "DOCKER_HOST")
    }

    fn docker_context_environment(name: &str) -> Option<OsString> {
        redirected_environment(name, "DOCKER_CONTEXT")
    }

    fn command_output(stdout: &str) -> CommandOutput {
        command_output_with_code(0, stdout)
    }

    fn command_output_with_code(code: i32, stdout: &str) -> CommandOutput {
        CommandOutput {
            code: Some(code),
            stdout: Zeroizing::new(stdout.to_owned()),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        }
    }
