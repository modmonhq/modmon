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
        if is_sensitive_file(input) {
            return Err(MmError::Security {
                message: format!(
                    "sensitive file {} cannot be a build extra input; remove it from build.extra-inputs",
                    input.display()
                ),
            }
            .in_own_words());
        }
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
    hash_context_excluding(context, context, dockerfile, &[])
}

/// The context hash without the `excluded` files, such as a marker a remote
/// checkout adds that the built commit does not contain. Messages name paths
/// relative to `root`.
pub(crate) fn hash_context_excluding(
    root: &Path,
    context: &Path,
    dockerfile: &Path,
    excluded: &[PathBuf],
) -> Result<String, MmError> {
    let scan = ContextScan::new(root, context, dockerfile)?;
    let mut files = scan.inputs()?;
    files.retain(|_, source| !excluded.contains(source));
    files.insert(scan.dockerfile_name(), dockerfile.to_path_buf());
    hash_context_files(Path::new(&scan.name), &files)
}

fn hash_context_files(
    context: &Path,
    files: &BTreeMap<PathBuf, PathBuf>,
) -> Result<String, MmError> {
    validate_context_file_count(context, files.len())?;

    let mut total = 0_u64;
    let mut reviewed = Vec::with_capacity(files.len());
    for (relative, file) in files {
        let relative = relative.to_str().ok_or_else(|| MmError::Security {
            message: format!(
                "{} in build context {} is not a valid UTF-8 path",
                relative.display(),
                context.display()
            ),
        })?;
        let metadata = fs::symlink_metadata(file).map_err(|source| MmError::Io {
            context: format!("reading build context metadata for {}", file.display()),
            source,
        })?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(MmError::Security {
                message: format!(
                    "{relative} in build context {} must be a regular file",
                    context.display()
                ),
            });
        }
        if metadata.len() > MAX_CONTEXT_FILE_BYTES {
            return Err(MmError::Build {
                message: format!(
                    "{relative} in build context {} exceeds the {MAX_CONTEXT_FILE_BYTES} byte file safety limit",
                    context.display()
                ),
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
        reviewed.push((file, relative, executable));
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

pub(crate) fn snapshot_build_context(
    root: &Path,
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

    let scan = ContextScan::new(root, context, dockerfile)?;
    let files = scan.inputs()?;
    for (relative, source) in &files {
        copy_into_snapshot(destination, relative, source)?;
    }
    let dockerfile_name = scan.dockerfile_name();
    if files
        .get(&dockerfile_name)
        .is_none_or(|source| source != dockerfile)
    {
        copy_into_snapshot(destination, &dockerfile_name, dockerfile)?;
    }
    let snapshot_dockerfile = destination.join(dockerfile_name);
    let snapshot_hash = hash_context(destination, &snapshot_dockerfile)?;
    if snapshot_hash != expected_hash {
        return Err(MmError::State {
            message: "build context changed after the reviewed plan; review a new plan".to_owned(),
        });
    }
    Ok(snapshot_dockerfile)
}

fn copy_into_snapshot(destination: &Path, relative: &Path, source: &Path) -> Result<(), MmError> {
    let target = destination.join(relative);
    let parent = target.parent().unwrap_or(destination);
    fs::create_dir_all(parent).map_err(|error| MmError::Io {
        context: format!("creating build snapshot directory {}", parent.display()),
        source: error,
    })?;
    fs::copy(source, &target).map_err(|error| MmError::Io {
        context: format!("copying build context file {}", source.display()),
        source: error,
    })?;
    Ok(())
}

/// One build context as BuildKit sends it: the files its ignore file leaves
/// in, the Dockerfile, and the ignore file itself.
struct ContextScan<'a> {
    context: &'a Path,
    dockerfile: &'a Path,
    name: String,
    ignore: ContextIgnore,
    ignore_file: Option<PathBuf>,
    advice: String,
}

impl<'a> ContextScan<'a> {
    fn new(root: &Path, context: &'a Path, dockerfile: &'a Path) -> Result<Self, MmError> {
        let mut own = dockerfile.as_os_str().to_owned();
        own.push(DOCKERIGNORE);
        let own = PathBuf::from(own);
        let shared = context.join(DOCKERIGNORE);
        let ignore_file = if present(&own)? {
            Some(own.clone())
        } else if present(&shared)? {
            Some(shared.clone())
        } else {
            None
        };
        let name = project_path(root, context);
        let conventional = dockerfile.file_name() == Some("Dockerfile".as_ref())
            && dockerfile
                .parent()
                .is_some_and(|parent| project_path(root, parent) == name);
        let advice = if ignore_file.as_ref() == Some(&own) {
            project_path(root, &own)
        } else if conventional {
            project_path(root, &shared)
        } else {
            format!(
                "{} or {}",
                project_path(root, &own),
                project_path(root, &shared)
            )
        };
        let ignore = match &ignore_file {
            Some(file) => read_ignore(file, &project_path(root, file))?,
            None => ContextIgnore::default(),
        };
        Ok(Self {
            context,
            dockerfile,
            name,
            ignore,
            ignore_file,
            advice,
        })
    }

    /// The files sent from the context, and the ignore file, by their path
    /// in the snapshot.
    fn inputs(&self) -> Result<BTreeMap<PathBuf, PathBuf>, MmError> {
        let mut files = BTreeMap::new();
        self.collect(self.context, None, &mut files)?;
        if let Some(file) = &self.ignore_file {
            files.insert(
                self.snapshot_name(file, SNAPSHOT_DOCKERIGNORE),
                file.clone(),
            );
        }
        Ok(files)
    }

    fn dockerfile_name(&self) -> PathBuf {
        self.snapshot_name(self.dockerfile, SNAPSHOT_DOCKERFILE)
    }

    fn snapshot_name(&self, file: &Path, external: &str) -> PathBuf {
        file.strip_prefix(self.context)
            .map_or_else(|_| PathBuf::from(external), Path::to_path_buf)
    }

    fn collect(
        &self,
        directory: &Path,
        parent: Option<&IgnoreVerdict>,
        files: &mut BTreeMap<PathBuf, PathBuf>,
    ) -> Result<(), MmError> {
        let entries = fs::read_dir(directory).map_err(|source| MmError::Io {
            context: format!("reading build context directory {}", directory.display()),
            source,
        })?;
        self.collect_entries(directory, parent, files, entries)
    }

    fn collect_entries(
        &self,
        directory: &Path,
        parent: Option<&IgnoreVerdict>,
        files: &mut BTreeMap<PathBuf, PathBuf>,
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
            let relative = path
                .strip_prefix(self.context)
                .map_err(|_| MmError::Security {
                    message: "build context traversal escaped its root".to_owned(),
                })?;
            if relative
                .components()
                .next()
                .is_some_and(|component| component == Component::Normal(".git".as_ref()))
            {
                continue;
            }
            let shown = relative.to_string_lossy();
            let verdict = self.ignore.verdict(&shown, parent);
            if verdict.excluded() && self.ignore.skips_excluded_directory(&shown) {
                continue;
            }
            let metadata = fs::symlink_metadata(&path).map_err(|source| MmError::Io {
                context: format!("reading build context metadata for {}", path.display()),
                source,
            })?;
            if metadata.is_dir() {
                self.collect(&path, Some(&verdict), files)?;
            } else if !verdict.excluded() {
                self.admit(relative, &path, &metadata)?;
                files.insert(relative.to_path_buf(), path);
            }
        }
        Ok(())
    }

    fn admit(&self, relative: &Path, path: &Path, metadata: &fs::Metadata) -> Result<(), MmError> {
        let shown = relative.to_string_lossy();
        let entry = if metadata.file_type().is_symlink() {
            let target = fs::read_link(path).map_err(|source| MmError::Io {
                context: format!("reading build context symlink {}", path.display()),
                source,
            })?;
            format!("symlink {shown} -> {}", target.display())
        } else if !metadata.is_file() {
            format!("special file {shown}")
        } else if relative.to_str().is_none() {
            format!("file {shown}, whose name is not valid UTF-8,")
        } else if is_sensitive_file(relative) {
            format!("sensitive file {shown}")
        } else {
            return Ok(());
        };
        Err(MmError::Security {
            message: format!(
                "{entry} would be sent in build context {} (exclude it in {})",
                self.name, self.advice
            ),
        }
        .in_own_words())
    }
}

fn present(path: &Path) -> Result<bool, MmError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(false)
        }
        Err(source) => Err(MmError::Io {
            context: format!("looking for ignore file {}", path.display()),
            source,
        }),
    }
}

fn read_ignore(file: &Path, name: &str) -> Result<ContextIgnore, MmError> {
    let metadata = fs::symlink_metadata(file).map_err(|source| MmError::Io {
        context: format!("reading ignore file {}", file.display()),
        source,
    })?;
    if !metadata.is_file() {
        return Err(MmError::Security {
            message: format!("ignore file {name} must be a regular file, not a symlink"),
        }
        .in_own_words());
    }
    let too_large = || {
        MmError::Build {
            message: format!(
                "ignore file {name} exceeds the {MAX_IGNORE_FILE_BYTES} byte safety limit"
            ),
        }
        .in_own_words()
    };
    if metadata.len() > MAX_IGNORE_FILE_BYTES {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    File::open(file)
        .and_then(|opened| {
            opened
                .take(MAX_IGNORE_FILE_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|source| MmError::Io {
            context: format!("reading ignore file {}", file.display()),
            source,
        })?;
    if bytes.len() as u64 > MAX_IGNORE_FILE_BYTES {
        return Err(too_large());
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        MmError::Build {
            message: format!("ignore file {name} is not valid UTF-8"),
        }
        .in_own_words()
    })?;
    ContextIgnore::parse(&text).map_err(|invalid| {
        MmError::Build {
            message: format!("{name} {invalid}"),
        }
        .in_own_words()
    })
}

fn project_path(root: &Path, path: &Path) -> String {
    let path = lexical(path);
    let shown = path
        .strip_prefix(lexical(root))
        .unwrap_or(&path)
        .to_string_lossy()
        .into_owned();
    if shown.is_empty() {
        ".".to_owned()
    } else {
        shown
    }
}

fn lexical(path: &Path) -> PathBuf {
    let mut tidy = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir
                if matches!(tidy.components().next_back(), Some(Component::Normal(_))) =>
            {
                tidy.pop();
            }
            other => tidy.push(other),
        }
    }
    tidy
}

fn is_sensitive_file(file: &Path) -> bool {
    let name = file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    name == ".env"
        || name.starts_with(".env.")
        || matches!(
            name,
            "id_rsa" | "id_ed25519" | "id_ecdsa" | "id_dsa" | "age-identity.txt"
        )
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
