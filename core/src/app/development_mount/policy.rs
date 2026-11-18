use super::*;

pub(crate) fn validate_intent(
    document: &ConfigDocument,
    module_name: &str,
    module: &ModuleConfig,
    mount: &DevelopmentMountConfig,
) -> Result<(), MmError> {
    validate_services(module_name, module, &mount.services)?;
    let mut targets = BTreeSet::new();
    for (source, target) in mount.sources() {
        if !targets.insert(target) {
            return Err(MmError::Security {
                message: format!(
                    "module '{module_name}' has duplicate development mount target '{target}'"
                ),
            });
        }
        let source = validate_source(document, module_name, module, source, target)?;
        scan_source_at(document, module_name, &source, target, mount)?;
    }
    let mut preserved = BTreeSet::<String>::new();
    for path in &mount.preserve {
        if !is_absolute_container_path(&path.path)
            || targets.contains(path.path.as_str())
            || !preserved.insert(path.path.clone())
        {
            return Err(MmError::Security {
                message: format!(
                    "module '{module_name}' preserved path '{}' is invalid or duplicated",
                    path.path
                ),
            });
        }
    }
    for path in mount.excludes.iter().chain(&mount.protected) {
        validate_subpath(module_name, path)?;
        let target = join_container_path(&mount.target, path)?;
        if targets.contains(target.as_str()) || !preserved.insert(target) {
            return Err(MmError::Security {
                message: format!("module '{module_name}' protected/excluded target is duplicated"),
            });
        }
    }
    for path in &mount.safe_exceptions {
        validate_subpath(module_name, path)?;
    }
    Ok(())
}

fn validate_source(
    document: &ConfigDocument,
    module_name: &str,
    module: &ModuleConfig,
    source: &Path,
    target: &str,
) -> Result<PathBuf, MmError> {
    if !is_absolute_container_path(target) {
        return Err(MmError::Security {
            message: format!(
                "module '{module_name}' development mount target '{target}' must be an absolute normalized container path"
            ),
        });
    }
    let source_text = source.to_str().ok_or_else(|| MmError::Security {
        message: format!("module '{module_name}' mount source must be valid Unicode"),
    })?;
    let normalized = HostPlatform::current()
        .normalize_relative(source_text)
        .map_err(|message| MmError::Security { message })?;
    if normalized != source {
        return Err(MmError::Security {
            message: format!(
                "module '{module_name}' mount source must use canonical portable separators"
            ),
        });
    }
    let project_root = dunce::canonicalize(&document.root).map_err(|source| MmError::Io {
        context: format!("resolving project root {}", document.root.display()),
        source,
    })?;
    let module_root =
        dunce::canonicalize(document.root.join(&module.path)).map_err(|source| MmError::Io {
            context: format!("resolving module '{module_name}' root"),
            source,
        })?;
    let source = dunce::canonicalize(module_root.join(source)).map_err(|source| MmError::Io {
        context: format!("resolving module '{module_name}' development mount source"),
        source,
    })?;
    if !source.starts_with(&project_root) || !source.starts_with(&module_root) {
        return Err(MmError::Security {
            message: format!(
                "module '{module_name}' development mount source escapes its project/module root after resolving symlinks"
            ),
        });
    }
    if !source.is_dir() {
        return Err(MmError::Security {
            message: format!("module '{module_name}' development mount source must be a directory"),
        });
    }
    Ok(source)
}

pub(super) fn validate_services(
    module_name: &str,
    module: &ModuleConfig,
    services: &[String],
) -> Result<(), MmError> {
    if services.is_empty() {
        return Err(MmError::Security {
            message: format!(
                "module '{module_name}' development mount must explicitly select at least one service"
            ),
        });
    }
    let mut unique = BTreeSet::new();
    for service in services {
        if !module.services.contains_key(service) {
            return Err(MmError::Security {
                message: format!(
                    "module '{module_name}' development mount selects unknown service '{service}'"
                ),
            });
        }
        if !unique.insert(service) {
            return Err(MmError::Security {
                message: format!(
                    "module '{module_name}' development mount selects service '{service}' more than once"
                ),
            });
        }
    }
    Ok(())
}

pub(super) fn validate_subpath(module_name: &str, path: &Path) -> Result<(), MmError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(MmError::Security {
            message: format!(
                "module '{module_name}' development mount subpaths must be portable and relative"
            ),
        });
    }
    Ok(())
}

pub(super) fn scan_source_at(
    document: &ConfigDocument,
    module_name: &str,
    source: &Path,
    target: &str,
    mount: &DevelopmentMountConfig,
) -> Result<(), MmError> {
    let mut masked = mount
        .preserve
        .iter()
        .filter_map(
            |preserve| match Path::new(&preserve.path).strip_prefix(target) {
                Ok(relative) if !relative.as_os_str().is_empty() => Some(relative),
                _ => None,
            },
        )
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    if target == mount.target {
        masked.extend(mount.excludes.iter().chain(&mount.protected).cloned());
    }
    let exceptions = mount
        .safe_exceptions
        .iter()
        .map(|path| normalize_relative_for_match(path))
        .collect::<BTreeSet<_>>();
    let mut pending = vec![source.to_path_buf()];
    let mut scanned = 0_usize;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(|source_error| MmError::Io {
            context: format!("scanning development mount source {}", directory.display()),
            source: source_error,
        })? {
            let entry = entry.map_err(|source_error| MmError::Io {
                context: format!("scanning development mount source {}", directory.display()),
                source: source_error,
            })?;
            scanned = checked_scan_count(module_name, scanned)?;
            let path = entry.path();
            let relative = path.strip_prefix(source).map_err(|_| MmError::Security {
                message: "development mount safety scan escaped its source".to_owned(),
            })?;
            if masked.iter().any(|mask| relative.starts_with(mask)) {
                continue;
            }
            let relative_match = normalize_relative_for_match(relative);
            let metadata = fs::symlink_metadata(&path).map_err(|source_error| MmError::Io {
                context: format!("inspecting development mount source {}", path.display()),
                source: source_error,
            })?;
            if metadata.file_type().is_symlink() {
                let resolved = dunce::canonicalize(&path).map_err(|source_error| MmError::Io {
                    context: format!("resolving development mount symlink {}", path.display()),
                    source: source_error,
                })?;
                if !resolved.starts_with(&document.root) || !resolved.starts_with(source) {
                    return Err(MmError::Security {
                        message: format!(
                            "module '{module_name}' development mount symlink '{}' escapes the mounted project/module source",
                            relative.display()
                        ),
                    });
                }
            }
            if sensitive_path(
                &relative_match,
                &document.config.development_mounts.sensitive_patterns,
            ) && !exceptions.contains(&relative_match)
            {
                return Err(MmError::Security {
                    message: format!(
                        "module '{module_name}' development mount would expose sensitive path '{}'; remove it, narrow the source, or review an explicit --safe-exception",
                        relative.display()
                    ),
                });
            }
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                pending.push(path);
            }
        }
    }
    Ok(())
}

pub(super) fn checked_scan_count(module_name: &str, scanned: usize) -> Result<usize, MmError> {
    let scanned = scanned.saturating_add(1);
    if scanned > MAX_SCANNED_ENTRIES {
        return Err(MmError::Security {
            message: format!(
                "module '{module_name}' development mount source exceeds the {MAX_SCANNED_ENTRIES}-entry safety scan limit"
            ),
        });
    }
    Ok(scanned)
}

pub(super) fn sensitive_path(relative: &str, configured: &[String]) -> bool {
    let lower = relative.to_ascii_lowercase();
    let components = lower.split('/').collect::<Vec<_>>();
    components.iter().any(|component| {
        *component == ".git"
            || *component == ".ssh"
            || (*component == ".env")
            || (component.starts_with(".env.") && *component != ".env.example")
            || *component == "id_rsa"
            || *component == "id_ed25519"
            || component.ends_with(".pem")
            || component.ends_with(".key")
            || *component == "secrets"
    }) || lower == ".config/mm.yaml"
        || lower.starts_with(".config/mm/")
        || configured
            .iter()
            .any(|pattern| wildcard_matches(&lower, &pattern.to_ascii_lowercase()))
}

pub(super) fn wildcard_matches(value: &str, pattern: &str) -> bool {
    let parts = pattern.split('*').collect::<Vec<_>>();
    if parts.len() == 1 {
        return value == pattern;
    }
    let mut remainder = value;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if index == 0 && !pattern.starts_with('*') {
            let Some(next) = remainder.strip_prefix(part) else {
                return false;
            };
            remainder = next;
        } else if let Some(position) = remainder.find(part) {
            remainder = &remainder[position + part.len()..];
        } else {
            return false;
        }
    }
    pattern.ends_with('*') || remainder.is_empty()
}

pub(super) fn reject_mount_conflicts(
    document: &ConfigDocument,
    mounts: &BTreeMap<String, DevelopmentMountConfig>,
) -> Result<(), MmError> {
    let mut service_targets = BTreeMap::<(&str, &str), (&str, &str)>::new();
    for (module, mount) in mounts {
        for service in &mount.services {
            let stack = document.config.modules[module].services[service]
                .stack
                .as_str();
            if let Some((owner_module, owner_target)) =
                service_targets.insert((stack, service), (module, &mount.target))
            {
                return Err(MmError::Security {
                    message: format!(
                        "modules '{owner_module}' and '{module}' both mount service '{service}' at '{owner_target}'/'{}'",
                        mount.target
                    ),
                });
            }
        }
    }
    Ok(())
}

pub(crate) fn join_container_path(target: &str, relative: &Path) -> Result<String, MmError> {
    let relative = normalize_relative_for_match(relative);
    normalize_container_path(&format!("{}/{}", target.trim_end_matches('/'), relative))
}

pub(super) fn normalize_relative_for_match(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn infrastructure_role(role: ServiceRole) -> bool {
    matches!(role, ServiceRole::Database | ServiceRole::Queue)
}

pub(super) fn infrastructure_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "postgres", "mysql", "mariadb", "mongo", "redis", "valkey", "rabbitmq", "kafka",
    ]
    .iter()
    .any(|candidate| lower == *candidate || lower.starts_with(&format!("{candidate}-")))
}

pub(super) fn sorted_unique(values: &[String]) -> Vec<String> {
    values
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn read_bounded(path: &Path) -> Result<Vec<u8>, MmError> {
    let metadata = fs::metadata(path).map_err(|source| MmError::Io {
        context: format!("reading discovery input {}", path.display()),
        source,
    })?;
    if !metadata.is_file() || metadata.len() > MAX_DISCOVERY_FILE_BYTES {
        return Err(MmError::Security {
            message: format!(
                "development mount discovery input '{}' is not a regular bounded file",
                path.display()
            ),
        });
    }
    fs::read(path).map_err(|source| MmError::Io {
        context: format!("reading discovery input {}", path.display()),
        source,
    })
}

pub(super) fn mapping_value<'a>(
    mapping: &'a serde_yaml_ng::Mapping,
    key: &str,
) -> Option<&'a serde_yaml_ng::Value> {
    mapping.get(serde_yaml_ng::Value::String(key.to_owned()))
}

pub(super) fn plan_hash(plan: &DevelopmentMountPlan) -> Result<String, MmError> {
    let bytes = serde_json::to_vec(plan).map_err(|error| MmError::State {
        message: format!("could not serialize development mount plan: {error}"),
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}
