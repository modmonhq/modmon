use std::sync::Mutex;

use zeroize::Zeroizing;

use super::*;

#[derive(Debug)]
struct FakeProcess {
    outputs: Mutex<Vec<CommandOutput>>,
}

#[async_trait]
impl ProcessRunner for FakeProcess {
    async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
        self.outputs
            .lock()
            .map_err(|_| MmError::State {
                message: "fake process lock was poisoned".to_owned(),
            })?
            .pop()
            .ok_or_else(|| MmError::State {
                message: "fake process has no output".to_owned(),
            })
    }
}

fn output(code: i32, stdout: &str) -> CommandOutput {
    CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(stdout.to_owned()),
        stderr: Zeroizing::new(String::new()),
        truncated: false,
    }
}

#[tokio::test]
async fn reports_branch_release_source_without_exposing_status() -> Result<(), MmError> {
    let process = Arc::new(FakeProcess {
        outputs: Mutex::new(vec![
            output(0, "origin/main\n"),
            output(0, "false\n"),
            output(0, "v26.4.0\nv26.5.0\n"),
            output(0, "v26.5.0\n"),
            output(0, "26.5.x\n"),
            output(0, " M src/main.rs\n"),
            output(0, "0123456789abcdef0123456789abcdef01234567\n"),
        ]),
    });
    let inspector = GitBuildSourceInspector::with_executable(process, "git");
    let source = inspector.inspect(Path::new("/project")).await?;
    assert_eq!(source.branch.as_deref(), Some("26.5.x"));
    assert_eq!(source.tags_at_head, vec!["v26.5.0"]);
    assert!(source.dirty);
    assert!(source.tags_complete);
    assert!(!source.default_branch);
    Ok(())
}
