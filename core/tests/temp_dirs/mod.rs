//! Temporary directories at canonical paths. On macOS the system temporary
//! directory lives under the `/var` symlink, and mm's path checks refuse
//! symlinked ancestors.

pub fn tempdir() -> std::io::Result<tempfile::TempDir> {
    tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)
}

/// The `mm` binary as a person runs it. mm refuses to run for an AI agent,
/// and these tests may themselves be run by one, so the variables a coding
/// tool sets are removed. Keys it creates keep their passphrase in a file.
#[allow(dead_code)]
pub fn mm() -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("mm"));
    for marker in mm::domain::agents::AGENT_ENVIRONMENT_MARKERS {
        command.env_remove(marker);
    }
    // A test never asks the keychain of whoever runs it: the passphrase of
    // a key made in a test goes to a file in the test's own home.
    command.env("MM_KEY_VAULT", "file");
    // And a test's engine is the `docker` or `podman` it puts on the PATH,
    // never the engine whose socket happens to be on this machine.
    command.env("MM_ENGINE_API", "off");
    command
}
