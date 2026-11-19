use super::*;

fn coded<T>(result: Result<T, MmError>, code: &str) -> bool {
    result.is_err_and(|error| error.code() == code)
}

fn write(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), MmError> {
    fs::write(path, contents).map_err(|source| MmError::Io {
        context: format!("writing test file {}", path.display()),
        source,
    })
}

#[test]
fn image_repository_normalization_strips_mutable_refs_and_rejects_unsafe_shapes()
-> Result<(), MmError> {
    for (image, expected) in [
        ("alpine:3.22", "alpine"),
        ("ghcr.io/mm/app:latest", "ghcr.io/mm/app"),
        (
            "localhost:5000/mm/app@sha256:deadbeef",
            "localhost:5000/mm/app",
        ),
        ("registry.example/mm/app", "registry.example/mm/app"),
    ] {
        assert_eq!(normalize_image_repository(image)?, expected);
    }
    for invalid in [
        "",
        " ",
        "UPPER/app",
        "/absolute",
        "trailing/",
        "https://registry/app",
        "${IMAGE}",
        "two images",
        ":tag",
        "bad_component!/app",
    ] {
        assert!(
            normalize_image_repository(invalid).is_err(),
            "accepted {invalid}"
        );
    }
    Ok(())
}

#[test]
fn build_variants_and_extra_inputs_are_deterministic_and_sensitive_safe() -> Result<(), MmError> {
    let base = vec!["1".to_owned(), "latest".to_owned()];
    let default = plan_variants(None, None, &base)?;
    assert_eq!(default.len(), 1);
    assert_eq!(default[0].target, None);
    assert_eq!(default[0].tags, base);

    let mut configured = ModuleBuildConfig::default();
    configured
        .targets
        .insert("runtime".to_owned(), "runtime".into());
    configured
        .targets
        .insert("worker".to_owned(), "worker".into());
    let variants = plan_variants(Some(&configured), None, &base)?;
    assert_eq!(variants.len(), 2);
    assert_eq!(variants[0].target.as_deref(), Some("runtime"));
    assert!(variants[0].tags.iter().all(|tag| tag.contains("-runtime")));
    configured
        .targets
        .insert("invalid".to_owned(), "x".repeat(200).as_str().into());
    assert!(plan_variants(Some(&configured), None, &base).is_err());

    let root = crate::test_support::tempdir().map_err(|source| MmError::Io {
        context: "creating build source test root".to_owned(),
        source,
    })?;
    write(&root.path().join("input.txt"), "input")?;
    let triggers = hash_extra_inputs(root.path(), &[PathBuf::from("input.txt")])?;
    assert_eq!(triggers[0].path, PathBuf::from("input.txt"));
    assert_eq!(
        triggers[0].digest,
        file_digest(&root.path().join("input.txt"))?
    );
    write(&root.path().join(".env"), "secret")?;
    assert!(hash_extra_inputs(root.path(), &[PathBuf::from(".env")]).is_err());
    assert!(hash_extra_inputs(root.path(), &[PathBuf::from("missing")]).is_err());
    #[cfg(target_os = "linux")]
    {
        let process = dunce::canonicalize("/proc/self").map_err(|source| MmError::Io {
            context: "resolving current process directory".to_owned(),
            source,
        })?;
        assert!(matches!(
            hash_extra_inputs(&process, &[PathBuf::from("mem")]),
            Err(MmError::Io { .. })
        ));
    }
    Ok(())
}

#[test]
fn dockerfile_and_project_paths_require_regular_in_root_files() -> Result<(), MmError> {
    let root = crate::test_support::tempdir().map_err(|source| MmError::Io {
        context: "creating build path test root".to_owned(),
        source,
    })?;
    let file = root.path().join("Dockerfile");
    write(&file, "FROM scratch\n")?;
    assert_eq!(read_dockerfile(&file)?, "FROM scratch\n");
    assert_eq!(
        project_relative(root.path(), &file, "Dockerfile")?,
        PathBuf::from("Dockerfile")
    );
    ensure_build_path(root.path(), root.path(), true, "context")?;
    ensure_build_path(root.path(), &file, false, "Dockerfile")?;
    assert!(ensure_build_path(root.path(), &file, true, "context").is_err());
    assert!(ensure_build_path(root.path(), root.path(), false, "Dockerfile").is_err());
    let outside = crate::test_support::tempdir().map_err(|source| MmError::Io {
        context: "creating outside build path".to_owned(),
        source,
    })?;
    write(&outside.path().join("file"), "outside")?;
    assert!(ensure_build_path(root.path(), &outside.path().join("file"), false, "input").is_err());
    assert!(project_relative(root.path(), Path::new("/outside"), "input").is_err());
    assert!(read_dockerfile(&root.path().join("missing")).is_err());
    assert!(matches!(
        read_dockerfile(root.path()),
        Err(MmError::Io { .. })
    ));
    write(&root.path().join("binary"), [0xff])?;
    assert!(read_dockerfile(&root.path().join("binary")).is_err());
    let oversized = root.path().join("oversized");
    let oversized_file = std::fs::File::create(&oversized).map_err(|source| MmError::Io {
        context: "creating oversized Dockerfile".to_owned(),
        source,
    })?;
    oversized_file
        .set_len(MAX_CONTEXT_FILE_BYTES + 1)
        .map_err(|source| MmError::Io {
            context: "sizing oversized Dockerfile".to_owned(),
            source,
        })?;
    assert!(matches!(
        read_dockerfile(&oversized),
        Err(MmError::Build { .. })
    ));

    for name in [
        ".env",
        ".env.local",
        "id_rsa",
        "id_ed25519",
        "id_ecdsa",
        "id_dsa",
        "age-identity.txt",
    ] {
        assert!(is_sensitive_file(&root.path().join(name)));
    }
    assert!(!is_sensitive_file(&file));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&file, root.path().join("linked-Dockerfile")).map_err(
            |source| MmError::Io {
                context: "creating build path symlink".to_owned(),
                source,
            },
        )?;
        assert!(matches!(
            ensure_build_path(
                root.path(),
                &root.path().join("linked-Dockerfile"),
                false,
                "Dockerfile"
            ),
            Err(MmError::Security { .. })
        ));
    }
    Ok(())
}

#[test]
fn context_limits_reject_excess_counts_bytes_and_arithmetic_overflow() -> Result<(), MmError> {
    let context = Path::new("context");
    validate_context_file_count(context, MAX_CONTEXT_FILES)?;
    assert!(matches!(
        validate_context_file_count(context, MAX_CONTEXT_FILES + 1),
        Err(MmError::Build { .. })
    ));
    assert_eq!(
        checked_context_size(context, MAX_CONTEXT_BYTES - 1, 1)?,
        MAX_CONTEXT_BYTES
    );
    assert!(matches!(
        checked_context_size(context, MAX_CONTEXT_BYTES, 1),
        Err(MmError::Build { .. })
    ));
    assert!(matches!(
        checked_context_size(context, u64::MAX, 1),
        Err(MmError::Build { .. })
    ));
    Ok(())
}

#[test]
fn context_hash_obeys_ignore_git_symlink_and_executable_boundaries() -> Result<(), MmError> {
    let root = crate::test_support::tempdir().map_err(|source| MmError::Io {
        context: "creating context hash test root".to_owned(),
        source,
    })?;
    write(&root.path().join("Dockerfile"), "FROM scratch\n")?;
    write(&root.path().join("included.txt"), "included")?;
    fs::create_dir(root.path().join("nested")).map_err(|source| MmError::Io {
        context: "creating nested context directory".to_owned(),
        source,
    })?;
    write(&root.path().join("nested/included.txt"), "nested")?;
    write(&root.path().join("ignored.txt"), "first")?;
    write(&root.path().join(".dockerignore"), "ignored.txt\n")?;
    fs::create_dir(root.path().join(".git")).map_err(|source| MmError::Io {
        context: "creating ignored git directory".to_owned(),
        source,
    })?;
    write(&root.path().join(".git/private"), "private")?;
    let dockerfile = root.path().join("Dockerfile");
    let first = hash_context(root.path(), &dockerfile)?;
    write(&root.path().join("ignored.txt"), "second")?;
    write(&root.path().join(".git/private"), "changed")?;
    assert_eq!(hash_context(root.path(), &dockerfile)?, first);
    write(&root.path().join("included.txt"), "changed")?;
    assert_ne!(hash_context(root.path(), &dockerfile)?, first);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let changed_contents = hash_context(root.path(), &dockerfile)?;
        fs::set_permissions(
            root.path().join("included.txt"),
            fs::Permissions::from_mode(0o700),
        )
        .map_err(|source| MmError::Io {
            context: "making context input executable".to_owned(),
            source,
        })?;
        assert_ne!(hash_context(root.path(), &dockerfile)?, changed_contents);
        std::os::unix::fs::symlink("included.txt", root.path().join("linked")).map_err(
            |source| MmError::Io {
                context: "creating context symlink".to_owned(),
                source,
            },
        )?;
        assert!(hash_context(root.path(), &dockerfile).is_err());
    }
    Ok(())
}

#[test]
fn context_hash_rejects_large_non_utf8_external_and_special_entries() -> Result<(), MmError> {
    let root = crate::test_support::tempdir().map_err(|source| MmError::Io {
        context: "creating unsafe context test root".to_owned(),
        source,
    })?;
    let context = root.path().join("context");
    fs::create_dir(&context).map_err(|source| MmError::Io {
        context: "creating unsafe context".to_owned(),
        source,
    })?;
    write(&context.join("Dockerfile"), "FROM scratch\n")?;

    let oversized = context.join("oversized");
    let file = std::fs::File::create(&oversized).map_err(|source| MmError::Io {
        context: "creating oversized context file".to_owned(),
        source,
    })?;
    file.set_len(MAX_CONTEXT_FILE_BYTES + 1)
        .map_err(|source| MmError::Io {
            context: "sizing oversized context file".to_owned(),
            source,
        })?;
    assert!(matches!(
        hash_context(&context, &context.join("Dockerfile")),
        Err(MmError::Build { .. })
    ));
    fs::remove_file(&oversized).map_err(|source| MmError::Io {
        context: "removing oversized context file".to_owned(),
        source,
    })?;

    let external_directory = root.path().join("external-directory");
    fs::create_dir(&external_directory).map_err(|source| MmError::Io {
        context: "creating external Dockerfile directory".to_owned(),
        source,
    })?;
    assert!(matches!(
        hash_context(&context, &external_directory),
        Err(MmError::Security { .. })
    ));

    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        use std::os::unix::net::UnixListener;

        if crate::test_support::NON_UTF8_FILE_NAMES {
            let invalid = context.join(OsString::from_vec(vec![b'i', 0xff]));
            write(&invalid, "invalid path")?;
            assert!(coded(
                hash_context(&context, &context.join("Dockerfile")),
                "MM_SECURITY"
            ));
            fs::remove_file(&invalid).map_err(|source| MmError::Io {
                context: "removing non-UTF-8 context file".to_owned(),
                source,
            })?;
        }

        let socket = context.join("build.sock");
        let _listener = UnixListener::bind(&socket).map_err(|source| MmError::Io {
            context: "creating context socket".to_owned(),
            source,
        })?;
        assert!(coded(
            hash_context(&context, &context.join("Dockerfile")),
            "MM_SECURITY"
        ));
    }
    Ok(())
}

#[test]
fn context_collection_and_hashing_preserve_specific_io_and_traversal_errors() -> Result<(), MmError>
{
    let root = crate::test_support::tempdir().map_err(|source| MmError::Io {
        context: "creating context error test root".to_owned(),
        source,
    })?;
    let context = root.path().join("context");
    fs::create_dir(&context).map_err(|source| MmError::Io {
        context: "creating context error directory".to_owned(),
        source,
    })?;
    write(&root.path().join("outside"), "outside")?;
    let dockerfile = context.join("Dockerfile");
    let scan = ContextScan::new(root.path(), &context, &dockerfile)?;
    assert!(matches!(
        scan.collect(&root.path().join("missing"), None, &mut BTreeMap::new()),
        Err(MmError::Io { .. })
    ));
    assert!(matches!(
        scan.collect(root.path(), None, &mut BTreeMap::new()),
        Err(MmError::Security { .. })
    ));
    assert!(matches!(
        file_digest(&root.path().join("missing")),
        Err(MmError::Io { .. })
    ));
    let mut hasher = blake3::Hasher::new();
    assert!(matches!(
        hash_file(root.path(), &mut hasher),
        Err(MmError::Io { .. })
    ));

    let entry_path = context.join("removed-before-metadata");
    write(&entry_path, "removed")?;
    let entries = fs::read_dir(&context)
        .map_err(|source| MmError::Io {
            context: "reading stale entry fixture".to_owned(),
            source,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| MmError::Io {
            context: "collecting stale entry fixture".to_owned(),
            source,
        })?;
    fs::remove_file(&entry_path).map_err(|source| MmError::Io {
        context: "removing stale entry fixture".to_owned(),
        source,
    })?;
    assert!(matches!(
        scan.collect_entries(
            &context,
            None,
            &mut BTreeMap::new(),
            entries.into_iter().map(Ok)
        ),
        Err(MmError::Io { .. })
    ));
    assert!(matches!(
        scan.collect_entries(
            &context,
            None,
            &mut BTreeMap::new(),
            std::iter::once(Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected directory iteration failure"
            )))
        ),
        Err(MmError::Io { .. })
    ));
    #[cfg(target_os = "linux")]
    {
        let process = dunce::canonicalize("/proc/self").map_err(|source| MmError::Io {
            context: "resolving current process directory".to_owned(),
            source,
        })?;
        let dockerfile = process.join("Dockerfile");
        let scan = ContextScan::new(&process, &process, &dockerfile)?;
        assert!(
            scan.collect(&process.join("task"), None, &mut BTreeMap::new())
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn context_hash_preflight_rejects_count_size_metadata_and_read_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let root = crate::test_support::tempdir()?;
    let missing = root.path().join("missing");
    let repeated = (0..=MAX_CONTEXT_FILES)
        .map(|index| (PathBuf::from(index.to_string()), missing.clone()))
        .collect::<BTreeMap<_, _>>();
    assert!(matches!(
        hash_context_files(root.path(), &repeated),
        Err(MmError::Build { .. })
    ));

    assert!(matches!(
        hash_context_files(
            root.path(),
            &BTreeMap::from([(PathBuf::from("missing"), missing)])
        ),
        Err(MmError::Io { .. })
    ));

    let mut sparse = BTreeMap::new();
    for index in 0..5 {
        let name = PathBuf::from(format!("sparse-{index}"));
        let path = root.path().join(&name);
        std::fs::File::create(&path)?.set_len(MAX_CONTEXT_FILE_BYTES)?;
        sparse.insert(name, path);
    }
    assert!(matches!(
        hash_context_files(root.path(), &sparse),
        Err(MmError::Build { .. })
    ));

    #[cfg(target_os = "linux")]
    assert!(matches!(
        hash_context_files(
            Path::new("/proc/self"),
            &BTreeMap::from([(PathBuf::from("mem"), PathBuf::from("/proc/self/mem"))])
        ),
        Err(MmError::Io { .. })
    ));
    Ok(())
}

#[test]
fn snapshots_copy_internal_and_external_dockerfiles_and_bind_the_reviewed_hash()
-> Result<(), MmError> {
    let root = crate::test_support::tempdir().map_err(|source| MmError::Io {
        context: "creating snapshot test root".to_owned(),
        source,
    })?;
    let context = root.path().join("context");
    fs::create_dir(&context).map_err(|source| MmError::Io {
        context: "creating snapshot context".to_owned(),
        source,
    })?;
    write(&context.join("app.txt"), "app")?;
    let external = root.path().join("External.Dockerfile");
    write(&external, "FROM scratch\n")?;
    let hash = hash_context(&context, &external)?;
    let destination = root.path().join("external-snapshot");
    let copied = snapshot_build_context(&context, &context, &external, &destination, &hash)?;
    assert_eq!(copied, destination.join(SNAPSHOT_DOCKERFILE));
    assert!(copied.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(
            fs::metadata(&destination)
                .map_err(|source| MmError::Io {
                    context: "reading snapshot permissions".to_owned(),
                    source,
                })?
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
    assert!(snapshot_build_context(&context, &context, &external, &destination, &hash).is_err());
    assert!(matches!(
        snapshot_build_context(
            &context,
            &context,
            &external,
            &context.join("nested-snapshot"),
            &hash
        ),
        Err(MmError::Security { .. })
    ));

    let internal = context.join("Dockerfile");
    write(&internal, "FROM scratch\n")?;
    let internal_hash = hash_context(&context, &internal)?;
    let internal_destination = root.path().join("internal-snapshot");
    let copied = snapshot_build_context(
        &context,
        &context,
        &internal,
        &internal_destination,
        &internal_hash,
    )?;
    assert_eq!(copied, internal_destination.join("Dockerfile"));

    write(&context.join(".dockerignore"), "Dockerfile\n")?;
    let excluded_hash = hash_context(&context, &internal)?;
    let excluded_destination = root.path().join("excluded-snapshot");
    let copied = snapshot_build_context(
        &context,
        &context,
        &internal,
        &excluded_destination,
        &excluded_hash,
    )?;
    assert_eq!(
        fs::read_to_string(copied).map_err(|source| MmError::Io {
            context: "reading explicitly copied Dockerfile".to_owned(),
            source,
        })?,
        "FROM scratch\n"
    );

    let stale_destination = root.path().join("stale-snapshot");
    assert!(matches!(
        snapshot_build_context(
            &context,
            &context,
            &external,
            &stale_destination,
            &"0".repeat(64)
        ),
        Err(MmError::State { .. })
    ));
    assert!(matches!(
        snapshot_build_context(
            &context,
            &context,
            &external,
            &root.path().join("missing-parent/snapshot"),
            &hash
        ),
        Err(MmError::Io { .. })
    ));
    Ok(())
}

#[test]
fn snapshot_detects_a_source_change_during_copy() -> Result<(), Box<dyn std::error::Error>> {
    let root = crate::test_support::tempdir()?;
    let context = root.path().join("context");
    fs::create_dir(&context)?;
    write(&context.join("0-large"), vec![b'a'; 32 * 1024 * 1024])?;
    let late = context.join("z-late");
    write(&late, "reviewed")?;
    let dockerfile = root.path().join("Dockerfile");
    write(&dockerfile, "FROM scratch\n")?;
    let expected = hash_context(&context, &dockerfile)?;
    let destination = root.path().join("snapshot");
    let copy_started = destination.join("0-large");

    let mutator = std::thread::spawn(move || -> Result<(), std::io::Error> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !copy_started.exists() {
            if std::time::Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "snapshot copy did not start",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        fs::write(late, "changed during snapshot")
    });
    let snapshot = snapshot_build_context(&context, &context, &dockerfile, &destination, &expected);
    mutator
        .join()
        .map_err(|_| std::io::Error::other("snapshot mutator panicked"))??;
    assert!(matches!(snapshot, Err(MmError::State { .. })));
    Ok(())
}

#[test]
fn snapshot_reports_a_source_deleted_during_copy() -> Result<(), Box<dyn std::error::Error>> {
    let root = crate::test_support::tempdir()?;
    let context = root.path().join("context");
    fs::create_dir(&context)?;
    write(&context.join("0-large"), vec![b'a'; 32 * 1024 * 1024])?;
    let late = context.join("z-late");
    write(&late, "reviewed")?;
    let dockerfile = root.path().join("Dockerfile");
    write(&dockerfile, "FROM scratch\n")?;
    let expected = hash_context(&context, &dockerfile)?;
    let destination = root.path().join("snapshot");
    let copy_started = destination.join("0-large");

    let remover = std::thread::spawn(move || -> Result<(), std::io::Error> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !copy_started.exists() {
            if std::time::Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "snapshot copy did not start",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        fs::remove_file(late)
    });
    let snapshot = snapshot_build_context(&context, &context, &dockerfile, &destination, &expected);
    remover
        .join()
        .map_err(|_| std::io::Error::other("snapshot remover panicked"))??;
    assert!(matches!(snapshot, Err(MmError::Io { .. })));
    Ok(())
}

#[test]
fn snapshot_reports_dockerfiles_deleted_during_copy() -> Result<(), Box<dyn std::error::Error>> {
    fn wait_then_remove(
        signal: PathBuf,
        path: PathBuf,
    ) -> std::thread::JoinHandle<std::io::Result<()>> {
        std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !signal.exists() {
                if std::time::Instant::now() >= deadline {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "snapshot copy did not start",
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            fs::remove_file(path)
        })
    }

    let root = crate::test_support::tempdir()?;
    let external_context = root.path().join("external-context");
    fs::create_dir(&external_context)?;
    write(
        &external_context.join("0-large"),
        vec![b'a'; 32 * 1024 * 1024],
    )?;
    let external = root.path().join("External.Dockerfile");
    write(&external, "FROM scratch\n")?;
    let expected = hash_context(&external_context, &external)?;
    let destination = root.path().join("external-snapshot");
    let remover = wait_then_remove(destination.join("0-large"), external.clone());
    let result = snapshot_build_context(
        &external_context,
        &external_context,
        &external,
        &destination,
        &expected,
    );
    remover
        .join()
        .map_err(|_| std::io::Error::other("external Dockerfile remover panicked"))??;
    assert!(matches!(result, Err(MmError::Io { .. })));

    let internal_context = root.path().join("internal-context");
    fs::create_dir(&internal_context)?;
    write(&internal_context.join(".dockerignore"), "Dockerfile\n")?;
    write(
        &internal_context.join("0-large"),
        vec![b'b'; 32 * 1024 * 1024],
    )?;
    let internal = internal_context.join("Dockerfile");
    write(&internal, "FROM scratch\n")?;
    let expected = hash_context(&internal_context, &internal)?;
    let destination = root.path().join("internal-snapshot");
    let remover = wait_then_remove(destination.join("0-large"), internal.clone());
    let result = snapshot_build_context(
        &internal_context,
        &internal_context,
        &internal,
        &destination,
        &expected,
    );
    remover
        .join()
        .map_err(|_| std::io::Error::other("internal Dockerfile remover panicked"))??;
    assert!(matches!(result, Err(MmError::Io { .. })));
    Ok(())
}

#[test]
fn snapshot_reports_target_directory_and_final_hash_races() -> Result<(), Box<dyn std::error::Error>>
{
    fn mutate_after_copy(
        signal: PathBuf,
        mutation: impl FnOnce() -> std::io::Result<()> + Send + 'static,
    ) -> std::thread::JoinHandle<std::io::Result<()>> {
        std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !signal.exists() {
                if std::time::Instant::now() >= deadline {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "snapshot copy did not start",
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            mutation()
        })
    }

    let root = crate::test_support::tempdir()?;
    let context = root.path().join("nested-context");
    fs::create_dir_all(context.join("nested"))?;
    write(&context.join("0-large"), vec![b'a'; 32 * 1024 * 1024])?;
    write(&context.join("nested/input"), "input")?;
    let external = root.path().join("External.Dockerfile");
    write(&external, "FROM scratch\n")?;
    let expected = hash_context(&context, &external)?;
    let destination = root.path().join("blocked-context-snapshot");
    let blocker = destination.join("nested");
    let mutator = mutate_after_copy(destination.join("0-large"), move || {
        fs::write(blocker, "not a directory")
    });
    let result = snapshot_build_context(&context, &context, &external, &destination, &expected);
    mutator
        .join()
        .map_err(|_| std::io::Error::other("target blocker panicked"))??;
    assert!(matches!(result, Err(MmError::Io { .. })));

    let context = root.path().join("excluded-dockerfile-context");
    fs::create_dir_all(context.join("nested"))?;
    write(&context.join(".dockerignore"), "nested/Dockerfile\n")?;
    write(&context.join("0-large"), vec![b'b'; 32 * 1024 * 1024])?;
    let internal = context.join("nested/Dockerfile");
    write(&internal, "FROM scratch\n")?;
    let expected = hash_context(&context, &internal)?;
    let destination = root.path().join("blocked-dockerfile-snapshot");
    let blocker = destination.join("nested");
    let mutator = mutate_after_copy(destination.join("0-large"), move || {
        fs::write(blocker, "not a directory")
    });
    let result = snapshot_build_context(&context, &context, &internal, &destination, &expected);
    mutator
        .join()
        .map_err(|_| std::io::Error::other("Dockerfile target blocker panicked"))??;
    assert!(matches!(result, Err(MmError::Io { .. })));

    let context = root.path().join("hash-race-context");
    fs::create_dir(&context)?;
    write(&context.join("0-early"), "early")?;
    write(&context.join("1-large"), vec![b'c'; 32 * 1024 * 1024])?;
    let expected = hash_context(&context, &external)?;
    let destination = root.path().join("hash-race-snapshot");
    let copied = destination.join("0-early");
    let signal = copied.clone();
    let mutator = mutate_after_copy(signal, move || fs::remove_file(copied));
    let result = snapshot_build_context(&context, &context, &external, &destination, &expected);
    mutator
        .join()
        .map_err(|_| std::io::Error::other("snapshot hash mutator panicked"))??;
    assert!(matches!(
        result,
        Err(MmError::Io { .. } | MmError::State { .. })
    ));

    let context = root.path().join("ignore-race-context");
    fs::create_dir(&context)?;
    write(&context.join(".dockerignore"), "ignored\n")?;
    write(&context.join("0-large"), vec![b'd'; 32 * 1024 * 1024])?;
    let expected = hash_context(&context, &external)?;
    let destination = root.path().join("ignore-race-snapshot");
    let copied_ignore = destination.join(".dockerignore");
    let mutator = mutate_after_copy(destination.join("0-large"), move || {
        fs::write(copied_ignore, "\\\n")
    });
    let result = snapshot_build_context(&context, &context, &external, &destination, &expected);
    mutator
        .join()
        .map_err(|_| std::io::Error::other("snapshot ignore mutator panicked"))??;
    assert!(coded(result, "MM_BUILD"));
    Ok(())
}

#[test]
fn snapshot_propagates_source_and_ignore_inventory_errors() -> Result<(), Box<dyn std::error::Error>>
{
    let root = crate::test_support::tempdir()?;
    let missing = root.path().join("missing");
    assert!(matches!(
        snapshot_build_context(
            &missing,
            &missing,
            &root.path().join("Dockerfile"),
            &root.path().join("missing-snapshot"),
            "unused"
        ),
        Err(MmError::Io { .. })
    ));

    let context = root.path().join("invalid-ignore");
    fs::create_dir(&context)?;
    write(&context.join(".dockerignore"), "\\\n")?;
    write(&context.join("Dockerfile"), "FROM scratch\n")?;
    assert!(coded(
        hash_context(&context, &context.join("Dockerfile")),
        "MM_BUILD"
    ));
    assert!(coded(
        snapshot_build_context(
            &context,
            &context,
            &context.join("Dockerfile"),
            &root.path().join("invalid-ignore-snapshot"),
            "unused"
        ),
        "MM_BUILD"
    ));
    Ok(())
}

#[test]
fn dockerignore_parse_and_size_errors_fail_with_build_context() -> Result<(), MmError> {
    let root = crate::test_support::tempdir().map_err(|source| MmError::Io {
        context: "creating dockerignore test root".to_owned(),
        source,
    })?;
    let dockerfile = root.path().join("Dockerfile");
    write(&root.path().join(".dockerignore"), "\\\n")?;
    assert!(coded(
        ContextScan::new(root.path(), root.path(), &dockerfile),
        "MM_BUILD"
    ));

    write(
        &root.path().join(".dockerignore"),
        format!("{}\n", "{a,b}".repeat(300_000)),
    )?;
    assert!(coded(
        ContextScan::new(root.path(), root.path(), &dockerfile),
        "MM_BUILD"
    ));
    Ok(())
}
