use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::adapters::private_fs::{ensure_no_symlink_components, read_regular_file};
use crate::app::ports::TargetIngressArtifactStore;
use crate::domain::error::MmError;
use crate::domain::target_ingress::{CompiledTargetIngressArtifact, PreparedTargetIngressArtifact};

const MAX_INGRESS_ARTIFACT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct FilesystemTargetIngressArtifactStore {
    root: PathBuf,
}

impl FilesystemTargetIngressArtifactStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl TargetIngressArtifactStore for FilesystemTargetIngressArtifactStore {
    fn publish(
        &self,
        artifact: &CompiledTargetIngressArtifact,
    ) -> Result<PreparedTargetIngressArtifact, MmError> {
        if artifact.manifest.canonical_hash() != artifact.manifest.artifact_hash {
            return Err(MmError::Security {
                message: "target ingress artifact hash does not match its canonical manifest"
                    .to_owned(),
            });
        }
        if blake3::hash(&artifact.compose).to_hex().as_str() != artifact.manifest.compose_digest {
            return Err(MmError::Target {
                message: "target ingress Compose digest changed before publication".to_owned(),
            });
        }
        if artifact.support_files.len() != artifact.manifest.support_files.len() {
            return Err(MmError::Target {
                message: "target ingress support-file manifest is incomplete".to_owned(),
            });
        }
        let mut names = BTreeSet::new();
        for (file, manifest) in artifact
            .support_files
            .iter()
            .zip(&artifact.manifest.support_files)
        {
            validate_support_name(&file.manifest.name)?;
            if &file.manifest != manifest
                || !names.insert(file.manifest.name.clone())
                || blake3::hash(&file.bytes).to_hex().as_str() != file.manifest.digest
            {
                return Err(MmError::Target {
                    message: "target ingress support file changed before publication".to_owned(),
                });
            }
        }
        ensure_no_symlink_components(&self.root)?;
        create_directories(&self.root)?;
        let parent = self.root.join(artifact.manifest.target_id.0.to_string());
        ensure_no_symlink_components(&parent)?;
        create_directories(&parent)?;
        publish_artifact(&parent, artifact)
    }
}

fn publish_artifact(
    parent: &Path,
    artifact: &CompiledTargetIngressArtifact,
) -> Result<PreparedTargetIngressArtifact, MmError> {
    let mut files = vec![("compose.yaml".to_owned(), artifact.compose.as_slice())];
    files.extend(
        artifact
            .support_files
            .iter()
            .map(|file| (file.manifest.name.clone(), file.bytes.as_slice())),
    );
    let mut manifest_bytes =
        serde_json::to_vec_pretty(&artifact.manifest).map_err(|error| MmError::Target {
            message: format!("could not serialize target ingress manifest: {error}"),
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
                "creating target ingress staging directory {}",
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
                    context: format!(
                        "publishing target ingress artifact {}",
                        artifact_root.display()
                    ),
                    source,
                }),
            }
        })();
        let cleanup = if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|source| MmError::Io {
                context: format!("removing target ingress staging {}", staging.display()),
                source,
            })
        } else {
            Ok(())
        };
        match (result, cleanup) {
            (Ok(()), Ok(())) => {}
            (Err(primary), Ok(())) => return Err(primary),
            (Ok(()), Err(cleanup)) => return Err(cleanup),
            (Err(primary), Err(cleanup)) => {
                return Err(MmError::operation_failure(
                    artifact.manifest.artifact_hash.clone(),
                    crate::domain::error::MutationStatus::NotStarted,
                    primary,
                    vec![cleanup.to_string()],
                    "remove the content-addressed ingress staging directory before retrying",
                ));
            }
        }
    }
    Ok(PreparedTargetIngressArtifact {
        manifest_path: artifact_root.join("manifest.json"),
        compose_path: artifact_root.join("compose.yaml"),
        support_paths: artifact
            .manifest
            .support_files
            .iter()
            .map(|file| artifact_root.join(&file.name))
            .collect(),
        root: artifact_root,
        manifest: artifact.manifest.clone(),
    })
}

fn verify_artifact(root: &Path, files: &[(String, &[u8])], manifest: &[u8]) -> Result<(), MmError> {
    ensure_no_symlink_components(root)?;
    let actual = fs::read_dir(root)
        .map_err(|source| MmError::Io {
            context: format!("reading target ingress artifact {}", root.display()),
            source,
        })?
        .map(|entry| {
            entry
                .map_err(|source| MmError::Io {
                    context: format!("reading target ingress artifact {}", root.display()),
                    source,
                })?
                .file_name()
                .into_string()
                .map_err(|_| MmError::Security {
                    message: "target ingress artifact contains a non-Unicode file name".to_owned(),
                })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let mut expected = files
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<BTreeSet<_>>();
    expected.insert("manifest.json".to_owned());
    if actual != expected {
        return Err(unexpected_artifact(root));
    }
    for (name, contents) in files {
        verify_file(&root.join(name), contents)?;
    }
    verify_file(&root.join("manifest.json"), manifest)
}

fn validate_support_name(name: &str) -> Result<(), MmError> {
    let path = Path::new(name);
    if name.is_empty()
        || name == "compose.yaml"
        || name == "manifest.json"
        || name.starts_with('.')
        || path.components().count() != 1
        || !name
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'.' | b'_' | b'-'))
    {
        return Err(MmError::Security {
            message: format!("invalid target ingress support-file name '{name}'"),
        });
    }
    Ok(())
}

fn verify_file(path: &Path, contents: &[u8]) -> Result<(), MmError> {
    match read_regular_file(path, MAX_INGRESS_ARTIFACT_BYTES)? {
        Some(existing) if existing == contents => Ok(()),
        _ => Err(unexpected_artifact(path)),
    }
}

fn write_new(path: &Path, contents: &[u8]) -> Result<(), MmError> {
    if contents.len() > MAX_INGRESS_ARTIFACT_BYTES {
        return Err(MmError::Target {
            message: format!(
                "target ingress artifact exceeds the {MAX_INGRESS_ARTIFACT_BYTES}-byte safety limit"
            ),
        });
    }
    ensure_no_symlink_components(path)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| MmError::Io {
            context: format!("creating target ingress artifact {}", path.display()),
            source,
        })?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing target ingress artifact {}", path.display()),
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
            context: format!("opening target ingress artifact lock {}", path.display()),
            source,
        })
}

fn create_directories(path: &Path) -> Result<(), MmError> {
    fs::create_dir_all(path).map_err(|source| MmError::Io {
        context: format!(
            "creating target ingress artifact directory {}",
            path.display()
        ),
        source,
    })
}

pub(super) fn unexpected_artifact(path: &Path) -> MmError {
    MmError::Security {
        message: format!(
            "content-addressed target ingress artifact has unexpected contents: {}",
            path.display()
        ),
    }
}

#[cfg(test)]
#[path = "ingress_artifacts/tests.rs"]
mod tests;
