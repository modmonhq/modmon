use std::collections::VecDeque;
use std::sync::Mutex;

use zeroize::Zeroizing;

use super::*;
use crate::app::build::hash_context;
use crate::app::ports::CommandOutput;
use crate::domain::build::{
    BuildDependencyReference, BuildReusePolicy, FirstPartyBuildEdge, ResolvedBuildDependency,
};

#[cfg(unix)]
#[path = "tests/environment.rs"]
mod environment;

#[test]
fn parses_buildx_digest_without_retaining_other_metadata() -> Result<(), MmError> {
    let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
        context: "creating test directory".to_owned(),
        source,
    })?;
    let path = directory.path().join("metadata.json");
    fs::write(
            &path,
            br#"{"containerimage.digest":"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","buildx.build.ref":"secret-looking-but-not-retained"}"#,
        )
        .map_err(|source| MmError::Io {
            context: "writing test metadata".to_owned(),
            source,
        })?;

    assert_eq!(
        read_digest(&path)?,
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    );
    Ok(())
}

#[derive(Debug, Default)]
struct RecordingProcess {
    arguments: Mutex<Vec<String>>,
    redacted_secret: Mutex<bool>,
}

#[async_trait]
impl ProcessRunner for RecordingProcess {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let arguments = specification
            .arguments()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let metadata_index = arguments
            .iter()
            .position(|argument| argument == "--metadata-file")
            .ok_or_else(|| MmError::State {
                message: "test command omitted metadata path".to_owned(),
            })?;
        let metadata_path = arguments
            .get(metadata_index + 1)
            .ok_or_else(|| MmError::State {
                message: "test command omitted metadata value".to_owned(),
            })?;
        fs::write(
                metadata_path,
                br#"{"containerimage.digest":"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}"#,
            )
            .map_err(|source| MmError::Io {
                context: "writing fake Buildx metadata".to_owned(),
                source,
            })?;
        if let Some(output_index) = arguments.iter().position(|argument| argument == "--output") {
            let output = arguments
                .get(output_index + 1)
                .and_then(|argument| argument.strip_prefix("type=oci,dest="))
                .ok_or_else(|| MmError::State {
                    message: "test command omitted OCI output".to_owned(),
                })?;
            fs::write(output, b"oci").map_err(|source| MmError::Io {
                context: "writing fake OCI archive".to_owned(),
                source,
            })?;
        }
        *self.redacted_secret.lock().map_err(|_| MmError::State {
            message: "test redaction lock was poisoned".to_owned(),
        })? = specification
            .sensitive_values()
            .any(|value| value == "super-secret-value");
        *self.arguments.lock().map_err(|_| MmError::State {
            message: "test argument lock was poisoned".to_owned(),
        })? = arguments;
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(String::new()),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

#[tokio::test]
async fn snapshots_context_and_never_passes_secret_material_as_an_argument()
-> Result<(), Box<dyn std::error::Error>> {
    let project = crate::test_support::tempdir()?;
    fs::write(project.path().join("Dockerfile"), "FROM scratch\n")?;
    fs::write(project.path().join("app.txt"), "content\n")?;
    let source_dockerfile = fs::read(project.path().join("Dockerfile"))?;
    let context_hash = hash_context(project.path(), &project.path().join("Dockerfile"))?;
    let artifacts = crate::test_support::tempdir()?;
    let process = Arc::new(RecordingProcess::default());
    let builder = DockerBuildxBuilder::new(process.clone(), artifacts.path().join("builds"));
    let unit = ImageBuildUnitPlan {
        module: "app".to_owned(),
        image: "ghcr.io/example/app".to_owned(),
        context: PathBuf::from("."),
        dockerfile: PathBuf::from("Dockerfile"),
        context_hash,
        dockerfile_hash: String::new(),
        args: BTreeMap::from([("VERSION".to_owned(), "01234567".to_owned())]),
        secret_ids: vec!["token".to_owned()],
        credential_secret_ids: Vec::new(),
        dependencies: Vec::new(),
        upstream_node_ids: Vec::new(),
        extra_inputs: Vec::new(),
        version_embedded: false,
        reuse_policy: crate::domain::build::BuildReusePolicy::Allowed,
        input_fingerprint: "0".repeat(64),
        variants: Vec::new(),
    };
    let variant = ImageBuildVariantPlan {
        target: None,
        tag_suffix: String::new(),
        tags: vec!["sha-0123456789abcdef0123456789abcdef01234567".to_owned()],
        stage: None,
        args: BTreeMap::new(),
    };
    let secrets = BTreeMap::from([(
        "token".to_owned(),
        SecretMaterial::new("super-secret-value".to_owned())?,
    )]);

    let operation_id = crate::domain::operation::OperationId::new();
    let operation_guard = builder.begin_operation(operation_id)?;
    let built = builder
        .build_variant(BuildVariantRequest {
            operation_id,
            project_root: project.path(),
            plan_hash: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            platforms: &["linux/amd64".to_owned()],
            publish: false,
            unit: &unit,
            variant: &variant,
            secrets: &secrets,
            dependencies: &[],
            upstream_contexts: &[],
        })
        .await?;
    operation_guard.preserve();

    assert!(built.archive.as_deref().is_some_and(Path::is_file));
    assert!(
        *process
            .redacted_secret
            .lock()
            .map_err(|_| "lock poisoned")?,
        "secret value was not registered for output redaction"
    );
    let arguments = process.arguments.lock().map_err(|_| "lock poisoned")?;
    assert!(
        arguments
            .iter()
            .all(|argument| !argument.contains("super-secret-value"))
    );
    let secret_path = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("id=token,src="))
        .map(PathBuf::from)
        .ok_or("secret path argument missing")?;
    assert!(!secret_path.exists());
    assert_eq!(
        fs::read(project.path().join("Dockerfile"))?,
        source_dockerfile
    );
    Ok(())
}

#[test]
fn operation_guard_cleans_failure_and_cancellation_resources()
-> Result<(), Box<dyn std::error::Error>> {
    let artifacts = crate::test_support::tempdir()?;
    let builder = DockerBuildxBuilder::new(
        Arc::new(RecordingProcess::default()),
        artifacts.path().join("builds"),
    );
    let cancelled = crate::domain::operation::OperationId::new();
    let guard = builder.begin_operation(cancelled)?;
    let cancelled_path = operation_directory(&builder.artifacts, cancelled);
    fs::write(cancelled_path.join("partial"), b"partial")?;
    drop(guard);
    assert!(!cancelled_path.exists());

    let succeeded = crate::domain::operation::OperationId::new();
    let guard = builder.begin_operation(succeeded)?;
    let succeeded_path = operation_directory(&builder.artifacts, succeeded);
    guard.preserve();
    assert!(succeeded_path.is_dir());
    builder.cleanup_operation(succeeded)?;
    assert!(!succeeded_path.exists());
    Ok(())
}

struct QueueProcess {
    outputs: Mutex<VecDeque<CommandOutput>>,
    commands: Mutex<Vec<String>>,
}

struct MaterializingBuildProcess {
    digest: String,
}

#[async_trait]
impl ProcessRunner for MaterializingBuildProcess {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let arguments = specification
            .arguments()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let metadata = arguments
            .windows(2)
            .find(|pair| pair[0] == "--metadata-file")
            .map(|pair| PathBuf::from(&pair[1]))
            .ok_or_else(|| MmError::Build {
                message: "test build omitted metadata path".to_owned(),
            })?;
        fs::write(
            metadata,
            serde_json::json!({"containerimage.digest": self.digest}).to_string(),
        )
        .map_err(|source| MmError::Io {
            context: "writing test build metadata".to_owned(),
            source,
        })?;
        if let Some(output) = arguments
            .windows(2)
            .find(|pair| pair[0] == "--output")
            .map(|pair| pair[1].as_str())
        {
            let archive = output
                .strip_prefix("type=oci,dest=")
                .ok_or_else(|| MmError::Build {
                    message: "test build output was not OCI".to_owned(),
                })?;
            write_oci_archive(Path::new(archive), true, false).map_err(|error| MmError::Build {
                message: error.to_string(),
            })?;
        }
        Ok(queued_output(0, "", ""))
    }
}

#[async_trait]
impl ProcessRunner for QueueProcess {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        self.commands
            .lock()
            .map_err(|_| MmError::State {
                message: "Buildx queue command lock was poisoned".to_owned(),
            })?
            .push(specification.redacted_display());
        self.outputs
            .lock()
            .map_err(|_| MmError::State {
                message: "Buildx queue output lock was poisoned".to_owned(),
            })?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "Buildx queue output was empty".to_owned(),
            })
    }
}

fn queued_output(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(stdout.to_owned()),
        stderr: Zeroizing::new(stderr.to_owned()),
        truncated: false,
    }
}

fn digest(character: char) -> String {
    format!("sha256:{}", character.to_string().repeat(64))
}

#[tokio::test]
async fn digest_inspection_and_retagging_verify_every_remote_result()
-> Result<(), Box<dyn std::error::Error>> {
    let expected = digest('a');
    for body in [
        format!(r#"{{"digest":"{expected}"}}"#),
        format!(r#"{{"manifest":{{"digest":"{expected}"}}}}"#),
    ] {
        let process = QueueProcess {
            outputs: Mutex::new(VecDeque::from([queued_output(0, &body, "")])),
            commands: Mutex::new(Vec::new()),
        };
        assert_eq!(
            inspect_remote_digest(&process, "docker", "example.test/app:tag").await?,
            expected
        );
    }
    for output in [
        queued_output(1, "", "denied"),
        queued_output(0, "not-json", ""),
        queued_output(0, "{}", ""),
    ] {
        let process = QueueProcess {
            outputs: Mutex::new(VecDeque::from([output])),
            commands: Mutex::new(Vec::new()),
        };
        assert!(
            inspect_remote_digest(&process, "docker", "example.test/app:tag")
                .await
                .is_err()
        );
    }

    let process = Arc::new(QueueProcess {
        outputs: Mutex::new(VecDeque::from([
            queued_output(0, "", ""),
            queued_output(0, &format!(r#"{{"digest":"{expected}"}}"#), ""),
            queued_output(0, &format!(r#"{{"digest":"{expected}"}}"#), ""),
        ])),
        commands: Mutex::new(Vec::new()),
    });
    let builder = DockerBuildxBuilder::new(process.clone(), PathBuf::from("unused"));
    assert_eq!(
        builder
            .retag_variant(
                "example.test/source",
                &expected,
                "example.test/destination",
                &["one".to_owned(), "two".to_owned()],
            )
            .await?,
        expected
    );
    assert_eq!(
        process.commands.lock().map_err(|_| "command lock")?.len(),
        3
    );
    assert!(
        builder
            .retag_variant("source", "bad", "destination", &["tag".to_owned()])
            .await
            .is_err()
    );
    assert!(
        builder
            .retag_variant("source", &digest('b'), "destination", &[])
            .await
            .is_err()
    );

    let mismatch = Arc::new(QueueProcess {
        outputs: Mutex::new(VecDeque::from([
            queued_output(0, "", ""),
            queued_output(0, &format!(r#"{{"digest":"{}"}}"#, digest('c')), ""),
        ])),
        commands: Mutex::new(Vec::new()),
    });
    let builder = DockerBuildxBuilder::new(mismatch, PathBuf::from("unused"));
    assert!(
        builder
            .retag_variant("source", &digest('b'), "destination", &["tag".to_owned()])
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn local_variant_build_materializes_and_publishes_an_immutable_oci_archive()
-> Result<(), Box<dyn std::error::Error>> {
    let project = crate::test_support::tempdir()?;
    fs::create_dir(project.path().join("app"))?;
    fs::write(project.path().join("app/Dockerfile"), "FROM scratch\n")?;
    let artifacts = crate::test_support::tempdir()?;
    let expected = digest('f');
    let builder = DockerBuildxBuilder::new(
        Arc::new(MaterializingBuildProcess {
            digest: expected.clone(),
        }),
        artifacts.path().join("builds"),
    );
    let operation_id = crate::domain::operation::OperationId::new();
    let guard = builder.begin_operation(operation_id)?;
    let unit = live_unit(project.path(), "app", "example.test/app")?;
    let variant = ImageBuildVariantPlan {
        target: Some("release".to_owned()),
        tag_suffix: "release".to_owned(),
        tags: vec!["test".to_owned()],
        stage: None,
        args: BTreeMap::new(),
    };
    let image = builder
        .build_variant(BuildVariantRequest {
            operation_id,
            project_root: project.path(),
            plan_hash: &"1".repeat(64),
            platforms: &["linux/amd64".to_owned()],
            publish: false,
            unit: &unit,
            variant: &variant,
            secrets: &BTreeMap::new(),
            dependencies: &[],
            upstream_contexts: &[],
        })
        .await?;
    assert_eq!(image.digest, expected);
    let archive = image.archive.ok_or("archive missing")?;
    assert!(archive.is_file());
    assert!(archive.starts_with(operation_directory(&builder.artifacts, operation_id)));
    guard.preserve();
    builder.cleanup_operation(operation_id)?;
    Ok(())
}

#[test]
fn metadata_parsing_accepts_descriptor_digest_and_rejects_every_malformed_shape()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let path = temp.path().join("metadata.json");
    let expected = digest('d');
    fs::write(
        &path,
        format!(r#"{{"containerimage.descriptor":{{"digest":"{expected}"}}}}"#),
    )?;
    assert_eq!(read_digest(&path)?, expected);
    for body in [
        "not-json".to_owned(),
        "{}".to_owned(),
        r#"{"containerimage.digest":"sha256:ABC"}"#.to_owned(),
    ] {
        fs::write(&path, body)?;
        assert!(read_digest(&path).is_err());
    }
    fs::remove_file(&path)?;
    assert!(read_digest(&path).is_err());
    fs::write(&path, vec![b'x'; MAX_METADATA_BYTES + 1])?;
    assert!(read_digest(&path).is_err());
    assert!(is_sha256_digest(&digest('0')));
    assert!(!is_sha256_digest(&format!("sha256:{}", "A".repeat(64))));
    assert!(!is_sha256_digest("sha256:short"));
    Ok(())
}

fn write_oci_archive(
    path: &Path,
    include_layout: bool,
    include_unsafe: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(path)?;
    let mut archive = tar::Builder::new(file);
    if include_layout {
        for (name, contents) in [
            ("oci-layout", b"{}".as_slice()),
            ("index.json", b"{}".as_slice()),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(u64::try_from(contents.len())?);
            header.set_mode(0o600);
            header.set_cksum();
            archive.append_data(&mut header, name, contents)?;
        }
    }
    if include_unsafe {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_link_name("/etc/passwd")?;
        header.set_cksum();
        archive.append_data(&mut header, "unsafe-link", std::io::empty())?;
    }
    archive.finish()?;
    Ok(())
}

#[test]
fn upstream_contexts_extract_only_complete_safe_oci_layouts()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let expected = digest('e');
    let remote = UpstreamBuildContext {
        immutable_reference: format!("example.test/base@{expected}"),
        digest: expected.clone(),
        local_archive: None,
    };
    assert_eq!(
        build_context_source(temp.path(), 0, &remote)?,
        format!("docker-image://example.test/base@{expected}")
    );
    let mut unpinned = remote.clone();
    unpinned.immutable_reference = "example.test/base:latest".to_owned();
    assert!(build_context_source(temp.path(), 0, &unpinned).is_err());
    unpinned.digest = "bad".to_owned();
    assert!(build_context_source(temp.path(), 0, &unpinned).is_err());

    let valid_archive = temp.path().join("valid.tar");
    write_oci_archive(&valid_archive, true, false)?;
    let local = UpstreamBuildContext {
        local_archive: Some(valid_archive),
        ..remote.clone()
    };
    let source = build_context_source(temp.path(), 1, &local)?;
    assert!(source.starts_with("oci-layout://"));
    assert!(temp.path().join("upstream-1/oci-layout").is_file());

    let incomplete_archive = temp.path().join("incomplete.tar");
    write_oci_archive(&incomplete_archive, false, false)?;
    let incomplete = UpstreamBuildContext {
        local_archive: Some(incomplete_archive),
        ..remote.clone()
    };
    assert!(build_context_source(temp.path(), 2, &incomplete).is_err());

    let unsafe_archive = temp.path().join("unsafe.tar");
    write_oci_archive(&unsafe_archive, true, true)?;
    let unsafe_context = UpstreamBuildContext {
        local_archive: Some(unsafe_archive),
        ..remote
    };
    assert!(build_context_source(temp.path(), 3, &unsafe_context).is_err());
    Ok(())
}

#[test]
fn operation_secret_archive_and_directory_helpers_fail_closed_and_replace_regular_outputs()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let temp = crate::test_support::tempdir()?;
    let builder = DockerBuildxBuilder::new(
        Arc::new(RecordingProcess::default()),
        temp.path().join("builds"),
    );
    assert!(format!("{builder:?}").contains("DockerBuildxBuilder"));
    let operation_id = crate::domain::operation::OperationId::new();
    let guard = builder.begin_operation(operation_id)?;
    assert!(builder.begin_operation(operation_id).is_err());
    builder.cleanup_operation(crate::domain::operation::OperationId::new())?;

    let operation = operation_directory(&builder.artifacts, operation_id);
    let secret = operation.join("secret");
    write_secret(&secret, &SecretMaterial::new("value".to_owned())?)?;
    assert_eq!(fs::metadata(&secret)?.permissions().mode() & 0o777, 0o600);
    assert!(write_secret(&secret, &SecretMaterial::new("again".to_owned())?).is_err());
    assert!(
        write_secret(
            &operation.join("missing/secret"),
            &SecretMaterial::new("value".to_owned())?
        )
        .is_err()
    );
    assert!(set_private_directory_permissions(&operation.join("missing")).is_err());
    let blocked = operation.join("blocked");
    fs::write(&blocked, b"file")?;
    assert!(prepare_private_directory(&blocked).is_err());

    let project = crate::test_support::tempdir()?;
    fs::create_dir(project.path().join("app"))?;
    fs::write(project.path().join("app/Dockerfile"), "FROM scratch\n")?;
    let unit = live_unit(project.path(), "app", "example.test/app")?;
    let variant = ImageBuildVariantPlan {
        target: None,
        tag_suffix: String::new(),
        tags: vec!["test".to_owned()],
        stage: None,
        args: BTreeMap::new(),
    };
    assert!(
        publish_archive(
            &builder.artifacts,
            operation_id,
            &unit,
            &variant,
            &operation.join("missing.tar"),
        )
        .is_err()
    );
    let first = operation.join("first.tar");
    fs::write(&first, b"first")?;
    let destination = publish_archive(&builder.artifacts, operation_id, &unit, &variant, &first)?;
    assert_eq!(fs::read(&destination)?, b"first");
    let second = operation.join("second.tar");
    fs::write(&second, b"second")?;
    assert_eq!(
        publish_archive(&builder.artifacts, operation_id, &unit, &variant, &second,)?,
        destination
    );
    assert_eq!(fs::read(&destination)?, b"second");
    fs::remove_file(&destination)?;
    fs::create_dir(&destination)?;
    let third = operation.join("third.tar");
    fs::write(&third, b"third")?;
    assert!(publish_archive(&builder.artifacts, operation_id, &unit, &variant, &third,).is_err());

    assert!(extract_oci_layout(&operation.join("missing.tar"), &operation).is_err());
    let malformed = operation.join("malformed.tar");
    fs::write(&malformed, b"not a tar archive")?;
    assert!(extract_oci_layout(&malformed, &operation).is_err());
    drop(guard);
    Ok(())
}

#[tokio::test]
async fn published_variant_uses_push_without_requiring_a_local_archive()
-> Result<(), Box<dyn std::error::Error>> {
    let project = crate::test_support::tempdir()?;
    fs::create_dir(project.path().join("app"))?;
    fs::write(project.path().join("app/Dockerfile"), "FROM scratch\n")?;
    let artifacts = crate::test_support::tempdir()?;
    let builder = DockerBuildxBuilder::new(
        Arc::new(MaterializingBuildProcess {
            digest: digest('9'),
        }),
        artifacts.path().join("builds"),
    );
    let operation_id = crate::domain::operation::OperationId::new();
    let guard = builder.begin_operation(operation_id)?;
    let unit = live_unit(project.path(), "app", "example.test/app")?;
    let variant = ImageBuildVariantPlan {
        target: None,
        tag_suffix: String::new(),
        tags: vec!["published".to_owned()],
        stage: None,
        args: BTreeMap::new(),
    };
    let image = builder
        .build_variant(BuildVariantRequest {
            operation_id,
            project_root: project.path(),
            plan_hash: &"2".repeat(64),
            platforms: &["linux/amd64".to_owned()],
            publish: true,
            unit: &unit,
            variant: &variant,
            secrets: &BTreeMap::new(),
            dependencies: &[],
            upstream_contexts: &[],
        })
        .await?;
    assert!(image.archive.is_none());
    drop(guard);
    Ok(())
}

#[tokio::test]
#[ignore = "requires a live Docker Buildx builder"]
async fn live_local_graph_resolves_parent_from_its_oci_digest()
-> Result<(), Box<dyn std::error::Error>> {
    live_graph(None, &["linux/amd64".to_owned()], false).await
}

#[tokio::test]
#[ignore = "requires MM_TEST_REGISTRY and a live multi-platform Buildx builder"]
async fn live_published_graph_resolves_parent_manifest_digest()
-> Result<(), Box<dyn std::error::Error>> {
    let registry = std::env::var("MM_TEST_REGISTRY")?;
    live_graph(
        Some(&registry),
        &["linux/amd64".to_owned(), "linux/arm64".to_owned()],
        false,
    )
    .await
}

#[tokio::test]
#[ignore = "requires MM_TEST_REGISTRY and a live multi-platform Buildx builder"]
async fn live_parent_publication_then_child_failure_cleans_operation_resources()
-> Result<(), Box<dyn std::error::Error>> {
    let registry = std::env::var("MM_TEST_REGISTRY")?;
    live_graph(Some(&registry), &["linux/amd64".to_owned()], true).await
}

async fn live_graph(
    registry: Option<&str>,
    platforms: &[String],
    fail_child: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let project = crate::test_support::tempdir()?;
    fs::create_dir(project.path().join("base"))?;
    fs::create_dir(project.path().join("child"))?;
    fs::write(
        project.path().join("base/Dockerfile"),
        "FROM scratch\nCOPY marker /parent-marker\n",
    )?;
    fs::write(project.path().join("base/marker"), "new-parent\n")?;
    let prefix = registry.map_or_else(|| "build-graph.test".to_owned(), str::to_owned);
    let parent_repository = format!("{prefix}/mm-build-parent");
    let child_repository = format!("{prefix}/mm-build-child");
    let child_source = format!(
        "FROM {parent_repository}:mutable\nCOPY marker /child-marker\n{}",
        if fail_child {
            "RUN [\"/definitely-missing\"]\n"
        } else {
            ""
        }
    );
    fs::write(project.path().join("child/Dockerfile"), &child_source)?;
    fs::write(project.path().join("child/marker"), "child\n")?;
    let artifacts = crate::test_support::tempdir()?;
    let builder = DockerBuildxBuilder::new(
        Arc::new(crate::adapters::process::TokioProcessRunner),
        artifacts.path().join("builds"),
    );
    let operation_id = crate::domain::operation::OperationId::new();
    let guard = builder.begin_operation(operation_id)?;
    let parent = live_unit(project.path(), "base", &parent_repository)?;
    let child = live_unit(project.path(), "child", &child_repository)?;
    let variant = ImageBuildVariantPlan {
        target: None,
        tag_suffix: String::new(),
        tags: vec!["build-graph-live".to_owned()],
        stage: None,
        args: BTreeMap::new(),
    };
    let secrets = BTreeMap::new();
    let publish = registry.is_some();
    let parent_image = builder
        .build_variant(BuildVariantRequest {
            operation_id,
            project_root: project.path(),
            plan_hash: &"1".repeat(64),
            platforms,
            publish,
            unit: &parent,
            variant: &variant,
            secrets: &secrets,
            dependencies: &[],
            upstream_contexts: &[],
        })
        .await?;
    let immutable_reference = format!("{parent_repository}@{}", parent_image.digest);
    let edge = FirstPartyBuildEdge {
        producer_module: "base".to_owned(),
        dependent_module: "child".to_owned(),
        stage: None,
        reference: BuildDependencyReference::Dockerfile {
            original_reference: format!("{parent_repository}:mutable"),
            instruction_index: 0,
            source_line: 1,
        },
    };
    let dependency = ResolvedBuildDependency {
        edge,
        dependent_target: None,
        digest: parent_image.digest.clone(),
        immutable_reference: immutable_reference.clone(),
    };
    let upstream = UpstreamBuildContext {
        immutable_reference,
        digest: parent_image.digest.clone(),
        local_archive: parent_image.archive.clone(),
    };
    let child_result = builder
        .build_variant(BuildVariantRequest {
            operation_id,
            project_root: project.path(),
            plan_hash: &"1".repeat(64),
            platforms,
            publish,
            unit: &child,
            variant: &variant,
            secrets: &secrets,
            dependencies: &[dependency],
            upstream_contexts: &[upstream],
        })
        .await;
    if fail_child {
        assert!(matches!(child_result, Err(MmError::ProcessFailed { .. })));
        let operation_path = operation_directory(&builder.artifacts, operation_id);
        drop(guard);
        assert!(!operation_path.exists());
        assert_eq!(
            fs::read(project.path().join("child/Dockerfile"))?,
            child_source.as_bytes()
        );
        return Ok(());
    }
    let child_image = child_result?;
    assert!(is_sha256_digest(&parent_image.digest));
    assert!(is_sha256_digest(&child_image.digest));
    assert_eq!(
        fs::read(project.path().join("child/Dockerfile"))?,
        child_source.as_bytes()
    );
    guard.preserve();
    Ok(())
}

fn live_unit(root: &Path, module: &str, repository: &str) -> Result<ImageBuildUnitPlan, MmError> {
    let context = root.join(module);
    let dockerfile = context.join("Dockerfile");
    Ok(ImageBuildUnitPlan {
        module: module.to_owned(),
        image: repository.to_owned(),
        context: PathBuf::from(module),
        dockerfile: PathBuf::from(module).join("Dockerfile"),
        context_hash: hash_context(&context, &dockerfile)?,
        dockerfile_hash: String::new(),
        args: BTreeMap::from([("VERSION".to_owned(), "build-graph-version".to_owned())]),
        secret_ids: Vec::new(),
        credential_secret_ids: Vec::new(),
        dependencies: Vec::new(),
        upstream_node_ids: Vec::new(),
        extra_inputs: Vec::new(),
        version_embedded: false,
        reuse_policy: BuildReusePolicy::Allowed,
        input_fingerprint: "0".repeat(64),
        variants: Vec::new(),
    })
}
