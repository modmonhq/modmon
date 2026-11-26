//! `mm update` against a local GitHub-style release: an archive of fake
//! `mm`, `mmd`, and `mm-ingress-helper` scripts that print their version.

mod temp_dirs;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mm::adapters::process::TokioProcessRunner;
use mm::adapters::release::ReqwestReleaseClient;
use mm::adapters::self_update::LocalInstallation;
use mm::app::SelfUpdateService;
use mm::app::ports::{InstallationFiles, NoProgress};
use mm::domain::error::MmError;
use mm::domain::release::{ReleaseTag, installed_target};
use mm::domain::self_update::{Installation, ReplacementKind, SelfUpdatePlan, SelfUpdateRequest};
use sha2::Digest;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const MANIFEST: &str = include_str!("fixtures/release-manifest.json");

type Files = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

/// A release origin on 127.0.0.1 that serves `files` by path, with
/// GitHub's redirect from the latest release to its tag.
struct Origin {
    url: String,
    files: Files,
}

impl Origin {
    fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let url = format!("http://{}", listener.local_addr()?);
        let files: Files = Arc::default();
        let served = Arc::clone(&files);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                if respond(&stream, &served).is_err() {
                    continue;
                }
            }
        });
        Ok(Self { url, files })
    }

    fn put(&self, path: &str, bytes: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        self.files
            .lock()
            .map_err(|_| "the fixture lock is poisoned")?
            .insert(path.to_owned(), bytes);
        Ok(())
    }

    fn client(&self) -> Result<Arc<ReqwestReleaseClient>, String> {
        ReqwestReleaseClient::new(&self.url).map(Arc::new)
    }
}

fn respond(stream: &std::net::TcpStream, files: &Files) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" {
            break;
        }
    }
    let path = request
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let body = files
        .lock()
        .map_err(|_| std::io::Error::other("poisoned"))?
        .get(&path)
        .cloned();
    let mut writer = stream;
    if let Some(rest) = path.strip_prefix("/releases/latest/download/") {
        let latest = files
            .lock()
            .map_err(|_| std::io::Error::other("poisoned"))?
            .get("latest")
            .cloned()
            .unwrap_or_default();
        let tag = String::from_utf8_lossy(&latest);
        return write!(
            writer,
            "HTTP/1.1 302 Found\r\nLocation: /releases/download/{tag}/{rest}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
    }
    match body {
        Some(body) => {
            write!(
                writer,
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )?;
            writer.write_all(&body)
        }
        None => write!(
            writer,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
    }
}

fn target() -> Result<&'static str, &'static str> {
    installed_target().ok_or("this platform has no release")
}

fn script(name: &str, version: &str) -> Vec<u8> {
    format!("#!/bin/sh\nprintf '{name} {version}\\n'\n").into_bytes()
}

enum Entry {
    File(String, Vec<u8>),
    Link(String, String),
    RawPath(String, Vec<u8>),
}

fn archive(entries: &[Entry]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::fast(),
    ));
    for entry in entries {
        let mut header = tar::Header::new_gnu();
        match entry {
            Entry::File(path, bytes) => {
                header.set_size(bytes.len() as u64);
                header.set_mode(0o755);
                header.set_entry_type(tar::EntryType::Regular);
                builder.append_data(&mut header, path, bytes.as_slice())?;
            }
            Entry::Link(path, target) => {
                header.set_size(0);
                header.set_entry_type(tar::EntryType::Symlink);
                builder.append_link(&mut header, path, target)?;
            }
            Entry::RawPath(path, bytes) => {
                header.set_size(bytes.len() as u64);
                header.set_mode(0o755);
                header.set_entry_type(tar::EntryType::Regular);
                let name = &mut header.as_old_mut().name;
                name[..path.len()].copy_from_slice(path.as_bytes());
                header.set_cksum();
                builder.append(&header, bytes.as_slice())?;
            }
        }
    }
    Ok(builder.into_inner()?.finish()?)
}

fn release_archive(tag: &str, version: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let top = format!("mm-{tag}-{}", target()?);
    archive(&[
        Entry::File(format!("{top}/mm"), script("mm", version)),
        Entry::File(format!("{top}/mmd"), script("mmd", version)),
        Entry::File(
            format!("{top}/mm-ingress-helper"),
            script("mm-ingress-helper", version),
        ),
        Entry::File(format!("{top}/LICENSE"), b"fixture\n".to_vec()),
        Entry::File(
            format!("{top}/share/a-file-a-later-release-adds"),
            b"fixture\n".to_vec(),
        ),
    ])
}

fn sha256(bytes: &[u8]) -> String {
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Publishes `archive` as the stable release `tag` with its manifest.
fn publish(origin: &Origin, tag: &str, archive: &[u8]) -> TestResult {
    let version = tag.trim_start_matches('v');
    let target = target()?;
    let file = format!("mm-{tag}-{target}.tar.gz");
    let mut manifest: serde_json::Value = serde_json::from_str(MANIFEST)?;
    let text = manifest
        .to_string()
        .replace("v0.1.0", tag)
        .replace("\"0.1.0\"", &format!("\"{version}\""));
    manifest = serde_json::from_str(&text)?;
    manifest["artifacts"][target]["sha256"] = serde_json::json!(sha256(archive));
    manifest["artifacts"][target]["size"] = serde_json::json!(archive.len());
    origin.put(
        &format!("/releases/download/{tag}/manifest.json"),
        manifest.to_string().into_bytes(),
    )?;
    origin.put(
        &format!("/releases/download/{tag}/{file}"),
        archive.to_vec(),
    )?;
    origin.put(
        &format!("/releases/download/{tag}/SHA256SUMS"),
        format!("{}  {file}\n", sha256(archive)).into_bytes(),
    )?;
    origin.put("latest", tag.as_bytes().to_vec())
}

/// An installation of mm 0.1.1: `mm`, and `names` beside it.
fn installation(
    names: &[&str],
) -> Result<(tempfile::TempDir, PathBuf), Box<dyn std::error::Error>> {
    let directory = temp_dirs::tempdir()?;
    let bin = directory.path().join("bin");
    std::fs::create_dir(&bin)?;
    for name in std::iter::once("mm").chain(names.iter().copied()) {
        write_script(&bin.join(name), &script(name, "0.1.1"))?;
    }
    Ok((directory, bin))
}

fn write_script(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, bytes)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

fn request(
    bin: &Path,
    current: &str,
    version: Option<&str>,
    force: bool,
) -> Result<SelfUpdateRequest, String> {
    Ok(SelfUpdateRequest {
        current: current.to_owned(),
        executable: bin.join("mm"),
        target: installed_target().map(str::to_owned),
        version: version.map(ReleaseTag::parse).transpose()?,
        force,
    })
}

fn service(
    origin: &Origin,
    files: Arc<dyn InstallationFiles>,
) -> Result<SelfUpdateService, String> {
    Ok(SelfUpdateService::new(
        origin.client()?,
        files,
        Arc::new(TokioProcessRunner),
    ))
}

fn version_of(binary: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .output()?;
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

/// What is in `bin` besides the binaries: nothing should be left behind.
fn leftovers(bin: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(bin)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if !["mm", "mmd", "mm-ingress-helper", "install-receipt.txt"].contains(&name.as_str()) {
            names.push(name);
        }
    }
    Ok(names)
}

async fn update(
    service: &SelfUpdateService,
    request: &SelfUpdateRequest,
) -> Result<SelfUpdatePlan, Box<dyn std::error::Error>> {
    let plan = service.plan(request).await?;
    if let SelfUpdatePlan::Replace(replacement) = &plan {
        service.apply(replacement, &NoProgress).await?;
    }
    Ok(plan)
}

#[tokio::test]
async fn update_replaces_mm_and_the_binaries_beside_it() -> TestResult {
    let origin = Origin::start()?;
    publish(&origin, "v9.9.9", &release_archive("v9.9.9", "9.9.9")?)?;
    let (_directory, bin) = installation(&["mmd"])?;
    let service = service(&origin, Arc::new(LocalInstallation))?;
    let plan = service.plan(&request(&bin, "0.1.1", None, false)?).await?;
    let SelfUpdatePlan::Replace(replacement) = plan else {
        return Err(format!("expected a replacement, got {plan:?}").into());
    };
    assert_eq!(replacement.kind, ReplacementKind::Upgrade);
    assert_eq!(replacement.binaries, ["mm", "mmd"]);
    assert_eq!(replacement.download.checked_against, "manifest.json");
    let result = service.apply(&replacement, &NoProgress).await?;
    assert_eq!(result.version, "9.9.9");
    assert!(result.hosts.contains("mm target setup plan --upgrade"));
    assert_eq!(version_of(&bin.join("mm"))?, "mm 9.9.9");
    assert_eq!(version_of(&bin.join("mmd"))?, "mmd 9.9.9");
    assert!(
        !bin.join("mm-ingress-helper").exists(),
        "a helper that was not there was added"
    );
    assert_eq!(leftovers(&bin)?, Vec::<String>::new());

    let again = service.plan(&request(&bin, "9.9.9", None, false)?).await?;
    assert!(
        matches!(again, SelfUpdatePlan::UpToDate { .. }),
        "{again:?}"
    );
    Ok(())
}

#[tokio::test]
async fn up_to_date_newer_and_older_releases_change_nothing_unless_forced() -> TestResult {
    let origin = Origin::start()?;
    publish(&origin, "v9.9.9", &release_archive("v9.9.9", "9.9.9")?)?;
    let (_directory, bin) = installation(&[])?;
    let service = service(&origin, Arc::new(LocalInstallation))?;
    let newer = service.plan(&request(&bin, "10.0.0", None, false)?).await?;
    assert!(
        matches!(newer, SelfUpdatePlan::InstalledNewer { .. }),
        "{newer:?}"
    );
    let forced = service.plan(&request(&bin, "10.0.0", None, true)?).await?;
    assert!(
        matches!(forced, SelfUpdatePlan::InstalledNewer { .. }),
        "{forced:?}"
    );
    let refused = service
        .plan(&request(&bin, "10.0.0", Some("v9.9.9"), false)?)
        .await
        .err()
        .ok_or("a downgrade was planned without --force")?;
    assert_eq!(refused.code(), "MM_UPDATE");
    assert!(refused.to_string().contains("--force"), "{refused}");
    let SelfUpdatePlan::Replace(downgrade) = service
        .plan(&request(&bin, "10.0.0", Some("v9.9.9"), true)?)
        .await?
    else {
        return Err("a forced downgrade was not planned".into());
    };
    assert_eq!(downgrade.kind, ReplacementKind::Downgrade);
    let SelfUpdatePlan::Replace(reinstall) =
        service.plan(&request(&bin, "9.9.9", None, true)?).await?
    else {
        return Err("a forced reinstall was not planned".into());
    };
    assert_eq!(reinstall.kind, ReplacementKind::Reinstall);
    assert_eq!(version_of(&bin.join("mm"))?, "mm 0.1.1");
    Ok(())
}

#[tokio::test]
async fn a_damaged_or_unsafe_archive_leaves_the_old_binaries_untouched() -> TestResult {
    let target = target()?;
    let top = format!("mm-v9.9.9-{target}");
    let good = release_archive("v9.9.9", "9.9.9")?;
    let cases: Vec<(&str, Vec<u8>, Vec<u8>)> = vec![
        (
            "checksum mismatch",
            good.clone(),
            [good.as_slice(), b"tampered"].concat(),
        ),
        (
            "checksum mismatch",
            good.clone(),
            good[..good.len() / 2].to_vec(),
        ),
        (
            "a link",
            archive(&[Entry::Link(format!("{top}/mm"), "../../outside".to_owned())])?,
            Vec::new(),
        ),
        (
            "unsafe path",
            archive(&[Entry::RawPath(
                format!("{top}/../mm"),
                script("mm", "9.9.9"),
            )])?,
            Vec::new(),
        ),
        (
            "more than one top directory",
            archive(&[
                Entry::File(format!("{top}/mm"), script("mm", "9.9.9")),
                Entry::File("elsewhere/mm".to_owned(), script("mm", "9.9.9")),
            ])?,
            Vec::new(),
        ),
        (
            "printed 'mm 1.0.0'",
            archive(&[Entry::File(format!("{top}/mm"), script("mm", "1.0.0"))])?,
            Vec::new(),
        ),
    ];
    for (expected, published, served) in cases {
        let origin = Origin::start()?;
        publish(&origin, "v9.9.9", &published)?;
        if !served.is_empty() {
            origin.put(
                &format!("/releases/download/v9.9.9/mm-v9.9.9-{target}.tar.gz"),
                served,
            )?;
        }
        let (_directory, bin) = installation(&["mmd"])?;
        let service = service(&origin, Arc::new(LocalInstallation))?;
        let error = update(&service, &request(&bin, "0.1.1", None, false)?)
            .await
            .err()
            .ok_or(format!("{expected}: the update went through"))?;
        assert!(error.to_string().contains(expected), "{expected}: {error}");
        assert!(error.to_string().contains("nothing was changed"), "{error}");
        assert_eq!(version_of(&bin.join("mm"))?, "mm 0.1.1", "{expected}");
        assert_eq!(version_of(&bin.join("mmd"))?, "mmd 0.1.1", "{expected}");
        assert_eq!(leftovers(&bin)?, Vec::<String>::new(), "{expected}");
    }
    Ok(())
}

#[tokio::test]
async fn an_unwritable_directory_or_a_checkout_build_is_refused_with_the_next_command() -> TestResult
{
    use std::os::unix::fs::PermissionsExt;
    let origin = Origin::start()?;
    publish(&origin, "v9.9.9", &release_archive("v9.9.9", "9.9.9")?)?;
    let service = service(&origin, Arc::new(LocalInstallation))?;

    let (_directory, bin) = installation(&["mmd"])?;
    std::fs::write(
        bin.join("install-receipt.txt"),
        "target=x\nsource=/srv/checkouts/mm\ncommit=abc\n",
    )?;
    let refused = service
        .plan(&request(&bin, "0.1.1", None, false)?)
        .await
        .err()
        .ok_or("a checkout build was replaced without --force")?;
    assert!(
        refused.to_string().contains("git pull && ./install.sh"),
        "{refused}"
    );
    assert!(
        refused.to_string().contains("/srv/checkouts/mm"),
        "{refused}"
    );
    update(&service, &request(&bin, "0.1.1", None, true)?).await?;
    assert_eq!(version_of(&bin.join("mm"))?, "mm 9.9.9");
    assert!(
        !bin.join("install-receipt.txt").exists(),
        "the checkout's receipt stayed"
    );

    let (_directory, bin) = installation(&["mmd"])?;
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o555))?;
    let privileged = std::fs::write(bin.join("probe"), "").is_ok();
    if !privileged {
        let refused = service
            .plan(&request(&bin, "0.1.1", None, false)?)
            .await
            .err()
            .ok_or("an unwritable directory was planned")?;
        assert!(refused.to_string().contains("is not writable"), "{refused}");
        assert!(refused.to_string().contains("--install-dir"), "{refused}");
        assert!(refused.to_string().contains("sudo mm update"), "{refused}");
    }
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

/// Files that fail to put `mmd` in place, as a full disk would.
struct FailingSecondSwap(LocalInstallation);

impl InstallationFiles for FailingSecondSwap {
    fn inspect(&self, executable: &Path) -> Result<Installation, MmError> {
        self.0.inspect(executable)
    }
    fn staging(&self, directory: &Path) -> Result<PathBuf, MmError> {
        self.0.staging(directory)
    }
    fn unpack(&self, archive: &Path, into: &Path) -> Result<Vec<String>, MmError> {
        self.0.unpack(archive, into)
    }
    fn replace(&self, staged: &Path, destination: &Path) -> Result<PathBuf, MmError> {
        if destination.ends_with("mmd") {
            return Err(MmError::Io {
                context: format!("moving the new {} into place", destination.display()),
                source: std::io::Error::other("no space left on device"),
            });
        }
        self.0.replace(staged, destination)
    }
    fn restore(&self, kept: &Path, destination: &Path) -> Result<(), MmError> {
        self.0.restore(kept, destination)
    }
    fn remove(&self, path: &Path) -> Result<(), MmError> {
        self.0.remove(path)
    }
    fn same_file_contents(&self, left: &Path, right: &Path) -> Result<bool, MmError> {
        self.0.same_file_contents(left, right)
    }
}

#[tokio::test]
async fn a_failed_second_swap_puts_every_binary_back() -> TestResult {
    let origin = Origin::start()?;
    publish(&origin, "v9.9.9", &release_archive("v9.9.9", "9.9.9")?)?;
    let (_directory, bin) = installation(&["mm-ingress-helper", "mmd"])?;
    let service = service(&origin, Arc::new(FailingSecondSwap(LocalInstallation)))?;
    let error = update(&service, &request(&bin, "0.1.1", None, false)?)
        .await
        .err()
        .ok_or("the update went through")?;
    assert!(
        error.to_string().contains("replacing mmd failed"),
        "{error}"
    );
    assert!(error.to_string().contains("put back"), "{error}");
    for name in ["mm", "mm-ingress-helper", "mmd"] {
        assert_eq!(version_of(&bin.join(name))?, format!("{name} 0.1.1"));
    }
    assert_eq!(leftovers(&bin)?, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn a_named_release_candidate_is_checked_against_sha256sums() -> TestResult {
    let origin = Origin::start()?;
    publish(&origin, "v9.9.9", &release_archive("v9.9.9", "9.9.9")?)?;
    let candidate = release_archive("v10.0.0-rc.1", "10.0.0")?;
    let file = format!("mm-v10.0.0-rc.1-{}.tar.gz", target()?);
    origin.put(
        &format!("/releases/download/v10.0.0-rc.1/{file}"),
        candidate.clone(),
    )?;
    origin.put(
        "/releases/download/v10.0.0-rc.1/SHA256SUMS",
        format!(
            "{}  mm-v10.0.0-rc.1-source.tar.gz\n{}  {file}\n",
            "0".repeat(64),
            sha256(&candidate)
        )
        .into_bytes(),
    )?;
    let (_directory, bin) = installation(&["mmd", "mm-ingress-helper"])?;
    let service = service(&origin, Arc::new(LocalInstallation))?;
    let plan = update(
        &service,
        &request(&bin, "0.1.1", Some("v10.0.0-rc.1"), false)?,
    )
    .await?;
    let SelfUpdatePlan::Replace(replacement) = plan else {
        return Err("the candidate was not planned".into());
    };
    assert_eq!(replacement.download.checked_against, "SHA256SUMS");
    assert_eq!(replacement.binaries, ["mm", "mm-ingress-helper", "mmd"]);
    assert_eq!(version_of(&bin.join("mm"))?, "mm 10.0.0");
    assert_eq!(version_of(&bin.join("mmd"))?, "mmd 10.0.0");
    Ok(())
}

#[test]
fn the_cli_checks_and_refuses_without_a_terminal_or_a_release_tag() -> TestResult {
    let origin = Origin::start()?;
    publish(&origin, "v9.9.9", &release_archive("v9.9.9", "9.9.9")?)?;
    let home = temp_dirs::tempdir()?;
    let mm = || {
        let mut command = temp_dirs::mm();
        command
            .env("HOME", home.path())
            .env("MM_RELEASE_TEST_ORIGIN", &origin.url)
            .env("MM_RELEASE_ALLOW_INSECURE_TEST_ORIGIN", "1");
        command
    };
    let check = mm().args(["update", "--check"]).output()?;
    let stdout = String::from_utf8(check.stdout)?;
    assert!(check.status.success(), "{stdout}");
    assert!(
        stdout.contains("can be updated to 9.9.9; run `mm update`"),
        "{stdout}"
    );

    let json = mm()
        .args(["--output", "json", "doctor", "--check-updates"])
        .output()?;
    assert!(String::from_utf8(json.stdout)?.contains("\"update-available\""));

    let refused = mm().args(["update"]).write_stdin("").output()?;
    let stderr = String::from_utf8(refused.stderr)?;
    assert!(!refused.status.success());
    assert!(stderr.contains("pass --yes"), "{stderr}");

    let invalid = mm().args(["update", "--version", "9.9"]).output()?;
    assert!(String::from_utf8(invalid.stderr)?.contains("vMAJOR.MINOR.PATCH"));

    let plain_http = temp_dirs::mm()
        .env("HOME", home.path())
        .env("MM_RELEASE_TEST_ORIGIN", "http://example.invalid")
        .env("MM_RELEASE_ALLOW_INSECURE_TEST_ORIGIN", "1")
        .args(["--output", "json", "update", "--check"])
        .output()?;
    let report: serde_json::Value = serde_json::from_slice(&plain_http.stdout)?;
    assert_eq!(report["status"], "unavailable", "{report}");
    assert_eq!(plain_http.status.code(), Some(1));
    Ok(())
}

#[test]
fn release_tags_archive_paths_and_versions_follow_the_installer() -> TestResult {
    use mm::domain::release::checksums_download;
    use mm::domain::self_update::{VersionStep, archive_member, reports_version, version_step};

    for valid in ["v0.1.2", "v10.0.0-rc.1", "v0.1.2-rc.12"] {
        ReleaseTag::parse(valid)?;
    }
    for invalid in [
        "0.1.2",
        "v0.1",
        "v0.1.2-beta.1",
        "v0.1.2-rc.01",
        "v0.1.2+build",
        "v01.1.2",
    ] {
        assert!(ReleaseTag::parse(invalid).is_err(), "{invalid}");
    }
    let candidate = ReleaseTag::parse("v0.1.2-rc.1")?;
    assert_eq!(candidate.binary_version(), "0.1.2");
    let stable = ReleaseTag::parse("v0.1.2")?;
    assert_eq!(
        version_step("0.1.1", &stable, false, false)?,
        VersionStep::Replace(ReplacementKind::Upgrade)
    );
    assert_eq!(
        version_step("0.1.2", &stable, false, false)?,
        VersionStep::UpToDate
    );
    assert_eq!(
        version_step("0.1.3", &stable, false, false)?,
        VersionStep::InstalledNewer
    );
    assert!(version_step("0.1.3", &stable, true, false).is_err());
    assert_eq!(
        version_step("0.1.2", &candidate, true, true)?,
        VersionStep::Replace(ReplacementKind::Downgrade)
    );

    let target = "x86_64-unknown-linux-gnu";
    let file = "mm-v0.1.2-rc.1-x86_64-unknown-linux-gnu.tar.gz";
    let digest = "a".repeat(64);
    let found = checksums_download(&format!("{digest}  {file}\n"), &candidate, target)?;
    assert_eq!((found.file.as_str(), found.size), (file, None));
    assert!(checksums_download("", &candidate, target).is_err());
    assert!(
        checksums_download(
            &format!("{digest}  {file}\n{digest}  {file}\n"),
            &candidate,
            target
        )
        .is_err()
    );
    assert!(checksums_download(&format!("NOT-A-DIGEST  {file}\n"), &candidate, target).is_err());

    assert_eq!(archive_member("top/mm")?, ("top", Some("mm")));
    assert_eq!(archive_member("top/")?, ("top", None));
    assert_eq!(archive_member("top/completions/mm.bash")?, ("top", None));
    assert_eq!(archive_member("top/bin/mm")?, ("top", None));
    for unsafe_path in ["/top/mm", "top/../mm", "./top/mm", "top//mm", "top\\mm", ""] {
        assert!(archive_member(unsafe_path).is_err(), "{unsafe_path}");
    }
    assert!(reports_version("mm 0.1.2\n", "mm", "0.1.2"));
    assert!(!reports_version("mm 0.1.20\n", "mm", "0.1.2"));
    assert!(!reports_version("", "mm", "0.1.2"));
    Ok(())
}

#[test]
fn an_up_to_date_mm_ends_with_what_runs_which_release() -> TestResult {
    let current = env!("CARGO_PKG_VERSION");
    let tag = format!("v{current}");
    let origin = Origin::start()?;
    publish(&origin, &tag, &release_archive(&tag, current)?)?;
    let home = temp_dirs::tempdir()?;
    let elsewhere = temp_dirs::tempdir()?;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal");
    let run = |directory: &Path| {
        temp_dirs::mm()
            .current_dir(directory)
            .env("HOME", home.path())
            .env_remove("MM_HOME")
            .env_remove("MM_ENVIRONMENT")
            .env("MM_RELEASE_TEST_ORIGIN", &origin.url)
            .env("MM_RELEASE_ALLOW_INSECURE_TEST_ORIGIN", "1")
            .args(["--output", "json", "update"])
            .output()
    };

    let outside = run(elsewhere.path())?;
    let report: serde_json::Value = serde_json::from_slice(&outside.stdout)?;
    assert!(outside.status.success(), "{report}");
    assert_eq!(report["action"], "up-to-date", "{report}");
    assert_eq!(report["summary"]["version"], current, "{report}");
    assert_eq!(
        report["summary"]["services"],
        serde_json::json!([]),
        "{report}"
    );
    assert_eq!(
        report["summary"]["hosts"]["scope"], "no-project",
        "{report}"
    );

    let human = temp_dirs::mm()
        .current_dir(elsewhere.path())
        .env("HOME", home.path())
        .env_remove("MM_HOME")
        .env("MM_RELEASE_TEST_ORIGIN", &origin.url)
        .env("MM_RELEASE_ALLOW_INSECURE_TEST_ORIGIN", "1")
        .arg("update")
        .output()?;
    let stdout = String::from_utf8(human.stdout)?;
    assert!(
        stdout.contains(&format!("mm {current} is up to date.")),
        "{stdout}"
    );
    assert!(stdout.contains("Now running:"), "{stdout}");
    assert!(stdout.contains("target setup plan --upgrade"), "{stdout}");

    let project = run(&fixture)?;
    let report: serde_json::Value = serde_json::from_slice(&project.stdout)?;
    assert!(project.status.success(), "{report}");
    assert_eq!(
        report["summary"]["hosts"],
        serde_json::json!({"scope": "project", "environments": []}),
        "{report}"
    );
    Ok(())
}
