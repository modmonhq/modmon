use super::*;
use crate::domain::development_mount::{
    DevelopmentMountMode, MaskedPath, MaskedPathKind, PreservedVolumeManifest,
};

#[derive(Default)]
pub(super) struct DevelopmentVolumes {
    pub(super) volumes: BTreeMap<String, GeneratedVolume>,
    pub(super) manifests: BTreeMap<String, PreservedVolumeManifest>,
    pub(super) masked: BTreeSet<MaskedPath>,
}

pub(super) fn development_volumes(
    input: &GeneratedOverlayInput<'_>,
    labels: &BTreeMap<String, String>,
    services: &mut BTreeMap<String, GeneratedService>,
) -> Result<DevelopmentVolumes, MmError> {
    let user_volume_keys = compose_volume_keys(input.source_files)?;
    let mut development = DevelopmentVolumes::default();
    for managed in input.managed_services {
        attach_development_mount(
            input,
            managed,
            labels,
            services,
            &user_volume_keys,
            &mut development,
        )?;
    }
    Ok(development)
}

fn attach_development_mount(
    input: &GeneratedOverlayInput<'_>,
    managed: &StackServiceManifest,
    labels: &BTreeMap<String, String>,
    services: &mut BTreeMap<String, GeneratedService>,
    user_volume_keys: &BTreeSet<String>,
    development: &mut DevelopmentVolumes,
) -> Result<(), MmError> {
    let Some(mount) = &managed.development_mount else {
        return Ok(());
    };
    let module = input
        .document
        .config
        .modules
        .get(&managed.module)
        .ok_or_else(|| MmError::Security {
            message: format!("development mount module '{}' disappeared", managed.module),
        })?;
    let generated = services
        .entry(managed.service.clone())
        .or_insert_with(|| empty_generated_service(managed.image.clone(), labels));
    let resolve = |source: &Path| {
        dunce::canonicalize(input.document.root.join(&module.path).join(source)).map_err(|source| {
            MmError::Io {
                context: format!(
                    "resolving development mount for module '{}'",
                    managed.module
                ),
                source,
            }
        })
    };
    let primary = resolve(mount.source.as_path())?;
    for (source, target) in mount.sources() {
        generated.volumes.push(GeneratedServiceMount::Bind {
            source: resolve(source)?.to_string_lossy().into_owned(),
            target: target.to_owned(),
            read_only: mount.mode == DevelopmentMountMode::ReadOnly,
        });
    }
    let mut preserved_paths = mount
        .preserve
        .iter()
        .map(|path| (path.path.clone(), false))
        .collect::<BTreeMap<_, _>>();
    for path in mount.excludes.iter().chain(&mount.protected) {
        let container_path =
            crate::app::development_mount::join_container_path(&mount.target, path)?;
        let kind = masked_kind(&primary, path, &managed.module)?;
        development.masked.insert(MaskedPath {
            module: managed.module.clone(),
            container_path: container_path.clone(),
            kind,
        });
        match kind {
            MaskedPathKind::Directory => {
                preserved_paths.insert(container_path, true);
            }
            MaskedPathKind::File => generated.volumes.push(GeneratedServiceMount::Bind {
                source: input
                    .config_files
                    .mask_file()
                    .path
                    .to_string_lossy()
                    .into_owned(),
                target: container_path,
                read_only: true,
            }),
            MaskedPathKind::Missing => {}
        }
    }
    for (container_path, nocopy) in preserved_paths {
        let volume = preserve_volume(
            input,
            &managed.module,
            container_path,
            nocopy,
            user_volume_keys,
            development,
        )?;
        generated.volumes.push(volume);
    }
    generated
        .volumes
        .sort_by(|left, right| left.target().cmp(right.target()));
    Ok(())
}

fn preserve_volume(
    input: &GeneratedOverlayInput<'_>,
    module: &str,
    container_path: String,
    nocopy: bool,
    user_volume_keys: &BTreeSet<String>,
    development: &mut DevelopmentVolumes,
) -> Result<GeneratedServiceMount, MmError> {
    let manifest = preserved_volume_manifest(
        input.document,
        input.environment,
        input.target,
        module,
        &container_path,
    );
    if user_volume_keys.contains(&manifest.logical_name) {
        return Err(MmError::Security {
            message: format!(
                "generated development volume key '{}' collides with a user Compose volume",
                manifest.logical_name
            ),
        });
    }
    development.volumes.insert(
        manifest.logical_name.clone(),
        GeneratedVolume {
            name: manifest.physical_name.clone(),
            labels: preserved_volume_labels(input, &manifest),
            driver: None,
            driver_opts: BTreeMap::new(),
        },
    );
    let mount = GeneratedServiceMount::Volume(GeneratedLongServiceMount {
        kind: "volume",
        source: manifest.logical_name.clone(),
        target: container_path,
        read_only: false,
        volume: Some(GeneratedVolumeOptions { nocopy }),
    });
    development
        .manifests
        .insert(manifest.physical_name.clone(), manifest);
    Ok(mount)
}

fn masked_kind(source: &Path, path: &Path, module: &str) -> Result<MaskedPathKind, MmError> {
    match std::fs::metadata(source.join(path)) {
        Ok(metadata) if metadata.is_dir() => Ok(MaskedPathKind::Directory),
        Ok(_) => Ok(MaskedPathKind::File),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(MaskedPathKind::Missing)
        }
        Err(error) => Err(MmError::Io {
            context: format!(
                "inspecting '{}' of module '{module}', which development-mount.excludes or .protected masks",
                path.display()
            ),
            source: error,
        }),
    }
}

fn preserved_volume_labels(
    input: &GeneratedOverlayInput<'_>,
    manifest: &PreservedVolumeManifest,
) -> BTreeMap<String, String> {
    let ownership = crate::domain::ownership::ResourceOwnership::for_named_target(
        input.target,
        crate::domain::ownership::ResourceKind::Volume,
        manifest.physical_name.clone(),
        manifest.resource.clone(),
    );
    let mut labels = ownership.labels();
    labels.insert(
        crate::domain::ownership::LABEL_PROJECT_ID.to_owned(),
        input.document.config.project.id.0.to_string(),
    );
    labels.insert(
        crate::domain::ownership::LABEL_ENVIRONMENT.to_owned(),
        input.environment.to_owned(),
    );
    labels
}

fn preserved_volume_manifest(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
    module: &str,
    container_path: &str,
) -> PreservedVolumeManifest {
    let identity = format!(
        "{}\0{}\0{}\0{}\0{}",
        target.definition.id.0, document.config.project.id.0, environment, module, container_path
    );
    let digest = blake3::hash(identity.as_bytes()).to_hex().to_string();
    let short = &digest[..16];
    PreservedVolumeManifest {
        module: module.to_owned(),
        container_path: container_path.to_owned(),
        logical_name: format!("mm-dev-{short}"),
        physical_name: format!(
            "mm-dev-{}-{short}",
            &document.config.project.id.0.simple().to_string()[..12]
        ),
        resource: format!("development-mount-{short}"),
    }
}

fn compose_volume_keys(
    source_files: &[CompiledComposeSource],
) -> Result<BTreeSet<String>, MmError> {
    let mut keys = BTreeSet::new();
    for source in source_files {
        let parsed: serde_yaml_ng::Value =
            serde_yaml_ng::from_slice(&source.bytes).map_err(|error| MmError::Environment {
                message: format!(
                    "Compose file '{}' is invalid YAML: {error}",
                    source.manifest.path.display()
                ),
            })?;
        if let Some(mapping) = parsed
            .as_mapping()
            .and_then(|root| mapping_value(root, "volumes"))
            .and_then(serde_yaml_ng::Value::as_mapping)
        {
            keys.extend(
                mapping
                    .keys()
                    .filter_map(serde_yaml_ng::Value::as_str)
                    .map(str::to_owned),
            );
        }
    }
    Ok(keys)
}
