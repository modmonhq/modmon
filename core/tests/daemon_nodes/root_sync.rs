//! `mmd access sync`: the first team sync, run as root on the host.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::fixture::{FakeUseCases, config};
use mm::adapters::remote::{HOST_ROOT, RemoteDaemon, RootAccessSyncRequest};
use mm::app::ports::RemoteUseCases;
use mm::domain::host_policy::{AccessLevel, EnvironmentAccessRecord};
use mm::domain::workspace::ProjectId;

const ALICE: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w";
const CAROL: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINlgyKENv7afsOxxzNFVX/GU+789xK97zuDNx8Thx+g+";

fn git(
    directory: &std::path::Path,
    arguments: &[&str],
) -> Result<String, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.test"])
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn project_config(users: &str) -> String {
    format!(
        "schema-version: 1\nproject:\n  id: {}\n  name: example\ntarget: local\nstacks:\n  app:\n    files: [compose.yaml]\nenvironments:\n  production: {{}}\nusers:\n{users}",
        uuid::Uuid::from_u128(4)
    )
}

/// An origin whose `main` has two commits: the first lets alice deploy to
/// production, the second adds carol. It sits where the shared test
/// `config` looks for an origin, so the host enrolls it.
fn origin_with_users(
    path: &std::path::Path,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let work = path.join("origin-work");
    std::fs::create_dir_all(work.join(".config"))?;
    git(&work, &["init", "--quiet", "--initial-branch=main"])?;
    std::fs::write(
        work.join("compose.yaml"),
        "services:\n  api:\n    image: busybox\n",
    )?;
    let alice = format!("  alice:\n    keys: [\"{ALICE}\"]\n    access: {{production: deploy}}\n");
    std::fs::write(work.join(".config/mm.yaml"), project_config(&alice))?;
    git(&work, &["add", "."])?;
    git(&work, &["commit", "--quiet", "-m", "alice"])?;
    let first = git(&work, &["rev-parse", "HEAD"])?;
    let carol =
        format!("{alice}  carol:\n    keys: [\"{CAROL}\"]\n    access: {{production: view}}\n");
    std::fs::write(work.join(".config/mm.yaml"), project_config(&carol))?;
    git(&work, &["commit", "--quiet", "-am", "carol"])?;
    let second = git(&work, &["rev-parse", "HEAD"])?;
    git(
        path,
        &["clone", "--quiet", "--bare", "origin-work", "origin.git"],
    )?;
    std::fs::write(path.join("origin.commit"), &second)?;
    Ok((first, second))
}

struct Host {
    _temporary: tempfile::TempDir,
    daemon: RemoteDaemon,
    record: std::path::PathBuf,
    audit: std::path::PathBuf,
    authorized_keys: std::path::PathBuf,
    first: String,
    second: String,
}

fn host() -> Result<Host, Box<dyn std::error::Error>> {
    let temporary = crate::temp_dirs::tempdir()?;
    let (first, second) = origin_with_users(temporary.path())?;
    let root = temporary.path().join("project");
    std::fs::create_dir(&root)?;
    let uid = rustix::process::getuid().as_raw();
    let gid = rustix::process::getgid().as_raw();
    let daemon_config = config(temporary.path(), &root, uid, gid)?;
    let record = daemon_config
        .state_root
        .join("projects")
        .join(uuid::Uuid::from_u128(4).to_string())
        .join("access/production.json");
    let audit = daemon_config.state_root.join("audit.jsonl");
    let authorized_keys = daemon_config.authorized_keys_path.clone();
    let handler: Arc<dyn RemoteUseCases> = Arc::new(FakeUseCases {
        cancelled: Arc::new(AtomicBool::new(false)),
        stopped: Arc::default(),
    });
    let daemon = RemoteDaemon::new(daemon_config, handler)?;
    Ok(Host {
        _temporary: temporary,
        daemon,
        record,
        audit,
        authorized_keys,
        first,
        second,
    })
}

fn request(commit: &str, dry_run: bool) -> RootAccessSyncRequest {
    RootAccessSyncRequest {
        project_id: ProjectId(uuid::Uuid::from_u128(4)),
        environment: "production".to_owned(),
        commit: commit.to_owned(),
        dry_run,
    }
}

fn synced_events(host: &Host) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let contents = match std::fs::read_to_string(&host.audit) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut events = Vec::new();
    for line in contents.lines() {
        let event: serde_json::Value = serde_json::from_str(line)?;
        if event["event"] == "access-synced" {
            events.push(event);
        }
    }
    Ok(events)
}

fn stored(host: &Host) -> Result<EnvironmentAccessRecord, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&std::fs::read(&host.record)?)?)
}

#[tokio::test]
async fn first_root_sync_writes_access_and_a_rerun_changes_nothing()
-> Result<(), Box<dyn std::error::Error>> {
    let host = host()?;
    let synced = host
        .daemon
        .sync_access_as_root(&request(&host.first, false))
        .await?;
    assert!(synced.changed);
    assert_eq!(synced.previous_commit, None);
    assert_eq!(
        synced
            .users
            .iter()
            .map(|user| (user.user.as_str(), user.access.level))
            .collect::<Vec<_>>(),
        [("alice", AccessLevel::Deploy)]
    );
    let record = stored(&host)?;
    assert_eq!(
        (record.commit.as_str(), record.reconciled_by.as_str()),
        (host.first.as_str(), HOST_ROOT)
    );
    assert!(std::fs::read_to_string(&host.authorized_keys)?.contains(ALICE));
    let events = synced_events(&host)?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["principal"], HOST_ROOT);
    assert_eq!(events[0]["commit"], host.first.as_str());

    let bytes = std::fs::read(&host.record)?;
    let again = host
        .daemon
        .sync_access_as_root(&request(&host.first, false))
        .await?;
    assert!(!again.changed);
    assert!(again.changes.is_empty());
    assert_eq!(again.previous_commit.as_deref(), Some(host.first.as_str()));
    assert_eq!(
        std::fs::read(&host.record)?,
        bytes,
        "a re-run writes nothing"
    );
    assert_eq!(synced_events(&host)?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn root_sync_only_moves_access_forward() -> Result<(), Box<dyn std::error::Error>> {
    let host = host()?;
    host.daemon
        .sync_access_as_root(&request(&host.first, false))
        .await?;
    let synced = host
        .daemon
        .sync_access_as_root(&request(&host.second, false))
        .await?;
    assert!(synced.changed);
    assert_eq!(synced.previous_commit.as_deref(), Some(host.first.as_str()));
    assert_eq!(
        synced
            .changes
            .iter()
            .map(|change| change.user.as_str())
            .collect::<Vec<_>>(),
        ["carol"]
    );
    assert!(std::fs::read_to_string(&host.authorized_keys)?.contains(CAROL));

    let refused = host
        .daemon
        .sync_access_as_root(&request(&host.first, false))
        .await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|error| error.to_string().contains("only moves access forward")),
        "{refused:?}"
    );
    let dry = host
        .daemon
        .sync_access_as_root(&request(&host.first, true))
        .await;
    assert!(dry.is_err(), "a dry run applies the same rule: {dry:?}");
    assert_eq!(stored(&host)?.commit, host.second);
    assert!(std::fs::read_to_string(&host.authorized_keys)?.contains(CAROL));
    Ok(())
}

#[tokio::test]
async fn dry_run_reports_the_change_and_writes_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let host = host()?;
    let keys = std::fs::read_to_string(&host.authorized_keys)?;
    let planned = host
        .daemon
        .sync_access_as_root(&request(&host.second, true))
        .await?;
    assert!(planned.changed && planned.dry_run);
    assert_eq!(
        planned
            .users
            .iter()
            .map(|user| (user.user.as_str(), user.access.level))
            .collect::<Vec<_>>(),
        [("alice", AccessLevel::Deploy), ("carol", AccessLevel::View)]
    );
    assert_eq!(planned.changes.len(), 2);
    assert!(!host.record.exists());
    assert_eq!(std::fs::read_to_string(&host.authorized_keys)?, keys);
    assert!(synced_events(&host)?.is_empty());

    let mut elsewhere = request(&host.second, true);
    "staging".clone_into(&mut elsewhere.environment);
    let refused = host.daemon.sync_access_as_root(&elsewhere).await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|error| error.to_string().contains("does not run environment")),
        "{refused:?}"
    );
    Ok(())
}
