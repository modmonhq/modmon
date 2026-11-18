use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::app::ports::{BuildSourceInspector, CommandOutput, CommandSpec, ProcessRunner};
use crate::domain::build::BuildSource;
use crate::domain::error::MmError;

#[derive(Clone)]
pub struct GitBuildSourceInspector {
    process: Arc<dyn ProcessRunner>,
    executable: String,
}

impl std::fmt::Debug for GitBuildSourceInspector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GitBuildSourceInspector")
            .field("executable", &self.executable)
            .finish_non_exhaustive()
    }
}

impl GitBuildSourceInspector {
    #[must_use]
    pub fn new(process: Arc<dyn ProcessRunner>) -> Self {
        Self {
            process,
            executable: "git".to_owned(),
        }
    }

    #[cfg(test)]
    fn with_executable(process: Arc<dyn ProcessRunner>, executable: impl Into<String>) -> Self {
        Self {
            process,
            executable: executable.into(),
        }
    }

    async fn required(&self, root: &Path, args: &[&str]) -> Result<String, MmError> {
        let output = self.command(root, args).await?;
        if !output.success() {
            return Err(git_failed(&self.executable, output));
        }
        checked_output(output, "git")
    }

    async fn optional(&self, root: &Path, args: &[&str]) -> Result<Option<String>, MmError> {
        let output = self.command(root, args).await?;
        if output.success() {
            return checked_output(output, "git").map(Some);
        }
        if output.code == Some(1) {
            return Ok(None);
        }
        Err(git_failed(&self.executable, output))
    }

    async fn command(&self, root: &Path, args: &[&str]) -> Result<CommandOutput, MmError> {
        self.process
            .run(
                CommandSpec::new(&self.executable)
                    .args(args.iter().copied())
                    .current_dir(root)
                    .environment("LC_ALL", "C")
                    .timeout(Duration::from_secs(30)),
            )
            .await
    }
}

#[async_trait]
impl BuildSourceInspector for GitBuildSourceInspector {
    async fn inspect(&self, project_root: &Path) -> Result<BuildSource, MmError> {
        let commit = self
            .required(project_root, &["rev-parse", "--verify", "HEAD^{commit}"])
            .await?
            .trim()
            .to_owned();
        let status = self
            .required(
                project_root,
                &["status", "--porcelain=v1", "--untracked-files=normal"],
            )
            .await?;
        let branch = self
            .required(project_root, &["branch", "--show-current"])
            .await?
            .trim()
            .to_owned();
        let branch = (!branch.is_empty()).then_some(branch);
        let tags_at_head = checked_lines(
            &self
                .required(project_root, &["tag", "--points-at", "HEAD"])
                .await?,
        )?;
        let repository_tags =
            checked_lines(&self.required(project_root, &["tag", "--list"]).await?)?;
        let shallow = self
            .required(project_root, &["rev-parse", "--is-shallow-repository"])
            .await?;
        let tags_complete = match shallow.trim() {
            "true" => false,
            "false" => true,
            _ => {
                return Err(MmError::Build {
                    message: "git returned an invalid shallow-repository status".to_owned(),
                });
            }
        };
        let remote_default = self
            .optional(
                project_root,
                &[
                    "symbolic-ref",
                    "--quiet",
                    "--short",
                    "refs/remotes/origin/HEAD",
                ],
            )
            .await?
            .map(|value| value.trim().trim_start_matches("origin/").to_owned());
        let default_branch = branch.as_deref().is_some_and(|branch| {
            remote_default
                .as_deref()
                .map_or(matches!(branch, "main" | "master"), |default| {
                    branch == default
                })
        });

        Ok(BuildSource {
            commit,
            branch,
            tags_at_head,
            repository_tags,
            tags_complete,
            default_branch,
            dirty: !status.is_empty(),
        })
    }
}

fn checked_output(output: CommandOutput, program: &str) -> Result<String, MmError> {
    if output.truncated {
        return Err(MmError::Build {
            message: format!("{program} output exceeded the safety limit"),
        });
    }
    Ok(output.stdout.to_string())
}

fn checked_lines(output: &str) -> Result<Vec<String>, MmError> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            if line.len() > 256 || line.chars().any(char::is_control) {
                Err(MmError::Build {
                    message: "git returned an unsafe branch or tag name".to_owned(),
                })
            } else {
                Ok(line.to_owned())
            }
        })
        .collect()
}

fn git_failed(executable: &str, output: CommandOutput) -> MmError {
    MmError::ProcessFailed {
        program: executable.to_owned(),
        code: output.code,
        stderr: output.stderr.trim().to_owned(),
    }
}

#[cfg(test)]
#[path = "git/tests.rs"]
mod tests;
