use super::*;

pub(super) fn proposal(
    document: &ConfigDocument,
    request: &DevelopmentMountPlanRequest,
) -> Result<DevelopmentMountProposal, MmError> {
    let module = document
        .config
        .modules
        .get(&request.module)
        .ok_or_else(|| MmError::Environment {
            message: format!("module '{}' is not configured", request.module),
        })?;
    let discovered_preset = discover_preset(&document.root.join(&module.path))?;
    let preset = request
        .preset
        .or_else(|| {
            module
                .development_mount
                .as_ref()
                .and_then(|mount| mount.preset)
        })
        .or(discovered_preset);
    let selected_services = if !request.services.is_empty() {
        sorted_unique(&request.services)
    } else if let Some(configured) = &module.development_mount {
        configured.services.clone()
    } else {
        module.services.keys().cloned().collect()
    };
    validate_services(&request.module, module, &selected_services)?;

    let (evidence, mut confirmations) =
        collect_evidence(document, module, request, &selected_services, preset)?;
    append_evidence_confirmations(module, &selected_services, &evidence, &mut confirmations);
    let selected_target = request.target.clone().or_else(|| select_target(&evidence));
    if selected_target.is_none() {
        confirmations.push(
            "no Compose working_dir, final Dockerfile WORKDIR, or framework preset was found"
                .to_owned(),
        );
    }
    let (source, safe_exceptions) = mount_paths(module, request)?;
    let intent = selected_target.map(|target| {
        let existing = module.development_mount.as_ref();
        DevelopmentMountConfig {
            source,
            target,
            additional: existing
                .map(|mount| mount.additional.clone())
                .unwrap_or_default(),
            mode: if request.mode == DevelopmentMountMode::ReadOnly {
                DevelopmentMountMode::ReadOnly
            } else {
                existing.map_or(DevelopmentMountMode::ReadWrite, |mount| mount.mode)
            },
            preserve: existing
                .map(|mount| mount.preserve.clone())
                .unwrap_or_else(|| preset_preserves(preset)),
            excludes: existing
                .map(|mount| mount.excludes.clone())
                .unwrap_or_default(),
            protected: existing
                .map(|mount| mount.protected.clone())
                .unwrap_or_default(),
            safe_exceptions,
            services: selected_services,
            preset,
        }
    });
    if let Some(intent) = &intent {
        validate_intent(document, &request.module, module, intent)?;
    }
    let confidence = confidence(&evidence, &confirmations, intent.is_some());
    Ok(DevelopmentMountProposal {
        module: request.module.clone(),
        intent,
        evidence,
        confidence,
        confirmation_required: true,
        confirmations,
        warnings: vec![
            "reload and file watching remain framework-owned; mm guarantees only source visibility and stable container paths"
                .to_owned(),
        ],
        configured: module.development_mount.is_some(),
    })
}

fn mount_paths(
    module: &ModuleConfig,
    request: &DevelopmentMountPlanRequest,
) -> Result<(PathBuf, Vec<PathBuf>), MmError> {
    let source = request
        .source
        .as_deref()
        .or_else(|| {
            module
                .development_mount
                .as_ref()
                .and_then(|mount| mount.source.to_str())
        })
        .unwrap_or(".");
    let source = HostPlatform::current()
        .normalize_relative(source)
        .map_err(|message| MmError::Security { message })?;
    let safe_exceptions = if !request.safe_exceptions.is_empty() {
        request
            .safe_exceptions
            .iter()
            .map(|path| HostPlatform::current().normalize_relative(path))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|message| MmError::Security { message })?
    } else {
        module
            .development_mount
            .as_ref()
            .map(|mount| mount.safe_exceptions.clone())
            .unwrap_or_default()
    };
    Ok((source, safe_exceptions))
}

fn collect_evidence(
    document: &ConfigDocument,
    module: &ModuleConfig,
    request: &DevelopmentMountPlanRequest,
    selected_services: &[String],
    preset: Option<DevelopmentMountPreset>,
) -> Result<(Vec<DevelopmentMountEvidence>, Vec<String>), MmError> {
    let mut evidence = Vec::new();
    let mut confirmations = Vec::new();
    if let Some(target) = &request.target {
        evidence.push(DevelopmentMountEvidence {
            source: MountEvidenceSource::Explicit,
            location: "command line".to_owned(),
            service: None,
            target: target.clone(),
        });
    } else if let Some(configured) = &module.development_mount {
        evidence.push(DevelopmentMountEvidence {
            source: MountEvidenceSource::Explicit,
            location: "mm.yaml".to_owned(),
            service: None,
            target: configured.target.clone(),
        });
    } else {
        evidence.extend(compose_working_directories(document, selected_services)?);
        let (dockerfile_evidence, stages_differ) = dockerfile_workdirs(document, module)?;
        evidence.extend(dockerfile_evidence);
        if stages_differ {
            confirmations.push(
                "Dockerfile stages use different WORKDIR values; the final stage was selected"
                    .to_owned(),
            );
        }
        if let Some(preset) = preset {
            evidence.push(DevelopmentMountEvidence {
                source: MountEvidenceSource::FrameworkPreset,
                location: format!("{} preset", preset.label()),
                service: None,
                target: "/app".to_owned(),
            });
        }
    }
    Ok((evidence, confirmations))
}

fn append_evidence_confirmations(
    module: &ModuleConfig,
    selected_services: &[String],
    evidence: &[DevelopmentMountEvidence],
    confirmations: &mut Vec<String>,
) {
    let candidate_targets = evidence
        .iter()
        .map(|item| item.target.as_str())
        .collect::<BTreeSet<_>>();
    if candidate_targets.len() > 1 {
        confirmations.push(format!(
            "mount target evidence disagrees: {}",
            candidate_targets.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    let compose_targets = evidence
        .iter()
        .filter(|item| item.source == MountEvidenceSource::ComposeWorkingDirectory)
        .map(|item| item.target.as_str())
        .collect::<BTreeSet<_>>();
    if compose_targets.len() > 1 {
        confirmations.push("selected Compose services use different working_dir values".to_owned());
    }
    if selected_services.iter().any(|service| {
        module.services.get(service).is_some_and(|config| {
            config.role.is_some_and(infrastructure_role) || infrastructure_name(service)
        })
    }) {
        confirmations.push(
            "one or more selected services are infrastructure-like; verify source mounting is intentional"
                .to_owned(),
        );
    }
}

pub(crate) fn discovery_proposals(document: &ConfigDocument) -> Vec<DevelopmentMountProposal> {
    document
        .config
        .modules
        .keys()
        .filter_map(|module| {
            match proposal(
                document,
                &DevelopmentMountPlanRequest {
                    module: module.clone(),
                    ..Default::default()
                },
            ) {
                Ok(proposal) => Some(proposal),
                Err(error) => {
                    tracing::debug!(module, %error, "skipping development mount proposal");
                    None
                }
            }
        })
        .collect()
}

pub(super) fn select_target(evidence: &[DevelopmentMountEvidence]) -> Option<String> {
    for source in [
        MountEvidenceSource::Explicit,
        MountEvidenceSource::ComposeWorkingDirectory,
        MountEvidenceSource::DockerfileWorkdir,
        MountEvidenceSource::FrameworkPreset,
    ] {
        let values = evidence
            .iter()
            .filter(|item| item.source == source)
            .map(|item| item.target.as_str())
            .collect::<BTreeSet<_>>();
        if values.len() == 1 {
            return values.into_iter().next().map(str::to_owned);
        }
    }
    None
}

pub(super) fn confidence(
    evidence: &[DevelopmentMountEvidence],
    confirmations: &[String],
    has_intent: bool,
) -> MountConfidence {
    if !has_intent {
        MountConfidence::Missing
    } else if !confirmations.is_empty() {
        MountConfidence::Low
    } else if evidence
        .iter()
        .any(|item| item.source == MountEvidenceSource::ComposeWorkingDirectory)
    {
        MountConfidence::High
    } else {
        MountConfidence::Medium
    }
}

pub(super) fn preset_preserves(
    preset: Option<DevelopmentMountPreset>,
) -> Vec<PreservedContainerPath> {
    let paths: &[&str] = match preset {
        Some(DevelopmentMountPreset::Laravel) => {
            &["/app/vendor", "/app/storage", "/app/bootstrap/cache"]
        }
        Some(DevelopmentMountPreset::Node) => &["/app/node_modules"],
        Some(DevelopmentMountPreset::Dotnet) => &["/app/bin", "/app/obj", "/root/.nuget/packages"],
        Some(DevelopmentMountPreset::Rust) => &[
            "/app/target",
            "/usr/local/cargo/registry",
            "/usr/local/cargo/git",
        ],
        None => &[],
    };
    paths
        .iter()
        .map(|path| PreservedContainerPath {
            path: (*path).to_owned(),
        })
        .collect()
}

pub(super) fn discover_preset(
    module_root: &Path,
) -> Result<Option<DevelopmentMountPreset>, MmError> {
    if module_root.join("artisan").is_file() && module_root.join("composer.json").is_file() {
        return Ok(Some(DevelopmentMountPreset::Laravel));
    }
    if module_root.join("package.json").is_file() {
        return Ok(Some(DevelopmentMountPreset::Node));
    }
    if module_root.join("Cargo.toml").is_file() {
        return Ok(Some(DevelopmentMountPreset::Rust));
    }
    let entries = match fs::read_dir(module_root) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(MmError::Io {
                context: format!("reading module directory {}", module_root.display()),
                source,
            });
        }
    };
    for entry in entries {
        let entry = entry.map_err(|source| MmError::Io {
            context: format!("reading module directory {}", module_root.display()),
            source,
        })?;
        if entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("csproj"))
        {
            return Ok(Some(DevelopmentMountPreset::Dotnet));
        }
    }
    Ok(None)
}

pub(super) fn compose_working_directories(
    document: &ConfigDocument,
    selected_services: &[String],
) -> Result<Vec<DevelopmentMountEvidence>, MmError> {
    let mut working_dirs = BTreeMap::<String, (String, String)>::new();
    for stack in document.config.stacks.values() {
        for relative in &stack.files {
            let path = document.root.join(relative);
            let bytes = read_bounded(&path)?;
            let value: serde_yaml_ng::Value =
                serde_yaml_ng::from_slice(&bytes).map_err(|error| MmError::Environment {
                    message: format!("Compose file '{}' is invalid YAML: {error}", path.display()),
                })?;
            let Some(services) = value
                .as_mapping()
                .and_then(|root| mapping_value(root, "services"))
                .and_then(serde_yaml_ng::Value::as_mapping)
            else {
                continue;
            };
            for service in selected_services {
                let Some(mapping) = services
                    .get(serde_yaml_ng::Value::String(service.clone()))
                    .and_then(serde_yaml_ng::Value::as_mapping)
                else {
                    continue;
                };
                if let Some(target) =
                    mapping_value(mapping, "working_dir").and_then(serde_yaml_ng::Value::as_str)
                {
                    working_dirs.insert(
                        service.clone(),
                        (target.to_owned(), relative.display().to_string()),
                    );
                }
            }
        }
    }
    Ok(working_dirs
        .into_iter()
        .map(|(service, (target, location))| DevelopmentMountEvidence {
            source: MountEvidenceSource::ComposeWorkingDirectory,
            location,
            service: Some(service),
            target,
        })
        .collect())
}

pub(super) fn dockerfile_workdirs(
    document: &ConfigDocument,
    module: &ModuleConfig,
) -> Result<(Vec<DevelopmentMountEvidence>, bool), MmError> {
    let relative = module
        .build
        .as_ref()
        .and_then(|build| build.dockerfile.clone())
        .unwrap_or_else(|| PathBuf::from("Dockerfile"));
    let path = document.root.join(&module.path).join(&relative);
    if !path.is_file() {
        return Ok((Vec::new(), false));
    }
    let bytes = read_bounded(&path)?;
    let source = std::str::from_utf8(&bytes).map_err(|_| MmError::Environment {
        message: format!("Dockerfile '{}' must be UTF-8", path.display()),
    })?;
    let mut stages = Vec::<String>::new();
    let mut current = "/".to_owned();
    let mut in_stage = false;
    for line in logical_dockerfile_lines(source) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (instruction, argument) = trimmed
            .split_once(char::is_whitespace)
            .map_or((trimmed, ""), |(instruction, argument)| {
                (instruction, argument.trim())
            });
        if instruction.eq_ignore_ascii_case("FROM") {
            if in_stage {
                stages.push(current.clone());
            }
            current = "/".to_owned();
            in_stage = true;
        } else if instruction.eq_ignore_ascii_case("WORKDIR") && in_stage {
            if argument.contains('$') || argument.is_empty() {
                continue;
            }
            current = if argument.starts_with('/') {
                normalize_container_path(argument)?
            } else {
                normalize_container_path(&format!("{current}/{argument}"))?
            };
        }
    }
    if in_stage {
        stages.push(current);
    }
    if stages.is_empty() {
        return Ok((Vec::new(), false));
    }
    let differs = stages.iter().collect::<BTreeSet<_>>().len() > 1;
    let final_target = stages.last().cloned().unwrap_or_default();
    if final_target == "/" {
        return Ok((Vec::new(), differs));
    }
    Ok((
        vec![DevelopmentMountEvidence {
            source: MountEvidenceSource::DockerfileWorkdir,
            location: relative.display().to_string(),
            service: None,
            target: final_target,
        }],
        differs,
    ))
}

pub(super) fn logical_dockerfile_lines(source: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut current = String::new();
    for line in source.lines() {
        let trimmed = line.trim_end();
        if let Some(continued) = trimmed.strip_suffix('\\') {
            current.push_str(continued);
            current.push(' ');
        } else {
            current.push_str(trimmed);
            output.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        output.push(current);
    }
    output
}

pub(super) fn normalize_container_path(value: &str) -> Result<String, MmError> {
    let mut components = Vec::new();
    for component in value.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            component => components.push(component),
        }
    }
    let normalized = format!("/{}", components.join("/"));
    if is_absolute_container_path(&normalized) || normalized == "/" {
        Ok(normalized)
    } else {
        Err(MmError::Security {
            message: format!("invalid container path '{value}'"),
        })
    }
}
