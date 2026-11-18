impl ConfigLoader {
    /// Replaces `development.enabled` of `.config/mm.overrides.yaml` under
    /// the configuration lock, and creates the file when it is missing.
    pub fn set_development_enabled(
        &self,
        start: &Path,
        enabled: &[String],
    ) -> Result<ConfigDocument, MmError> {
        let path = self.discover(start)?;
        let lock = lock_configuration(&path)?;
        let document = self.load(&path)?;
        let target = path.with_file_name("mm.overrides.yaml");
        ensure_no_symlink_components(&target)?;
        let source = read_regular_file(&target, CONFIG_SOURCE_BYTES)?
            .map(String::from_utf8)
            .transpose()
            .map_err(|_| MmError::ConfigParse {
                path: target.clone(),
                message: "configuration must be valid UTF-8".to_owned(),
            })?;
        if source.is_none() {
            if enabled.is_empty() {
                return Ok(document);
            }
            refuse_unignored_overrides(&document, &target)?;
        }
        let edited = edit_local_development(source.as_deref(), enabled).map_err(|message| {
            MmError::ConfigParse {
                path: target.clone(),
                message,
            }
        })?;
        parse_configuration_with(&document.path, &[(target.clone(), Some(edited.clone()))])?;
        let mut file = AtomicWriteFile::options()
            .open(&target)
            .map_err(|source| MmError::Io {
                context: format!("opening {} for atomic update", target.display()),
                source,
            })?;
        file.write_all(edited.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|source| MmError::Io {
                context: format!("writing {}", target.display()),
                source,
            })?;
        let current = read_regular_file(&target, CONFIG_SOURCE_BYTES)?
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        if current != source || self.load(&document.path)?.source_hash != document.source_hash {
            return Err(MmError::State {
                message: "configuration changed while it was being edited; retry the command"
                    .to_owned(),
            });
        }
        file.commit().map_err(|source| MmError::Io {
            context: format!("publishing {}", target.display()),
            source,
        })?;
        let updated = self.load(&document.path)?;
        FileExt::unlock(&lock).map_err(|source| MmError::Io {
            context: format!("unlocking configuration {}", document.path.display()),
            source,
        })?;
        Ok(updated)
    }
}

/// The units this machine runs must stay on it, so the file is only created
/// where git ignores it.
fn refuse_unignored_overrides(document: &ConfigDocument, target: &Path) -> Result<(), MmError> {
    let directory = target.parent().unwrap_or(&document.root);
    match git_ignored(document, directory, vec![target.to_path_buf()]) {
        Some(ignored) if ignored.is_empty() => Err(MmError::ConfigParse {
            path: target.to_path_buf(),
            message: format!(
                "git does not ignore {OVERRIDES_RELATIVE_PATH}, so the units this machine runs would be committed; add /{OVERRIDES_RELATIVE_PATH} to the project's .gitignore and run the command again"
            ),
        }),
        _ => Ok(()),
    }
}

/// `source` with its top-level `development:` listing `enabled`, or without
/// it when `enabled` is empty. Every other line stays as it was.
fn edit_local_development(source: Option<&str>, enabled: &[String]) -> Result<String, String> {
    let source = source.unwrap_or_default();
    if source.contains("\r\n") {
        return Err(format!(
            "{OVERRIDES_RELATIVE_PATH} uses CRLF line endings, which cannot be edited losslessly yet; set development.enabled by hand"
        ));
    }
    let mut output = String::new();
    let mut skipping = false;
    let mut found = 0;
    for line in source.split_inclusive('\n') {
        let content = line.trim_end_matches('\n');
        let top_level = !content.is_empty() && !content.starts_with([' ', '\t']);
        if top_level {
            skipping = is_development_key(content);
            found += usize::from(skipping);
        }
        if !skipping {
            output.push_str(line);
        }
    }
    if found > 1 {
        return Err("the local overrides set `development:` more than once".to_owned());
    }
    let content = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && *line != "---")
        .collect::<Vec<_>>();
    if content == ["{}"] {
        output = output
            .lines()
            .filter(|line| line.trim() != "{}")
            .map(|line| format!("{line}\n"))
            .collect();
    } else if has_yaml_content(&output) {
        let file = YamlFile::from_str(&output).map_err(|error| error.to_string())?;
        let flow = file
            .documents()
            .next()
            .and_then(|document| document.as_mapping())
            .is_none_or(|root| root.is_flow_style());
        if flow {
            return Err(format!(
                "{OVERRIDES_RELATIVE_PATH} must be a block mapping to be edited; set development.enabled by hand"
            ));
        }
    }
    if !enabled.is_empty() {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str("development:\n  enabled:\n");
        for name in enabled {
            let scalar = serde_yaml_ng::to_string(name).map_err(|error| error.to_string())?;
            output.push_str(&format!("    - {}\n", scalar.trim_end()));
        }
    }
    if !has_yaml_content(&output) {
        output.push_str("{}\n");
    }
    Ok(output)
}

fn is_development_key(line: &str) -> bool {
    line.strip_prefix("development:")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t', '#']))
}
