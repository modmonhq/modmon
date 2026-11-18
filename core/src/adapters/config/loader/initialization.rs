struct AnsibleScaffold {
    starter_files: Vec<PathBuf>,
    created_files: Vec<PathBuf>,
    created_directories: Vec<PathBuf>,
    marker: PathBuf,
}

fn complete_initialization(marker: &Path, root: &Path) -> Result<(), MmError> {
    fs::remove_file(marker).map_err(|source| MmError::Io {
        context: format!("completing Ansible starter publication {}", marker.display()),
        source,
    })?;
    let config_directory = root.join(".config");
    for directory in [marker.parent(), Some(config_directory.as_path()), Some(root)]
        .into_iter()
        .flatten()
    {
        sync_project_directory(directory)?;
    }
    Ok(())
}

fn recover_completed_initialization(
    loader: &ConfigLoader,
    root: &Path,
    path: &Path,
    marker: &Path,
) -> Result<InitializedConfigDocument, MmError> {
    let config_directory = root.join(".config");
    let ansible_directory = config_directory.join("ansible");
    ensure_user_controlled_directory(root)?;
    ensure_user_controlled_directory(&config_directory)?;
    ensure_user_controlled_directory(&ansible_directory)?;
    ensure_user_controlled_file(path)?;
    verify_recoverable_ansible_scaffold(&ansible_directory)?;
    for starter in [
        root.join(ANSIBLE_CONFIG_RELATIVE_PATH),
        root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
    ] {
        ensure_user_controlled_file(&starter)?;
    }
    let document = loader.load(path)?;
    let gitignore = preview_gitignore(root)?;
    apply_gitignore(&gitignore)?;
    sync_project_directory(&ansible_directory)?;
    sync_project_directory(&config_directory)?;
    fs::remove_file(marker).map_err(|source| MmError::Io {
        context: format!("completing interrupted initialization {}", marker.display()),
        source,
    })?;
    if let Err(error) = sync_project_directory(&ansible_directory) {
        return match publish_new_project_file(marker, INITIALIZING_MARKER_CONTENT) {
            Ok(()) => Err(error),
            Err(restore) => Err(MmError::State {
                message: format!(
                    "{error}; interrupted-initialization marker restoration also failed: {restore}"
                ),
            }),
        };
    }
    let mut starter_files = vec![
        root.join(ANSIBLE_CONFIG_RELATIVE_PATH),
        root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
    ];
    if gitignore.changed() {
        starter_files.push(gitignore.path.clone());
    }
    Ok(InitializedConfigDocument {
        starter_files,
        document,
        created_files: Vec::new(),
        created_directories: Vec::new(),
        gitignore,
    })
}

fn initialize_ansible_scaffold(root: &Path) -> Result<AnsibleScaffold, MmError> {
    let directory = root.join(".config/ansible");
    let starter_files = vec![
        root.join(ANSIBLE_CONFIG_RELATIVE_PATH),
        root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
    ];
    let marker = directory.join(INITIALIZING_MARKER);
    ensure_no_symlink_components(&directory)?;
    if directory.exists() {
        ensure_user_controlled_directory(&directory)?;
        if !marker.exists() {
            return Err(MmError::ConfigAlreadyExists { path: directory });
        }
        verify_recoverable_ansible_scaffold(&directory)?;
        return Ok(AnsibleScaffold {
            created_files: starter_files
                .iter()
                .cloned()
                .chain(std::iter::once(marker.clone()))
                .collect(),
            created_directories: vec![directory],
            starter_files,
            marker,
        });
    }

    let parent = directory.parent().ok_or_else(|| MmError::Io {
        context: format!("resolving parent directory for {}", directory.display()),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Ansible starter directory has no parent",
        ),
    })?;
    ensure_user_controlled_directory(parent)?;
    let staging = parent.join(format!(".ansible.{}.tmp", Uuid::new_v4()));
    create_project_directory(&staging).map_err(|source| MmError::Io {
        context: format!(
            "creating Ansible starter staging directory {}",
            staging.display()
        ),
        source,
    })?;
    let result = (|| {
        write_new_project_file(&staging.join("ansible.cfg"), ANSIBLE_CONFIG.as_bytes())?;
        write_new_project_file(&staging.join("bootstrap.yml"), ANSIBLE_PLAYBOOK.as_bytes())?;
        write_new_project_file(
            &staging.join(INITIALIZING_MARKER),
            INITIALIZING_MARKER_CONTENT,
        )?;
        sync_project_directory(&staging)?;
        fs::rename(&staging, &directory).map_err(|source| {
            if source.kind() == std::io::ErrorKind::AlreadyExists {
                MmError::ConfigAlreadyExists {
                    path: directory.clone(),
                }
            } else {
                MmError::Io {
                    context: format!("publishing Ansible starter {}", directory.display()),
                    source,
                }
            }
        })?;
        ensure_user_controlled_directory(&directory)?;
        sync_project_directory(parent)
    })();
    let cleanup = if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|source| MmError::Io {
            context: format!(
                "removing Ansible starter staging directory {}",
                staging.display()
            ),
            source,
        })
    } else {
        Ok(())
    };
    if let Err(error) = result {
        return match cleanup {
            Ok(()) => Err(error),
            Err(cleanup) => Err(MmError::State {
                message: format!("{error}; Ansible staging cleanup also failed: {cleanup}"),
            }),
        };
    }
    cleanup?;
    Ok(AnsibleScaffold {
        created_files: starter_files
            .iter()
            .cloned()
            .chain(std::iter::once(marker.clone()))
            .collect(),
        created_directories: vec![directory],
        starter_files,
        marker,
    })
}

fn verify_recoverable_ansible_scaffold(directory: &Path) -> Result<(), MmError> {
    let actual = fs::read_dir(directory)
        .map_err(|source| MmError::Io {
            context: format!("reading incomplete Ansible starter {}", directory.display()),
            source,
        })?
        .map(|entry| {
            entry
                .map_err(|source| MmError::Io {
                    context: format!("reading incomplete Ansible starter {}", directory.display()),
                    source,
                })?
                .file_name()
                .into_string()
                .map_err(|_| MmError::Security {
                    message: "incomplete Ansible starter contains a non-Unicode file name"
                        .to_owned(),
                })
        })
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    let expected = ["ansible.cfg", "bootstrap.yml", INITIALIZING_MARKER]
        .into_iter()
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
    let config = read_regular_file(&directory.join("ansible.cfg"), ANSIBLE_CONFIG.len())?;
    let playbook = read_regular_file(&directory.join("bootstrap.yml"), ANSIBLE_PLAYBOOK.len())?;
    let marker = read_regular_file(&directory.join(INITIALIZING_MARKER), 128)?;
    if actual != expected
        || config.as_deref() != Some(ANSIBLE_CONFIG.as_bytes())
        || playbook.as_deref() != Some(ANSIBLE_PLAYBOOK.as_bytes())
        || marker.as_deref() != Some(INITIALIZING_MARKER_CONTENT)
    {
        return Err(MmError::Security {
            message: format!(
                "incomplete Ansible starter '{}' has unexpected contents",
                directory.display()
            ),
        });
    }
    Ok(())
}

fn write_new_project_file(path: &Path, contents: &[u8]) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.mode(0o644);
    }
    let mut file = options
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| MmError::Io {
            context: format!("creating {}", path.display()),
            source,
        })?;
    if let Err(source) = file.write_all(contents).and_then(|()| file.sync_all()) {
        drop(file);
        let error = MmError::Io {
            context: format!("writing {}", path.display()),
            source,
        };
        return Err(project_file_cleanup_error(error, path));
    }
    if let Err(error) = set_project_file_permissions(path) {
        return Err(project_file_cleanup_error(error, path));
    }
    Ok(())
}

fn project_file_cleanup_error(primary: MmError, path: &Path) -> MmError {
    match fs::remove_file(path) {
        Ok(()) => primary,
        Err(cleanup) => MmError::State {
            message: format!("{primary}; project file cleanup also failed: {cleanup}"),
        },
    }
}

pub(super) fn publish_new_project_file(path: &Path, contents: &[u8]) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    let parent = path.parent().ok_or_else(|| MmError::Io {
        context: format!("resolving parent directory for {}", path.display()),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "project file has no parent directory",
        ),
    })?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project-file");
    let temporary = parent.join(format!(".{name}.{}.tmp", Uuid::new_v4()));
    write_new_project_file(&temporary, contents)?;
    if let Err(source) = fs::hard_link(&temporary, path) {
        let cleanup = fs::remove_file(&temporary);
        let error = if source.kind() == std::io::ErrorKind::AlreadyExists {
            MmError::ConfigAlreadyExists {
                path: path.to_path_buf(),
            }
        } else {
            MmError::Io {
                context: format!("publishing project file {}", path.display()),
                source,
            }
        };
        return match cleanup {
            Ok(()) => Err(error),
            Err(cleanup) => Err(MmError::State {
                message: format!("{error}; temporary project file cleanup also failed: {cleanup}"),
            }),
        };
    }
    if let Err(source) = fs::remove_file(&temporary) {
        let error = MmError::Io {
            context: format!("removing temporary project file {}", temporary.display()),
            source,
        };
        return match fs::remove_file(path) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(MmError::State {
                message: format!("{error}; published project file cleanup also failed: {cleanup}"),
            }),
        };
    }
    if let Err(error) = sync_project_directory(parent) {
        let cleanup = fs::remove_file(path);
        return match cleanup {
            Ok(()) => Err(error),
            Err(cleanup) => Err(MmError::State {
                message: format!("{error}; published project file cleanup also failed: {cleanup}"),
            }),
        };
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn create_project_directory(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.mode(0o755).create(path)
}

#[cfg(not(unix))]
pub(super) fn create_project_directory(path: &Path) -> Result<(), std::io::Error> {
    fs::create_dir(path)
}

#[cfg(unix)]
fn set_project_file_permissions(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o644)).map_err(|source| MmError::Io {
        context: format!("setting editable permissions on {}", path.display()),
        source,
    })?;
    ensure_owned_by_current_user(path)
}

#[cfg(not(unix))]
fn set_project_file_permissions(_path: &Path) -> Result<(), MmError> {
    Ok(())
}

#[cfg(unix)]
fn sync_project_directory(path: &Path) -> Result<(), MmError> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("syncing project directory {}", path.display()),
            source,
        })
}

#[cfg(not(unix))]
fn sync_project_directory(_path: &Path) -> Result<(), MmError> {
    Ok(())
}

fn rollback_paths(files: &[PathBuf], directories: &[PathBuf]) -> Result<(), MmError> {
    let mut failure = None;
    for path in files.iter().rev() {
        if let Err(source) = fs::remove_file(path)
            && source.kind() != std::io::ErrorKind::NotFound
            && failure.is_none()
        {
            failure = Some(MmError::Io {
                context: format!("rolling back project file {}", path.display()),
                source,
            });
        }
    }
    for path in directories.iter().rev() {
        if let Err(source) = fs::remove_dir(path)
            && source.kind() != std::io::ErrorKind::NotFound
            && failure.is_none()
        {
            failure = Some(MmError::Io {
                context: format!("rolling back project directory {}", path.display()),
                source,
            });
        }
    }
    failure.map_or(Ok(()), Err)
}

fn rollback_or(original: MmError, files: &[PathBuf], directories: &[PathBuf]) -> MmError {
    match rollback_paths(files, directories) {
        Ok(()) => original,
        Err(cleanup) => MmError::State {
            message: format!("{original}; project initialization cleanup also failed: {cleanup}"),
        },
    }
}
