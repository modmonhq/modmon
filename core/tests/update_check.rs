//! `mm doctor --check-updates` against a GitHub-style release: the manifest the
//! release workflow assembles, behind the latest-release redirect.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;

use mm::adapters::release::ReqwestReleaseManifestClient;
use mm::app::UpdateCheckService;
use mm::domain::release::{RELEASE_MANIFEST_URL, UpdateStatus, installed_target};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const MANIFEST: &str = include_str!("fixtures/release-manifest.json");

fn respond(stream: std::net::TcpStream, manifest: &str) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" {
            break;
        }
    }
    let response = if request.starts_with("GET /releases/latest/download/manifest.json ") {
        "HTTP/1.1 302 Found\r\nLocation: /releases/download/v0.1.0/manifest.json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned()
    } else if request.starts_with("GET /releases/download/v0.1.0/manifest.json ") {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{manifest}",
            manifest.len()
        )
    } else {
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned()
    };
    (&stream).write_all(response.as_bytes())
}

fn serve(manifest: String) -> std::io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    std::thread::spawn(move || {
        for stream in listener.incoming().take(2).flatten() {
            if respond(stream, &manifest).is_err() {
                return;
            }
        }
    });
    Ok(format!(
        "http://{address}/releases/latest/download/manifest.json"
    ))
}

fn service(url: String) -> Result<UpdateCheckService, String> {
    Ok(UpdateCheckService::new(Arc::new(
        ReqwestReleaseManifestClient::new(url)?,
    )))
}

#[test]
fn the_update_check_reads_the_manifest_of_the_latest_github_release() {
    assert_eq!(
        RELEASE_MANIFEST_URL,
        "https://github.com/modmonhq/modmon/releases/latest/download/manifest.json"
    );
}

#[tokio::test]
async fn a_newer_same_or_older_release_is_reported_as_such() -> TestResult {
    if installed_target().is_none() {
        return Ok(());
    }
    for (installed, status) in [
        ("0.0.9", UpdateStatus::UpdateAvailable),
        ("0.1.0", UpdateStatus::UpToDate),
        ("0.2.0", UpdateStatus::InstalledVersionNewer),
    ] {
        let report = service(serve(MANIFEST.to_owned())?)?.check(installed).await;
        assert_eq!(report.status, status, "{installed}: {}", report.message);
        assert_eq!(report.latest.as_deref(), Some("0.1.0"), "{installed}");
        if status == UpdateStatus::UpdateAvailable {
            assert!(
                report
                    .message
                    .contains("rerun the installer, https://modmon.dev/install.sh"),
                "{}",
                report.message
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn an_unreachable_release_a_candidate_or_a_changed_manifest_is_never_an_update() -> TestResult
{
    let closed = TcpListener::bind("127.0.0.1:0")?;
    let address = closed.local_addr()?;
    drop(closed);
    let unreachable = service(format!(
        "http://{address}/releases/latest/download/manifest.json"
    ))?
    .check("0.0.9")
    .await;
    assert_eq!(unreachable.status, UpdateStatus::Unavailable);
    assert_eq!(unreachable.latest, None);

    let candidate = MANIFEST
        .replace("\"version\": \"0.1.0\"", "\"version\": \"0.1.1-rc.1\"")
        .replace("\"tag\": \"v0.1.0\"", "\"tag\": \"v0.1.1-rc.1\"");
    assert_ne!(candidate, MANIFEST);
    let digest = MANIFEST
        .lines()
        .find(|line| line.contains("\"sha256\""))
        .ok_or("the fixture has no digest")?;
    let changed = MANIFEST.replacen(digest, "      \"sha256\": \"not-a-digest\",", 1);
    for manifest in [candidate, changed] {
        let report = service(serve(manifest)?)?.check("0.0.9").await;
        assert_eq!(
            report.status,
            UpdateStatus::Unavailable,
            "{}",
            report.message
        );
    }
    Ok(())
}
