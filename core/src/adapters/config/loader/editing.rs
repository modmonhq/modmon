fn edit_default_environment(source: &str, value: Option<&str>) -> Result<String, String> {
    const KEY: &str = "default-environment";
    let file = YamlFile::from_str(source).map_err(|error| error.to_string())?;
    let documents = file.documents().collect::<Vec<_>>();
    if documents.len() != 1 {
        return Err("expected exactly one YAML document".to_owned());
    }
    let root = documents[0]
        .as_mapping()
        .ok_or_else(|| "configuration root must be a mapping".to_owned())?;
    if root.is_flow_style() {
        return Err("flow-style root mappings cannot be edited safely".to_owned());
    }
    let entries = root.find_all_entries_by_key(KEY).collect::<Vec<_>>();
    if entries.len() > 1 {
        return Err("configuration contains duplicate default-environment keys".to_owned());
    }
    match (entries.into_iter().next(), value) {
        (Some(entry), Some(value)) => {
            let current = entry
                .value_node()
                .filter(|node| node.is_scalar())
                .ok_or_else(|| "default-environment must be a scalar".to_owned())?;
            if !current.yaml_eq(&value) {
                entry.set_value(value, false);
            }
        }
        (Some(entry), None) => entry.remove(),
        (None, Some(value)) => {
            if source.contains("\r\n") {
                return Err(
                    "adding default-environment to a CRLF file is not yet lossless".to_owned(),
                );
            }
            root.set_with_field_order(
                KEY,
                value,
                [
                    "default-environment",
                    "project",
                    "target",
                    "runtime",
                    "stacks",
                    "modules",
                    "secrets",
                    "environments",
                ],
            );
        }
        (None, None) => {}
    }
    Ok(file.to_string())
}

/// The whole text of a split file whose value is one optional scalar.
fn edit_split_scalar(value: Option<&str>) -> Result<String, String> {
    serde_yaml_ng::to_string(&value).map_err(|error| error.to_string())
}

/// `split` means `source` is `.config/mm/environments.yaml`, whose root mapping
/// is the environments mapping itself.
fn edit_environment_delivery(
    source: &str,
    environment: &str,
    mode: EnvironmentDeliveryMode,
    split: bool,
) -> Result<String, String> {
    const ENVIRONMENTS: &str = "environments";
    const KEY: &str = "environment-delivery";
    let file = YamlFile::from_str(source).map_err(|error| error.to_string())?;
    let documents = file.documents().collect::<Vec<_>>();
    if documents.len() != 1 {
        return Err("expected exactly one YAML document".to_owned());
    }
    let root = documents[0]
        .as_mapping()
        .ok_or_else(|| "configuration root must be a mapping".to_owned())?;
    if root.is_flow_style() {
        return Err("flow-style root mappings cannot be edited safely".to_owned());
    }
    let environments = if split {
        root
    } else {
        root.get_mapping(ENVIRONMENTS)
            .ok_or_else(|| "environments must be a mapping".to_owned())?
    };
    if environments.is_flow_style() {
        return Err("flow-style environment mappings cannot be edited safely".to_owned());
    }
    let environment_mapping = environments.get_mapping(environment).ok_or_else(|| {
        format!("environment '{environment}' must be a mapping to set environment-delivery")
    })?;
    if environment_mapping.is_flow_style() {
        return Err("flow-style environment entries cannot be edited safely".to_owned());
    }
    let entries = environment_mapping
        .find_all_entries_by_key(KEY)
        .collect::<Vec<_>>();
    if entries.len() > 1 {
        return Err(format!(
            "environment '{environment}' contains duplicate environment-delivery keys"
        ));
    }
    match entries.into_iter().next() {
        Some(entry) => {
            let current = entry
                .value_node()
                .filter(|node| node.is_scalar())
                .ok_or_else(|| "environment-delivery must be a scalar".to_owned())?;
            if !current.yaml_eq(&mode.as_str()) {
                entry.set_value(mode.as_str(), false);
            }
        }
        None => {
            if source.contains("\r\n") {
                return Err(
                    "adding environment-delivery to a CRLF file is not yet lossless".to_owned(),
                );
            }
            let insertion = usize::try_from(environment_mapping.byte_range().end)
                .map_err(|_| "environment byte range does not fit in memory".to_owned())?;
            if insertion > source.len() || !source.is_char_boundary(insertion) {
                return Err("environment byte range is outside the configuration".to_owned());
            }
            let key_start = environment_mapping
                .entries()
                .next()
                .and_then(|entry| entry.key_node())
                .and_then(|key| {
                    key.as_scalar()
                        .and_then(|key| usize::try_from(key.byte_range().start).ok())
                })
                .ok_or_else(|| {
                    "environment mapping has no block entry to infer indentation".to_owned()
                })?;
            let line_start = source[..key_start]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            let indent = source[line_start..key_start]
                .chars()
                .take_while(|character| *character == ' ')
                .count();
            let indent = " ".repeat(indent);
            let mut output = source.to_owned();
            // A mapping that ends the file already ends with its newline.
            let separator = if output[..insertion].ends_with('\n') { "" } else { "\n" };
            output.insert_str(
                insertion,
                &format!("{separator}{indent}{KEY}: {}\n", mode.as_str()),
            );
            return Ok(output);
        }
    }
    Ok(file.to_string())
}

/// The modules mapping: the root of `.config/mm/modules.yaml` when split, or
/// the anchor's `modules:` entry.
fn modules_mapping(root: yaml_edit::Mapping, split: bool) -> Option<yaml_edit::Mapping> {
    if split {
        Some(root)
    } else {
        root.get_mapping("modules")
    }
}

/// `split` means `source` is `.config/mm/modules.yaml`, whose root mapping is
/// the modules mapping itself.
fn edit_development_mount(
    source: &str,
    module: &str,
    mount: Option<&crate::domain::development_mount::DevelopmentMountConfig>,
    split: bool,
) -> Result<String, String> {
    const KEY: &str = "development-mount";
    let file = YamlFile::from_str(source).map_err(|error| error.to_string())?;
    let documents = file.documents().collect::<Vec<_>>();
    if documents.len() != 1 {
        return Err("expected exactly one YAML document".to_owned());
    }
    let root = documents[0]
        .as_mapping()
        .ok_or_else(|| "configuration root must be a mapping".to_owned())?;
    let modules =
        modules_mapping(root, split).ok_or_else(|| "modules must be a mapping".to_owned())?;
    let module_mapping = modules
        .get_mapping(module)
        .ok_or_else(|| format!("module '{module}' must be a mapping"))?;
    if modules.is_flow_style() || module_mapping.is_flow_style() {
        return Err("flow-style module mappings cannot be edited safely".to_owned());
    }
    let entries = module_mapping
        .find_all_entries_by_key(KEY)
        .collect::<Vec<_>>();
    if entries.len() > 1 {
        return Err(format!(
            "module '{module}' contains duplicate development-mount keys"
        ));
    }
    let without_mount = if entries.is_empty() {
        source.to_owned()
    } else {
        remove_development_mount_block(source, &module_mapping)?
    };
    match mount {
        Some(mount) => {
            if source.contains("\r\n") {
                return Err(
                    "adding development-mount to a CRLF file is not yet lossless".to_owned(),
                );
            }
            #[derive(serde::Serialize)]
            struct MountValue<'a> {
                #[serde(rename = "development-mount")]
                mount: &'a crate::domain::development_mount::DevelopmentMountConfig,
            }
            let generated = serde_yaml_ng::to_string(&MountValue { mount })
                .map_err(|error| format!("could not serialize development mount: {error}"))?;
            let reparsed = YamlFile::from_str(&without_mount).map_err(|error| error.to_string())?;
            let root = reparsed
                .documents()
                .next()
                .and_then(|document| document.as_mapping())
                .ok_or_else(|| "configuration root must be a mapping".to_owned())?;
            let module_mapping = modules_mapping(root, split)
                .and_then(|modules| modules.get_mapping(module))
                .ok_or_else(|| format!("module '{module}' must be a mapping"))?;
            let insertion = usize::try_from(module_mapping.byte_range().end)
                .map_err(|_| "module byte range does not fit in memory".to_owned())?;
            if insertion > without_mount.len() || !without_mount.is_char_boundary(insertion) {
                return Err("module byte range is outside the configuration".to_owned());
            }
            let key_start = module_mapping
                .entries()
                .next()
                .and_then(|entry| entry.key_node())
                .and_then(|key| {
                    key.as_scalar()
                        .and_then(|key| usize::try_from(key.byte_range().start).ok())
                })
                .ok_or_else(|| {
                    "module mapping has no block entry to infer indentation".to_owned()
                })?;
            let line_start = without_mount[..key_start]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            let indent = without_mount[line_start..key_start]
                .chars()
                .take_while(|character| *character == ' ')
                .collect::<String>();
            let generated = generated
                .trim_end()
                .lines()
                .map(|line| format!("{indent}{line}"))
                .collect::<Vec<_>>()
                .join("\n");
            let mut output = without_mount;
            let separator = if output[..insertion].ends_with('\n') { "" } else { "\n" };
            output.insert_str(insertion, &format!("{separator}{generated}\n"));
            Ok(output)
        }
        None => Ok(without_mount),
    }
}

fn remove_development_mount_block(
    source: &str,
    module_mapping: &yaml_edit::Mapping,
) -> Result<String, String> {
    const KEY: &str = "development-mount";
    let range = module_mapping.byte_range();
    let range_start = usize::try_from(range.start)
        .map_err(|_| "module byte range does not fit in memory".to_owned())?;
    let range_end = usize::try_from(range.end)
        .map_err(|_| "module byte range does not fit in memory".to_owned())?;
    let indent = module_mapping.detect_indentation_level();
    let needle = format!("{}{KEY}:", " ".repeat(indent));
    let mut offset = 0;
    let mut block_start = None;
    let mut block_end = source.len();
    for line in source.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        if block_start.is_none() {
            if offset >= range_start && offset < range_end && content.trim_end() == needle {
                block_start = Some(offset);
            }
        } else if !content.trim().is_empty() {
            let leading = content.len().saturating_sub(content.trim_start().len());
            if leading <= indent {
                block_end = offset;
                break;
            }
        }
        offset += line.len();
    }
    let block_start = block_start.ok_or_else(|| {
        "development-mount uses unsupported key formatting and cannot be edited losslessly"
            .to_owned()
    })?;
    let mut output = source.to_owned();
    output.replace_range(block_start..block_end, "");
    Ok(output)
}

fn discover_compose_file(root: &Path) -> Option<PathBuf> {
    [
        "compose.yaml",
        "compose.yml",
        "docker-compose.yaml",
        "docker-compose.yml",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|candidate| root.join(candidate).is_file())
}

fn initial_config(root: &Path, requested_name: Option<&str>) -> MmConfig {
    let name = requested_name.map_or_else(|| project_name_from_root(root), sanitize_name);
    let compose_file = discover_compose_file(root).unwrap_or_else(|| PathBuf::from("compose.yaml"));
    MmConfig {
        schema_version: CONFIG_SCHEMA_VERSION,
        project: ProjectConfig {
            id: deterministic_project_id(root, &name),
            name,
            development_environment: Some("local".to_owned()),
        },
        default_environment: Some("local".to_owned()),
        target: "local".to_owned(),
        runtime: RuntimeConfig {
            driver: Some(crate::domain::runtime::RuntimeKind::PodmanCompose),
        },
        build: Default::default(),
        networks: Default::default(),
        stacks: [(
            "app".to_owned(),
            StackConfig {
                project_name: None,
                files: vec![compose_file],
                networks: Vec::new(),
                ingress: Default::default(),
                shared_volumes: Vec::new(),
                after: Vec::new(),
                storage: None,
            },
        )]
        .into_iter()
        .collect(),
        modules: Default::default(),
        providers: Default::default(),
        secrets: Default::default(),
        bindings: Default::default(),
        environment_delivery: EnvironmentDeliveryPolicy {
            default: Some(EnvironmentDeliveryMode::Direct),
            development_default: Some(EnvironmentDeliveryMode::EnvFile),
        },
        environments: [("local".to_owned(), EnvironmentConfig::default())]
            .into_iter()
            .collect(),
        ingress: Default::default(),
        development_mounts: Default::default(),
        development_mail: Default::default(),
        jobs: Default::default(),
        schedules: Default::default(),
        gates: Default::default(),
        users: Default::default(),
        agents: crate::domain::agents::initial_agents("local"),
        targets: Default::default(),
        registries: Default::default(),
        hardening: None,
        release_inputs: Default::default(),
        host_runtimes: None,
        release: None,
        development: Default::default(),
    }
}

pub(super) fn initialization_preview(
    root: &Path,
    requested_name: Option<&str>,
) -> Result<(PathBuf, MmConfig, String), MmError> {
    let root = dunce::canonicalize(root).map_err(|source| MmError::Io {
        context: format!("resolving project root {}", root.display()),
        source,
    })?;
    let path = root.join(CONFIG_RELATIVE_PATH);
    if path.exists() {
        return Err(MmError::ConfigAlreadyExists { path });
    }
    let config = initial_config(&root, requested_name);
    let source = serde_yaml_ng::to_string(&config).map_err(|error| MmError::ConfigParse {
        path,
        message: error.to_string(),
    })?;
    Ok((root, config, source))
}

fn deterministic_project_id(root: &Path, name: &str) -> ProjectId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"mm-project-id-v1\0");
    hasher.update(root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(name.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    ProjectId(Uuid::from_bytes(bytes))
}

fn project_name_from_root(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| "project".to_owned(), sanitize_name)
}

fn sanitize_name(name: &str) -> String {
    let sanitized = name
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let sanitized = sanitized
        .trim_matches(|character| matches!(character, '-' | '_'))
        .to_owned();

    if sanitized.is_empty() {
        "project".to_owned()
    } else {
        sanitized
    }
}

#[cfg(test)]
#[path = "editing/tests.rs"]
mod editing_tests;
