use std::fs;
use std::io::Write;
use std::path::Path;

use atomic_write_file::AtomicWriteFile;
use fs4::FileExt;

use super::ConfigLoader;
use crate::app::ports::GuidedConfigEditor;
use crate::domain::error::MmError;
use crate::domain::onboarding::{
    ConfigLayoutPreview, ConfigLayoutRequest, ConfigMutationAction, ConfigMutationPreview,
    ConfigMutationRequest, ConfigResourceKind, DiscoveryConfidence, DiscoveryProposal,
    InitializationPreview, ProjectDiscovery,
};

impl GuidedConfigEditor for ConfigLoader {
    fn preview_initialization(
        &self,
        root: &Path,
        requested_name: Option<&str>,
    ) -> Result<InitializationPreview, MmError> {
        let (root, _, config_source) = super::loader::initialization_preview(root, requested_name)?;
        let (config, config_hash) = super::loader::parse_configuration(
            &root.join(super::CONFIG_RELATIVE_PATH),
            &config_source,
        )?;
        Ok(InitializationPreview {
            gitignore: super::loader::preview_gitignore(&root)?,
            config_path: root.join(super::CONFIG_RELATIVE_PATH),
            project_name: config.project.name,
            project_id: config.project.id,
            starter_files: vec![
                root.join(super::ANSIBLE_CONFIG_RELATIVE_PATH),
                root.join(super::ANSIBLE_PLAYBOOK_RELATIVE_PATH),
            ],
            root,
            config_source,
            config_hash,
        })
    }

    fn preview(
        &self,
        start: &Path,
        request: &ConfigMutationRequest,
    ) -> Result<ConfigMutationPreview, MmError> {
        let document = self.load_from(start)?;
        let location = resource_location(request)?;
        let top = location.parents.first().ok_or_else(|| MmError::State {
            message: "configuration resource has no top-level key".to_owned(),
        })?;
        // A split file holds the top-level key's value, so its root mapping is
        // the first parent.
        let target = document.source(top).to_path_buf();
        let split = target != document.path;
        let parents = if split {
            &location.parents[1..]
        } else {
            &location.parents[..]
        };
        let before = fs::read_to_string(&target).map_err(|source| MmError::ConfigRead {
            path: target.clone(),
            source,
        })?;
        let mut after = edit_resource(&target, &before, parents, &location.key, request)?;
        if split && !super::loader::has_yaml_content(&after) {
            if !after.is_empty() && !after.ends_with('\n') {
                after.push('\n');
            }
            after.push_str("{}\n");
        }
        super::loader::parse_configuration_with(
            &document.path,
            &[(target.clone(), Some(after.clone()))],
        )?;
        Ok(ConfigMutationPreview {
            config_path: target,
            source_hash: document.source_hash,
            before,
            after,
        })
    }

    fn commit(
        &self,
        preview: &ConfigMutationPreview,
        expected_source_hash: &str,
        expected_result_hash: &str,
    ) -> Result<crate::domain::config::ConfigDocument, MmError> {
        let anchor = super::loader::anchor_for_source(&preview.config_path);
        let lock = super::loader::lock_configuration(&anchor)?;
        let current =
            fs::read_to_string(&preview.config_path).map_err(|source| MmError::ConfigRead {
                path: preview.config_path.clone(),
                source,
            })?;
        let current_hash = super::loader::parse_configuration_with(
            &anchor,
            &[(preview.config_path.clone(), Some(current.clone()))],
        )?
        .source_hash;
        if current_hash != expected_source_hash || current != preview.before {
            return Err(MmError::State {
                message: "configuration changed after review; no changes were written".to_owned(),
            });
        }
        let result_hash = blake3::hash(preview.after.as_bytes()).to_hex().to_string();
        if result_hash != expected_result_hash {
            return Err(MmError::State {
                message: "reviewed configuration result does not match its plan".to_owned(),
            });
        }
        super::loader::parse_configuration_with(
            &anchor,
            &[(preview.config_path.clone(), Some(preview.after.clone()))],
        )?;
        let mut file = AtomicWriteFile::options()
            .open(&preview.config_path)
            .map_err(|source| MmError::Io {
                context: format!(
                    "opening {} for atomic update",
                    preview.config_path.display()
                ),
                source,
            })?;
        file.write_all(preview.after.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|source| MmError::Io {
                context: format!("writing {}", preview.config_path.display()),
                source,
            })?;
        file.commit().map_err(|source| MmError::Io {
            context: format!("publishing {}", preview.config_path.display()),
            source,
        })?;
        let document = self.load(&anchor)?;
        FileExt::unlock(&lock).map_err(|source| MmError::Io {
            context: format!("unlocking configuration {}", anchor.display()),
            source,
        })?;
        Ok(document)
    }

    fn definition(
        &self,
        start: &Path,
        request: &ConfigMutationRequest,
    ) -> Result<Option<serde_json::Value>, MmError> {
        let document = self.load_from(start)?;
        let location = resource_location(request)?;
        let top = location.parents.first().ok_or_else(|| MmError::State {
            message: "configuration resource has no top-level key".to_owned(),
        })?;
        // A split file holds the top-level key's value at its root.
        let target = document.source(top).to_path_buf();
        let parents = if target == document.path {
            &location.parents[..]
        } else {
            &location.parents[1..]
        };
        let source = fs::read_to_string(&target).map_err(|source| MmError::ConfigRead {
            path: target.clone(),
            source,
        })?;
        let root: serde_json::Value =
            serde_yaml_ng::from_str(&source).map_err(|error| MmError::ConfigParse {
                path: target.clone(),
                message: error.to_string(),
            })?;
        Ok(parents
            .iter()
            .chain(std::iter::once(&location.key))
            .try_fold(&root, |value, key| value.get(key))
            .cloned())
    }

    fn discover_project(&self, root: &Path) -> Result<ProjectDiscovery, MmError> {
        discover_project(root)
    }

    fn preview_layout(
        &self,
        start: &Path,
        request: &ConfigLayoutRequest,
    ) -> Result<ConfigLayoutPreview, MmError> {
        super::layout::preview_layout(self, start, request)
    }

    fn commit_layout(
        &self,
        preview: &ConfigLayoutPreview,
        expected_source_hash: &str,
        expected_result_hash: &str,
    ) -> Result<crate::domain::config::ConfigDocument, MmError> {
        super::layout::commit_layout(self, preview, expected_source_hash, expected_result_hash)
    }
}

struct ResourceLocation {
    parents: Vec<String>,
    key: String,
}

fn resource_location(request: &ConfigMutationRequest) -> Result<ResourceLocation, MmError> {
    let parent = || {
        request.parent.clone().ok_or_else(|| MmError::Environment {
            message: format!("{} requires --parent", request.resource.label()),
        })
    };
    let location = match request.resource {
        ConfigResourceKind::Module => ResourceLocation {
            parents: vec!["modules".to_owned()],
            key: request.name.clone(),
        },
        ConfigResourceKind::Service => ResourceLocation {
            parents: vec!["modules".to_owned(), parent()?, "services".to_owned()],
            key: request.name.clone(),
        },
        ConfigResourceKind::Stack => ResourceLocation {
            parents: vec!["stacks".to_owned()],
            key: request.name.clone(),
        },
        ConfigResourceKind::Build => ResourceLocation {
            parents: vec!["modules".to_owned(), request.name.clone()],
            key: "build".to_owned(),
        },
        ConfigResourceKind::Environment => ResourceLocation {
            parents: vec!["environments".to_owned()],
            key: request.name.clone(),
        },
        ConfigResourceKind::Binding => ResourceLocation {
            parents: request.parent.as_ref().map_or_else(
                || vec!["bindings".to_owned()],
                |environment| {
                    vec![
                        "environments".to_owned(),
                        environment.clone(),
                        "bindings".to_owned(),
                    ]
                },
            ),
            key: request.name.clone(),
        },
        ConfigResourceKind::Secret => ResourceLocation {
            parents: vec!["secrets".to_owned()],
            key: request.name.clone(),
        },
        ConfigResourceKind::Ingress => ResourceLocation {
            parents: vec!["stacks".to_owned(), parent()?],
            key: "ingress".to_owned(),
        },
        ConfigResourceKind::LiveMount => ResourceLocation {
            parents: vec!["modules".to_owned(), request.name.clone()],
            key: "development-mount".to_owned(),
        },
        ConfigResourceKind::Job => ResourceLocation {
            parents: vec!["jobs".to_owned()],
            key: request.name.clone(),
        },
        ConfigResourceKind::Schedule => ResourceLocation {
            parents: vec!["schedules".to_owned()],
            key: request.name.clone(),
        },
        ConfigResourceKind::Gate => ResourceLocation {
            parents: vec!["gates".to_owned()],
            key: request.name.clone(),
        },
        ConfigResourceKind::User => ResourceLocation {
            parents: vec!["users".to_owned()],
            key: request.name.clone(),
        },
    };
    Ok(location)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MappingRange {
    start: usize,
    end: usize,
    indent: usize,
    flow_empty: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct EntryRange {
    pub(super) start: usize,
    pub(super) end: usize,
}

fn edit_resource(
    path: &Path,
    source: &str,
    parents: &[String],
    key: &str,
    request: &ConfigMutationRequest,
) -> Result<String, MmError> {
    validate_yaml_key(key)?;
    for parent in parents {
        validate_yaml_key(parent)?;
    }
    let (mapping, missing_at) = locate_mapping(path, source, parents)?;
    if let Some(index) = missing_at {
        if request.action != ConfigMutationAction::Add {
            return Err(MmError::Environment {
                message: format!(
                    "{} '{}' is not configured",
                    request.resource.label(),
                    request.name
                ),
            });
        }
        let definition = request.definition.as_ref().ok_or_else(|| MmError::State {
            message: "configuration definition is missing".to_owned(),
        })?;
        return insert_missing_path(source, mapping, &parents[index..], key, definition);
    }
    let existing = direct_entry(source, mapping, key);
    match request.action {
        ConfigMutationAction::Add if existing.is_some() => Err(MmError::Environment {
            message: format!(
                "{} '{}' is already configured",
                request.resource.label(),
                request.name
            ),
        }),
        ConfigMutationAction::Edit | ConfigMutationAction::Remove if existing.is_none() => {
            Err(MmError::Environment {
                message: format!(
                    "{} '{}' is not configured",
                    request.resource.label(),
                    request.name
                ),
            })
        }
        ConfigMutationAction::Remove => {
            let entry = existing.ok_or_else(|| MmError::State {
                message: "configuration entry disappeared during preview".to_owned(),
            })?;
            let mut output = source.to_owned();
            output.replace_range(entry.start..entry.end, "");
            Ok(output)
        }
        ConfigMutationAction::Add | ConfigMutationAction::Edit => {
            let definition = request.definition.as_ref().ok_or_else(|| MmError::State {
                message: "configuration definition is missing".to_owned(),
            })?;
            let rendered = render_entry(key, definition, mapping.indent);
            let mut output = source.to_owned();
            if let Some(entry) = existing {
                output.replace_range(entry.start..entry.end, &rendered);
            } else if mapping.flow_empty {
                output.replace_range(
                    mapping.start..mapping.end,
                    &flow_replacement(mapping, &rendered),
                );
            } else {
                output.insert_str(mapping.end, &rendered);
            }
            Ok(output)
        }
    }
}

fn locate_mapping(
    path: &Path,
    source: &str,
    parents: &[String],
) -> Result<(MappingRange, Option<usize>), MmError> {
    let mut mapping = root_mapping(source);
    for (index, parent) in parents.iter().enumerate() {
        let Some(entry) = direct_entry(source, mapping, parent) else {
            return Ok((mapping, Some(index)));
        };
        let line_end = source[entry.start..]
            .find('\n')
            .map_or(source.len(), |offset| entry.start + offset + 1);
        let line = source[entry.start..line_end].trim_end_matches(['\r', '\n']);
        let value = line.split_once(':').map_or("", |(_, value)| value.trim());
        if value == "{}" {
            let value_offset = line.find("{}").ok_or_else(|| MmError::State {
                message: "empty mapping location could not be resolved".to_owned(),
            })?;
            mapping = MappingRange {
                start: entry.start + value_offset,
                end: entry.start + value_offset + 2,
                indent: mapping.indent + 2,
                flow_empty: true,
            };
        } else if value.is_empty() {
            mapping = MappingRange {
                start: line_end,
                end: entry.end,
                indent: mapping.indent + 2,
                flow_empty: false,
            };
        } else {
            return Err(MmError::ConfigParse {
                path: path.to_path_buf(),
                message: format!("'{parent}' must be a block mapping for a lossless edit"),
            });
        }
    }
    Ok((mapping, None))
}

/// The whole file as a mapping. A split file whose only content is `{}` is an
/// empty flow mapping that the first added entry replaces.
pub(super) fn root_mapping(source: &str) -> MappingRange {
    let mut offset = 0;
    let mut empty_flow = None;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            if trimmed == "{}" && empty_flow.is_none() {
                empty_flow = line.find("{}").map(|column| offset + column);
            } else {
                empty_flow = None;
                break;
            }
        }
        offset += line.len();
    }
    match empty_flow {
        Some(start) => MappingRange {
            start,
            end: start + 2,
            indent: 0,
            flow_empty: true,
        },
        None => MappingRange {
            start: 0,
            end: source.len(),
            indent: 0,
            flow_empty: false,
        },
    }
}

pub(super) fn direct_entry(source: &str, mapping: MappingRange, key: &str) -> Option<EntryRange> {
    if mapping.flow_empty {
        return None;
    }
    let needle = format!("{}{key}:", " ".repeat(mapping.indent));
    let mut offset = mapping.start;
    while offset < mapping.end {
        let next = source[offset..mapping.end]
            .find('\n')
            .map_or(mapping.end, |relative| offset + relative + 1);
        let line = source[offset..next].trim_end_matches(['\r', '\n']);
        if line.starts_with(&needle)
            && line[needle.len()..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
        {
            let mut end = next;
            while end < mapping.end {
                let child_next = source[end..mapping.end]
                    .find('\n')
                    .map_or(mapping.end, |relative| end + relative + 1);
                let child = source[end..child_next].trim_end_matches(['\r', '\n']);
                if !child.trim().is_empty() && !child.trim_start().starts_with('#') {
                    let indent = child.len().saturating_sub(child.trim_start().len());
                    if indent <= mapping.indent {
                        break;
                    }
                }
                end = child_next;
            }
            return Some(EntryRange { start: offset, end });
        }
        offset = next;
    }
    None
}

fn insert_missing_path(
    source: &str,
    mapping: MappingRange,
    missing: &[String],
    key: &str,
    definition: &serde_json::Value,
) -> Result<String, MmError> {
    let first = missing.first().ok_or_else(|| MmError::State {
        message: "missing configuration path is empty".to_owned(),
    })?;
    let mut value = serde_json::Map::new();
    value.insert(key.to_owned(), definition.clone());
    for component in missing.iter().skip(1).rev() {
        let mut parent = serde_json::Map::new();
        parent.insert(component.clone(), serde_json::Value::Object(value));
        value = parent;
    }
    let nested = serde_json::Value::Object(value);
    let mut output = source.to_owned();
    if mapping.flow_empty {
        output.replace_range(
            mapping.start..mapping.end,
            &flow_replacement(mapping, &render_entry(first, &nested, mapping.indent)),
        );
    } else {
        output.insert_str(mapping.end, &render_entry(first, &nested, mapping.indent));
    }
    Ok(output)
}

/// Text that replaces an empty `{}` mapping with its first block entry. At the
/// root of a split file the `{}` starts its own line, which already ends in a
/// newline.
fn flow_replacement(mapping: MappingRange, rendered: &str) -> String {
    if mapping.indent == 0 {
        rendered.trim_end_matches('\n').to_owned()
    } else {
        format!("\n{rendered}")
    }
}

fn render_entry(key: &str, definition: &serde_json::Value, indent: usize) -> String {
    format!(
        "{}{}:\n{}",
        " ".repeat(indent),
        key,
        render_entry_content(definition, indent + 2)
    )
}

fn render_entry_content(definition: &serde_json::Value, indent: usize) -> String {
    let yaml = serde_yaml_ng::to_string(definition).unwrap_or_else(|_| "null\n".to_owned());
    yaml.lines()
        .map(|line| format!("{}{}\n", " ".repeat(indent), line))
        .collect()
}

fn validate_yaml_key(key: &str) -> Result<(), MmError> {
    if key.is_empty()
        || !key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
    {
        return Err(MmError::Environment {
            message: format!("'{key}' is not a safe portable configuration key"),
        });
    }
    Ok(())
}

fn discover_project(root: &Path) -> Result<ProjectDiscovery, MmError> {
    let root = dunce::canonicalize(root).map_err(|source| MmError::Io {
        context: format!("resolving project root {}", root.display()),
        source,
    })?;
    let mut proposals = Vec::new();
    for name in [
        "compose.yaml",
        "compose.yml",
        "docker-compose.yaml",
        "docker-compose.yml",
    ] {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        proposals.push(DiscoveryProposal {
            subject: "compose-file".to_owned(),
            proposed_value: name.to_owned(),
            confidence: DiscoveryConfidence::High,
            reasons: vec!["recognized Compose filename".to_owned()],
        });
        discover_compose(&path, &mut proposals)?;
    }
    let entries = fs::read_dir(&root).map_err(|source| MmError::Io {
        context: format!("reading project root {}", root.display()),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| MmError::Io {
            context: format!("reading project root {}", root.display()),
            source,
        })?;
        let path = entry.path();
        if !path.is_dir() || path.file_name().is_some_and(|name| name == ".config") {
            continue;
        }
        let module = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("module");
        let indicators = ["Dockerfile", "package.json", "Cargo.toml", "composer.json"]
            .into_iter()
            .filter(|name| path.join(name).is_file())
            .collect::<Vec<_>>();
        if !indicators.is_empty() {
            proposals.push(DiscoveryProposal {
                subject: format!("module:{module}"),
                proposed_value: module.to_owned(),
                confidence: DiscoveryConfidence::High,
                reasons: vec![format!("found {}", indicators.join(", "))],
            });
            proposals.push(DiscoveryProposal {
                subject: format!("development-mount:{module}"),
                proposed_value: module.to_owned(),
                confidence: DiscoveryConfidence::Medium,
                reasons: vec![
                    "source directory has a recognized build manifest; confirm the container target"
                        .to_owned(),
                ],
            });
        }
        for template in [".env.example", ".env.template", ".env.sample"] {
            if path.join(template).is_file() {
                proposals.push(DiscoveryProposal {
                    subject: format!("environment-template:{module}"),
                    proposed_value: format!("{module}/{template}"),
                    confidence: DiscoveryConfidence::High,
                    reasons: vec![
                        "recognized public environment template; values were not read".to_owned(),
                    ],
                });
            }
        }
    }
    proposals.sort_by(|left, right| {
        left.subject
            .cmp(&right.subject)
            .then_with(|| left.proposed_value.cmp(&right.proposed_value))
    });
    Ok(ProjectDiscovery { root, proposals })
}

fn discover_compose(path: &Path, proposals: &mut Vec<DiscoveryProposal>) -> Result<(), MmError> {
    let source = fs::read_to_string(path).map_err(|source| MmError::ConfigRead {
        path: path.to_path_buf(),
        source,
    })?;
    let value = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&source).map_err(|error| {
        MmError::ConfigParse {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    let Some(services) = value
        .as_mapping()
        .and_then(|root| root.get("services"))
        .and_then(serde_yaml_ng::Value::as_mapping)
    else {
        return Ok(());
    };
    for (name, service) in services {
        let Some(name) = name.as_str() else {
            continue;
        };
        let mapping = service.as_mapping();
        let mut reasons = vec!["declared in Compose services".to_owned()];
        if mapping.and_then(|service| service.get("build")).is_some() {
            reasons.push("has a Compose build declaration".to_owned());
        }
        if mapping.and_then(|service| service.get("ports")).is_some() {
            reasons.push("publishes one or more ports; confirm the HTTP port".to_owned());
        }
        if mapping.and_then(|service| service.get("volumes")).is_some() {
            reasons.push("declares volumes that may indicate a development mount".to_owned());
        }
        if mapping
            .and_then(|service| service.get("env_file"))
            .is_some()
        {
            reasons.push(
                "references environment files; secret plaintext was not inspected".to_owned(),
            );
        }
        proposals.push(DiscoveryProposal {
            subject: format!("service:{name}"),
            proposed_value: name.to_owned(),
            confidence: DiscoveryConfidence::High,
            reasons,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "guided_tests.rs"]
mod tests;
