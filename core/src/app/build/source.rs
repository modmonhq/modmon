use super::*;

#[cfg(test)]
pub(super) fn file_digest(path: &Path) -> Result<String, MmError> {
    let mut hasher = blake3::Hasher::new();
    hash_file(path, &mut hasher)?;
    Ok(hasher.finalize().to_hex().to_string())
}

pub(super) fn read_dockerfile(path: &Path) -> Result<String, MmError> {
    let metadata = fs::metadata(path).map_err(|source| MmError::Io {
        context: format!("reading Dockerfile {}", path.display()),
        source,
    })?;
    if metadata.len() > MAX_CONTEXT_FILE_BYTES {
        return Err(MmError::Build {
            message: format!("Dockerfile {} exceeds the safety limit", path.display()),
        });
    }
    let bytes = fs::read(path).map_err(|source| MmError::Io {
        context: format!("reading Dockerfile {}", path.display()),
        source,
    })?;
    if bytes.len() > MAX_CONTEXT_FILE_BYTES as usize {
        return Err(MmError::Build {
            message: format!("Dockerfile {} exceeds the safety limit", path.display()),
        });
    }
    String::from_utf8(bytes).map_err(|_| MmError::Build {
        message: format!("Dockerfile {} is not valid UTF-8", path.display()),
    })
}

pub(super) fn hash_extra_inputs(
    root: &Path,
    inputs: &[PathBuf],
) -> Result<Vec<ImageBuildTrigger>, MmError> {
    let mut triggers = Vec::new();
    for input in inputs {
        let path = root.join(input);
        ensure_build_path(root, &path, false, "build extra input")?;
        reject_sensitive_context_file(root, &path)?;
        let mut hasher = blake3::Hasher::new();
        hash_file(&path, &mut hasher)?;
        triggers.push(ImageBuildTrigger {
            path: input.clone(),
            digest: hasher.finalize().to_hex().to_string(),
        });
    }
    Ok(triggers)
}

/// The variant the environment selects, or every variant when it selects
/// none of several.
pub(super) fn plan_variants(
    configured: Option<&ModuleBuildConfig>,
    environment_variant: Option<&str>,
    base_tags: &[String],
) -> Result<Vec<ImageBuildVariantPlan>, MmError> {
    let Some(build) = configured.filter(|build| !build.targets.is_empty()) else {
        return Ok(vec![ImageBuildVariantPlan {
            target: None,
            stage: None,
            tag_suffix: String::new(),
            tags: base_tags.to_vec(),
            args: BTreeMap::new(),
        }]);
    };
    let selected = match build.select_target(environment_variant) {
        BuildTargetSelection::Selected(name) => Some(name),
        _ => None,
    };
    build
        .targets
        .iter()
        .filter(|(name, _)| selected.is_none_or(|selected| selected == name.as_str()))
        .map(|(name, target)| plan_variant(name, target, base_tags))
        .collect()
}

pub(crate) fn plan_variant(
    name: &str,
    target: &BuildTarget,
    base_tags: &[String],
) -> Result<ImageBuildVariantPlan, MmError> {
    let stage = target.stage(name);
    let suffix = target.suffix(name);
    Ok(ImageBuildVariantPlan {
        target: Some(name.to_owned()),
        stage: (stage != name).then(|| stage.to_owned()),
        tag_suffix: suffix.to_owned(),
        tags: suffixed_tags(base_tags, suffix).map_err(|message| MmError::Build { message })?,
        args: target
            .args()
            .iter()
            .map(|(arg, value)| (arg.clone(), value.to_string()))
            .collect(),
    })
}

pub(crate) fn normalize_image_repository(image: &str) -> Result<String, MmError> {
    let image = image.trim();
    if image.is_empty()
        || image.contains(char::is_whitespace)
        || image.contains("${")
        || image.contains("://")
    {
        return Err(MmError::Build {
            message: format!(
                "image repository '{image}' cannot be used for deterministic build planning"
            ),
        });
    }
    let without_digest = image
        .split_once('@')
        .map_or(image, |(repository, _)| repository);
    let last_slash = without_digest.rfind('/');
    let repository = without_digest.rfind(':').map_or(without_digest, |colon| {
        if last_slash.is_none_or(|slash| colon > slash) {
            &without_digest[..colon]
        } else {
            without_digest
        }
    });
    if repository.is_empty()
        || repository.starts_with('/')
        || repository.ends_with('/')
        || repository
            .bytes()
            .any(|byte| byte.is_ascii_uppercase() || byte.is_ascii_control())
        || !is_image_repository(repository)
    {
        return Err(MmError::Build {
            message: format!("image repository '{image}' is not OCI-compatible"),
        });
    }
    Ok(repository.to_owned())
}

pub(super) fn ensure_build_path(
    root: &Path,
    path: &Path,
    directory: bool,
    kind: &str,
) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    let canonical = dunce::canonicalize(path).map_err(|source| MmError::Io {
        context: format!("resolving {kind} {}", path.display()),
        source,
    })?;
    if !canonical.starts_with(root)
        || directory && !canonical.is_dir()
        || !directory && !canonical.is_file()
    {
        return Err(MmError::Security {
            message: format!("{kind} must be a regular path inside the project root"),
        });
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn project_relative(root: &Path, path: &Path, kind: &str) -> Result<PathBuf, MmError> {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .map_err(|_| MmError::Security {
            message: format!("{kind} escapes the project root"),
        })
}

pub(crate) fn hash_context(context: &Path, dockerfile: &Path) -> Result<String, MmError> {
    hash_context_excluding(context, dockerfile, &[])
}

/// The context hash without the `excluded` files, such as a marker a remote
/// checkout adds that the built commit does not contain.
pub(crate) fn hash_context_excluding(
    context: &Path,
    dockerfile: &Path,
    excluded: &[PathBuf],
) -> Result<String, MmError> {
    let matcher = dockerignore(context)?;
    let mut files = Vec::new();
    collect_context_files(context, context, &matcher, &mut files)?;
    files.retain(|file| !excluded.contains(file));
    if !dockerfile.starts_with(context) || !files.iter().any(|path| path == dockerfile) {
        files.push(dockerfile.to_path_buf());
    }
    files.sort_by(|left, right| {
        context_hash_path(context, left).cmp(context_hash_path(context, right))
    });
    files.dedup();
    hash_context_files(context, &files)
}

fn hash_context_files(context: &Path, files: &[PathBuf]) -> Result<String, MmError> {
    validate_context_file_count(context, files.len())?;

    let mut total = 0_u64;
    let mut reviewed = Vec::with_capacity(files.len());
    for file in files {
        reject_sensitive_context_file(context, file)?;
        let relative = context_hash_path(context, file);
        let relative = relative.to_str().ok_or_else(|| MmError::Security {
            message: "build context paths must be valid UTF-8".to_owned(),
        })?;
        let metadata = fs::symlink_metadata(file).map_err(|source| MmError::Io {
            context: format!("reading build context metadata for {}", file.display()),
            source,
        })?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(MmError::Security {
                message: format!(
                    "build context entry must be a regular file without symlinks: {}",
                    file.display()
                ),
            });
        }
        if metadata.len() > MAX_CONTEXT_FILE_BYTES {
            return Err(MmError::Build {
                message: format!("build context file is too large: {}", file.display()),
            });
        }
        total = checked_context_size(context, total, metadata.len())?;

        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111
        };
        #[cfg(not(unix))]
        let executable = 0;
        reviewed.push((file, relative.to_owned(), executable));
    }

    let mut hasher = blake3::Hasher::new();
    for (file, relative, executable) in reviewed {
        hasher.update(&(relative.len() as u64).to_le_bytes());
        hasher.update(relative.as_bytes());
        hasher.update(&executable.to_le_bytes());
        hash_file(file, &mut hasher)?;
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn validate_context_file_count(context: &Path, count: usize) -> Result<(), MmError> {
    if count > MAX_CONTEXT_FILES {
        return Err(MmError::Build {
            message: format!(
                "build context {} exceeds the {} file safety limit",
                context.display(),
                MAX_CONTEXT_FILES
            ),
        });
    }
    Ok(())
}

fn checked_context_size(context: &Path, total: u64, next: u64) -> Result<u64, MmError> {
    let total = total.checked_add(next).ok_or_else(|| MmError::Build {
        message: "build context size overflowed".to_owned(),
    })?;
    if total > MAX_CONTEXT_BYTES {
        return Err(MmError::Build {
            message: format!(
                "build context {} exceeds the {} byte safety limit",
                context.display(),
                MAX_CONTEXT_BYTES
            ),
        });
    }
    Ok(total)
}

pub(super) fn context_hash_path<'a>(context: &'a Path, file: &'a Path) -> &'a Path {
    if !file.starts_with(context)
        || file
            .strip_prefix(context)
            .is_ok_and(|relative| relative == Path::new(SNAPSHOT_DOCKERFILE))
    {
        Path::new(SNAPSHOT_DOCKERFILE)
    } else {
        file.strip_prefix(context).unwrap_or(file)
    }
}

pub(crate) fn snapshot_build_context(
    context: &Path,
    dockerfile: &Path,
    destination: &Path,
    expected_hash: &str,
) -> Result<PathBuf, MmError> {
    if destination.starts_with(context) {
        return Err(MmError::Security {
            message: "build context snapshot destination must be outside its source context"
                .to_owned(),
        });
    }
    if destination.exists() {
        return Err(MmError::Security {
            message: "build context snapshot destination already exists".to_owned(),
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(destination)
            .map_err(|source| MmError::Io {
                context: format!("creating build context snapshot {}", destination.display()),
                source,
            })?;
    }
    #[cfg(not(unix))]
    {
        fs::create_dir(destination).map_err(|source| MmError::Io {
            context: format!("creating build context snapshot {}", destination.display()),
            source,
        })?;
    }

    let matcher = dockerignore(context)?;
    let mut files = Vec::new();
    collect_context_files(context, context, &matcher, &mut files)?;
    for source in &files {
        let relative = source
            .components()
            .skip(context.components().count())
            .collect::<PathBuf>();
        let target = destination.join(relative);
        let parent = target.parent().unwrap_or(destination);
        fs::create_dir_all(parent).map_err(|source| MmError::Io {
            context: format!("creating build snapshot directory {}", parent.display()),
            source,
        })?;
        fs::copy(source, &target).map_err(|error| MmError::Io {
            context: format!("copying build context file {}", source.display()),
            source: error,
        })?;
    }

    let snapshot_dockerfile = if dockerfile.starts_with(context) {
        let relative = dockerfile
            .components()
            .skip(context.components().count())
            .collect::<PathBuf>();
        let target = destination.join(relative);
        if !files.iter().any(|source| source == dockerfile) {
            let parent = target.parent().unwrap_or(destination);
            fs::create_dir_all(parent).map_err(|source| MmError::Io {
                context: format!(
                    "creating Dockerfile snapshot directory {}",
                    parent.display()
                ),
                source,
            })?;
            fs::copy(dockerfile, &target).map_err(|source| MmError::Io {
                context: format!(
                    "copying Dockerfile {} into its build snapshot",
                    dockerfile.display()
                ),
                source,
            })?;
        }
        target
    } else {
        let target = destination.join(SNAPSHOT_DOCKERFILE);
        fs::copy(dockerfile, &target).map_err(|source| MmError::Io {
            context: format!(
                "copying Dockerfile {} into its build snapshot",
                dockerfile.display()
            ),
            source,
        })?;
        target
    };
    let snapshot_hash = hash_context(destination, &snapshot_dockerfile)?;
    if snapshot_hash != expected_hash {
        return Err(MmError::State {
            message: "build context changed after the reviewed plan; review a new plan".to_owned(),
        });
    }
    Ok(snapshot_dockerfile)
}

pub(super) fn dockerignore(context: &Path) -> Result<Gitignore, MmError> {
    let mut builder = GitignoreBuilder::new(context);
    let path = context.join(".dockerignore");
    if path.is_file()
        && let Some(error) = builder.add(path)
    {
        return Err(MmError::Build {
            message: format!("parsing .dockerignore: {error}"),
        });
    }
    builder.build().map_err(|error| MmError::Build {
        message: format!("parsing .dockerignore: {error}"),
    })
}

pub(super) fn collect_context_files(
    context: &Path,
    directory: &Path,
    matcher: &Gitignore,
    files: &mut Vec<PathBuf>,
) -> Result<(), MmError> {
    let entries = fs::read_dir(directory).map_err(|source| MmError::Io {
        context: format!("reading build context directory {}", directory.display()),
        source,
    })?;
    collect_context_entries(context, directory, matcher, files, entries)
}

fn collect_context_entries(
    context: &Path,
    directory: &Path,
    matcher: &Gitignore,
    files: &mut Vec<PathBuf>,
    entries: impl IntoIterator<Item = std::io::Result<fs::DirEntry>>,
) -> Result<(), MmError> {
    let mut entries = entries
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| MmError::Io {
            context: format!("reading build context directory {}", directory.display()),
            source,
        })?;
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(context).map_err(|_| MmError::Security {
            message: "build context traversal escaped its root".to_owned(),
        })?;
        if relative
            .components()
            .next()
            .is_some_and(|component| component == Component::Normal(".git".as_ref()))
        {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|source| MmError::Io {
            context: format!("reading build context metadata for {}", path.display()),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(MmError::Security {
                message: format!("build contexts cannot contain symlinks: {}", path.display()),
            });
        }
        let is_directory = metadata.is_dir();
        let always_include = relative == Path::new(".dockerignore");
        if !always_include
            && matcher
                .matched_path_or_any_parents(relative, is_directory)
                .is_ignore()
        {
            continue;
        }
        if is_directory {
            collect_context_files(context, &path, matcher, files)?;
        } else if metadata.is_file() {
            files.push(path);
        } else {
            return Err(MmError::Security {
                message: format!("unsupported build context entry: {}", path.display()),
            });
        }
    }
    Ok(())
}

pub(super) fn reject_sensitive_context_file(context: &Path, file: &Path) -> Result<(), MmError> {
    let name = file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let looks_sensitive = name == ".env"
        || name.starts_with(".env.")
        || matches!(
            name,
            "id_rsa" | "id_ed25519" | "id_ecdsa" | "id_dsa" | "age-identity.txt"
        );
    if looks_sensitive {
        return Err(MmError::Security {
            message: format!(
                "sensitive file {} is included in build context {}; exclude it with .dockerignore and use a build secret",
                file.display(),
                context.display()
            ),
        });
    }
    Ok(())
}

pub(super) fn hash_file(path: &Path, hasher: &mut blake3::Hasher) -> Result<(), MmError> {
    let file = File::open(path).map_err(|source| MmError::Io {
        context: format!("opening build context file {}", path.display()),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|source| MmError::Io {
            context: format!("reading build context file {}", path.display()),
            source,
        })?;
        if read == 0 {
            return Ok(());
        }
        hasher.update(&buffer[..read]);
    }
}

#[cfg(test)]
mod tests;
