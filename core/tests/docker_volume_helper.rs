//! The Docker volume helper runs as the owner of the volume's root with no
//! capability, and the engine unpacks the file with its owner and mode.

use std::collections::VecDeque;
use std::error::Error;
use std::io::Read;
use std::sync::{Arc, Mutex};

use mm::adapters::runtime::DockerComposeRuntime;
use mm::app::ports::{
    CommandOutput, CommandSpec, LocalDeploymentRuntime, OwnedVolumeFile, ProcessRunner,
    VolumeOwnership,
};
use mm::domain::error::MmError;
use mm::domain::ownership::{ResourceKind, ResourceOwnership};
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId};
use zeroize::Zeroizing;

mod temp_dirs;

struct Call {
    arguments: Vec<String>,
    stdin: Option<Vec<u8>>,
}

struct Engine {
    answers: Mutex<VecDeque<Result<CommandOutput, MmError>>>,
    calls: Mutex<Vec<Call>>,
}

impl Engine {
    fn answering(answers: impl IntoIterator<Item = Result<CommandOutput, MmError>>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.into_iter().collect()),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Result<Vec<Call>, MmError> {
        let mut calls = self.calls.lock().map_err(|_| poisoned())?;
        Ok(std::mem::take(&mut *calls))
    }
}

fn poisoned() -> MmError {
    MmError::State {
        message: "the fake engine's lock was poisoned".to_owned(),
    }
}

#[async_trait::async_trait]
impl ProcessRunner for Engine {
    async fn run(&self, mut specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let stdin = match specification.take_stdin_file() {
            Some(mut file) => {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes).map_err(|source| MmError::Io {
                    context: "reading the helper's standard input".to_owned(),
                    source,
                })?;
                Some(bytes)
            }
            None => None,
        };
        let arguments = specification.redacted_record().arguments;
        self.calls
            .lock()
            .map_err(|_| poisoned())?
            .push(Call { arguments, stdin });
        self.answers
            .lock()
            .map_err(|_| poisoned())?
            .pop_front()
            .unwrap_or_else(|| {
                Err(MmError::State {
                    message: "the fake engine has no answer left".to_owned(),
                })
            })
    }
}

fn answer(code: i32, stdout: &str, stderr: &str) -> Result<CommandOutput, MmError> {
    Ok(CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(stdout.to_owned()),
        stderr: Zeroizing::new(stderr.to_owned()),
        truncated: false,
    })
}

fn ok(stdout: &str) -> Result<CommandOutput, MmError> {
    answer(0, stdout, "")
}

fn target() -> NamedTarget {
    NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(uuid::Uuid::from_u128(7))),
    )
}

fn owned_volume() -> Result<CommandOutput, MmError> {
    let ownership =
        ResourceOwnership::for_named_target(&target(), ResourceKind::Volume, "data", "volume");
    ok(&serde_json::json!([{"Name": "data", "Labels": ownership.labels()}]).to_string())
}

fn config(uid: u32, gid: u32, mode: u32) -> OwnedVolumeFile<'static> {
    OwnedVolumeFile {
        name: "config.hcl",
        ownership: VolumeOwnership { uid, gid, mode },
    }
}

struct Archived {
    path: String,
    owner: (u64, u64),
    mode: u32,
    contents: Vec<u8>,
}

fn archived(call: &Call) -> Result<Archived, Box<dyn Error>> {
    let bytes = call
        .stdin
        .as_deref()
        .ok_or("docker cp was given no archive")?;
    let mut archive = tar::Archive::new(bytes);
    let mut entries = archive.entries()?;
    let mut entry = entries.next().ok_or("the archive is empty")??;
    let header = entry.header();
    let (owner, mode) = ((header.uid()?, header.gid()?), header.mode()?);
    let path = entry.path()?.to_string_lossy().into_owned();
    let mut contents = Vec::new();
    entry.read_to_end(&mut contents)?;
    drop(entry);
    assert!(entries.next().is_none(), "the archive holds one file");
    Ok(Archived {
        path,
        owner,
        mode,
        contents,
    })
}

fn locked_down(arguments: &[String]) -> bool {
    let has = |pair: [&str; 2]| arguments.windows(2).any(|window| window == pair);
    has(["--cap-drop", "ALL"])
        && has(["--network", "none"])
        && has(["--security-opt", "no-new-privileges"])
        && has(["--volume", "data:/volume"])
        && arguments.iter().any(|argument| argument == "--read-only")
        && !arguments.iter().any(|argument| argument == "--cap-add")
}

fn user(arguments: &[String]) -> Option<&str> {
    arguments
        .windows(2)
        .find(|window| window[0] == "--user")
        .map(|window| window[1].as_str())
}

#[tokio::test]
async fn a_file_reaches_a_volume_another_uid_owns_with_no_capability() -> Result<(), Box<dyn Error>>
{
    let temporary = temp_dirs::tempdir()?;
    let source = temporary.path().join("config.hcl");
    std::fs::write(&source, "listener {}\n")?;
    let engine = Engine::answering([
        owned_volume(),
        ok("100:1000\n"),
        ok("0123abcd\n"),
        ok(""),
        ok(""),
        ok(""),
    ]);
    DockerComposeRuntime::new(engine.clone())
        .replace_volume_file_owned(
            &target(),
            "data",
            "volume",
            config(100, 1000, 0o600),
            &source,
        )
        .await?;

    let calls = engine.calls()?;
    assert_eq!(calls.len(), 6);
    let owner = &calls[1].arguments;
    assert_eq!(owner[..2], ["run", "--rm"]);
    assert!(owner.ends_with(&["stat", "-c", "%u:%g", "/volume"].map(String::from)));
    assert!(locked_down(owner) && user(owner).is_none());
    let helper = &calls[2].arguments;
    assert_eq!(helper[0], "create");
    assert!(locked_down(helper));
    assert_eq!(user(helper), Some("100:1000"));
    assert_eq!(calls[3].arguments, ["cp", "-", "0123abcd:/volume"]);
    let staged = archived(&calls[3])?;
    assert!(staged.path.starts_with(".mm-source-"));
    assert_eq!((staged.owner, staged.mode), ((100, 1000), 0o600));
    assert_eq!(staged.contents, b"listener {}\n");
    assert_eq!(
        helper[helper.len() - 2..],
        [staged.path, "config.hcl".to_owned()]
    );
    assert_eq!(calls[4].arguments, ["start", "--attach", "0123abcd"]);
    assert_eq!(calls[5].arguments, ["rm", "--force", "0123abcd"]);
    Ok(())
}

#[tokio::test]
async fn a_file_for_another_owner_needs_no_capability_either() -> Result<(), Box<dyn Error>> {
    let temporary = temp_dirs::tempdir()?;
    let source = temporary.path().join("route.yaml");
    std::fs::write(&source, "http: {}\n")?;
    let engine = Engine::answering([
        owned_volume(),
        ok("100:1000\n"),
        ok("0123abcd\n"),
        ok(""),
        ok(""),
        ok(""),
    ]);
    DockerComposeRuntime::new(engine.clone())
        .replace_volume_file(&target(), "data", "volume", "route.yaml", &source, 0o644)
        .await?;

    let calls = engine.calls()?;
    assert_eq!(user(&calls[2].arguments), Some("100:1000"));
    assert!(calls.iter().all(|call| {
        !call
            .arguments
            .iter()
            .any(|argument| argument == "--cap-add")
    }));
    let staged = archived(&calls[3])?;
    assert_eq!((staged.owner, staged.mode), ((0, 0), 0o644));
    Ok(())
}

#[tokio::test]
async fn creating_and_removing_a_file_run_as_the_volume_root_owner() -> Result<(), Box<dyn Error>> {
    let engine = Engine::answering([
        owned_volume(),
        ok("100:1000\n"),
        ok(""),
        owned_volume(),
        ok("100:1000\n"),
        ok(""),
    ]);
    let runtime = DockerComposeRuntime::new(engine.clone());
    runtime
        .create_volume_file_if_absent(&target(), "data", "volume", "empty", 0o600)
        .await?;
    runtime
        .remove_volume_file(&target(), "data", "volume", "empty")
        .await?;

    let calls = engine.calls()?;
    for helper in [&calls[2].arguments, &calls[5].arguments] {
        assert_eq!(helper[..2], ["run", "--rm"]);
        assert!(locked_down(helper));
        assert_eq!(user(helper), Some("100:1000"));
    }
    Ok(())
}

#[tokio::test]
async fn failures_name_the_volume_and_the_step() -> Result<(), Box<dyn Error>> {
    let temporary = temp_dirs::tempdir()?;
    let source = temporary.path().join("config.hcl");
    std::fs::write(&source, "listener {}\n")?;
    let copy = |engine: Arc<Engine>| {
        let source = source.clone();
        async move {
            DockerComposeRuntime::new(engine)
                .replace_volume_file_owned(
                    &target(),
                    "data",
                    "volume",
                    config(100, 1000, 0o600),
                    &source,
                )
                .await
        }
    };

    let unreadable = Engine::answering([
        owned_volume(),
        answer(1, "", "stat: can't stat '/volume': Permission denied\n"),
    ]);
    let error = copy(unreadable)
        .await
        .err()
        .ok_or("reading the owner should fail")?;
    assert_eq!(error.code(), "MM_PROCESS_FAILED");
    assert!(
        error
            .to_string()
            .contains("reading the owner of volume 'data'"),
        "{error}"
    );
    assert!(error.to_string().contains("Permission denied"), "{error}");

    let named = Engine::answering([owned_volume(), ok("root:root\n")]);
    let error = copy(named).await.err().ok_or("a named owner should fail")?;
    assert_eq!(error.code(), "MM_RUNTIME_RESPONSE");
    assert!(error.to_string().contains("volume 'data'"), "{error}");

    let late = Engine::answering([
        owned_volume(),
        Err(MmError::ProcessTimeout {
            program: "docker".to_owned(),
            seconds: 90,
        }),
    ]);
    let error = copy(late)
        .await
        .err()
        .ok_or("a late owner read should fail")?;
    assert_eq!(error.code(), "MM_PROCESS_TIMEOUT");
    assert!(error.to_string().contains("volume 'data'"), "{error}");

    let refused = Engine::answering([
        owned_volume(),
        ok("100:1000\n"),
        ok("0123abcd\n"),
        answer(
            1,
            "",
            "Error response from daemon: no space left on device\n",
        ),
        ok(""),
    ]);
    let error = copy(refused.clone())
        .await
        .err()
        .ok_or("the copy should fail")?;
    assert!(
        error
            .to_string()
            .contains("copying 'config.hcl' into volume 'data'"),
        "{error}"
    );
    assert_eq!(refused.calls()?[4].arguments, ["rm", "--force", "0123abcd"]);

    let stuck = Engine::answering([
        owned_volume(),
        ok("100:1000\n"),
        ok("0123abcd\n"),
        ok(""),
        answer(
            1,
            "",
            "mv: can't rename '/volume/.mm-source': Permission denied\n",
        ),
        ok(""),
    ]);
    let error = copy(stuck.clone())
        .await
        .err()
        .ok_or("the move should fail")?;
    assert!(
        error
            .to_string()
            .contains("moving 'config.hcl' into place in volume 'data'"),
        "{error}"
    );
    assert_eq!(stuck.calls()?[5].arguments, ["rm", "--force", "0123abcd"]);
    Ok(())
}

#[tokio::test]
async fn a_mode_beyond_permission_bits_is_refused_before_any_helper_runs()
-> Result<(), Box<dyn Error>> {
    let temporary = temp_dirs::tempdir()?;
    let source = temporary.path().join("config.hcl");
    std::fs::write(&source, "listener {}\n")?;
    let engine = Engine::answering([owned_volume()]);
    let refused = DockerComposeRuntime::new(engine.clone())
        .replace_volume_file_owned(
            &target(),
            "data",
            "volume",
            config(100, 1000, 0o4755),
            &source,
        )
        .await;
    assert!(matches!(refused, Err(MmError::Security { .. })));
    assert_eq!(engine.calls()?.len(), 1);
    Ok(())
}
