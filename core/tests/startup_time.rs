//! Startup stays fast, and a regression fails loudly.
//!
//! The budgets are generous: they hold for an unoptimized build on one slow,
//! shared CPU, where the release build is tens of times faster (the numbers
//! are in CONTRIBUTING.md, "Speed"). What they catch is work that does not
//! belong at startup: a project walk, a state database, an engine probe, a
//! connection.

use std::time::{Duration, Instant};

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// `--help`, `--version`, and shell completion: under 50 ms in a release
/// build, and under this in any build.
const PRINT_BUDGET: Duration = Duration::from_millis(1_500);
/// The fastest of a few runs: noise only ever adds time.
fn fastest(
    mut command: impl FnMut() -> std::io::Result<std::process::Output>,
) -> std::io::Result<Duration> {
    let mut best = Duration::MAX;
    for _ in 0..5 {
        let started = Instant::now();
        let output = command()?;
        best = best.min(started.elapsed());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty());
    }
    Ok(best)
}

#[test]
fn printing_commands_start_fast_and_touch_no_project_state_or_engine() -> TestResult {
    let nowhere = temp_dirs::tempdir()?;
    // No project, a home that cannot be one, and no program on PATH: these
    // commands must not look for any of them.
    let blocked_home = nowhere.path().join("not-a-directory");
    std::fs::write(&blocked_home, "blocked")?;
    let empty_path = nowhere.path().join("bin");
    std::fs::create_dir(&empty_path)?;
    for arguments in [
        vec!["--version"],
        vec!["--help"],
        vec!["service", "--help"],
        vec!["__generate", "completions", "bash"],
        vec!["__generate", "completions", "zsh"],
    ] {
        let took = fastest(|| {
            temp_dirs::mm()
                .env("PATH", &empty_path)
                .env("MM_HOME", &blocked_home)
                .current_dir(nowhere.path())
                .args(&arguments)
                .output()
        })?;
        assert!(
            took < PRINT_BUDGET,
            "`mm {}` took {took:?}; the budget is {PRINT_BUDGET:?}",
            arguments.join(" ")
        );
    }
    Ok(())
}

/// The terminal UI needs a terminal: `script` gives it one on Linux.
#[cfg(target_os = "linux")]
mod terminal_ui {
    use std::io::{Read as _, Write as _};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use super::TestResult;

    /// The first frame of the terminal UI: under 150 ms in a release build.
    const FIRST_FRAME_BUDGET: Duration = Duration::from_secs(4);
    /// How long the engine stand-ins take to answer. The first frame must
    /// not wait for them, so it has to arrive well before this.
    const ENGINE_DELAY: Duration = Duration::from_secs(15);

    fn fixture_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal")
    }

    fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(destination)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let target = destination.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                copy_tree(&entry.path(), &target)?;
            } else {
                std::fs::copy(entry.path(), target)?;
            }
        }
        Ok(())
    }

    /// A program by the name of an engine that answers only after
    /// `ENGINE_DELAY`, and says when it was started.
    fn slow_engine(directory: &Path, name: &str, started: &Path) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt as _;

        let path = directory.join(name);
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\necho \"{name} $*\" >> '{}'\n/bin/sleep {}\nexit 1\n",
                started.display(),
                ENGINE_DELAY.as_secs()
            ),
        )?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
    }

    /// The terminal UI draws before any engine has answered: its first frame
    /// shows the project from the configuration and says what is still being
    /// read. Runs where `script` can give it a terminal.
    #[test]
    fn the_terminal_ui_draws_its_first_frame_before_any_engine_answers() -> TestResult {
        let (Some(script), Some(stty)) = (program("script"), program("stty")) else {
            eprintln!("skipped: `script` and `stty` are needed to give mm a terminal");
            return Ok(());
        };
        let project = super::temp_dirs::tempdir()?;
        let home = super::temp_dirs::tempdir()?;
        let engines = super::temp_dirs::tempdir()?;
        copy_tree(&fixture_root(), project.path())?;
        let started = engines.path().join("started");
        for engine in ["podman", "docker"] {
            slow_engine(engines.path(), engine, &started)?;
        }
        let mm = assert_cmd::cargo::cargo_bin!("mm");
        let mut command = Command::new(script);
        for marker in mm::domain::agents::AGENT_ENVIRONMENT_MARKERS {
            command.env_remove(marker);
        }
        let mut child = command
            .arg("-qec")
            .arg(format!(
                "{} rows 40 cols 140 && exec '{}' --root '{}' --mm-home '{}'",
                stty.display(),
                mm.display(),
                project.path().display(),
                home.path().display()
            ))
            .arg("/dev/null")
            .env("PATH", engines.path())
            .env("TERM", "xterm-256color")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let began = Instant::now();
        let mut stdout = child.stdout.take().ok_or("script has no output")?;
        let (frames, screen) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 16 * 1024];
            while let Ok(count) = stdout.read(&mut buffer) {
                if count == 0 || frames.send(buffer[..count].to_vec()).is_err() {
                    break;
                }
            }
        });
        // The first frame: the alternate screen, then the name of the view.
        let mut output = Vec::new();
        let first_frame = loop {
            let waited = began.elapsed();
            if waited > FIRST_FRAME_BUDGET {
                break None;
            }
            if let Ok(bytes) = screen.recv_timeout(Duration::from_millis(20)) {
                output.extend(bytes);
            }
            let text = String::from_utf8_lossy(&output);
            if text.contains("\u{1b}[?1049h") && text.contains("Overview") {
                break Some(began.elapsed());
            }
        };
        let mut stdin = child.stdin.take().ok_or("script has no input")?;
        stdin.write_all(b"q")?;
        stdin.flush()?;
        let quit = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break Some(status);
            }
            if quit.elapsed() > ENGINE_DELAY {
                child.kill()?;
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        drop(stdin);
        let text = String::from_utf8_lossy(&output).into_owned();
        let first_frame = first_frame.ok_or_else(|| {
            format!("no first frame within {FIRST_FRAME_BUDGET:?}; the terminal showed: {text:?}")
        })?;
        assert!(
            first_frame < FIRST_FRAME_BUDGET && first_frame < ENGINE_DELAY,
            "the first frame took {first_frame:?}"
        );
        // The frame says the environment is still being read instead of
        // showing an empty one.
        assert!(text.contains("reading"), "{text:?}");
        // Leaving does not wait for the engine either.
        assert!(
            status.is_some_and(|status| status.success()),
            "mm did not quit while an engine was still answering: {status:?}"
        );
        assert!(quit.elapsed() < ENGINE_DELAY);
        Ok(())
    }

    /// The path of `name` in the usual system directories.
    fn program(name: &str) -> Option<PathBuf> {
        ["/usr/bin", "/bin", "/usr/local/bin"]
            .iter()
            .map(|directory| Path::new(directory).join(name))
            .find(|path| path.is_file())
    }
}
