use std::error::Error;
use std::process::Command;
use std::sync::Arc;

use mm::adapters::process::TokioProcessRunner;
use mm::adapters::runtime::{DockerComposeRuntime, PodmanComposeRuntime};
use mm::app::ports::{LocalDeploymentRuntime, OwnedVolumeFile, VolumeOwnership};
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId};

mod temp_dirs;

const BUSYBOX_IMAGE: &str = "docker.io/library/busybox@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0";

fn as_root(engine: &str, volume: &str, script: &str) -> Result<String, Box<dyn Error>> {
    let output = Command::new(engine)
        .args([
            "run",
            "--rm",
            "--network",
            "none",
            "--user",
            "0:0",
            "--volume",
        ])
        .arg(format!("{volume}:/volume"))
        .args([BUSYBOX_IMAGE, "sh", "-c", script])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "{engine} helper failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn owned(name: &str, uid: u32, gid: u32, mode: u32) -> OwnedVolumeFile<'_> {
    OwnedVolumeFile {
        name,
        ownership: VolumeOwnership { uid, gid, mode },
    }
}

async fn deliver(
    engine: &str,
    runtime: &dyn LocalDeploymentRuntime,
    target: &NamedTarget,
    volume: &str,
) -> Result<(), Box<dyn Error>> {
    as_root(
        engine,
        volume,
        "chown 100:1000 /volume && chmod 700 /volume",
    )?;
    let temporary = temp_dirs::tempdir()?;
    let source = |name: &str, contents: &str| -> Result<std::path::PathBuf, std::io::Error> {
        let path = temporary.path().join(name);
        std::fs::write(&path, contents)?;
        Ok(path)
    };
    let config = source("config.hcl", "first\n")?;
    runtime
        .replace_volume_file_owned(
            target,
            volume,
            "data",
            owned("config.hcl", 100, 1000, 0o600),
            &config,
        )
        .await?;
    let config = source("config.hcl", "second\n")?;
    runtime
        .replace_volume_file_owned(
            target,
            volume,
            "data",
            owned("config.hcl", 100, 1000, 0o600),
            &config,
        )
        .await?;
    let route = source("route.yaml", "http: {}\n")?;
    runtime
        .replace_volume_file_owned(
            target,
            volume,
            "data",
            owned("route.yaml", 0, 0, 0o644),
            &route,
        )
        .await?;
    let plain = source("plain", "plain\n")?;
    runtime
        .replace_volume_file(target, volume, "data", "plain", &plain, 0o640)
        .await?;
    runtime
        .create_volume_file_if_absent(target, volume, "data", "empty", 0o600)
        .await?;
    runtime
        .remove_volume_file(target, volume, "data", "route.yaml")
        .await?;

    let listing = as_root(
        engine,
        volume,
        "cd /volume && stat -c '%n %u:%g %a' . config.hcl plain && stat -c '%n %a' empty && ls -A && cat config.hcl",
    )?;
    assert_eq!(
        listing,
        ". 100:1000 700\nconfig.hcl 100:1000 600\nplain 0:0 640\nempty 600\nconfig.hcl\nempty\nplain\nsecond\n"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires MM_TEST_RUNTIME and a live local Docker/Podman engine"]
async fn files_reach_a_volume_whose_root_uid_100_owns_with_mode_0700() -> Result<(), Box<dyn Error>>
{
    let process = Arc::new(TokioProcessRunner);
    let (engine, runtime): (&str, Box<dyn LocalDeploymentRuntime>) =
        match std::env::var("MM_TEST_RUNTIME")?.as_str() {
            "docker-compose" => ("docker", Box::new(DockerComposeRuntime::new(process))),
            "podman-compose" => ("podman", Box::new(PodmanComposeRuntime::new(process))),
            _ => return Err("MM_TEST_RUNTIME must name a supported runtime".into()),
        };
    let target = NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(uuid::Uuid::new_v4())),
    );
    let volume = format!("mm-live-volume-files-{}", uuid::Uuid::new_v4().simple());
    runtime.ensure_volume(&target, &volume, "data").await?;
    let delivered = deliver(engine, runtime.as_ref(), &target, &volume).await;
    let removed = runtime.remove_volume(&target, &volume, "data").await;
    delivered?;
    removed?;
    Ok(())
}
