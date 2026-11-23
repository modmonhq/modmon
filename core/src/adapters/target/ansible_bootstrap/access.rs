//! The remote access step of `mm target setup`. Unlike the project's
//! bootstrap playbook, this playbook and the files it installs ship inside
//! mm, so the closed project-input model does not apply to them. Every value
//! it receives comes from the reviewed plan.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::adapters::private_fs::{ensure_no_symlink_components, read_regular_file};
use crate::domain::error::MmError;
use crate::domain::target::{BootstrapAccess, BootstrapAccessReport, TargetBootstrapPlan};

const ACCESS_PLAYBOOK: &str = include_str!("access.yml");
const RELAY_SHELL: &str = include_str!("../../../../../packaging/mm-relay-shell");
const SSHD_DROP_IN: &str = include_str!("../../../../../packaging/sshd/90-mm-relay.conf");
const MMD_SERVICE: &str = include_str!("../../../../../packaging/systemd/mmd.service");
const MESH_SERVICE: &str = include_str!("../../../../../packaging/systemd/mm-mesh.service");
const NETWORK_SERVICE: &str = include_str!("../../../../../packaging/systemd/mm-network.service");
const CONTAINER_PROFILE: &str = include_str!("../../../../../packaging/apparmor/mm-containers");
const SHARED_PID_PROFILE: &str =
    include_str!("../../../../../packaging/apparmor/mm-containers-shared-pid");
const MAX_REPORT_BYTES: usize = 64 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;

/// Where the staged access step lives inside the private staging directory.
#[derive(Debug)]
pub(super) struct StagedAccess {
    pub playbook: PathBuf,
    pub variables: PathBuf,
    pub report: PathBuf,
}

/// Copies the reviewed binaries and mm's packaging files into `staging`,
/// checking each binary against the digest the operator reviewed.
pub(super) fn stage(
    staging: &Path,
    plan: &TargetBootstrapPlan,
    access: &BootstrapAccess,
) -> Result<StagedAccess, MmError> {
    let directory = staging.join("access");
    fs::create_dir(&directory).map_err(|source| MmError::Io {
        context: "creating the remote access staging directory".to_owned(),
        source,
    })?;
    super::set_private_directory_permissions(&directory)?;
    for set in &access.binaries {
        let cpu = directory.join(&set.architecture);
        fs::create_dir(&cpu).map_err(|source| MmError::Io {
            context: "creating the remote access staging directory".to_owned(),
            source,
        })?;
        super::set_private_directory_permissions(&cpu)?;
        copy_reviewed_binary(&set.binary("mm"), &cpu.join("mm"), &set.mm_digest)?;
        copy_reviewed_binary(&set.binary("mmd"), &cpu.join("mmd"), &set.mmd_digest)?;
    }
    for (name, contents) in [
        ("mm-relay-shell", RELAY_SHELL),
        ("90-mm-relay.conf", SSHD_DROP_IN),
        ("mmd.service", MMD_SERVICE),
        ("mm-mesh.service", MESH_SERVICE),
        ("mm-network.service", NETWORK_SERVICE),
        ("mm-containers", CONTAINER_PROFILE),
        ("mm-containers-shared-pid", SHARED_PID_PROFILE),
        ("access.yml", ACCESS_PLAYBOOK),
    ] {
        super::write_private_file(&directory.join(name), contents.as_bytes())?;
    }
    let report = directory.join("report.json");
    let variables = directory.join("variables.json");
    let values = serde_json::json!({
        "mm_access_files": utf8(&directory)?,
        "mm_access_report": utf8(&report)?,
        "mm_access_architectures": access
            .binaries
            .iter()
            .map(|set| set.architecture.as_str())
            .collect::<Vec<_>>(),
        "mm_access_enroll_argv": enroll_arguments(plan, access),
        "mm_access_apparmor_profiles": access.apparmor_profiles,
    });
    reject_templating(&values)?;
    let bytes = serde_json::to_vec_pretty(&values).map_err(|error| MmError::Target {
        message: format!("serializing remote access variables: {error}"),
    })?;
    super::write_private_file(&variables, &bytes)?;
    Ok(StagedAccess {
        playbook: directory.join("access.yml"),
        variables,
        report,
    })
}

/// The `mmd access enroll` command the playbook runs on the host.
pub(super) fn enroll_arguments(
    plan: &TargetBootstrapPlan,
    access: &BootstrapAccess,
) -> Vec<String> {
    // A host mm pushes the project's files to has no origin to fetch from.
    let source = if access.pushes() {
        ["--delivery", "push"]
    } else {
        ["--origin", access.origin.as_str()]
    };
    let mut arguments = [
        "/usr/libexec/mmd",
        "access",
        "enroll",
        "--project",
        &plan.project_id.0.to_string(),
        "--name",
        &plan.project_name,
        source[0],
        source[1],
        "--environment",
        &plan.environment,
    ]
    .map(str::to_owned)
    .to_vec();
    if let Some(key) = &access.origin_host_key {
        arguments.extend(["--origin-host-key".to_owned(), key.clone()]);
    }
    for entry in &access.break_glass {
        arguments.extend([
            "--break-glass".to_owned(),
            format!("{}={}", entry.name, entry.key),
        ]);
    }
    arguments
}

/// Reads the facts the playbook reported from the host.
pub(super) fn read_report(path: &Path) -> Result<BootstrapAccessReport, MmError> {
    let bytes = read_regular_file(path, MAX_REPORT_BYTES)?.ok_or_else(|| MmError::Target {
        message: "the remote access playbook finished without reporting the host's keys".to_owned(),
    })?;
    let report: BootstrapAccessReport =
        serde_json::from_slice(&bytes).map_err(|error| MmError::Target {
            message: format!("the remote access report is invalid: {error}"),
        })?;
    report
        .validate()
        .map_err(|message| MmError::Security { message })?;
    Ok(report)
}

/// Ansible templates variable values when it uses them, so no value may
/// carry Jinja delimiters.
fn reject_templating(value: &serde_json::Value) -> Result<(), MmError> {
    match value {
        serde_json::Value::String(text) if text.contains(['{', '}']) => Err(MmError::Target {
            message: "remote access setup cannot pass '{' or '}' to Ansible; check the project name and setup flags".to_owned(),
        }),
        serde_json::Value::Array(values) => values.iter().try_for_each(reject_templating),
        serde_json::Value::Object(values) => values.values().try_for_each(reject_templating),
        _ => Ok(()),
    }
}

fn utf8(path: &Path) -> Result<&str, MmError> {
    path.to_str().ok_or_else(|| MmError::Target {
        message: format!(
            "remote access staging path '{}' is not valid UTF-8",
            path.display()
        ),
    })
}

/// Copies one binary while hashing it, so the staged copy is exactly the
/// reviewed file even if the source changes during the copy.
fn copy_reviewed_binary(source: &Path, destination: &Path, digest: &str) -> Result<(), MmError> {
    ensure_no_symlink_components(destination)?;
    let metadata = fs::symlink_metadata(source).map_err(|source_error| MmError::Io {
        context: format!("reading host binary {}", source.display()),
        source: source_error,
    })?;
    if !metadata.file_type().is_file() {
        return Err(MmError::State {
            message: format!(
                "host binary '{}' changed after plan review",
                source.display()
            ),
        });
    }
    let mut input = fs::File::open(source).map_err(|source_error| MmError::Io {
        context: format!("opening host binary {}", source.display()),
        source: source_error,
    })?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o700);
    }
    let mut output = options
        .open(destination)
        .map_err(|source_error| MmError::Io {
            context: format!("staging host binary {}", destination.display()),
            source: source_error,
        })?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|source_error| MmError::Io {
                context: format!("reading host binary {}", source.display()),
                source: source_error,
            })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|source_error| MmError::Io {
                context: format!("staging host binary {}", destination.display()),
                source: source_error,
            })?;
    }
    output.sync_all().map_err(|source_error| MmError::Io {
        context: format!("staging host binary {}", destination.display()),
        source: source_error,
    })?;
    if hasher.finalize().to_hex().as_str() != digest {
        return Err(MmError::State {
            message: format!(
                "host binary '{}' changed after plan review",
                source.display()
            ),
        });
    }
    Ok(())
}
