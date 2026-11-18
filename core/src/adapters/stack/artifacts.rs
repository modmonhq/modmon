use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::adapters::private_fs::{ensure_no_symlink_components, read_regular_file};
use crate::app::ports::StackArtifactStore;
use crate::domain::error::MmError;
use crate::domain::stack::{CompiledStackArtifact, PreparedStackArtifact};

const MAX_ARTIFACT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct FilesystemStackArtifactStore {
    root: PathBuf,
}

impl FilesystemStackArtifactStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl StackArtifactStore for FilesystemStackArtifactStore {
    fn publish(
        &self,
        artifacts: &[CompiledStackArtifact],
    ) -> Result<Vec<PreparedStackArtifact>, MmError> {
        ensure_no_symlink_components(&self.root)?;
        create_directories(&self.root)?;
        let mut prepared = Vec::with_capacity(artifacts.len());
        for artifact in artifacts {
            validate_component(&artifact.manifest.target, "target")?;
            validate_component(&artifact.manifest.environment, "environment")?;
            validate_component(&artifact.manifest.stack, "stack")?;
            validate_hash(&artifact.manifest.artifact_hash)?;
            let artifact_parent = self
                .root
                .join(artifact.manifest.target_id.0.to_string())
                .join(artifact.manifest.project_id.0.to_string())
                .join(&artifact.manifest.environment)
                .join(&artifact.manifest.stack);
            ensure_no_symlink_components(&artifact_parent)?;
            create_directories(&artifact_parent)?;
            // The overlay mounts these, so they are there before it is.
            for file in &artifact.config_files {
                super::config_files::publish(file)?;
            }
            prepared.push(publish_artifact(&artifact_parent, artifact)?);
        }
        Ok(prepared)
    }

    fn config_file_root(
        &self,
        target_id: crate::domain::target::TargetId,
        project_id: crate::domain::workspace::ProjectId,
        environment: &str,
    ) -> PathBuf {
        super::config_files::directory(
            &self
                .root
                .join(target_id.0.to_string())
                .join(project_id.0.to_string())
                .join(environment),
        )
    }

    fn prune_config_files(
        &self,
        target_id: crate::domain::target::TargetId,
        project_id: crate::domain::workspace::ProjectId,
        environment: &str,
        keep: &BTreeSet<String>,
    ) -> Result<Vec<String>, MmError> {
        validate_component(environment, "environment")?;
        super::config_files::prune(
            &self.config_file_root(target_id, project_id, environment),
            keep,
        )
    }
}

fn publish_artifact(
    parent: &Path,
    artifact: &CompiledStackArtifact,
) -> Result<PreparedStackArtifact, MmError> {
    let canonical_hash = artifact
        .manifest
        .canonical_hash()
        .map_err(|message| MmError::Environment { message })?;
    if canonical_hash != artifact.manifest.artifact_hash {
        return Err(MmError::Security {
            message: "stack artifact hash does not match its canonical manifest".to_owned(),
        });
    }
    if artifact.user_files.len() != artifact.manifest.user_files.len() {
        return Err(MmError::Environment {
            message: "compiled stack source manifest is incomplete".to_owned(),
        });
    }
    let mut files = Vec::<(String, &[u8])>::new();
    for (index, source) in artifact.user_files.iter().enumerate() {
        if source.manifest != artifact.manifest.user_files[index]
            || blake3::hash(&source.bytes).to_hex().as_str() != source.manifest.digest
        {
            return Err(MmError::Environment {
                message: "compiled stack source digest changed before publication".to_owned(),
            });
        }
        files.push((source_file_name(index), &source.bytes));
    }
    if blake3::hash(&artifact.generated_overlay).to_hex().as_str()
        != artifact.manifest.generated_overlay_digest
    {
        return Err(MmError::Environment {
            message: "generated stack overlay digest changed before publication".to_owned(),
        });
    }
    files.push(("mm.overlay.yaml".to_owned(), &artifact.generated_overlay));
    let mut manifest_bytes =
        serde_json::to_vec_pretty(&artifact.manifest).map_err(|error| MmError::Environment {
            message: format!("could not serialize stack artifact manifest: {error}"),
        })?;
    manifest_bytes.push(b'\n');

    let lock_path = parent.join(format!(".{}.lock", artifact.manifest.artifact_hash));
    let lock = artifact_lock(&lock_path)?;
    crate::adapters::file_lock::exclusive(&lock, &lock_path)?;
    let artifact_root = parent.join(&artifact.manifest.artifact_hash);
    if artifact_root.exists() {
        verify_artifact(&artifact_root, &files, &manifest_bytes)?;
    } else {
        let staging = parent.join(format!(
            ".{}.{}.tmp",
            artifact.manifest.artifact_hash,
            uuid::Uuid::new_v4()
        ));
        ensure_no_symlink_components(&staging)?;
        fs::create_dir(&staging).map_err(|source| MmError::Io {
            context: format!(
                "creating stack artifact staging directory {}",
                staging.display()
            ),
            source,
        })?;
        let result = (|| {
            for (name, contents) in &files {
                write_new(&staging.join(name), contents)?;
            }
            write_new(&staging.join("manifest.json"), &manifest_bytes)?;
            match fs::rename(&staging, &artifact_root) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    verify_artifact(&artifact_root, &files, &manifest_bytes)
                }
                Err(source) => Err(MmError::Io {
                    context: format!("publishing stack artifact {}", artifact_root.display()),
                    source,
                }),
            }
        })();
        let cleanup = if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|source| MmError::Io {
                context: format!("removing stack artifact staging {}", staging.display()),
                source,
            })
        } else {
            Ok(())
        };
        resolve_publication(result, cleanup, &artifact.manifest.artifact_hash)?;
    }

    Ok(prepared_artifact(artifact_root, artifact))
}

fn resolve_publication(
    result: Result<(), MmError>,
    cleanup: Result<(), MmError>,
    artifact_hash: &str,
) -> Result<(), MmError> {
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => Err(MmError::operation_failure(
            artifact_hash,
            crate::domain::error::MutationStatus::NotStarted,
            primary,
            vec![cleanup.to_string()],
            "remove the content-addressed stack staging directory before retrying",
        )),
    }
}

fn prepared_artifact(root: PathBuf, artifact: &CompiledStackArtifact) -> PreparedStackArtifact {
    let overlay = root.join("mm.overlay.yaml");
    let mut compose_files = (0..artifact.user_files.len())
        .map(|index| root.join(source_file_name(index)))
        .collect::<Vec<_>>();
    compose_files.push(overlay.clone());
    PreparedStackArtifact {
        manifest_path: root.join("manifest.json"),
        generated_overlay_path: overlay,
        compose_files,
        root,
        manifest: artifact.manifest.clone(),
    }
}

fn verify_artifact(root: &Path, files: &[(String, &[u8])], manifest: &[u8]) -> Result<(), MmError> {
    ensure_no_symlink_components(root)?;
    let mut expected = files
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<BTreeSet<_>>();
    expected.insert("manifest.json".to_owned());
    let actual = fs::read_dir(root)
        .map_err(|source| MmError::Io {
            context: format!("reading stack artifact {}", root.display()),
            source,
        })?
        .map(|entry| {
            entry
                .map_err(|source| MmError::Io {
                    context: format!("reading stack artifact {}", root.display()),
                    source,
                })?
                .file_name()
                .into_string()
                .map_err(|_| MmError::Security {
                    message: "stack artifact contains a non-Unicode file name".to_owned(),
                })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if actual != expected {
        return Err(unexpected_artifact(root));
    }
    for (name, contents) in files {
        verify_file(&root.join(name), contents)?;
    }
    verify_file(&root.join("manifest.json"), manifest)
}

fn verify_file(path: &Path, contents: &[u8]) -> Result<(), MmError> {
    match read_regular_file(path, MAX_ARTIFACT_BYTES)? {
        Some(existing) if existing == contents => Ok(()),
        _ => Err(unexpected_artifact(path)),
    }
}

fn write_new(path: &Path, contents: &[u8]) -> Result<(), MmError> {
    if contents.len() > MAX_ARTIFACT_BYTES {
        return Err(MmError::Environment {
            message: format!("stack artifact exceeds the {MAX_ARTIFACT_BYTES}-byte safety limit"),
        });
    }
    ensure_no_symlink_components(path)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| MmError::Io {
            context: format!("creating stack artifact {}", path.display()),
            source,
        })?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing stack artifact {}", path.display()),
            source,
        })
}

fn artifact_lock(path: &Path) -> Result<File, MmError> {
    ensure_no_symlink_components(path)?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|source| MmError::Io {
            context: format!("opening stack artifact lock {}", path.display()),
            source,
        })
}

fn unexpected_artifact(path: &Path) -> MmError {
    MmError::Security {
        message: format!(
            "content-addressed stack artifact has unexpected contents: {}",
            path.display()
        ),
    }
}

fn source_file_name(index: usize) -> String {
    format!("user.{index:03}.compose.yaml")
}

fn create_directories(path: &Path) -> Result<(), MmError> {
    fs::create_dir_all(path).map_err(|source| MmError::Io {
        context: format!("creating stack artifact directory {}", path.display()),
        source,
    })
}

fn validate_component(value: &str, kind: &str) -> Result<(), MmError> {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        Ok(())
    } else {
        Err(MmError::Security {
            message: format!("refusing unsafe stack artifact {kind} name"),
        })
    }
}

fn validate_hash(value: &str) -> Result<(), MmError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(MmError::Security {
            message: "refusing invalid stack artifact hash".to_owned(),
        })
    }
}

#[cfg(test)]
#[path = "artifacts/tests.rs"]
mod tests;
