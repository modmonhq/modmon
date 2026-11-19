//! The `mm` and `mmd` that `mm target setup` installs: which CPU they are
//! built for, and, without `--host-binaries`, this mm's own release for each
//! host's CPU from a local GitHub-style release, kept in a cache.

mod temp_dirs;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mm::adapters::release::ReqwestReleaseClient;
use mm::adapters::release_cache::FilesystemReleaseCache;
use mm::adapters::self_update::LocalInstallation;
use mm::app::ports::NoProgress;
use mm::app::{HostBinarySource, HostCpu, given_binaries};
use mm::domain::target::{
    HOST_ARCHITECTURES, HostBinaries, binaries_for, host_cpu, linux_architecture, split_machine,
};
use sha2::Digest;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const MANIFEST: &str = include_str!("fixtures/release-manifest.json");

fn elf(class: u8, data: u8, machine: u16) -> Vec<u8> {
    let mut header = vec![0x7f, b'E', b'L', b'F', class, data, 1, 0];
    header.resize(18, 0);
    header.extend_from_slice(&machine.to_le_bytes());
    header.resize(64, 0);
    header
}

fn binary(cpu: &str, name: &str) -> Vec<u8> {
    let machine = if cpu == "aarch64" { 183 } else { 62 };
    [elf(2, 1, machine), format!("{name} for {cpu}").into_bytes()].concat()
}

#[test]
fn linux_executables_name_their_cpu_and_anything_else_says_what_it_is() {
    assert_eq!(HOST_ARCHITECTURES, ["x86_64", "aarch64"]);
    assert_eq!(linux_architecture(&elf(2, 1, 62)), Ok("x86_64"));
    assert_eq!(linux_architecture(&elf(2, 1, 183)), Ok("aarch64"));
    let riscv = linux_architecture(&elf(2, 1, 243))
        .err()
        .unwrap_or_default();
    assert!(riscv.contains("ELF machine 243"), "{riscv}");
    for other in [elf(1, 1, 62), elf(2, 2, 62)] {
        let refused = linux_architecture(&other).err().unwrap_or_default();
        assert!(refused.contains("64-bit little-endian"), "{refused}");
    }
    for macos in [
        vec![0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1],
        vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 2],
    ] {
        assert_eq!(
            linux_architecture(&macos),
            Err("is built for macOS, not Linux".to_owned())
        );
    }
    for other in [
        b"#!/bin/sh\n".to_vec(),
        Vec::new(),
        elf(2, 1, 62)[..12].to_vec(),
    ] {
        assert!(linux_architecture(&other).is_err());
    }
}

#[test]
fn the_facts_probe_names_the_cpu_first_and_setup_maps_it() {
    for (machine, cpu) in [
        ("x86_64", Some("x86_64")),
        ("amd64", Some("x86_64")),
        ("aarch64", Some("aarch64")),
        ("arm64", Some("aarch64")),
        ("riscv64", None),
        ("armv7l", None),
    ] {
        assert_eq!(host_cpu(machine), cpu, "{machine}");
    }
    assert_eq!(
        split_machine("aarch64\n{\"public-key\":\"k\"}\n"),
        (Some("aarch64"), "{\"public-key\":\"k\"}\n")
    );
    assert_eq!(split_machine("x86_64\n{}\n"), (Some("x86_64"), "{}\n"));
    assert_eq!(split_machine("{}\n"), (None, "{}\n"));
    let script = include_str!("../src/adapters/target/cluster_shell/mesh-facts.sh");
    let first = script
        .lines()
        .find(|line| !line.starts_with('#') && !line.starts_with("set "))
        .unwrap_or_default();
    assert_eq!(first, "uname -m");
}

#[test]
fn the_access_playbook_installs_the_pair_for_the_hosts_cpu_and_refuses_another()
-> Result<(), Box<dyn std::error::Error>> {
    let playbook: serde_yaml_ng::Value = serde_yaml_ng::from_str(include_str!(
        "../src/adapters/target/ansible_bootstrap/access.yml"
    ))?;
    let tasks = playbook[0]["tasks"]
        .as_sequence()
        .ok_or("the access playbook has no tasks")?;
    let name = |task: &serde_yaml_ng::Value| task["name"].as_str().unwrap_or_default().to_owned();
    let check = tasks
        .iter()
        .position(|task| name(task) == "Require mm and mmd built for this host's CPU")
        .ok_or("no task checks the host's CPU")?;
    let first_change = tasks
        .iter()
        .position(|task| task.get("ansible.builtin.assert").is_none())
        .ok_or("the playbook changes nothing")?;
    assert!(check < first_change);
    let condition = format!("{:?}", tasks[check]["ansible.builtin.assert"]["that"]);
    assert!(
        condition.contains("mm_access_cpu in mm_access_architectures"),
        "{condition}"
    );
    let install = tasks
        .iter()
        .find(|task| name(task) == "Install mmd and stage mm")
        .ok_or("no task installs mmd")?;
    assert_eq!(
        install["ansible.builtin.copy"]["src"].as_str(),
        Some("{{ mm_access_files }}/{{ mm_access_cpu }}/{{ item.name }}")
    );
    Ok(())
}

type Hits = Arc<Mutex<BTreeMap<String, usize>>>;

/// A release origin on 127.0.0.1 that serves `files` and counts requests.
struct Origin {
    url: String,
    files: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    hits: Hits,
}

impl Origin {
    fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let url = format!("http://{}", listener.local_addr()?);
        let files: Arc<Mutex<BTreeMap<String, Vec<u8>>>> = Arc::default();
        let hits: Hits = Arc::default();
        let (served, counted) = (Arc::clone(&files), Arc::clone(&hits));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                if respond(&stream, &served, &counted).is_err() {
                    continue;
                }
            }
        });
        Ok(Self { url, files, hits })
    }

    fn put(&self, path: &str, bytes: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        self.files
            .lock()
            .map_err(|_| "poisoned")?
            .insert(path.to_owned(), bytes);
        Ok(())
    }

    fn hits(&self, path: &str) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(self
            .hits
            .lock()
            .map_err(|_| "poisoned")?
            .get(path)
            .copied()
            .unwrap_or(0))
    }
}

fn respond(
    stream: &std::net::TcpStream,
    files: &Mutex<BTreeMap<String, Vec<u8>>>,
    hits: &Hits,
) -> std::io::Result<()> {
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
    *hits
        .lock()
        .map_err(|_| std::io::Error::other("poisoned"))?
        .entry(path.clone())
        .or_default() += 1;
    let body = files
        .lock()
        .map_err(|_| std::io::Error::other("poisoned"))?
        .get(&path)
        .cloned();
    let mut writer = stream;
    match body {
        Some(body) => {
            write!(
                writer,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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

fn sha256(bytes: &[u8]) -> String {
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn archive(tag: &str, cpu: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let top = format!("mm-{tag}-{cpu}-unknown-linux-gnu");
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::fast(),
    ));
    for name in ["mm", "mmd", "mm-ingress-helper"] {
        let bytes = binary(cpu, name);
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o755);
        builder.append_data(&mut header, format!("{top}/{name}"), bytes.as_slice())?;
    }
    Ok(builder.into_inner()?.finish()?)
}

/// Publishes release `tag` with a Linux archive for each CPU; returns the
/// archive paths by CPU.
fn publish(
    origin: &Origin,
    tag: &str,
) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let text = MANIFEST
        .replace("v0.1.0", tag)
        .replace("\"0.1.0\"", &format!("\"{}\"", tag.trim_start_matches('v')));
    let mut manifest: serde_json::Value = serde_json::from_str(&text)?;
    let mut paths = BTreeMap::new();
    for cpu in ["x86_64", "aarch64"] {
        let target = format!("{cpu}-unknown-linux-gnu");
        let bytes = archive(tag, cpu)?;
        manifest["artifacts"][&target]["sha256"] = serde_json::json!(sha256(&bytes));
        manifest["artifacts"][&target]["size"] = serde_json::json!(bytes.len());
        let path = format!("/releases/download/{tag}/mm-{tag}-{target}.tar.gz");
        origin.put(&path, bytes)?;
        paths.insert(cpu.to_owned(), path);
    }
    origin.put(
        &format!("/releases/download/{tag}/manifest.json"),
        manifest.to_string().into_bytes(),
    )?;
    Ok(paths)
}

fn pair(directory: &Path, cpu: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(directory)?;
    for name in ["mm", "mmd"] {
        std::fs::write(directory.join(name), binary(cpu, name))?;
    }
    Ok(())
}

fn source(
    origin: &Origin,
    cache: &Path,
    beside: Option<PathBuf>,
    version: &str,
) -> Result<HostBinarySource, String> {
    Ok(HostBinarySource::new(
        Arc::new(ReqwestReleaseClient::new(&origin.url)?),
        Arc::new(LocalInstallation),
        Arc::new(FilesystemReleaseCache::new(cache.to_path_buf())),
        beside,
        version,
    ))
}

fn host(name: &str, cpu: Option<&str>) -> HostCpu {
    HostCpu {
        host: name.to_owned(),
        cpu: cpu.map(str::to_owned),
    }
}

fn cpus(sets: &[HostBinaries]) -> Vec<&str> {
    sets.iter().map(|set| set.architecture.as_str()).collect()
}

#[tokio::test]
async fn a_host_of_another_cpu_gets_this_mms_release_once_and_then_the_cache() -> TestResult {
    let origin = Origin::start()?;
    let archives = publish(&origin, "v9.9.9")?;
    let directory = temp_dirs::tempdir()?;
    let beside = directory.path().join("bin");
    pair(&beside, "x86_64")?;
    let cache = directory.path().join("home/releases");
    let source = source(&origin, &cache, Some(beside.clone()), "9.9.9")?;

    let sets = source
        .resolve(
            None,
            &[
                host("app-1", Some("x86_64")),
                host("app-2", Some("aarch64")),
            ],
            &NoProgress,
        )
        .await?;
    assert_eq!(cpus(&sets), ["aarch64", "x86_64"]);
    let release = cache.join("v9.9.9/aarch64-unknown-linux-gnu");
    assert_eq!(sets[0].directory, release);
    assert_eq!(
        sets[0].source,
        format!(
            "release v9.9.9 for aarch64, cached in {}",
            release.display()
        )
    );
    assert_eq!(
        sets[0].mmd_digest,
        blake3::hash(&binary("aarch64", "mmd")).to_hex().to_string()
    );
    assert_eq!(sets[1].directory, beside);
    assert!(
        sets[1].source.starts_with("this mm's directory "),
        "{}",
        sets[1].source
    );
    assert_eq!(origin.hits(&archives["aarch64"])?, 1);
    assert_eq!(origin.hits(&archives["x86_64"])?, 0);

    let again = source
        .resolve(None, &[host("app-2", Some("aarch64"))], &NoProgress)
        .await?;
    assert_eq!(again, sets[..1]);
    assert_eq!(
        origin.hits(&archives["aarch64"])?,
        1,
        "the cache was not reused"
    );
    let leftovers = std::fs::read_dir(cache.join("v9.9.9"))?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(leftovers, ["aarch64-unknown-linux-gnu"]);

    std::fs::write(release.join("archive.sha256"), "0".repeat(64))?;
    let refreshed = source
        .resolve(None, &[host("app-2", Some("aarch64"))], &NoProgress)
        .await?;
    assert_eq!(refreshed, sets[..1]);
    assert_eq!(
        origin.hits(&archives["aarch64"])?,
        2,
        "a stale cache was used"
    );
    Ok(())
}

#[tokio::test]
async fn an_unread_cpu_gets_both_pairs_and_a_given_directory_must_fit_every_host() -> TestResult {
    let origin = Origin::start()?;
    publish(&origin, "v9.9.9")?;
    let directory = temp_dirs::tempdir()?;
    let source = source(&origin, &directory.path().join("releases"), None, "9.9.9")?;
    let sets = source
        .resolve(
            None,
            &[host("app-1", Some("x86_64")), host("app-3", None)],
            &NoProgress,
        )
        .await?;
    assert_eq!(cpus(&sets), ["aarch64", "x86_64"]);
    assert_eq!(binaries_for(&sets, None).len(), 2);
    assert_eq!(
        cpus(
            &binaries_for(&sets, Some("x86_64"))
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        ),
        ["x86_64"]
    );

    let given = directory.path().join("given");
    pair(&given, "x86_64")?;
    let refused = source
        .resolve(
            Some(given.as_path()),
            &[
                host("app-1", Some("x86_64")),
                host("app-2", Some("aarch64")),
            ],
            &NoProgress,
        )
        .await
        .err()
        .ok_or("x86_64 binaries were planned for an aarch64 host")?
        .to_string();
    assert!(refused.contains("app-2 is aarch64"), "{refused}");
    assert!(refused.contains("leave it out"), "{refused}");
    let fits = given_binaries(
        &given,
        &[host("app-1", Some("x86_64")), host("app-3", None)],
    )?;
    assert_eq!(
        fits.source,
        std::fs::canonicalize(&given)?.display().to_string()
    );
    Ok(())
}

#[tokio::test]
async fn without_a_release_or_with_a_damaged_one_setup_says_what_to_pass() -> TestResult {
    let origin = Origin::start()?;
    let archives = publish(&origin, "v9.9.9")?;
    let directory = temp_dirs::tempdir()?;
    let cache = directory.path().join("releases");

    let unreleased = source(&origin, &cache, None, "9.9.10")?
        .resolve(None, &[host("app-1", Some("aarch64"))], &NoProgress)
        .await
        .err()
        .ok_or("an unreleased mm found host binaries")?
        .to_string();
    assert!(
        unreleased.contains("mm 9.9.10 has no release"),
        "{unreleased}"
    );
    assert!(unreleased.contains("--host-binaries DIR"), "{unreleased}");
    assert!(unreleased.contains("`mm update`"), "{unreleased}");

    origin.put(
        &archives["aarch64"],
        b"not the archive the manifest names".to_vec(),
    )?;
    let damaged = source(&origin, &cache, None, "9.9.9")?
        .resolve(None, &[host("app-1", Some("aarch64"))], &NoProgress)
        .await
        .err()
        .ok_or("a damaged archive was used")?;
    assert_eq!(damaged.code(), "MM_SECURITY");
    assert!(!cache.join("v9.9.9/aarch64-unknown-linux-gnu").exists());
    let staged = std::fs::read_dir(cache.join("v9.9.9"))?.count();
    assert_eq!(staged, 0, "a staging directory was left behind");
    Ok(())
}
