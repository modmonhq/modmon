//! Push-mode delivery: mm sends a commit's tracked files to hosts
//! that fetch nothing. A bundle holds what git tracks at the commit and
//! nothing else, a host keeps one only when it is proven to be that commit,
//! and an upload that stops half way leaves nothing behind.

mod daemon_host;
mod temp_dirs;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use daemon_host::{ALICE, Host, OPS, PROMPT, TestResult, project};
use mm::adapters::remote::{
    DeliveryScope, GitCommitBundles, ProjectRepositories, RootAccessSyncRequest,
};
use mm::app::ports::PreparedBundle;
use mm::domain::delivery::{BundleOffer, Delivered};
use mm::domain::remote::{ClientFrame, PROTOCOL_VERSION, RemoteMethod, RemoteResult, ServerFrame};

fn git(directory: &Path, arguments: &[&str]) -> TestResult<String> {
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

/// A repository with one commit: files in directories, an executable, a
/// symbolic link. Next to them, in the working tree only: an ignored
/// `.env`, a file nobody added, and a change nobody committed.
fn repository(path: &Path) -> TestResult<(PathBuf, String)> {
    use std::os::unix::fs::PermissionsExt;

    let work = path.join("work");
    std::fs::create_dir_all(work.join("deploy/nginx"))?;
    git(&work, &["init", "--quiet", "--initial-branch=main"])?;
    std::fs::write(work.join(".gitignore"), ".env\nbuild/\n")?;
    std::fs::write(
        work.join("compose.yaml"),
        "services:\n  web:\n    image: busybox\n",
    )?;
    std::fs::write(work.join("deploy/nginx/site.conf"), "server {}\n")?;
    std::fs::write(work.join("deploy/start.sh"), "#!/bin/sh\nexec true\n")?;
    std::fs::set_permissions(
        work.join("deploy/start.sh"),
        std::fs::Permissions::from_mode(0o755),
    )?;
    std::os::unix::fs::symlink("nginx/site.conf", work.join("deploy/current.conf"))?;
    git(&work, &["add", "."])?;
    git(&work, &["commit", "--quiet", "-m", "first"])?;
    let commit = git(&work, &["rev-parse", "HEAD"])?;
    std::fs::write(work.join(".env"), "DATABASE_PASSWORD=not-for-any-host\n")?;
    std::fs::write(work.join("notes.txt"), "never added\n")?;
    std::fs::create_dir(work.join("build"))?;
    std::fs::write(work.join("build/output.bin"), "ignored\n")?;
    std::fs::write(work.join("compose.yaml"), "services: {}  # not committed\n")?;
    Ok((work, commit))
}

fn bundle_bytes(prepared: &PreparedBundle) -> TestResult<Vec<u8>> {
    let path = prepared.path.as_ref().ok_or("the bundle has no file")?;
    Ok(std::fs::read(path)?)
}

/// The paths in a bundle, with the content of each regular file.
fn entries(bytes: &[u8]) -> TestResult<Vec<(String, String)>> {
    let mut archive = tar::Archive::new(bytes);
    let mut listed = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let mut content = String::new();
        entry.read_to_string(&mut content)?;
        listed.push((path, content));
    }
    Ok(listed)
}

fn repositories(path: &Path) -> ProjectRepositories {
    ProjectRepositories::new(path.join("state"), path.join("projects"))
}

fn scope() -> DeliveryScope<'static> {
    DeliveryScope {
        project: project(),
        environment: "production",
        principal: "tester",
        now_unix_ms: 1,
    }
}

/// Sends `bytes` as the bundle of `offer`, in pieces.
fn receive(
    store: &ProjectRepositories,
    offer: &BundleOffer,
    bytes: &[u8],
) -> Result<Delivered, mm::MmError> {
    let mut receiver = store.receive(&scope(), offer)?;
    for piece in bytes.chunks(700) {
        receiver.write(piece)?;
    }
    receiver.finish()
}

/// The offer for other bytes under the same commit: what a client that
/// changed the bundle and hashed it again would state.
fn offer_for(offer: &BundleOffer, bytes: &[u8]) -> BundleOffer {
    let mut offer = offer.clone();
    if let Some(bundle) = &mut offer.bundle {
        bundle.hash = blake3::hash(bytes).to_hex().to_string();
        bundle.bytes = bytes.len() as u64;
    }
    offer
}

/// The same archive with `change` applied to each file's content, and
/// `extra` files added.
fn rewritten(
    bytes: &[u8],
    change: impl Fn(&str, Vec<u8>) -> Vec<u8>,
    extra: &[(&str, &str)],
) -> TestResult<Vec<u8>> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut archive = tar::Archive::new(bytes);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let mut header = entry.header().clone();
        if header.entry_type().is_symlink() {
            let target = entry.link_name()?.ok_or("a link without a target")?;
            builder.append_link(&mut header, &path, target)?;
            continue;
        }
        let mut content = Vec::new();
        entry.read_to_end(&mut content)?;
        let content = change(&path, content);
        header.set_size(content.len() as u64);
        builder.append_data(&mut header, &path, content.as_slice())?;
    }
    for (path, content) in extra {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        builder.append_data(&mut header, path, content.as_bytes())?;
    }
    Ok(builder.into_inner()?)
}

/// Whether the host kept anything of a delivery: a checkout, a record, or
/// a partly received file.
fn kept_anything(path: &Path) -> bool {
    let state = path.join("state/projects").join(project().0.to_string());
    let files =
        |directory: PathBuf| std::fs::read_dir(directory).is_ok_and(|entries| entries.count() > 0);
    files(
        path.join("projects")
            .join(project().0.to_string())
            .join("production"),
    ) || files(state.join("bundles/production"))
        || files(state.join("incoming"))
}

fn refusal(result: Result<Delivered, mm::MmError>) -> String {
    result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default()
}

#[test]
fn a_bundle_holds_what_the_commit_tracks_and_nothing_from_the_working_tree() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let temporary = temp_dirs::tempdir()?;
    let (work, commit) = repository(temporary.path())?;
    let prepared = GitCommitBundles::new(work).build(&commit)?;
    let bytes = bundle_bytes(&prepared)?;
    let listed = entries(&bytes)?;
    let mut paths = listed
        .iter()
        .map(|(path, _)| path.as_str())
        .collect::<Vec<_>>();
    paths.sort_unstable();
    assert_eq!(
        paths,
        [
            ".gitignore",
            "compose.yaml",
            "deploy/current.conf",
            "deploy/nginx/site.conf",
            "deploy/start.sh"
        ],
        "tracked files only: no .env, nothing ignored, nothing that was never added"
    );
    let compose = listed.iter().find(|(path, _)| path == "compose.yaml");
    assert_eq!(
        compose.map(|(_, content)| content.as_str()),
        Some("services:\n  web:\n    image: busybox\n"),
        "the committed content, not the working tree's"
    );
    assert!(
        !String::from_utf8_lossy(&bytes).contains("not-for-any-host"),
        "the value in .env is nowhere in the bundle"
    );
    assert_eq!(prepared.files, 5);
    assert_eq!(prepared.offer.commit, commit);

    // The host keeps it as the commit's checkout, file for file.
    let store = repositories(temporary.path());
    let delivered = receive(&store, &prepared.offer, &bytes)?;
    assert!(delivered.received);
    assert_eq!(delivered.files, 5);
    let checkout = store.checkout_path(project(), "production", &commit);
    assert_eq!(
        std::fs::read_to_string(checkout.join("deploy/nginx/site.conf"))?,
        "server {}\n"
    );
    assert_eq!(
        std::fs::read_link(checkout.join("deploy/current.conf"))?,
        PathBuf::from("nginx/site.conf")
    );
    let mode = std::fs::metadata(checkout.join("deploy/start.sh"))?
        .permissions()
        .mode();
    assert_ne!(mode & 0o100, 0, "an executable stays executable");
    assert!(!checkout.join(".env").exists());
    assert!(!checkout.join("notes.txt").exists());
    assert!(!checkout.join("build").exists());
    assert_eq!(
        std::fs::read_to_string(checkout.join(".mm-commit"))?.trim(),
        commit
    );
    let held = store.delivered(project(), "production", &commit)?;
    assert_eq!(
        held.map(|record| record.bundle_hash),
        Some(delivered.bundle_hash)
    );
    Ok(())
}

#[test]
fn a_bundle_of_a_commit_is_the_same_bytes_every_time() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let (work, commit) = repository(temporary.path())?;
    let bundles = GitCommitBundles::new(work);
    let (first, second) = (bundles.build(&commit)?, bundles.build(&commit)?);
    assert_eq!(first.offer, second.offer);
    assert_eq!(bundle_bytes(&first)?, bundle_bytes(&second)?);
    let path = first.path.clone().ok_or("no bundle file")?;
    drop(first);
    assert!(!path.exists(), "the bundle file goes with its last holder");
    Ok(())
}

#[test]
fn a_commit_that_tracks_a_dotenv_file_is_refused_before_anything_is_built() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let (work, _) = repository(temporary.path())?;
    git(&work, &["checkout", "--quiet", "--", "compose.yaml"])?;
    std::fs::write(
        work.join("deploy/.env.production"),
        "TOKEN=committed-by-mistake\n",
    )?;
    git(&work, &["add", "--force", "deploy/.env.production"])?;
    git(&work, &["commit", "--quiet", "-m", "a .env file"])?;
    let commit = git(&work, &["rev-parse", "HEAD"])?;
    let refused = GitCommitBundles::new(work.clone())
        .build(&commit)
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        refused.contains("deploy/.env.production") && refused.contains("git rm --cached"),
        "{refused}"
    );
    // A template for one is an ordinary file.
    git(
        &work,
        &["rm", "--quiet", "--cached", "deploy/.env.production"],
    )?;
    std::fs::write(work.join(".env.example"), "TOKEN=\n")?;
    git(&work, &["add", "--force", ".env.example"])?;
    git(&work, &["commit", "--quiet", "-m", "an example instead"])?;
    let commit = git(&work, &["rev-parse", "HEAD"])?;
    assert_eq!(GitCommitBundles::new(work).build(&commit)?.files, 6);
    Ok(())
}

#[test]
fn a_commit_this_checkout_does_not_hold_is_offered_without_a_bundle() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let (work, _) = repository(temporary.path())?;
    let prepared = GitCommitBundles::new(work).build(&"b".repeat(40))?;
    assert!(prepared.offer.bundle.is_none() && prepared.path.is_none());
    Ok(())
}

#[test]
fn a_tampered_bundle_is_refused_and_nothing_of_it_is_kept() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let (work, commit) = repository(temporary.path())?;
    let prepared = GitCommitBundles::new(work).build(&commit)?;
    let bytes = bundle_bytes(&prepared)?;
    let store = repositories(temporary.path());

    // Bytes changed on the way: they no longer hash to what was stated.
    let mut flipped = bytes.clone();
    let at = flipped.len() / 3;
    flipped[at] ^= 0x01;
    let refused = refusal(receive(&store, &prepared.offer, &flipped));
    assert!(refused.contains("hashes to"), "{refused}");
    assert!(!kept_anything(temporary.path()));

    // A file changed, and the hash stated for the changed bundle: the
    // bytes are what was stated, but they are not the commit.
    let changed = rewritten(
        &bytes,
        |path, content| {
            if path == "compose.yaml" {
                b"services:\n  web:\n    image: attacker/miner\n".to_vec()
            } else {
                content
            }
        },
        &[],
    )?;
    let refused = refusal(receive(
        &store,
        &offer_for(&prepared.offer, &changed),
        &changed,
    ));
    assert!(
        refused.contains("not the files of that commit"),
        "{refused}"
    );
    assert!(!kept_anything(temporary.path()));

    // A file added.
    let added = rewritten(
        &bytes,
        |_, content| content,
        &[("deploy/extra.sh", "curl evil\n")],
    )?;
    let refused = refusal(receive(&store, &offer_for(&prepared.offer, &added), &added));
    assert!(
        refused.contains("not the files of that commit"),
        "{refused}"
    );

    // A file removed (the archive cut after its first entries).
    let fewer = rewritten(&bytes, |_, content| content, &[])?;
    let mut archive = tar::Archive::new(fewer.as_slice());
    let mut builder = tar::Builder::new(Vec::new());
    for entry in archive.entries()?.take(2) {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let mut header = entry.header().clone();
        let mut content = Vec::new();
        entry.read_to_end(&mut content)?;
        builder.append_data(&mut header, &path, content.as_slice())?;
    }
    let fewer = builder.into_inner()?;
    let refused = refusal(receive(&store, &offer_for(&prepared.offer, &fewer), &fewer));
    assert!(
        refused.contains("not the files of that commit"),
        "{refused}"
    );

    // A .env file slipped in.
    let dotenv = rewritten(&bytes, |_, content| content, &[(".env", "SECRET=1\n")])?;
    let refused = refusal(receive(
        &store,
        &offer_for(&prepared.offer, &dotenv),
        &dotenv,
    ));
    assert!(refused.contains("'.env'"), "{refused}");

    // Not an archive at all.
    let noise = vec![7_u8; 4096];
    let refused = refusal(receive(&store, &offer_for(&prepared.offer, &noise), &noise));
    assert!(refused.contains("was refused"), "{refused}");
    assert!(!kept_anything(temporary.path()));

    // The untouched bundle is still taken afterwards.
    assert!(receive(&store, &prepared.offer, &bytes)?.received);
    Ok(())
}

#[test]
fn a_bundle_cannot_take_another_commits_name() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let (work, first) = repository(temporary.path())?;
    git(&work, &["checkout", "--quiet", "--", "compose.yaml"])?;
    std::fs::write(
        work.join("deploy/nginx/site.conf"),
        "server { listen 81; }\n",
    )?;
    git(&work, &["commit", "--quiet", "-am", "second"])?;
    let second = git(&work, &["rev-parse", "HEAD"])?;
    let bundles = GitCommitBundles::new(work);
    let (old, new) = (bundles.build(&first)?, bundles.build(&second)?);
    let store = repositories(temporary.path());

    // The first commit's files under the second commit's name and object.
    let mut offer = offer_for(&new.offer, &bundle_bytes(&old)?);
    let refused = refusal(receive(&store, &offer, &bundle_bytes(&old)?));
    assert!(
        refused.contains("not the files of that commit"),
        "{refused}"
    );

    // The second commit's name over the first commit's object.
    offer.bundle = old.offer.bundle.clone();
    let refused = refusal(receive(&store, &offer, &bundle_bytes(&old)?));
    assert!(refused.contains("another commit's"), "{refused}");

    // Something that is no commit object.
    if let Some(bundle) = &mut offer.bundle {
        bundle.commit_object = base64::engine::general_purpose::STANDARD.encode("tree nonsense");
    }
    let refused = refusal(receive(&store, &offer, &bundle_bytes(&old)?));
    assert!(refused.contains("not a commit object"), "{refused}");
    assert!(!kept_anything(temporary.path()));
    Ok(())
}

#[test]
fn a_bundle_larger_than_the_host_takes_is_refused_before_a_byte_is_written() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let (work, commit) = repository(temporary.path())?;
    let prepared = GitCommitBundles::new(work).build(&commit)?;
    let bytes = bundle_bytes(&prepared)?;
    let small = repositories(temporary.path()).with_bundle_limit(1024);
    let refused = refusal(receive(&small, &prepared.offer, &bytes));
    assert!(
        refused.contains("at most 1 KiB") && refused.contains(&commit),
        "{refused}"
    );
    assert!(!kept_anything(temporary.path()));

    // A client that states a small size and sends more is stopped at the
    // size it stated.
    let store = repositories(temporary.path());
    let mut understated = prepared.offer.clone();
    if let Some(bundle) = &mut understated.bundle {
        bundle.bytes = 512;
    }
    let refused = refusal(receive(&store, &understated, &bytes));
    assert!(
        refused.contains("more than the 512 B it stated"),
        "{refused}"
    );
    assert!(!kept_anything(temporary.path()));
    Ok(())
}

#[test]
fn an_interrupted_upload_leaves_nothing_and_the_next_one_is_taken() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let (work, commit) = repository(temporary.path())?;
    let prepared = GitCommitBundles::new(work).build(&commit)?;
    let bytes = bundle_bytes(&prepared)?;
    let store = repositories(temporary.path());
    let incoming = temporary
        .path()
        .join("state/projects")
        .join(project().0.to_string())
        .join("incoming");

    let mut receiver = store.receive(&scope(), &prepared.offer)?;
    receiver.write(&bytes[..bytes.len() / 2])?;
    assert_eq!(
        std::fs::read_dir(&incoming)?.count(),
        1,
        "the upload is on disk, not in memory"
    );
    drop(receiver);
    assert!(
        !kept_anything(temporary.path()),
        "a cut upload leaves nothing"
    );

    // One that ends early is refused, not kept as a shorter commit.
    let mut receiver = store.receive(&scope(), &prepared.offer)?;
    receiver.write(&bytes[..bytes.len() / 2])?;
    let refused = refusal(receiver.finish());
    assert!(refused.contains("ended after"), "{refused}");
    assert!(!kept_anything(temporary.path()));

    assert!(receive(&store, &prepared.offer, &bytes)?.received);
    assert_eq!(std::fs::read_dir(&incoming)?.count(), 0);
    Ok(())
}

#[test]
fn a_bundle_entry_outside_the_project_is_refused() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let (work, commit) = repository(temporary.path())?;
    let prepared = GitCommitBundles::new(work).build(&commit)?;
    let store = repositories(temporary.path());
    // The path written into the header by hand: the archive library
    // refuses to build such an entry.
    let mut header = tar::Header::new_gnu();
    let name = b"../../escaped.txt";
    header.as_old_mut().name[..name.len()].copy_from_slice(name);
    header.set_size(4);
    header.set_mode(0o644);
    header.set_cksum();
    let mut bytes = header.as_bytes().to_vec();
    bytes.extend_from_slice(b"evil");
    bytes.resize(bytes.len().next_multiple_of(512) + 1024, 0);
    let refused = refusal(receive(&store, &offer_for(&prepared.offer, &bytes), &bytes));
    assert!(refused.contains("was refused"), "{refused}");
    assert!(!temporary.path().join("projects/escaped.txt").exists());
    assert!(!temporary.path().join("escaped.txt").exists());
    assert!(!kept_anything(temporary.path()));
    Ok(())
}

// The daemon: a host whose policy has the project pushed.

/// Rewrites the host policy so the project is pushed: no origin. With
/// `deploy_from`, the environment takes commits of those branches only.
fn push_policy(host: &Host, deploy_from: Option<&str>) -> TestResult {
    let limit = deploy_from.map_or_else(String::new, |branch| {
        format!("    initial-deploy-from:\n      production: [{branch}]\n")
    });
    std::fs::write(
        &host.config.host_policy_path,
        format!(
            "schema-version: 2\nbreak-glass:\n  - name: ops\n    key: {OPS}\nprojects:\n  {}:\n    name: example\n    delivery: push\n    environments: [production]\n{limit}",
            project().0
        ),
    )?;
    // The forge is gone: nothing may reach for it from here on.
    std::fs::remove_dir_all(host.temporary.path().join("origin.git"))?;
    Ok(())
}

/// The bundle of the host's commit, built from the fixture's repository.
fn host_bundle(host: &Host, branches: &[&str]) -> TestResult<(PreparedBundle, Vec<u8>)> {
    let mut prepared =
        GitCommitBundles::new(host.temporary.path().join("origin-work")).build(&host.commit)?;
    prepared.offer.branches = branches.iter().map(|branch| (*branch).to_owned()).collect();
    let bytes = bundle_bytes(&prepared)?;
    Ok((prepared, bytes))
}

fn data(request_id: uuid::Uuid, bytes: &[u8]) -> ClientFrame {
    ClientFrame::Data {
        version: PROTOCOL_VERSION,
        request_id,
        bytes: base64::engine::general_purpose::STANDARD.encode(bytes),
    }
}

/// Delivers as a client does: offer, wait to be asked, send, wait for the
/// host's check.
async fn deliver(
    host: &Host,
    key: &str,
    offer: &BundleOffer,
    bytes: &[u8],
) -> TestResult<Delivered> {
    let method = RemoteMethod::Deliver(Box::new(offer.clone()));
    let (mut client, request_id) = host.ask_with_id(key, method).await?;
    match client.result(PROMPT).await? {
        RemoteResult::Delivered(delivered) => return Ok(*delivered),
        RemoteResult::StreamStarted => {}
        other => return Err(format!("unexpected answer to an offer: {other:?}").into()),
    }
    for piece in bytes.chunks(900) {
        client.send(&data(request_id, piece)).await?;
    }
    match client.result(PROMPT).await? {
        RemoteResult::Delivered(delivered) => Ok(*delivered),
        other => Err(format!("unexpected answer to a bundle: {other:?}").into()),
    }
}

async fn plan(host: &Host, key: &str) -> TestResult<mm::domain::remote::RemoteDeploymentPlan> {
    let method = RemoteMethod::PlanDeployment {
        commit: host.commit.clone(),
        receipt: None,
        release: Default::default(),
    };
    match host.ask(key, method).await?.result(PROMPT).await? {
        RemoteResult::DeploymentPlan(plan) => Ok(*plan),
        other => Err(format!("unexpected plan result: {other:?}").into()),
    }
}

async fn plan_refusal(host: &Host, key: &str) -> TestResult<String> {
    let method = RemoteMethod::PlanDeployment {
        commit: host.commit.clone(),
        receipt: None,
        release: Default::default(),
    };
    let (code, message) = host.ask(key, method).await?.refusal(PROMPT).await?;
    Ok(format!("{code}: {message}"))
}

#[tokio::test]
async fn a_host_with_no_origin_plans_and_deploys_the_commit_it_was_sent() -> TestResult {
    let host = Host::with_alice().await?;
    push_policy(&host, None)?;
    let (prepared, bytes) = host_bundle(&host, &["main"])?;
    let offered = prepared.offer.bundle.clone().ok_or("no bundle")?;

    // Before the delivery the host says what to run, and fetches nothing.
    let refused = plan_refusal(&host, ALICE).await?;
    assert!(
        refused.contains("was not delivered to this host"),
        "{refused}"
    );

    let delivered = deliver(&host, ALICE, &prepared.offer, &bytes).await?;
    assert!(delivered.received);
    assert_eq!(delivered.bundle_hash, offered.hash);
    let planned = plan(&host, ALICE).await?;
    assert_eq!(
        planned.binding.bundle_hash.as_deref(),
        Some(offered.hash.as_str())
    );
    planned.validate()?;
    let apply = RemoteMethod::ApplyDeployment {
        remote_plan_hash: planned.binding.plan_hash.clone(),
    };
    assert!(matches!(
        host.ask(ALICE, apply).await?.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    assert_eq!(host.deployed_commit()?, host.commit);

    // The next deployment of the commit, and a rollback to it from a clone
    // that no longer has it, send nothing.
    let again = deliver(&host, ALICE, &prepared.offer, &[]).await?;
    assert!(!again.received);
    let without = BundleOffer {
        commit: host.commit.clone(),
        branches: Vec::new(),
        bundle: None,
    };
    let held = deliver(&host, ALICE, &without, &[]).await?;
    assert_eq!(
        (held.received, held.bundle_hash.as_str()),
        (false, offered.hash.as_str())
    );
    assert!(plan(&host, ALICE).await.is_ok());

    // The audit log names the bundle where it arrived and where it was planned.
    let audit = host.audit()?;
    let arrived = audit
        .iter()
        .find(|record| record["event"] == "bundle-delivered" && record["received"] == true)
        .ok_or("no audit record of the delivery")?;
    assert_eq!(arrived["bundle_hash"], offered.hash.as_str());
    assert_eq!(arrived["commit"], host.commit.as_str());
    assert_eq!(arrived["principal"], "alice");
    assert!(audit.iter().any(|record| {
        record["result"] == "planned" && record["bundle_hash"] == offered.hash.as_str()
    }));
    Ok(())
}

#[tokio::test]
async fn a_host_that_fetches_takes_no_files_from_a_client() -> TestResult {
    let host = Host::with_alice().await?;
    let (prepared, _) = host_bundle(&host, &["main"])?;
    let method = RemoteMethod::Deliver(Box::new(prepared.offer.clone()));
    let (code, message) = host.ask(ALICE, method).await?.refusal(PROMPT).await?;
    assert_eq!(code, "MM_SECURITY");
    assert!(
        message.contains("takes no files from a client"),
        "{message}"
    );
    // And it still plans by fetching.
    assert!(plan(&host, ALICE).await?.binding.bundle_hash.is_none());
    Ok(())
}

#[tokio::test]
async fn enrolling_a_project_for_push_turns_a_fetching_host_over() -> TestResult {
    let host = Host::with_alice().await?;
    // What `mmd access enroll --delivery push` writes.
    let enrollment = mm::domain::host_policy::ProjectEnrollment {
        project_id: project(),
        name: "example".to_owned(),
        origin: String::new(),
        origin_host_key: None,
        delivery: mm::domain::delivery::Delivery::Push,
        environments: std::collections::BTreeSet::from(["production".to_owned()]),
        break_glass: Vec::new(),
    };
    // What fetching left behind: the deploy key, the origin's host key,
    // and the clone.
    let store = mm::adapters::remote::HostAccessStore::new(&host.config);
    let state = host
        .config
        .state_root
        .join("projects")
        .join(project().0.to_string());
    std::fs::write(
        state.join("deploy-key"),
        "a stand-in for the private half\n",
    )?;
    std::fs::write(
        state.join("deploy-key.pub"),
        "ssh-ed25519 AAAA mmd-deploy\n",
    )?;
    std::fs::write(
        state.join("origin_known_hosts"),
        "git.example ssh-ed25519 AAAA\n",
    )?;
    std::fs::create_dir_all(state.join("repo.git/objects"))?;
    assert!(host.root().enroll(&enrollment).await?, "the policy changed");
    assert_eq!(
        store.remove_fetch_material(project())?,
        [
            "deploy-key",
            "deploy-key.pub",
            "origin_known_hosts",
            "repo.git"
        ],
        "setup's push enrollment removes what fetching left"
    );
    for name in [
        "deploy-key",
        "deploy-key.pub",
        "origin_known_hosts",
        "repo.git",
    ] {
        assert!(!state.join(name).exists(), "{name} is gone");
    }
    assert!(
        store.remove_fetch_material(project())?.is_empty(),
        "the second time removes nothing"
    );
    let policy = std::fs::read_to_string(&host.config.host_policy_path)?;
    assert!(
        policy.contains("delivery: push") && !policy.contains("origin"),
        "{policy}"
    );
    assert!(
        !host.root().enroll(&enrollment).await?,
        "enrolling again changes nothing"
    );
    let (prepared, bytes) = host_bundle(&host, &["main"])?;
    assert!(
        deliver(&host, ALICE, &prepared.offer, &bytes)
            .await?
            .received
    );
    assert!(plan(&host, ALICE).await?.binding.bundle_hash.is_some());
    Ok(())
}

#[tokio::test]
async fn a_tampered_upload_is_refused_by_the_daemon_and_the_next_delivery_is_taken() -> TestResult {
    let host = Host::with_alice().await?;
    push_policy(&host, None)?;
    let (prepared, bytes) = host_bundle(&host, &["main"])?;
    let mut flipped = bytes.clone();
    let at = flipped.len() / 2;
    flipped[at] ^= 0x40;
    let refused = deliver(&host, ALICE, &prepared.offer, &flipped)
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(refused.contains("hashes to"), "{refused}");
    let refused = plan_refusal(&host, ALICE).await?;
    assert!(refused.contains("was not delivered"), "{refused}");
    assert!(
        deliver(&host, ALICE, &prepared.offer, &bytes)
            .await?
            .received
    );
    Ok(())
}

#[tokio::test]
async fn an_upload_the_client_abandons_frees_the_host_and_leaves_no_file() -> TestResult {
    let host = Host::with_alice().await?;
    push_policy(&host, None)?;
    let (prepared, bytes) = host_bundle(&host, &["main"])?;
    let method = RemoteMethod::Deliver(Box::new(prepared.offer.clone()));
    let (mut client, request_id) = host.ask_with_id(ALICE, method).await?;
    assert!(matches!(
        client.result(PROMPT).await?,
        RemoteResult::StreamStarted
    ));
    client
        .send(&data(request_id, &bytes[..bytes.len() / 2]))
        .await?;
    client.hang_up();

    // The turn is free again at once: the next delivery does not wait for
    // the first one's deadline.
    let delivered = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        deliver(&host, ALICE, &prepared.offer, &bytes),
    )
    .await??;
    assert!(delivered.received);
    let incoming = host
        .config
        .state_root
        .join("projects")
        .join(project().0.to_string())
        .join("incoming");
    assert_eq!(std::fs::read_dir(incoming)?.count(), 0);
    let audit = host.audit()?;
    assert!(
        audit
            .iter()
            .any(|record| { record["method"] == "deliver" && record["result"] == "failed" })
    );
    Ok(())
}

async fn deployment_status(host: &Host) -> TestResult<mm::domain::rollout::NodeDeploymentStatus> {
    match host
        .ask(OPS, RemoteMethod::DeploymentStatus)
        .await?
        .result(PROMPT)
        .await?
    {
        RemoteResult::DeploymentStatus(status) => Ok(*status),
        other => Err(format!("unexpected status: {other:?}").into()),
    }
}

/// An upload in progress is an operation in flight like a deployment step:
/// `mm deploy status` lists it as at work while it runs, and `mm
/// deploy cancel` stops it, keeps nothing of it, and frees the host's turn.
#[tokio::test]
async fn an_upload_in_progress_is_listed_and_cancel_stops_it() -> TestResult {
    let host = Host::with_alice().await?;
    push_policy(&host, None)?;
    let (prepared, bytes) = host_bundle(&host, &["main"])?;
    let method = RemoteMethod::Deliver(Box::new(prepared.offer.clone()));
    let (mut client, request_id) = host.ask_with_id(ALICE, method).await?;
    assert!(matches!(
        client.result(PROMPT).await?,
        RemoteResult::StreamStarted
    ));
    client
        .send(&data(request_id, &bytes[..bytes.len() / 2]))
        .await?;

    let first = deployment_status(&host).await?;
    let [listed] = first.operations.as_slice() else {
        return Err(format!("the upload is not listed: {:?}", first.operations).into());
    };
    let bundle = prepared.offer.bundle.as_ref().ok_or("no bundle offered")?;
    assert_eq!(listed.id, bundle.hash);
    assert!(
        listed.what.starts_with("delivery of commit "),
        "{}",
        listed.what
    );
    tokio::time::sleep(std::time::Duration::from_millis(4_500)).await;
    let later = deployment_status(&host).await?;
    assert!(
        later
            .operations
            .first()
            .is_some_and(|later| later.alive_at_unix_ms >= listed.alive_at_unix_ms + 2_000),
        "the upload is not seen at work: {listed:?} then {:?}",
        later.operations
    );

    let cancelled = match host
        .ask(OPS, RemoteMethod::Cancel { operation: None })
        .await?
        .result(PROMPT)
        .await?
    {
        RemoteResult::Cancelled(outcome) => outcome,
        other => return Err(format!("unexpected cancel answer: {other:?}").into()),
    };
    assert_eq!(cancelled.stopped.len(), 1);
    let (code, _) = client.refusal(PROMPT).await?;
    assert_eq!(code, "MM_CANCELLED");
    let incoming = host
        .config
        .state_root
        .join("projects")
        .join(project().0.to_string())
        .join("incoming");
    assert_eq!(
        std::fs::read_dir(incoming)?.count(),
        0,
        "nothing of the upload is kept"
    );
    let delivered = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        deliver(&host, ALICE, &prepared.offer, &bytes),
    )
    .await??;
    assert!(delivered.received, "the next delivery has the host's turn");
    Ok(())
}

#[tokio::test]
async fn an_upload_that_says_it_is_cancelled_is_dropped() -> TestResult {
    let host = Host::with_alice().await?;
    push_policy(&host, None)?;
    let (prepared, bytes) = host_bundle(&host, &["main"])?;
    let method = RemoteMethod::Deliver(Box::new(prepared.offer.clone()));
    let (mut client, request_id) = host.ask_with_id(ALICE, method).await?;
    assert!(matches!(
        client.result(PROMPT).await?,
        RemoteResult::StreamStarted
    ));
    client.send(&data(request_id, &bytes[..100])).await?;
    client
        .send(&ClientFrame::Cancel {
            version: PROTOCOL_VERSION,
            request_id,
        })
        .await?;
    let (_, message) = client.refusal(PROMPT).await?;
    assert!(message.contains("nothing was kept"), "{message}");
    assert!(
        plan_refusal(&host, ALICE)
            .await?
            .contains("was not delivered")
    );
    Ok(())
}

#[tokio::test]
async fn deploy_from_is_held_against_the_branch_the_client_states() -> TestResult {
    let host = Host::with_alice().await?;
    push_policy(&host, Some("main"))?;
    // Refused at the offer, before a byte of the bundle is taken.
    let (prepared, bytes) = host_bundle(&host, &[])?;
    let refused = offer_refusal(&host, ALICE, &prepared.offer).await?;
    assert!(refused.contains("stated no branch"), "{refused}");
    // Nothing of it was taken: no partial upload and no bundle record.
    let state = host
        .config
        .state_root
        .join("projects")
        .join(project().0.to_string());
    let empty = |directory: PathBuf| {
        std::fs::read_dir(directory).map_or(true, |mut entries| entries.next().is_none())
    };
    assert!(empty(state.join("incoming")), "a partial upload was kept");
    assert!(empty(state.join("bundles/production")), "a bundle was kept");

    // The same bundle, stated to be on a branch the host does not take.
    let (feature, _) = host_bundle(&host, &["feature/login"])?;
    let refused = offer_refusal(&host, ALICE, &feature.offer).await?;
    assert!(
        refused.contains("not on an allowed branch (main)") && refused.contains("feature/login"),
        "{refused}"
    );
    let refused = plan_refusal(&host, ALICE).await?;
    assert!(!refused.is_empty(), "the host holds nothing to plan from");

    let (main, _) = host_bundle(&host, &["main", "feature/login"])?;
    assert!(deliver(&host, ALICE, &main.offer, &bytes).await?.received);
    assert!(plan(&host, ALICE).await.is_ok());
    Ok(())
}

/// The refusal of an offer, which the host answers before asking for the
/// bundle.
async fn offer_refusal(host: &Host, key: &str, offer: &BundleOffer) -> TestResult<String> {
    let method = RemoteMethod::Deliver(Box::new(offer.clone()));
    let (code, message) = host.ask(key, method).await?.refusal(PROMPT).await?;
    Ok(format!("{code}: {message}"))
}

#[tokio::test]
async fn a_plan_is_not_applied_once_the_hosts_files_are_another_bundle() -> TestResult {
    let host = Host::with_alice().await?;
    push_policy(&host, None)?;
    let (prepared, bytes) = host_bundle(&host, &["main"])?;
    deliver(&host, ALICE, &prepared.offer, &bytes).await?;
    let planned = plan(&host, ALICE).await?;
    // The record of what the checkout was made from changes under the plan.
    let record = host
        .config
        .state_root
        .join("projects")
        .join(project().0.to_string())
        .join("bundles/production")
        .join(format!("{}.json", host.commit));
    let text = std::fs::read_to_string(&record)?;
    let hash = planned
        .binding
        .bundle_hash
        .clone()
        .ok_or("no bundle hash")?;
    std::fs::write(&record, text.replace(&hash, &"0".repeat(64)))?;
    let apply = RemoteMethod::ApplyDeployment {
        remote_plan_hash: planned.binding.plan_hash,
    };
    let (code, message) = host.ask(ALICE, apply).await?.refusal(PROMPT).await?;
    assert_eq!(code, "MM_SECURITY");
    assert!(
        message.contains("not the bundle the plan was made from"),
        "{message}"
    );
    assert_ne!(host.deployed_commit()?, host.commit);
    Ok(())
}

#[tokio::test]
async fn setup_sends_a_new_host_the_commit_with_its_first_access_sync() -> TestResult {
    // A host that never had an origin: nobody but the break-glass key has
    // access until root syncs the users file from a bundle.
    let host = Host::new()?;
    push_policy(&host, None)?;
    let (prepared, bytes) = host_bundle(&host, &["main"])?;
    let mut text = serde_json::to_string(&prepared.offer)?;
    text.push('\n');
    for piece in bytes.chunks(57 * 1024) {
        text.push_str(&base64::engine::general_purpose::STANDARD.encode(piece));
        text.push('\n');
    }
    let daemon = Arc::clone(&host.daemon);
    let commit = host.commit.clone();
    let synced = host
        .on_daemon(async move {
            let delivered =
                daemon.deliver_as_root(project(), "production", &mut text.as_bytes())?;
            let request = RootAccessSyncRequest {
                project_id: project(),
                environment: "production".to_owned(),
                commit,
                dry_run: false,
            };
            daemon
                .sync_access_as_root(&request)
                .await
                .map(|summary| (delivered, summary))
        })
        .await??;
    assert!(synced.0.received);
    assert!(synced.1.changed);
    assert!(synced.1.users.iter().any(|user| user.user == "alice"));
    // Alice plans the commit root delivered, with nothing sent again.
    let planned = plan(&host, ALICE).await?;
    assert_eq!(planned.binding.bundle_hash, Some(synced.0.bundle_hash));
    // A root sync without the bundle says what is missing.
    let other = Host::new()?;
    push_policy(&other, None)?;
    let daemon = Arc::clone(&other.daemon);
    let request = RootAccessSyncRequest {
        project_id: project(),
        environment: "production".to_owned(),
        commit: other.commit.clone(),
        dry_run: false,
    };
    let refused = other
        .on_daemon(async move { daemon.sync_access_as_root(&request).await })
        .await?
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        refused.contains("was not delivered to this host"),
        "{refused}"
    );
    Ok(())
}

#[tokio::test]
async fn the_server_frames_of_a_delivery_are_one_start_and_one_answer() -> TestResult {
    let host = Host::with_alice().await?;
    push_policy(&host, None)?;
    let (prepared, bytes) = host_bundle(&host, &["main"])?;
    let method = RemoteMethod::Deliver(Box::new(prepared.offer.clone()));
    let (mut client, request_id) = host.ask_with_id(ALICE, method).await?;
    assert!(matches!(
        client.next(PROMPT).await?,
        ServerFrame::Result { result, .. } if *result == RemoteResult::StreamStarted
    ));
    // The host says nothing while the bundle arrives.
    client
        .send(&data(request_id, &bytes[..bytes.len() - 1]))
        .await?;
    assert!(
        client
            .is_silent(std::time::Duration::from_millis(300))
            .await
    );
    client
        .send(&data(request_id, &bytes[bytes.len() - 1..]))
        .await?;
    assert!(matches!(
        client.next(PROMPT).await?,
        ServerFrame::Result { result, .. } if matches!(*result, RemoteResult::Delivered(_))
    ));
    Ok(())
}
