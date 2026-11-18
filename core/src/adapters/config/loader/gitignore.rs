use crate::domain::onboarding::InitializationFileChange;

pub(super) fn preview_gitignore(root: &Path) -> Result<InitializationFileChange, MmError> {
    let path = root.join(".gitignore");
    let before = read_gitignore(&path)?;
    let mut matcher = ignore::gitignore::GitignoreBuilder::new(root);
    if let Some(source) = &before {
        for line in source.lines() {
            matcher
                .add_line(Some(path.clone()), line)
                .map_err(|error| MmError::ConfigParse {
                    path: path.clone(),
                    message: error.to_string(),
                })?;
        }
    }
    let matcher = matcher.build().map_err(|error| MmError::ConfigParse {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let mut after = before.clone().unwrap_or_default();
    if !matcher
        .matched_path_or_any_parents(root.join(OVERRIDES_RELATIVE_PATH), false)
        .is_ignore()
    {
        let newline = if after.contains("\r\n") { "\r\n" } else { "\n" };
        if !after.is_empty() && !after.ends_with('\n') {
            after.push_str(newline);
        }
        after.push_str(&format!("/{OVERRIDES_RELATIVE_PATH}{newline}"));
    }
    // A child ignore file can undo the project's rule. Do not silently edit a
    // second operator-owned file outside the reviewed initialization change.
    let child_path = root.join(".config/.gitignore");
    if let Some(child) = read_gitignore(&child_path)? {
        let mut child_matcher = ignore::gitignore::GitignoreBuilder::new(root.join(".config"));
        for line in child.lines() {
            child_matcher
                .add_line(Some(child_path.clone()), line)
                .map_err(|error| MmError::ConfigParse {
                    path: child_path.clone(),
                    message: error.to_string(),
                })?;
        }
        let child_matcher = child_matcher
            .build()
            .map_err(|error| MmError::ConfigParse {
                path: child_path.clone(),
                message: error.to_string(),
            })?;
        if !matcher
            .matched_path_or_any_parents(root.join(".config"), true)
            .is_ignore()
            && child_matcher
                .matched("mm.overrides.yaml", false)
                .is_whitelist()
        {
            return Err(MmError::ConfigParse {
                path: child_path,
                message: "a negation exposes mm.overrides.yaml; remove it before initializing"
                    .to_owned(),
            });
        }
    }
    Ok(InitializationFileChange {
        path,
        before,
        after,
    })
}

fn read_gitignore(path: &Path) -> Result<Option<String>, MmError> {
    ensure_no_symlink_components(path)?;
    let bytes = read_regular_file(path, 1024 * 1024)?;
    if bytes.is_some() {
        ensure_user_controlled_file(path)?;
    }
    bytes
        .map(|bytes| {
            String::from_utf8(bytes).map_err(|_| MmError::ConfigParse {
                path: path.to_path_buf(),
                message: "ignore file must be valid UTF-8".to_owned(),
            })
        })
        .transpose()
}

fn apply_gitignore(change: &InitializationFileChange) -> Result<(), MmError> {
    if read_gitignore(&change.path)? != change.before {
        return Err(MmError::State {
            message: ".gitignore changed after review; review a new initialization plan".to_owned(),
        });
    }
    if !change.changed() {
        return Ok(());
    }
    if change.before.is_none() {
        publish_new_project_file(&change.path, change.after.as_bytes())
    } else {
        replace_gitignore(&change.path, &change.after)
    }
}

fn replace_gitignore(path: &Path, source: &str) -> Result<(), MmError> {
    let result = (|| -> std::io::Result<()> {
        let permissions = fs::metadata(path)?.permissions();
        let mut file = AtomicWriteFile::options().open(path)?;
        file.set_permissions(permissions)?;
        file.write_all(source.as_bytes())?;
        file.sync_all()?;
        file.commit()
    })();
    result.map_err(|source| MmError::Io {
        context: format!("updating {}", path.display()),
        source,
    })
}

fn rollback_gitignore(change: &InitializationFileChange) -> Result<(), MmError> {
    if !change.changed() {
        return Ok(());
    }
    if read_gitignore(&change.path)?.as_deref() != Some(&change.after) {
        return Err(MmError::State {
            message: ".gitignore changed after initialization; preserving the newer contents"
                .to_owned(),
        });
    }
    if let Some(before) = &change.before {
        replace_gitignore(&change.path, before)
    } else {
        fs::remove_file(&change.path).map_err(|source| MmError::Io {
            context: "rolling back new .gitignore".to_owned(),
            source,
        })
    }
}

fn rollback_initialized(
    original: MmError,
    files: &[PathBuf],
    directories: &[PathBuf],
    gitignore: &InitializationFileChange,
) -> MmError {
    let cleanup = rollback_gitignore(gitignore);
    let original = rollback_or(original, files, directories);
    match cleanup {
        Ok(()) => original,
        Err(error) => MmError::State {
            message: format!("{original}; ignore-file rollback also failed: {error}"),
        },
    }
}
