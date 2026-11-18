    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;

    #[derive(Default)]
    struct FakeProcess {
        commands: Mutex<Vec<crate::app::ports::RedactedCommandRecord>>,
    }

    struct OutputProcess {
        code: i32,
        truncated: bool,
    }

    #[async_trait]
    impl ProcessRunner for OutputProcess {
        async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
            Ok(CommandOutput {
                code: Some(self.code),
                stdout: zeroize::Zeroizing::new(String::new()),
                stderr: zeroize::Zeroizing::new(" command failed ".to_owned()),
                truncated: self.truncated,
            })
        }
    }

    #[async_trait]
    impl ProcessRunner for FakeProcess {
        async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
            self.commands
                .lock()
                .map_err(|_| MmError::State {
                    message: "fake command lock poisoned".to_owned(),
                })?
                .push(specification.redacted_record());
            Ok(CommandOutput {
                code: Some(0),
                stdout: zeroize::Zeroizing::new(String::new()),
                stderr: zeroize::Zeroizing::new(String::new()),
                truncated: false,
            })
        }
    }

    #[tokio::test]
    async fn unsupported_platform_never_claims_installation() -> Result<(), MmError> {
        let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
            context: "creating trust test directory".to_owned(),
            source,
        })?;
        let store = PlatformTrustStore::for_platform(
            directory.path().join("trust"),
            Arc::new(FakeProcess::default()),
            TrustScope::CurrentUser,
            Platform::Unsupported,
        );
        let status = store
            .status(Some(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            ))
            .await?;
        assert!(!status.supported);
        assert!(!status.installed);
        assert!(
            store
                .install(
                    "invalid",
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                )
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn same_name_foreign_certificate_is_never_replaced_or_deleted() -> Result<(), MmError> {
        let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
            context: "creating trust test directory".to_owned(),
            source,
        })?;
        let process = Arc::new(FakeProcess::default());
        let fingerprint = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
        let store = PlatformTrustStore::for_test(
            directory.path().join("trust"),
            process.clone(),
            BackendStatus {
                supported: true,
                installed: false,
                mismatch: true,
                needs_privilege: false,
                entry: "same-name-entry".to_owned(),
            },
        );
        store.save_ownership(&TrustOwnership {
            entries: [(
                fingerprint.to_owned(),
                OwnedTrustEntry {
                    platform: "test".to_owned(),
                    scope: TrustScope::CurrentUser,
                    entry: "same-name-entry".to_owned(),
                },
            )]
            .into_iter()
            .collect(),
        })?;

        assert!(store.install("foreign", fingerprint).await.is_err());
        assert!(store.uninstall(fingerprint).await.is_err());
        assert!(
            process
                .commands
                .lock()
                .map_err(|_| MmError::State {
                    message: "fake command lock poisoned".to_owned()
                })?
                .is_empty()
        );
        Ok(())
    }

    #[tokio::test]
    async fn ownership_cannot_be_moved_to_another_scope() -> Result<(), MmError> {
        let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
            context: "creating trust test directory".to_owned(),
            source,
        })?;
        let fingerprint = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
        let mut store = PlatformTrustStore::for_test(
            directory.path().join("trust"),
            Arc::new(FakeProcess::default()),
            BackendStatus::empty(true),
        );
        store.scope = TrustScope::System;
        store.save_ownership(&TrustOwnership {
            entries: [(
                fingerprint.to_owned(),
                OwnedTrustEntry {
                    platform: "test".to_owned(),
                    scope: TrustScope::CurrentUser,
                    entry: "user-root".to_owned(),
                },
            )]
            .into_iter()
            .collect(),
        })?;

        let error = store.install("invalid", fingerprint).await;
        assert!(matches!(error, Err(MmError::Security { .. })));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn private_key_repair_restores_owner_only_modes() -> Result<(), MmError> {
        use std::os::unix::fs::PermissionsExt;

        let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
            context: "creating trust key test directory".to_owned(),
            source,
        })?;
        let ca = crate::adapters::ca::FilesystemLocalCaStore::new(directory.path().join("ca"));
        ca.install()?;
        ca.issue(&["app.test".to_owned()].into_iter().collect())?;
        let key = directory.path().join("ca/issued/app.test.key");
        fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).map_err(|source| {
            MmError::Io {
                context: "loosening test key permissions".to_owned(),
                source,
            }
        })?;
        assert!(!private_keys_valid_unix(&directory.path().join("trust"))?);

        secure_private_keys_unix(&directory.path().join("trust"))?;
        assert!(private_keys_valid_unix(&directory.path().join("trust"))?);
        Ok(())
    }

    #[tokio::test]
    async fn trust_orchestrator_covers_owned_unowned_and_backend_failure_lifecycles()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = crate::test_support::tempdir()?;
        let root = directory.path().join("trust");
        let fingerprint = format!("sha256:{}", "1".repeat(64));
        let empty = BackendStatus::empty(true);
        let store = PlatformTrustStore::for_test(
            root.clone(),
            Arc::new(FakeProcess::default()),
            empty.clone(),
        );
        assert!(store.status(None).await?.supported);
        assert!(!store.status(Some(&fingerprint)).await?.installed);
        assert!(!store.uninstall(&fingerprint).await?.installed);

        let installed = BackendStatus {
            supported: true,
            installed: true,
            mismatch: false,
            needs_privilege: false,
            entry: "owned-entry".to_owned(),
        };
        let installed_store = PlatformTrustStore::for_test(
            root.clone(),
            Arc::new(FakeProcess::default()),
            installed.clone(),
        );
        assert!(installed_store.uninstall(&fingerprint).await.is_err());
        installed_store.save_ownership(&TrustOwnership {
            entries: [(
                fingerprint.clone(),
                OwnedTrustEntry {
                    platform: "test".to_owned(),
                    scope: TrustScope::CurrentUser,
                    entry: "owned-entry".to_owned(),
                },
            )]
            .into_iter()
            .collect(),
        })?;
        assert!(installed_store.status(Some(&fingerprint)).await?.installed);
        assert!(installed_store.install("unused", &fingerprint).await?.installed);
        assert!(installed_store.repair("unused", &fingerprint).await?.installed);
        assert!(installed_store.uninstall(&fingerprint).await.is_err());

        let mismatch_store = PlatformTrustStore::for_test(
            directory.path().join("mismatch"),
            Arc::new(FakeProcess::default()),
            BackendStatus { mismatch: true, ..empty.clone() },
        );
        assert!(mismatch_store.install("unused", &fingerprint).await.is_err());
        assert!(mismatch_store.repair("unused", &fingerprint).await.is_err());
        let unsupported = PlatformTrustStore::for_test(
            directory.path().join("unsupported"),
            Arc::new(FakeProcess::default()),
            BackendStatus::unsupported(),
        );
        assert!(unsupported.install("unused", &fingerprint).await.is_err());

        let ca_root = directory.path().join("ca");
        let ca = crate::adapters::ca::FilesystemLocalCaStore::new(ca_root.clone());
        ca.install()?;
        let pem = fs::read_to_string(ca_root.join("ca.crt"))?;
        let actual = crate::domain::ca::certificate_fingerprint(pem.as_bytes())?.to_string();
        let failing = PlatformTrustStore::for_test(
            directory.path().join("backend-failure"),
            Arc::new(FakeProcess::default()),
            empty,
        );
        assert!(failing.install(&pem, &actual).await.is_err());
        assert!(!failing.root.join(format!("{}.crt", fingerprint_token(&actual)?)).exists());
        assert!(failing.prepare_certificate(&pem, &fingerprint).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn trust_helpers_cover_platform_dispatch_command_bounds_and_invalid_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = crate::test_support::tempdir()?;
        let unsupported = PlatformTrustStore::for_platform(
            directory.path().join("unsupported"),
            Arc::new(FakeProcess::default()),
            TrustScope::CurrentUser,
            Platform::Unsupported,
        );
        assert!(unsupported.secure_private_keys().await.is_err());
        assert!(!unsupported.private_keys_valid().await?);
        assert_eq!(unsupported.platform_name(), "unsupported");
        let native = if cfg!(target_os = "linux") {
            "linux"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "unsupported"
        };
        assert_eq!(PlatformTrustStore::new(
            directory.path().join("native"),
            Arc::new(FakeProcess::default()),
            TrustScope::CurrentUser,
        ).platform_name(), native);

        let truncated = PlatformTrustStore::for_test(
            directory.path().join("truncated"),
            Arc::new(OutputProcess { code: 0, truncated: true }),
            BackendStatus::empty(true),
        );
        assert!(truncated.run(CommandSpec::new("tool")).await.is_err());
        let failed = PlatformTrustStore::for_test(
            directory.path().join("failed"),
            Arc::new(OutputProcess { code: 4, truncated: false }),
            BackendStatus::empty(true),
        );
        assert!(failed.run_checked(CommandSpec::new("tool"), "testing").await.is_err());
        assert!(fingerprint_token("invalid").is_err());
        assert!(fingerprint_token("sha256:short").is_err());
        assert!(fingerprint_token(&format!("sha256:{}", "a".repeat(64))).is_ok());

        fs::create_dir_all(failed.root.clone())?;
        fs::write(failed.root.join(OWNERSHIP_FILE), b"not-json")?;
        assert!(failed.load_ownership().is_err());
        Ok(())
    }

    #[tokio::test]
    async fn platform_dispatch_and_metadata_helpers_fail_closed_at_each_boundary()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = crate::test_support::tempdir()?;
        let process = Arc::new(FakeProcess::default());
        for (platform, name) in [
            (Platform::Macos, "macos"),
            (Platform::Windows, "windows"),
            (Platform::Unsupported, "unsupported"),
        ] {
            let store = PlatformTrustStore::for_platform(
                directory.path().join(name),
                process.clone(),
                TrustScope::CurrentUser,
                platform,
            );
            assert_eq!(store.platform_name(), name);
            assert!(format!("{store:?}").contains(name));
        }

        let unsupported = PlatformTrustStore::for_platform(
            directory.path().join("unsupported-mutation"),
            process.clone(),
            TrustScope::CurrentUser,
            Platform::Unsupported,
        );
        assert!(
            unsupported
                .install_backend(Path::new("unused"), &format!("sha256:{}", "a".repeat(64)))
                .await
                .is_err()
        );
        assert!(
            unsupported
                .uninstall_backend(&format!("sha256:{}", "a".repeat(64)), "unused")
                .await
                .is_err()
        );
        let mut fake = PlatformTrustStore::for_test(
            directory.path().join("missing-status"),
            process,
            BackendStatus::empty(true),
        );
        fake.test_status = None;
        assert!(fake.actual_status(&format!("sha256:{}", "b".repeat(64))).await.is_err());
        assert!(
            fake.install_backend(Path::new("unused"), &format!("sha256:{}", "b".repeat(64)))
                .await
                .is_err()
        );
        assert!(
            fake.uninstall_backend(&format!("sha256:{}", "b".repeat(64)), "unused")
                .await
                .is_err()
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn trust_file_and_private_key_helpers_reject_unsafe_filesystem_shapes()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = crate::test_support::tempdir()?;
        let fingerprint = format!("sha256:{}", "c".repeat(64));
        let certificate = directory
            .path()
            .join(format!("{}.crt", fingerprint_token(&fingerprint)?));
        fs::create_dir(&certificate)?;
        assert!(remove_prepared_certificate(directory.path(), &fingerprint).is_err());
        fs::remove_dir(&certificate)?;

        let blocked = directory.path().join("blocked.json");
        fs::create_dir(blocked.with_extension("tmp"))?;
        assert!(write_private(&blocked, b"value").is_err());
        fs::remove_dir(blocked.with_extension("tmp"))?;
        fs::create_dir(&blocked)?;
        assert!(write_private(&blocked, b"value").is_err());
        fs::remove_file(blocked.with_extension("tmp"))?;
        assert!(write_private(&directory.path().join("missing/value"), b"value").is_err());

        assert_eq!(bounded_detail(""), "no diagnostic output");
        assert_eq!(bounded_detail(&"x".repeat(600)).len(), 512);
        assert!(format!("{}", trust_error("failure")).contains("failure"));
        assert!(format!("{}", unsupported_error()).contains("no implemented"));
        assert!(private_key_paths_unix(Path::new("/")).is_err());

        let trust_root = directory.path().join("home/trust");
        fs::create_dir_all(&trust_root)?;
        let ca_root = directory.path().join("home/ca");
        fs::create_dir(&ca_root)?;
        assert!(!private_keys_valid_unix(&trust_root)?);
        assert!(secure_private_keys_unix(&trust_root).is_err());

        let key = ca_root.join("ca.key");
        fs::write(&key, b"key")?;
        let alias = ca_root.join("alias.key");
        fs::hard_link(&key, &alias)?;
        assert!(!private_keys_valid_unix(&trust_root)?);
        assert!(secure_private_keys_unix(&trust_root).is_err());
        Ok(())
    }
