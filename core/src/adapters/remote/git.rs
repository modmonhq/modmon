//! The daemon's own copy of each project's repository. `mmd` fetches commits
//! from the origin pinned in the host policy. Git runs with no system or
//! global config and no hooks. A project the host policy has delivered by
//! push is never fetched: its commits arrive as bundles (`bundle.rs`), and
//! a project that is fetched never takes one.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::adapters::process::TokioProcessRunner;
use crate::app::ports::{CommandOutput, CommandSpec, ProcessRunner};
use crate::domain::error::MmError;
use crate::domain::host_policy::EnrolledProject;
use crate::domain::remote::is_commit_sha;
use crate::domain::workspace::ProjectId;

/// The file at the root of every checkout that records its full commit.
/// It is not part of the commit, so build contexts leave it out.
pub const CHECKOUT_MARKER: &str = ".mm-commit";

/// A fetch and an export of a commit talk to the origin or copy a tree.
/// The process is ended, with everything it started, when this passes.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(300);
/// Every other git command only reads the mirror.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(60);
/// The list of a large tree's files is what git prints most of here.
const OUTPUT_LIMIT: usize = 64 * 1024 * 1024;

/// Mirrors and checkouts for one daemon.
#[derive(Debug, Clone)]
pub struct ProjectRepositories {
    state_root: PathBuf,
    projects_root: PathBuf,
    /// The largest bundle a client may push.
    pub(super) bundle_limit: u64,
}

impl ProjectRepositories {
    #[must_use]
    pub fn new(state_root: PathBuf, projects_root: PathBuf) -> Self {
        Self {
            state_root,
            projects_root,
            bundle_limit: crate::domain::delivery::MAX_BUNDLE_BYTES,
        }
    }

    /// The same store with another limit for a pushed bundle.
    #[must_use]
    pub const fn with_bundle_limit(mut self, bytes: u64) -> Self {
        self.bundle_limit = bytes;
        self
    }

    pub(super) fn project_state(&self, project: ProjectId) -> PathBuf {
        self.state_root.join("projects").join(project.0.to_string())
    }

    fn mirror(&self, project: ProjectId) -> PathBuf {
        self.project_state(project).join("repo.git")
    }

    /// `<projects_root>/<id>/<env>/<sha12>`: one immutable tree per commit.
    #[must_use]
    pub fn checkout_path(&self, project: ProjectId, environment: &str, commit: &str) -> PathBuf {
        self.projects_root
            .join(project.0.to_string())
            .join(environment)
            .join(commit.get(..12).unwrap_or(commit))
    }

    /// The environment's complete checkouts, newest first. Each passed the
    /// environment's deploy-from rule when it was made.
    #[must_use]
    pub fn checkouts(&self, project: ProjectId, environment: &str) -> Vec<PathBuf> {
        checkouts_in(
            &self
                .projects_root
                .join(project.0.to_string())
                .join(environment),
        )
    }

    /// Fetches every branch of the origin into the bare mirror.
    pub async fn fetch(
        &self,
        project: ProjectId,
        enrolled: &EnrolledProject,
    ) -> Result<(), MmError> {
        let mirror = self.mirror(project);
        if !mirror.join("HEAD").is_file() {
            std::fs::create_dir_all(&mirror)
                .map_err(|source| io("creating the project mirror", &mirror, source))?;
            self.git(project, enrolled, &mirror, &["init", "--bare", "--quiet"])
                .await?;
        }
        let refspec = "+refs/heads/*:refs/remotes/origin/*";
        let fetch = self
            .command(project, enrolled, &mirror)
            .args_append([
                "fetch",
                "--quiet",
                "--prune",
                "--no-tags",
                &enrolled.origin,
                refspec,
            ])
            .timeout(TRANSFER_TIMEOUT);
        match TokioProcessRunner.run(fetch).await {
            Ok(output) => succeeded("fetch", &output).map(drop),
            Err(MmError::ProcessTimeout { .. }) => Err(MmError::RuntimeResponse {
                message: format!("fetching {} timed out", enrolled.origin),
            }),
            Err(error) => Err(error),
        }
    }

    /// Checks that `commit` exists on the origin and, when `branches` is set,
    /// is reachable from one that matches (see [`crate::domain::branch_pattern`]).
    pub async fn check_reachable(
        &self,
        project: ProjectId,
        enrolled: &EnrolledProject,
        commit: &str,
        branches: Option<&[String]>,
    ) -> Result<(), MmError> {
        if !is_commit_sha(commit) {
            return Err(MmError::Environment {
                message: format!("'{commit}' is not a full 40-character commit SHA"),
            });
        }
        let mirror = self.mirror(project);
        let on_origin = self
            .git(
                project,
                enrolled,
                &mirror,
                &[
                    "branch",
                    "--remotes",
                    "--contains",
                    commit,
                    "--format=%(refname)",
                ],
            )
            .await
            .map_err(|_| MmError::Environment {
                message: format!(
                    "commit {commit} is not on {}; push it first",
                    enrolled.origin
                ),
            })?;
        let containing = on_origin
            .lines()
            .filter_map(|line| line.trim().strip_prefix("refs/remotes/origin/"))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if containing.is_empty() {
            return Err(MmError::Environment {
                message: format!(
                    "commit {commit} is not on any branch of {}",
                    enrolled.origin
                ),
            });
        }
        let Some(branches) = branches else {
            return Ok(());
        };
        if containing
            .iter()
            .any(|branch| crate::domain::branch_pattern::allows(branches, branch))
        {
            return Ok(());
        }
        Err(MmError::Security {
            message: format!(
                "commit {commit} is not on an allowed branch ({}); it is on {}",
                branches.join(", "),
                containing.join(", ")
            ),
        })
    }

    /// Whether `ancestor` is `commit` or one of its ancestors in the mirror.
    /// Fails when either commit is unknown to the mirror.
    pub async fn is_ancestor(
        &self,
        project: ProjectId,
        enrolled: &EnrolledProject,
        ancestor: &str,
        commit: &str,
    ) -> Result<bool, MmError> {
        let mirror = self.mirror(project);
        let related = self.command(project, enrolled, &mirror).args_append([
            "merge-base",
            "--is-ancestor",
            ancestor,
            commit,
        ]);
        let output = TokioProcessRunner.run(related).await?;
        match output.code {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(MmError::RuntimeResponse {
                message: format!(
                    "git merge-base failed: {}",
                    crate::domain::error::printed(output.stderr.trim())
                ),
            }),
        }
    }

    /// Exports `commit` into its immutable checkout, reusing an existing one.
    pub async fn checkout(
        &self,
        project: ProjectId,
        enrolled: &EnrolledProject,
        environment: &str,
        commit: &str,
    ) -> Result<PathBuf, MmError> {
        let target = self.checkout_path(project, environment, commit);
        if target.join(CHECKOUT_MARKER).is_file() {
            let recorded = std::fs::read_to_string(target.join(CHECKOUT_MARKER))
                .map_err(|source| io("reading the checkout marker", &target, source))?;
            if recorded.trim() == commit {
                return Ok(target);
            }
            return Err(MmError::Security {
                message: format!("{} belongs to another commit", target.display()),
            });
        }
        let mirror = self.mirror(project);
        let tree = self
            .git(
                project,
                enrolled,
                &mirror,
                &["ls-tree", "-r", "--name-only", commit],
            )
            .await?;
        if tree.lines().any(|path| path == ".gitmodules") {
            return Err(MmError::Environment {
                message: "git submodules are not supported for remote deployments".to_owned(),
            });
        }
        if tree.lines().any(|path| path == ".gitattributes") {
            let attributes = self
                .git(
                    project,
                    enrolled,
                    &mirror,
                    &["show", &format!("{commit}:.gitattributes")],
                )
                .await?;
            if attributes.contains("filter=lfs") {
                return Err(MmError::Environment {
                    message: "Git LFS files are not supported for remote deployments".to_owned(),
                });
            }
        }
        let parent = target.parent().ok_or_else(|| MmError::State {
            message: "checkout has no parent directory".to_owned(),
        })?;
        std::fs::create_dir_all(parent)
            .map_err(|source| io("creating the checkout directory", parent, source))?;
        let staging = parent.join(format!(".staging-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&staging)
            .map_err(|source| io("creating the checkout staging directory", &staging, source))?;
        let result = self
            .export(project, enrolled, &mirror, commit, &staging)
            .await;
        // Local overrides are a checkout's private layer; a server deploys the
        // shared configuration only.
        let result = result.and_then(|()| {
            let overrides = staging.join(crate::adapters::config::OVERRIDES_RELATIVE_PATH);
            match std::fs::remove_file(&overrides) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(source) => Err(io("removing committed local overrides", &overrides, source)),
            }
        });
        let result = match result {
            Ok(()) => std::fs::write(staging.join(CHECKOUT_MARKER), format!("{commit}\n"))
                .map_err(|source| io("writing the checkout marker", &staging, source)),
            Err(error) => Err(error),
        };
        let result = match result {
            Ok(()) => std::fs::rename(&staging, &target)
                .map_err(|source| io("publishing the checkout", &target, source)),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            if let Err(cleanup) = std::fs::remove_dir_all(&staging)
                && cleanup.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%cleanup, path = %staging.display(), "could not remove checkout staging");
            }
            return Err(error);
        }
        Ok(target)
    }

    async fn export(
        &self,
        project: ProjectId,
        enrolled: &EnrolledProject,
        mirror: &Path,
        commit: &str,
        destination: &Path,
    ) -> Result<(), MmError> {
        // git archive applies export-ignore and export-subst from the tree's
        // .gitattributes, so a checkout would differ from the commit CI built
        // and receipts could not be promoted. The mirror's info/attributes
        // takes precedence and turns both off.
        let info = mirror.join("info");
        std::fs::create_dir_all(&info)
            .and_then(|()| {
                std::fs::write(info.join("attributes"), "* -export-ignore -export-subst\n")
            })
            .map_err(|source| io("writing the mirror's archive attributes", mirror, source))?;
        // Into a file next to the checkout, then unpacked in-process rather
        // than with the system tar: GNU tar 1.35 resolves nested paths with
        // openat2(RESOLVE_BENEATH), which mmd's RestrictSUIDSGID seccomp
        // filter answers with ENOSYS.
        let tarball = destination.with_extension("tar");
        let archive = self
            .command(project, enrolled, mirror)
            .args_append(["archive", "--format=tar"])
            .arg(format!("--output={}", tarball.display()))
            .arg(commit)
            .timeout(TRANSFER_TIMEOUT);
        let archived = TokioProcessRunner.run(archive).await;
        let extracted = match archived {
            Ok(output) if output.success() => {
                let (source, target) = (tarball.clone(), destination.to_path_buf());
                tokio::task::spawn_blocking(move || {
                    unpack_checkout(std::fs::File::open(source)?, &target)
                })
                .await
                .map_err(std::io::Error::other)
                .and_then(|unpacked| unpacked)
                .map_err(|error| error.to_string())
            }
            Ok(output) => Err(crate::domain::error::printed(output.stderr.trim())),
            Err(error) => Err(error.to_string()),
        };
        if let Err(cleanup) = std::fs::remove_file(&tarball)
            && cleanup.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%cleanup, path = %tarball.display(), "could not remove the exported archive");
        }
        extracted.map_err(|reason| MmError::RuntimeResponse {
            message: format!("could not export commit {commit}: {reason}"),
        })
    }

    /// A git command on the mirror, with no system or global configuration
    /// and no hooks. It goes through the one place that starts child
    /// processes, so it is ended with everything it started when its
    /// deadline passes or nobody waits for it any more.
    fn command(
        &self,
        project: ProjectId,
        enrolled: &EnrolledProject,
        mirror: &Path,
    ) -> CommandSpec {
        let state = self.project_state(project);
        let mut command = CommandSpec::new("git")
            .environment("PATH", "/usr/local/bin:/usr/bin:/bin")
            .environment("HOME", state.display().to_string())
            .environment("GIT_CONFIG_NOSYSTEM", "1")
            .environment("GIT_CONFIG_GLOBAL", "/dev/null")
            .environment("GIT_TERMINAL_PROMPT", "0")
            .environment("GIT_DIR", mirror.display().to_string())
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "protocol.allow=never",
                "-c",
                "protocol.https.allow=always",
                "-c",
                "protocol.ssh.allow=always",
            ])
            .timeout(LOCAL_TIMEOUT)
            .with_output_limit(OUTPUT_LIMIT);
        // Root wrote a local origin into the host policy; allow it for the
        // top-level fetch only (there are no submodules).
        if enrolled.origin.starts_with('/') || enrolled.origin.starts_with("file://") {
            command = command.args_append(["-c", "protocol.file.allow=always"]);
        }
        let key = state.join("deploy-key");
        let mut ssh = String::from("ssh -F /dev/null -o BatchMode=yes -o IdentitiesOnly=yes");
        if key.is_file() {
            ssh.push_str(&format!(" -i {}", key.display()));
        }
        let known_hosts = state.join("origin_known_hosts");
        if let Some(host_key) = &enrolled.origin_host_key {
            let host = origin_host(&enrolled.origin).unwrap_or_default();
            if std::fs::write(&known_hosts, format!("{host} {host_key}\n")).is_ok() {
                ssh.push_str(&format!(
                    " -o StrictHostKeyChecking=yes -o UserKnownHostsFile={}",
                    known_hosts.display()
                ));
            }
        }
        command.environment("GIT_SSH_COMMAND", ssh)
    }

    async fn git(
        &self,
        project: ProjectId,
        enrolled: &EnrolledProject,
        mirror: &Path,
        arguments: &[&str],
    ) -> Result<String, MmError> {
        let command = self
            .command(project, enrolled, mirror)
            .args_append(arguments.iter().copied());
        let output = TokioProcessRunner.run(command).await?;
        succeeded(arguments.first().copied().unwrap_or_default(), &output)
    }
}

/// The output of `git <subcommand>`, or why it failed.
fn succeeded(subcommand: &str, output: &CommandOutput) -> Result<String, MmError> {
    if output.truncated {
        return Err(MmError::RuntimeResponse {
            message: format!("git {subcommand} printed more than this host reads"),
        });
    }
    if !output.success() {
        return Err(MmError::RuntimeResponse {
            message: format!(
                "git {subcommand} failed: {}",
                crate::domain::error::printed(output.stderr.trim())
            ),
        });
    }
    Ok(output.stdout.to_string())
}

/// The host part of `user@host:path` or `ssh://user@host[:port]/path`.
fn origin_host(origin: &str) -> Option<String> {
    if let Some(rest) = origin.strip_prefix("ssh://") {
        let authority = rest.split('/').next()?;
        let host = authority.rsplit('@').next()?;
        return Some(match host.split_once(':') {
            Some((host, port)) => format!("[{host}]:{port}"),
            None => host.to_owned(),
        });
    }
    let (authority, _) = origin.split_once(':')?;
    Some(authority.rsplit('@').next()?.to_owned())
}

/// Unpack a `git archive` tar stream into `destination`. Entries cannot escape
/// it, ownership and set-id bits are never applied, and the process umask
/// decides file modes, as `tar --no-same-owner --no-same-permissions` did.
fn unpack_checkout(reader: impl std::io::Read, destination: &Path) -> std::io::Result<()> {
    let mut archive = tar::Archive::new(reader);
    archive.set_preserve_permissions(false);
    archive.set_preserve_ownerships(false);
    archive.set_unpack_xattrs(false);
    archive.set_overwrite(false);
    archive.unpack(destination)
}

fn io(context: &str, path: &Path, source: std::io::Error) -> MmError {
    MmError::Io {
        context: format!("{context} {}", path.display()),
        source,
    }
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;

/// The complete checkouts in an environment's directory, newest first.
#[must_use]
pub fn checkouts_in(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let marker = entry.path().join(CHECKOUT_MARKER);
            let modified = std::fs::metadata(&marker)
                .and_then(|meta| meta.modified())
                .ok()?;
            Some((modified, entry.path()))
        })
        .collect::<Vec<_>>();
    found.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    found.into_iter().map(|(_, path)| path).collect()
}
