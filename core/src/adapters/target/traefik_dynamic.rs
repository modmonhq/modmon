use std::path::{Path, PathBuf};

use super::ingress_artifacts::unexpected_artifact;
use crate::app::ports::TraefikDynamicConfigStore;
use crate::domain::error::MmError;
use crate::domain::target::TargetId;
use crate::domain::traefik_dynamic::{PreparedTraefikDynamicConfig, RenderedTraefikDynamicConfig};

const DYNAMIC_CONFIG_FILE: &str = "dynamic.yaml";
const MAX_DYNAMIC_CONFIG_BYTES: usize = 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct FilesystemTraefikDynamicConfigStore {
    root: PathBuf,
}

impl FilesystemTraefikDynamicConfigStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl TraefikDynamicConfigStore for FilesystemTraefikDynamicConfigStore {
    fn publish(
        &self,
        expected_target_id: TargetId,
        config: &RenderedTraefikDynamicConfig,
    ) -> Result<PreparedTraefikDynamicConfig, MmError> {
        validate_config(expected_target_id, config)?;
        let manifest = serialize_manifest(config)?;
        publish_platform(&self.root, config, &manifest)
    }
}

fn validate_config(
    expected_target_id: TargetId,
    config: &RenderedTraefikDynamicConfig,
) -> Result<(), MmError> {
    if config.manifest.target_id != expected_target_id {
        return Err(MmError::Security {
            message: format!(
                "Traefik dynamic configuration belongs to target {}, not expected target {}",
                config.manifest.target_id.0, expected_target_id.0
            ),
        });
    }
    if config.bytes.len() > MAX_DYNAMIC_CONFIG_BYTES {
        return Err(MmError::Target {
            message: format!(
                "Traefik dynamic configuration exceeds the {MAX_DYNAMIC_CONFIG_BYTES}-byte safety limit"
            ),
        });
    }
    let digest = blake3::hash(&config.bytes).to_hex().to_string();
    if config.manifest.digest != digest {
        return Err(MmError::Security {
            message: "Traefik dynamic configuration digest changed before publication".to_owned(),
        });
    }
    Ok(())
}

fn serialize_manifest(config: &RenderedTraefikDynamicConfig) -> Result<Vec<u8>, MmError> {
    let mut manifest =
        serde_json::to_vec_pretty(&config.manifest).map_err(|error| MmError::Target {
            message: format!("could not serialize Traefik dynamic configuration manifest: {error}"),
        })?;
    manifest.push(b'\n');
    if manifest.len() > MAX_MANIFEST_BYTES {
        return Err(MmError::Target {
            message: format!(
                "Traefik dynamic configuration manifest exceeds the {MAX_MANIFEST_BYTES}-byte safety limit"
            ),
        });
    }
    Ok(manifest)
}

#[cfg(unix)]
fn publish_platform(
    root: &Path,
    config: &RenderedTraefikDynamicConfig,
    manifest: &[u8],
) -> Result<PreparedTraefikDynamicConfig, MmError> {
    unix::publish(root, config, manifest)
}

#[cfg(not(unix))]
fn publish_platform(
    _root: &Path,
    _config: &RenderedTraefikDynamicConfig,
    _manifest: &[u8],
) -> Result<PreparedTraefikDynamicConfig, MmError> {
    crate::adapters::private_fs::ensure_private_permissions_supported()?;
    Err(MmError::Security {
        message: "symlink-safe Traefik dynamic configuration publication is not implemented on this platform"
            .to_owned(),
    })
}

#[cfg(unix)]
mod unix {
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io::{Read, Write};
    use std::path::{Component, Path};

    use rustix::fs::{AtFlags, FileType, Mode, OFlags, Stat};

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple",
        target_os = "redox"
    ))]
    use rustix::fs::RenameFlags;

    use super::{
        DYNAMIC_CONFIG_FILE, MAX_DYNAMIC_CONFIG_BYTES, MAX_MANIFEST_BYTES, unexpected_artifact,
    };
    use crate::adapters::private_fs::{ensure_no_symlink_components, ensure_safe_path_ancestors};
    use crate::domain::error::MmError;
    use crate::domain::traefik_dynamic::{
        PreparedTraefikDynamicConfig, RenderedTraefikDynamicConfig, TraefikDynamicConfigManifest,
    };

    const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);

    pub(super) fn publish(
        root: &Path,
        config: &RenderedTraefikDynamicConfig,
        manifest: &[u8],
    ) -> Result<PreparedTraefikDynamicConfig, MmError> {
        let root_directory = open_private_root(root)?;
        let target_name = config.manifest.target_id.0.to_string();
        let target_root = root.join(&target_name);
        let target_directory =
            open_or_create_private_directory(&root_directory, &target_name, &target_root)?;
        let routes_root = target_root.join("routes");
        let routes_directory =
            open_or_create_private_directory(&target_directory, "routes", &routes_root)?;

        let digest = &config.manifest.digest;
        let artifact_root = routes_root.join(digest);
        let lock_name = format!(".{digest}.lock");
        let lock_path = routes_root.join(&lock_name);
        let lock = open_lock(&routes_directory, &lock_name, &lock_path)?;
        crate::adapters::file_lock::exclusive(&lock, &lock_path)?;
        verify_lock(&routes_directory, &lock_name, &lock, &lock_path)?;

        let publication =
            publish_locked(&routes_directory, digest, &artifact_root, config, manifest);
        verify_lock(&routes_directory, &lock_name, &lock, &lock_path)?;
        let artifact_directory = publication?;

        verify_directory_entry(
            &routes_directory,
            digest,
            &artifact_directory,
            &artifact_root,
        )?;
        verify_directory_entry(&target_directory, "routes", &routes_directory, &routes_root)?;
        verify_directory_entry(
            &root_directory,
            &target_name,
            &target_directory,
            &target_root,
        )?;
        verify_root_identity(root, &root_directory)?;

        Ok(PreparedTraefikDynamicConfig {
            path: artifact_root.join(DYNAMIC_CONFIG_FILE),
            root: artifact_root,
            manifest: config.manifest.clone(),
        })
    }

    pub(super) fn publish_locked(
        routes_directory: &File,
        digest: &str,
        artifact_root: &Path,
        config: &RenderedTraefikDynamicConfig,
        manifest: &[u8],
    ) -> Result<File, MmError> {
        if let Some(directory) = try_open_directory(routes_directory, digest, artifact_root)? {
            verify_artifact(&directory, artifact_root, config, manifest)?;
            verify_directory_entry(routes_directory, digest, &directory, artifact_root)?;
            return Ok(directory);
        }

        let staging_name = format!(".{digest}.{}.tmp", uuid::Uuid::new_v4());
        let staging_path = artifact_root
            .parent()
            .ok_or_else(|| MmError::Security {
                message: "Traefik dynamic configuration artifact path has no parent".to_owned(),
            })?
            .join(&staging_name);
        create_private_directory(routes_directory, &staging_name, &staging_path)?;
        let staging_directory = try_open_directory(routes_directory, &staging_name, &staging_path)?
            .ok_or_else(|| publication_race(&staging_path))?;
        validate_private_directory(&staging_directory, &staging_path)?;
        verify_directory_entry(
            routes_directory,
            &staging_name,
            &staging_directory,
            &staging_path,
        )?;

        let result = (|| {
            write_new_file(
                &staging_directory,
                DYNAMIC_CONFIG_FILE,
                &staging_path.join(DYNAMIC_CONFIG_FILE),
                &config.bytes,
            )?;
            write_new_file(
                &staging_directory,
                "manifest.json",
                &staging_path.join("manifest.json"),
                manifest,
            )?;
            rustix::fs::fsync(&staging_directory).map_err(|error| MmError::Io {
                context: format!(
                    "synchronizing Traefik dynamic configuration staging directory {}",
                    staging_path.display()
                ),
                source: error.into(),
            })?;
            verify_directory_entry(
                routes_directory,
                &staging_name,
                &staging_directory,
                &staging_path,
            )?;

            match rename_no_replace(routes_directory, &staging_name, digest) {
                Ok(()) => {}
                Err(error)
                    if error == rustix::io::Errno::EXIST
                        || error == rustix::io::Errno::NOTEMPTY =>
                {
                    return Err(publication_race(artifact_root));
                }
                Err(error) => {
                    return Err(MmError::Io {
                        context: format!(
                            "publishing Traefik dynamic configuration artifact {}",
                            artifact_root.display()
                        ),
                        source: error.into(),
                    });
                }
            }
            rustix::fs::fsync(routes_directory).map_err(|error| MmError::Io {
                context: format!(
                    "synchronizing Traefik dynamic configuration artifact parent {}",
                    artifact_root.parent().unwrap_or(artifact_root).display()
                ),
                source: error.into(),
            })?;
            let directory = try_open_directory(routes_directory, digest, artifact_root)?
                .ok_or_else(|| publication_race(artifact_root))?;
            if !same_file(&staging_directory, &directory)? {
                return Err(publication_race(artifact_root));
            }
            verify_artifact(&directory, artifact_root, config, manifest)?;
            verify_directory_entry(routes_directory, digest, &directory, artifact_root)?;
            Ok(directory)
        })();

        if result.is_err() {
            cleanup_staging(routes_directory, &staging_name, &staging_directory);
        }
        result
    }

    pub(super) fn verify_artifact(
        directory: &File,
        root: &Path,
        config: &RenderedTraefikDynamicConfig,
        expected_manifest_bytes: &[u8],
    ) -> Result<(), MmError> {
        validate_private_directory(directory, root)?;
        verify_exact_entries(directory, root)?;
        let dynamic = read_bounded_file(
            directory,
            DYNAMIC_CONFIG_FILE,
            &root.join(DYNAMIC_CONFIG_FILE),
            MAX_DYNAMIC_CONFIG_BYTES,
        )?;
        if dynamic != config.bytes {
            return Err(unexpected_artifact(root));
        }

        let manifest = read_bounded_file(
            directory,
            "manifest.json",
            &root.join("manifest.json"),
            MAX_MANIFEST_BYTES,
        )?;
        let parsed: TraefikDynamicConfigManifest =
            serde_json::from_slice(&manifest).map_err(|_| unexpected_artifact(root))?;
        if parsed.target_id != config.manifest.target_id
            || parsed.digest != config.manifest.digest
            || parsed != config.manifest
            || manifest != expected_manifest_bytes
        {
            return Err(unexpected_artifact(root));
        }
        verify_exact_entries(directory, root)
    }

    pub(super) fn verify_exact_entries(directory: &File, root: &Path) -> Result<(), MmError> {
        let entries = rustix::fs::Dir::read_from(directory).map_err(|error| MmError::Io {
            context: format!(
                "opening Traefik dynamic configuration artifact directory {}",
                root.display()
            ),
            source: error.into(),
        })?;
        let mut dynamic = false;
        let mut manifest = false;
        let mut count = 0_usize;
        for entry in entries {
            let entry = entry.map_err(|error| MmError::Io {
                context: format!(
                    "reading Traefik dynamic configuration artifact directory {}",
                    root.display()
                ),
                source: error.into(),
            })?;
            let name = entry.file_name().to_bytes();
            if matches!(name, b"." | b"..") {
                continue;
            }
            count += 1;
            if count > 2 {
                return Err(unexpected_artifact(root));
            }
            match name {
                b"dynamic.yaml" if !dynamic => dynamic = true,
                b"manifest.json" if !manifest => manifest = true,
                _ => return Err(unexpected_artifact(root)),
            }
        }
        if dynamic && manifest {
            Ok(())
        } else {
            Err(unexpected_artifact(root))
        }
    }

    pub(super) fn read_bounded_file(
        directory: &File,
        name: &str,
        path: &Path,
        maximum_bytes: usize,
    ) -> Result<Vec<u8>, MmError> {
        let descriptor = match rustix::fs::openat(
            directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(descriptor) => descriptor,
            Err(error)
                if error == rustix::io::Errno::NOENT
                    || error == rustix::io::Errno::LOOP
                    || error == rustix::io::Errno::NOTDIR =>
            {
                return Err(unexpected_artifact(path));
            }
            Err(error) => {
                return Err(MmError::Io {
                    context: format!(
                        "opening Traefik dynamic configuration artifact {}",
                        path.display()
                    ),
                    source: error.into(),
                });
            }
        };
        let file = File::from(descriptor);
        let before = validate_regular_file(&file, path)?;
        let size = u64::try_from(before.st_size).map_err(|_| unexpected_artifact(path))?;
        let maximum = u64::try_from(maximum_bytes).unwrap_or(u64::MAX);
        if size > maximum {
            return Err(unexpected_artifact(path));
        }
        let mut contents = Vec::with_capacity(
            usize::try_from(size)
                .unwrap_or(maximum_bytes)
                .min(maximum_bytes),
        );
        (&file)
            .take(maximum.saturating_add(1))
            .read_to_end(&mut contents)
            .map_err(|source| MmError::Io {
                context: format!(
                    "reading Traefik dynamic configuration artifact {}",
                    path.display()
                ),
                source,
            })?;
        if contents.len() > maximum_bytes {
            return Err(unexpected_artifact(path));
        }
        let after = validate_regular_file(&file, path)?;
        if before.st_dev != after.st_dev
            || before.st_ino != after.st_ino
            || before.st_size != after.st_size
        {
            return Err(publication_race(path));
        }
        verify_file_entry(directory, name, &file, path)?;
        Ok(contents)
    }

    pub(super) fn write_new_file(
        directory: &File,
        name: &str,
        path: &Path,
        contents: &[u8],
    ) -> Result<(), MmError> {
        let descriptor = rustix::fs::openat(
            directory,
            name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|error| MmError::Io {
            context: format!(
                "creating Traefik dynamic configuration artifact {}",
                path.display()
            ),
            source: error.into(),
        })?;
        let mut file = File::from(descriptor);
        validate_regular_file(&file, path)?;
        file.write_all(contents)
            .and_then(|()| file.sync_all())
            .map_err(|source| MmError::Io {
                context: format!(
                    "writing Traefik dynamic configuration artifact {}",
                    path.display()
                ),
                source,
            })?;
        verify_file_entry(directory, name, &file, path)
    }

    pub(super) fn open_private_root(path: &Path) -> Result<File, MmError> {
        validate_root_path(path)?;
        ensure_safe_path_ancestors(path)?;
        ensure_no_symlink_components(path)?;
        let directory = walk_path(path, true)?;
        validate_private_directory(&directory, path)?;
        verify_root_identity(path, &directory)?;
        Ok(directory)
    }

    pub(super) fn verify_root_identity(path: &Path, directory: &File) -> Result<(), MmError> {
        ensure_no_symlink_components(path)?;
        let reopened = walk_path(path, false)?;
        if same_file(directory, &reopened)? {
            Ok(())
        } else {
            Err(publication_race(path))
        }
    }

    pub(super) fn validate_root_path(path: &Path) -> Result<(), MmError> {
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
        {
            return Err(MmError::Security {
                message: format!(
                    "Traefik dynamic configuration root '{}' must not contain parent or platform-prefix components",
                    path.display()
                ),
            });
        }
        Ok(())
    }

    pub(super) fn walk_path(path: &Path, create: bool) -> Result<File, MmError> {
        let start = if path.is_absolute() { "/" } else { "." };
        let descriptor = rustix::fs::openat(rustix::fs::CWD, start, DIRECTORY_FLAGS, Mode::empty())
            .map_err(|error| MmError::Io {
                context: format!("opening start of private path {}", path.display()),
                source: error.into(),
            })?;
        let mut directory = File::from(descriptor);
        for component in path.components() {
            let Component::Normal(name) = component else {
                continue;
            };
            match try_open_directory(&directory, name, path)? {
                Some(child) => directory = child,
                None if create => {
                    create_private_directory(&directory, name, path)?;
                    directory = try_open_directory(&directory, name, path)?
                        .ok_or_else(|| publication_race(path))?;
                }
                None => return Err(publication_race(path)),
            }
        }
        Ok(directory)
    }

    pub(super) fn open_or_create_private_directory(
        parent: &File,
        name: &str,
        path: &Path,
    ) -> Result<File, MmError> {
        let directory = match try_open_directory(parent, name, path)? {
            Some(directory) => directory,
            None => {
                create_private_directory(parent, name, path)?;
                try_open_directory(parent, name, path)?.ok_or_else(|| publication_race(path))?
            }
        };
        validate_private_directory(&directory, path)?;
        verify_directory_entry(parent, name, &directory, path)?;
        Ok(directory)
    }

    pub(super) fn try_open_directory(
        parent: &File,
        name: impl AsRef<OsStr>,
        path: &Path,
    ) -> Result<Option<File>, MmError> {
        match rustix::fs::openat(parent, name.as_ref(), DIRECTORY_FLAGS, Mode::empty()) {
            Ok(descriptor) => Ok(Some(File::from(descriptor))),
            Err(error) if error == rustix::io::Errno::NOENT => Ok(None),
            Err(error)
                if error == rustix::io::Errno::LOOP || error == rustix::io::Errno::NOTDIR =>
            {
                Err(MmError::Security {
                    message: format!(
                        "refusing non-directory or symlinked Traefik dynamic configuration path '{}'",
                        path.display()
                    ),
                })
            }
            Err(error) => Err(MmError::Io {
                context: format!(
                    "opening Traefik dynamic configuration directory {}",
                    path.display()
                ),
                source: error.into(),
            }),
        }
    }

    pub(super) fn create_private_directory(
        parent: &File,
        name: impl AsRef<OsStr>,
        path: &Path,
    ) -> Result<(), MmError> {
        match rustix::fs::mkdirat(parent, name.as_ref(), Mode::RWXU) {
            Ok(()) => Ok(()),
            Err(error) if error == rustix::io::Errno::EXIST => Ok(()),
            Err(error) => Err(MmError::Io {
                context: format!(
                    "creating private Traefik dynamic configuration directory {}",
                    path.display()
                ),
                source: error.into(),
            }),
        }
    }

    pub(super) fn validate_private_directory(directory: &File, path: &Path) -> Result<(), MmError> {
        let metadata = rustix::fs::fstat(directory).map_err(|error| MmError::Io {
            context: format!(
                "inspecting private Traefik dynamic configuration directory {}",
                path.display()
            ),
            source: error.into(),
        })?;
        let mode = Mode::from_raw_mode(metadata.st_mode);
        if !FileType::from_raw_mode(metadata.st_mode).is_dir()
            || metadata.st_uid != rustix::process::geteuid().as_raw()
            || mode.intersects(Mode::RWXG | Mode::RWXO)
        {
            return Err(MmError::Security {
                message: format!(
                    "Traefik dynamic configuration directory '{}' must be owned by the current user and inaccessible to group and other users",
                    path.display()
                ),
            });
        }
        Ok(())
    }

    pub(super) fn open_lock(parent: &File, name: &str, path: &Path) -> Result<File, MmError> {
        let descriptor = match rustix::fs::openat(
            parent,
            name,
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        ) {
            Ok(descriptor) => descriptor,
            Err(error) if error == rustix::io::Errno::LOOP => {
                return Err(MmError::Security {
                    message: format!(
                        "refusing symlinked Traefik dynamic configuration lock '{}'",
                        path.display()
                    ),
                });
            }
            Err(error) => {
                return Err(MmError::Io {
                    context: format!(
                        "opening Traefik dynamic configuration lock {}",
                        path.display()
                    ),
                    source: error.into(),
                });
            }
        };
        Ok(File::from(descriptor))
    }

    pub(super) fn verify_lock(p: &File, n: &str, lock: &File, path: &Path) -> Result<(), MmError> {
        let metadata = validate_regular_file(lock, path)?;
        let mode = Mode::from_raw_mode(metadata.st_mode);
        if mode.intersects(Mode::RWXG | Mode::RWXO) {
            return Err(MmError::Security {
                message: format!(
                    "Traefik dynamic configuration lock '{}' is not private",
                    path.display()
                ),
            });
        }
        verify_file_entry(p, n, lock, path)
    }

    pub(super) fn validate_regular_file(file: &File, path: &Path) -> Result<Stat, MmError> {
        let metadata = rustix::fs::fstat(file).map_err(|error| MmError::Io {
            context: format!(
                "inspecting Traefik dynamic configuration artifact {}",
                path.display()
            ),
            source: error.into(),
        })?;
        if !FileType::from_raw_mode(metadata.st_mode).is_file()
            || metadata.st_uid != rustix::process::geteuid().as_raw()
            || metadata.st_nlink != 1
        {
            return Err(unexpected_artifact(path));
        }
        Ok(metadata)
    }

    pub(super) fn verify_directory_entry(
        parent: &File,
        name: &str,
        directory: &File,
        path: &Path,
    ) -> Result<(), MmError> {
        let opened = rustix::fs::fstat(directory).map_err(|error| MmError::Io {
            context: format!(
                "inspecting Traefik dynamic configuration directory {}",
                path.display()
            ),
            source: error.into(),
        })?;
        let entry = entry_metadata(parent, name, path)?;
        if !FileType::from_raw_mode(opened.st_mode).is_dir()
            || !FileType::from_raw_mode(entry.st_mode).is_dir()
            || opened.st_dev != entry.st_dev
            || opened.st_ino != entry.st_ino
        {
            return Err(publication_race(path));
        }
        Ok(())
    }

    pub(super) fn verify_file_entry(
        parent: &File,
        name: &str,
        file: &File,
        path: &Path,
    ) -> Result<(), MmError> {
        let opened = validate_regular_file(file, path)?;
        let entry = entry_metadata(parent, name, path)?;
        if !FileType::from_raw_mode(entry.st_mode).is_file()
            || opened.st_dev != entry.st_dev
            || opened.st_ino != entry.st_ino
        {
            return Err(publication_race(path));
        }
        Ok(())
    }

    pub(super) fn entry_metadata(p: &File, name: &str, path: &Path) -> Result<Stat, MmError> {
        rustix::fs::statat(p, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|error| {
            if error == rustix::io::Errno::NOENT {
                publication_race(path)
            } else {
                MmError::Io {
                    context: format!(
                        "inspecting Traefik dynamic configuration entry {}",
                        path.display()
                    ),
                    source: error.into(),
                }
            }
        })
    }

    pub(super) fn same_file(left: &File, right: &File) -> Result<bool, MmError> {
        let left = rustix::fs::fstat(left).map_err(|error| MmError::Io {
            context: "inspecting an opened Traefik dynamic configuration directory".to_owned(),
            source: error.into(),
        })?;
        let right = rustix::fs::fstat(right).map_err(|error| MmError::Io {
            context: "inspecting a reopened Traefik dynamic configuration directory".to_owned(),
            source: error.into(),
        })?;
        Ok(left.st_dev == right.st_dev && left.st_ino == right.st_ino)
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple",
        target_os = "redox"
    ))]
    fn rename_no_replace(parent: &File, source: &str, destination: &str) -> rustix::io::Result<()> {
        rustix::fs::renameat_with(parent, source, parent, destination, RenameFlags::NOREPLACE)
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple",
        target_os = "redox"
    )))]
    fn rename_no_replace(parent: &File, source: &str, destination: &str) -> rustix::io::Result<()> {
        match rustix::fs::statat(parent, destination, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => Err(rustix::io::Errno::EXIST),
            Err(error) if error == rustix::io::Errno::NOENT => {
                rustix::fs::renameat(parent, source, parent, destination)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn cleanup_staging(parent: &File, name: &str, directory: &File) {
        let Ok(entry) = rustix::fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) else {
            return;
        };
        let Ok(opened) = rustix::fs::fstat(directory) else {
            return;
        };
        if entry.st_dev != opened.st_dev || entry.st_ino != opened.st_ino {
            return;
        }
        for result in [
            rustix::fs::unlinkat(directory, DYNAMIC_CONFIG_FILE, AtFlags::empty()),
            rustix::fs::unlinkat(directory, "manifest.json", AtFlags::empty()),
            rustix::fs::unlinkat(parent, name, AtFlags::REMOVEDIR),
        ] {
            if let Err(error) = result
                && error != rustix::io::Errno::NOENT
            {
                tracing::warn!(%error, staging = name, "Traefik staging cleanup failed");
            }
        }
    }

    pub(super) fn publication_race(path: &Path) -> MmError {
        MmError::Security {
            message: format!(
                "Traefik dynamic configuration publication raced with a filesystem change at {}",
                path.display()
            ),
        }
    }
}

#[cfg(all(test, unix))]
mod tests;
