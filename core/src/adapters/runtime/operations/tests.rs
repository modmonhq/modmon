    use super::*;
    use crate::app::ports::{CommandOutput, ProcessOutputSource};
    use crate::domain::observability::{
        ApprovalContext, LogLimits, LogMode, LogOutputPolicy, LogRequest, ServiceSelector,
    };
    use crate::domain::runtime::ServiceIdentity;
    use crate::domain::workspace::ProjectId;
    use uuid::Uuid;

    #[test]
    fn line_buffer_bounds_long_and_split_non_utf8_lines() {
        let mut buffer = LineBuffer::new(4);
        assert!(
            buffer
                .push(ProcessOutputSource::Stdout, &[b'a', b'b', 0xff])
                .is_empty()
        );
        assert_eq!(
            buffer.push(ProcessOutputSource::Stdout, b"cdef\nnext\n"),
            [
                (vec![b'a', b'b', 0xff, b'c'], true),
                (b"next".to_vec(), false)
            ]
        );
    }

    #[test]
    fn timestamps_preserve_negative_epoch_values() {
        assert_eq!(docker_timestamp(1_234), "1.234");
        assert_eq!(docker_timestamp(-1), "-1.999");
    }

    #[test]
    fn compose_prefix_is_removed_before_timestamp_parsing() {
        let identity = ServiceIdentity {
                project_id: ProjectId(Uuid::from_u128(1)),
                environment: "local".to_owned(),
                stack: "app".to_owned(),
                module: Some("backend".to_owned()),
                service: "api".to_owned(),
            };
        let record = log_record(LogRecordInput {
            identity: &identity,
            sequence: 1,
            source: ProcessOutputSource::Stdout,
            line: b"api-1  | 2026-08-23T00:00:00Z ready".to_vec(),
            truncated: false,
            reconnect: 0,
            timestamps: true,
            redactions: &[],
        });
        assert_eq!(record.timestamp.as_deref(), Some("2026-08-23T00:00:00Z"));
        assert_eq!(record.message, "ready");
    }

    fn output(code: i32, stdout: &str, stderr: &str, truncated: bool) -> CommandOutput {
        CommandOutput {
            code: Some(code),
            stdout: Zeroizing::new(stdout.to_owned()),
            stderr: Zeroizing::new(stderr.to_owned()),
            truncated,
        }
    }

    fn request() -> LogRequest {
        LogRequest {
            selector: Some(ServiceSelector {
                stack: None,
                module: None,
                service: "api".to_owned(),
                container: None,
            }),
            nodes: Vec::new(),
            tail: 25,
            since_unix_ms: Some(-1),
            until_unix_ms: Some(1_234),
            follow: true,
            timestamps: true,
            output: LogOutputPolicy::Both,
            mode: LogMode::Redacted,
            limits: LogLimits {
                maximum_line_bytes: 8,
                buffered_records: 2,
                maximum_bytes_per_second: Some(4),
                maximum_lines_per_second: Some(2),
            },
            approval: ApprovalContext {
                principal: "test".to_owned(),
                source: "unit".to_owned(),
                approved_high_risk: false,
            },
        }
    }

    #[test]
    fn output_name_identity_and_json_helpers_cover_bounds_and_malformed_results()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(bounded_output(output(0, "out", "", false), 10), ("out".to_owned(), false));
        assert_eq!(bounded_output(output(0, "", "err", false), 10), ("err".to_owned(), false));
        assert_eq!(bounded_output(output(0, "out\n", "err", false), 10), ("out\nerr".to_owned(), false));
        assert_eq!(bounded_output(output(0, "aéz", "", false), 2), ("a".to_owned(), true));
        assert!(valid_job_name("mm-job-abc-123"));
        for invalid in ["job-abc", "mm-job-UPPER", "mm-job-a_1", &format!("mm-job-{}", "a".repeat(60))] {
            assert!(!valid_job_name(invalid));
        }
        verify_runtime_id(None, "abcdef")?;
        verify_runtime_id(Some("abc"), "abcdef")?;
        assert!(verify_runtime_id(Some("other"), "abcdef").is_err());
        assert!(valid_container_id("abcdef123456"));
        assert!(!valid_container_id("short"));
        assert!(!valid_container_id("zzzzzzzzzzzz"));

        assert!(successful_json_ids(output(0, "", "", false), "docker", &["ID"])?.is_empty());
        assert_eq!(successful_json_ids(output(0, r#"[{"ID":"abc"},{"Id":"def"}]"#, "", false), "docker", &["ID", "Id"])? , ["abc", "def"]);
        assert_eq!(successful_json_ids(output(0, "{\"ID\":\"abc\"}\n{\"ID\":\"def\"}", "", false), "docker", &["ID"])? , ["abc", "def"]);
        assert!(successful_json_ids(output(1, "", "failed", false), "docker", &["ID"]).is_err());
        assert!(successful_json_ids(output(0, "[]", "", true), "docker", &["ID"]).is_err());
        assert!(successful_json_ids(output(0, "not-json", "", false), "docker", &["ID"]).is_err());
        Ok(())
    }

    #[test]
    fn line_log_rate_record_and_option_helpers_cover_every_policy()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut buffer = LineBuffer::new(4);
        assert_eq!(buffer.push(ProcessOutputSource::Stdout, b"ok\r\n"), [(b"ok".to_vec(), false)]);
        assert!(buffer.push(ProcessOutputSource::Stderr, b"error").is_empty());
        assert_eq!(buffer.finish(), [(ProcessOutputSource::Stderr, b"erro".to_vec(), true)]);
        assert!(LineBuffer::new(4).finish().is_empty());

        for (policy, stdout, stderr) in [
            (LogOutputPolicy::Both, true, true),
            (LogOutputPolicy::Stdout, true, false),
            (LogOutputPolicy::Stderr, false, true),
        ] {
            assert_eq!(include_source(policy, ProcessOutputSource::Stdout), stdout);
            assert_eq!(include_source(policy, ProcessOutputSource::Stderr), stderr);
        }
        let mut request = request();
        let mut args = Vec::new();
        append_log_options(&mut args, &request);
        assert!(args.contains(&"--follow".to_owned()));
        assert!(args.contains(&"--timestamps".to_owned()));
        assert!(args.contains(&"-1.999".to_owned()));
        request.follow = false;
        request.timestamps = false;
        request.since_unix_ms = None;
        request.until_unix_ms = None;
        let mut minimal = Vec::new();
        append_log_options(&mut minimal, &request);
        // A read that ends asks for timestamps to put the lines in order.
        assert_eq!(minimal, ["--tail", "25", "--timestamps"]);
        request.follow = true;
        let mut following = Vec::new();
        append_log_options(&mut following, &request);
        // So does a followed read: the lines of several nodes are put in
        // order by them.
        assert_eq!(following, ["--tail", "25", "--follow", "--timestamps"]);
        request.follow = false;
        assert_eq!(log_stream_timeout(false), Duration::from_secs(120));
        assert_eq!(log_stream_timeout(true), Duration::from_secs(86_400));

        let mut rate = LogRate::new(&request);
        rate.consume(2)?;
        rate.consume(2)?;
        assert!(rate.consume(1).is_err());
        rate.started = tokio::time::Instant::now() - Duration::from_secs(2);
        rate.consume(1)?;

        let identity = ServiceIdentity {
            project_id: ProjectId(Uuid::from_u128(2)),
            environment: "local".to_owned(),
            stack: "app".to_owned(),
            module: None,
            service: "api".to_owned(),
        };
        let redactions = [Zeroizing::new("secret".to_owned())];
        let record = log_record(LogRecordInput {
            identity: &identity,
            sequence: 2,
            source: ProcessOutputSource::Stderr,
            line: b"wrong prefix | secret \xff".to_vec(),
            truncated: true,
            reconnect: 1,
            timestamps: false,
            redactions: &redactions,
        });
        assert_eq!(record.source, LogSource::Stderr);
        assert!(record.lossy_utf8);
        assert!(record.message.contains("[REDACTED]"));
        assert!(record.timestamp.is_none());
        assert!(unix_now_ms() > 0);
        Ok(())
    }

    #[test]
    fn a_finished_read_is_the_last_lines_of_the_service_in_time_order() {
        let identity = ServiceIdentity {
            project_id: ProjectId(Uuid::from_u128(2)),
            environment: "production".to_owned(),
            stack: "app".to_owned(),
            module: None,
            service: "web".to_owned(),
        };
        // A service with several containers prints each one's last lines in
        // turn, a stopped one included.
        let lines = [
            "web-2  | 2026-11-12T10:00:03.000000000Z third",
            "web-2  | 2026-11-12T10:00:05.500000000Z fifth",
            "web-1  | 2026-11-12T09:00:00.000000000Z from a stopped container",
            "web-1  | 2026-11-12T10:00:01.000000000Z first",
            "web-1  |     at continued (stack.js:1)",
            "web-3  | 2026-11-12T10:00:02+00:00 second",
            "web-3  | 2026-11-12T10:00:05.250000000Z fourth",
        ];
        let last = |tail: u32| {
            let mut last = LastLines::new(tail);
            for (line, sequence) in lines.iter().zip(1_u64..) {
                last.push(log_record(LogRecordInput {
                    identity: &identity,
                    sequence,
                    source: ProcessOutputSource::Stdout,
                    line: line.as_bytes().to_vec(),
                    truncated: false,
                    reconnect: 0,
                    timestamps: true,
                    redactions: &[],
                }));
            }
            last.finish()
        };
        let kept = last(6);
        let words = kept
            .iter()
            .map(|record| record.message.rsplit("| ").next().unwrap_or_default().trim())
            .collect::<Vec<_>>();
        assert_eq!(
            words,
            [
                "first",
                "at continued (stack.js:1)",
                "second",
                "third",
                "fourth",
                "fifth"
            ],
            "the oldest line is dropped and a continuation stays with its line"
        );
        assert_eq!(
            kept.iter().map(|record| record.sequence).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6]
        );
        // Every record carries the engine's timestamp beside its message;
        // whether a reader sees it is the interface's choice.
        assert!(kept.iter().all(|record| !record.message.contains("2026-11-12T")));
        assert_eq!(
            kept[2].timestamp.as_deref(),
            Some("2026-11-12T10:00:02+00:00")
        );
        assert!(kept[1].timestamp.is_none(), "a continuation has none of its own");
        assert!(last(0).is_empty());
        assert_eq!(last(100).len(), 7);

        // A long read keeps a bounded buffer and still ends with the newest lines.
        let mut long = LastLines::new(2);
        for second in 0..5_000_u32 {
            let line = format!(
                "2026-11-12T10:{:02}:{:02}Z line {second}",
                second / 60 % 60,
                second % 60
            );
            long.push(log_record(LogRecordInput {
                identity: &identity,
                sequence: u64::from(second),
                source: ProcessOutputSource::Stdout,
                line: line.into_bytes(),
                truncated: false,
                reconnect: 0,
                timestamps: true,
                redactions: &[],
            }));
            assert!(long.records.len() <= 2 * 2 + LastLines::SLACK + 1);
        }
        let newest = long.finish();
        assert_eq!(
            newest
                .iter()
                .map(|record| record.message.as_str())
                .collect::<Vec<_>>(),
            ["line 3598", "line 3599"],
            "ordered by time, not by arrival"
        );
    }

    struct ScriptedDocker {
        /// (argument substring, stdout) pairs; the first match answers.
        answers: Vec<(&'static str, &'static str)>,
        commands: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait]
    impl ProcessRunner for ScriptedDocker {
        async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
            let mut command = specification.redacted_display();
            for (key, _) in specification.environment_values() {
                if key == "DOCKER_CONFIG" || key == "REGISTRY_AUTH_FILE" {
                    command.push_str(&format!(" [{key}]"));
                }
            }
            if let Ok(mut commands) = self.commands.lock() {
                commands.push(command.clone());
            }
            let stdout = self
                .answers
                .iter()
                .find(|(needle, _)| command.contains(needle))
                .map_or("", |(_, stdout)| stdout);
            Ok(output(0, stdout, "", false))
        }
    }

    #[async_trait]
    impl StreamingProcessRunner for ScriptedDocker {
        async fn spawn(
            &self,
            _specification: StreamCommandSpec,
            _cancellation: OperationCancellation,
        ) -> Result<crate::app::ports::ProcessStream, MmError> {
            Err(MmError::RuntimeResponse { message: "no streams in this test".to_owned() })
        }
    }

    fn compose_target() -> RuntimeTarget {
        RuntimeTarget {
            root: "/srv/app".into(),
            project_directory: "/srv/app".into(),
            target: None,
            project_id: ProjectId(Uuid::from_u128(18)),
            environment: "production".to_owned(),
            stack_name: "app".to_owned(),
            project_name: "mm-app".to_owned(),
            artifact_hash: "a".repeat(64),
            compose_files: Vec::new(),
            user_compose_file_count: 0,
            interpolation_environment: crate::domain::runtime::RuntimeEnvironment::default(),
            installation_scoped: false,
            registry_credentials: Vec::new(),
        }
    }

    fn compose(answers: Vec<(&'static str, &'static str)>) -> (Arc<ScriptedDocker>, RuntimeOperationsAdapter) {
        let docker = Arc::new(ScriptedDocker { answers, commands: std::sync::Mutex::new(Vec::new()) });
        let adapter = RuntimeOperationsAdapter::new(RuntimeKind::DockerCompose, docker.clone(), docker.clone());
        (docker, adapter)
    }

    fn selector(container: Option<&str>) -> ServiceSelector {
        ServiceSelector {
            stack: None,
            module: Some("backend".to_owned()),
            service: "api".to_owned(),
            container: container.map(str::to_owned),
        }
    }

    fn recorded(docker: &ScriptedDocker) -> Result<Vec<String>, MmError> {
        docker
            .commands
            .lock()
            .map(|commands| commands.clone())
            .map_err(|_| MmError::State { message: "lock".to_owned() })
    }

    #[tokio::test]
    async fn compose_restart_targets_every_container_of_the_service_or_the_selected_one()
    -> Result<(), MmError> {
        const LISTING: &str = "{\"ID\":\"aaaaaaaaaaaa\"}\n{\"ID\":\"bbbbbbbbbbbb\"}";
        let (docker, adapter) = compose(vec![("ps --all", LISTING)]);
        let outcome = adapter.restart(&compose_target(), &selector(None)).await?;
        assert_eq!(outcome.containers, ["aaaaaaaaaaaa", "bbbbbbbbbbbb"]);
        assert!(!outcome.rolling);
        assert_eq!(outcome.identity.service, "api");
        let commands = recorded(&docker)?;
        assert!(commands[0].contains("label=com.docker.compose.project=mm-app"), "{commands:?}");
        assert!(commands[0].contains("label=com.docker.compose.service=api"), "{commands:?}");
        // A job running as the service is no container of it.
        assert!(commands[0].contains("label=com.docker.compose.oneoff=False"), "{commands:?}");
        assert!(commands[1].ends_with("restart aaaaaaaaaaaa bbbbbbbbbbbb"), "{commands:?}");

        let (docker, adapter) = compose(vec![("ps --all", LISTING)]);
        let outcome = adapter.restart(&compose_target(), &selector(Some("bbbb"))).await?;
        assert_eq!(outcome.containers, ["bbbbbbbbbbbb"]);
        assert!(recorded(&docker)?[1].ends_with("restart bbbbbbbbbbbb"));

        let (docker, adapter) = compose(vec![("ps --all", LISTING)]);
        let foreign = adapter.restart(&compose_target(), &selector(Some("cccccccccccc"))).await;
        assert!(matches!(foreign, Err(MmError::RuntimeResponse { .. })), "{foreign:?}");
        assert_eq!(recorded(&docker)?.len(), 1, "nothing may be restarted");

        let (_, adapter) = compose(Vec::new());
        let empty = adapter.restart(&compose_target(), &selector(None)).await;
        assert!(matches!(empty, Err(MmError::RuntimeResponse { ref message }) if message.contains("start the stack")), "{empty:?}");
        Ok(())
    }

    #[tokio::test]
    async fn a_job_launch_pulls_with_the_stacks_registry_logins() -> Result<(), MmError> {
        const CONTAINER: &str = "7abe09559dc6aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        for (kind, expected) in [
            (RuntimeKind::DockerCompose, &["[DOCKER_CONFIG]"][..]),
            (RuntimeKind::PodmanCompose, &["[DOCKER_CONFIG]", "[REGISTRY_AUTH_FILE]"][..]),
        ] {
            let docker = Arc::new(ScriptedDocker {
                answers: vec![("container inspect", "[]"), ("run --detach", CONTAINER)],
                commands: std::sync::Mutex::new(Vec::new()),
            });
            let adapter = RuntimeOperationsAdapter::new(kind, docker.clone(), docker.clone());
            let mut target = compose_target();
            target.compose_files = vec!["/srv/app/compose.yaml".into()];
            target.registry_credentials = vec![crate::domain::runtime::RegistryCredential {
                registry: "ghcr.io".to_owned(),
                username: "bot".to_owned(),
                password: zeroize::Zeroizing::new("token".to_owned()),
            }];
            let launched = adapter
                .launch(crate::domain::scheduler::JobLaunch {
                    run_id: Uuid::from_u128(7),
                    fence_token: 1,
                    runtime_name: "mm-job-7".to_owned(),
                    runtime: kind,
                    target,
                    module: "backend".to_owned(),
                    service: "api".to_owned(),
                    argv: vec!["migrate".to_owned()],
                    user: None,
                    timeout: Duration::from_secs(60),
                    output_limit_bytes: 1024,
                    environment: crate::domain::runtime::RuntimeEnvironment::default(),
                    redactions: Vec::new(),
                })
                .await?;
            assert_eq!(launched, CONTAINER);
            let commands = recorded(&docker)?;
            let run = commands
                .iter()
                .find(|command| command.contains("run --detach"))
                .ok_or_else(|| MmError::State { message: format!("{commands:?}") })?;
            for marker in expected {
                assert!(run.contains(marker), "{kind:?}: {run}");
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn exec_selects_the_services_one_running_container_on_this_host() -> Result<(), MmError> {
        const CONTAINER: &str = "7abe09559dc6aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let (docker, adapter) = compose(vec![(
            "status=running",
            "{\"ID\":\"7abe09559dc6aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}",
        )]);
        assert_eq!(adapter.select_container(&compose_target(), None, "api").await?, CONTAINER);
        let commands = recorded(&docker)?;
        assert_eq!(commands.len(), 1, "{commands:?}");
        assert!(commands[0].contains("label=com.docker.compose.project=mm-app"), "{commands:?}");
        assert!(commands[0].contains("label=com.docker.compose.service=api"), "{commands:?}");

        // Nothing runs here: a plain error, and no other node is named.
        let (docker, adapter) = compose(Vec::new());
        let error = adapter.select_container(&compose_target(), None, "api").await;
        assert!(
            matches!(error, Err(MmError::RuntimeResponse { ref message }) if message.contains("no running container on this node")),
            "{error:?}"
        );
        assert_eq!(recorded(&docker)?.len(), 1, "one listing and nothing else");
        Ok(())
    }
