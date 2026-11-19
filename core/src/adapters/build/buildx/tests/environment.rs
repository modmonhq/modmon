use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::adapters::process::TokioProcessRunner;

// Only shell builtins are used: PATH deliberately contains just the fixture directory.
const DOCKER: &str = r#"#!/bin/sh
set -eu
[ "$PATH" = "${0%/*}" ]
[ "${HOME-}" = '/mm-buildx-test-home' ]
[ "${DOCKER_CONFIG-}" = '/mm-buildx-test-config' ]
[ "${DOCKER_CONTEXT-}" = 'mm-buildx-test-context' ]
[ "${DOCKER_HOST-}" = 'unix:///mm-buildx-test.sock' ]
[ "${DOCKER_TLS_VERIFY-}" = '1' ]
[ "${DOCKER_CERT_PATH-}" = '/mm-buildx-test-certs' ]
[ "${MM_BUILDX_UNRELATED_SECRET+present}" != present ]
[ "${MM_BUILDX_ENVIRONMENT_CHILD+present}" != present ]
[ "$1" = buildx ]
shift
digest=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
case "$1" in
  build)
    [ "${DOCKER_BUILDKIT-}" = '1' ]
    shift
    metadata=
    archive=
    publish=false
    while [ "$#" -gt 0 ]; do
      case "$1" in
        --metadata-file) metadata=$2; shift ;;
        --output) archive=${2#type=oci,dest=}; shift ;;
        --push) publish=true ;;
      esac
      shift
    done
    if [ "$publish" = true ]; then
      [ -z "$archive" ]
    else
      [ -n "$archive" ]
      printf 'oci' > "$archive"
    fi
    printf '{"containerimage.digest":"%s"}' "$digest" > "$metadata"
    ;;
  imagetools)
    [ "${DOCKER_BUILDKIT+present}" != present ]
    case "$2" in
      create) exit 0 ;;
      inspect) printf '{"digest":"%s"}' "$digest" ;;
      *) exit 2 ;;
    esac
    ;;
  *) exit 2 ;;
esac
"#;

#[tokio::test]
async fn buildx_inherits_only_the_docker_environment() -> Result<(), Box<dyn std::error::Error>> {
    const CHILD: &str = "MM_BUILDX_ENVIRONMENT_CHILD";
    if let Ok(operation) = std::env::var(CHILD) {
        return exercise_buildx(&operation).await;
    }

    let bin = tempfile::Builder::new()
        .prefix("mm buildx bin ")
        .tempdir_in(crate::test_support::temp_root()?)?;
    // A unique executable name prevents fallback to a real Docker on the host.
    let executable = bin.path().join("mm-test-docker");
    fs::write(&executable, DOCKER)?;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))?;
    let test = concat!(
        module_path!(),
        "::buildx_inherits_only_the_docker_environment"
    );
    // libtest names omit the crate prefix present in module_path!().
    let (_, test) = test.split_once("::").ok_or("test module missing")?;
    for operation in ["local", "publish", "retag"] {
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([test, "--exact", "--nocapture"])
            .env(CHILD, operation)
            .env("PATH", bin.path())
            .env("HOME", "/mm-buildx-test-home")
            .env("DOCKER_CONFIG", "/mm-buildx-test-config")
            .env("DOCKER_CONTEXT", "mm-buildx-test-context")
            .env("DOCKER_HOST", "unix:///mm-buildx-test.sock")
            .env("DOCKER_TLS_VERIFY", "1")
            .env("DOCKER_CERT_PATH", "/mm-buildx-test-certs")
            .env("DOCKER_BUILDKIT", "0")
            .env("MM_BUILDX_UNRELATED_SECRET", "must-not-reach-docker")
            .output()?;
        assert!(
            output.status.success(),
            "{operation} failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "child did not run the regression test"
        );
    }
    Ok(())
}

async fn exercise_buildx(operation: &str) -> Result<(), Box<dyn std::error::Error>> {
    let artifacts = crate::test_support::tempdir()?;
    let mut builder = DockerBuildxBuilder::new(
        Arc::new(TokioProcessRunner),
        artifacts.path().join("builds"),
    );
    builder.executable = "mm-test-docker".to_owned();
    let expected = digest('a');
    if operation == "retag" {
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
        return Ok(());
    }

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
    let operation_id = crate::domain::operation::OperationId::new();
    let _guard = builder.begin_operation(operation_id)?;
    let publish = operation == "publish";
    let image = builder
        .build_variant(BuildVariantRequest {
            operation_id,
            project_root: project.path(),
            plan_hash: &"1".repeat(64),
            platforms: &["linux/amd64".to_owned()],
            publish,
            unit: &unit,
            variant: &variant,
            secrets: &BTreeMap::new(),
            dependencies: &[],
            upstream_contexts: &[],
            engine: None,
        })
        .await?;
    assert_eq!(image.digest, expected);
    if publish {
        assert!(image.archive.is_none());
    } else {
        assert!(image.archive.as_deref().is_some_and(Path::is_file));
    }
    Ok(())
}
