use super::*;
use serde::{Serialize, Serializer};

#[derive(Serialize)]
struct GeneratedOverlay<'a> {
    services: BTreeMap<String, GeneratedService>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    networks: BTreeMap<String, GeneratedNetwork>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    volumes: BTreeMap<String, GeneratedVolume>,
    #[serde(rename = "x-mm")]
    metadata: GeneratedMetadata<'a>,
}

#[derive(Serialize)]
struct GeneratedService {
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pull_policy: Option<&'static str>,
    labels: BTreeMap<String, String>,
    deploy: GeneratedDeploy,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    networks: BTreeMap<String, GeneratedServiceNetwork>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    volumes: Vec<GeneratedServiceMount>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    environment: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    restart: Option<&'static str>,
    #[serde(flatten)]
    security: super::hardening::GeneratedSecurity,
    #[serde(skip_serializing_if = "Option::is_none")]
    userns_mode: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    sysctls: BTreeMap<&'static str, &'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    profiles: Vec<&'static str>,
}

#[derive(Serialize)]
struct GeneratedDeploy {
    labels: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct GeneratedServiceNetwork {
    aliases: Vec<String>,
}

#[derive(Serialize)]
struct GeneratedNetwork {
    external: bool,
    name: String,
}

#[derive(Serialize)]
struct GeneratedVolume {
    name: String,
    labels: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    driver: Option<&'static str>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    driver_opts: BTreeMap<String, String>,
}

struct DevelopmentVolumes {
    volumes: BTreeMap<String, GeneratedVolume>,
    manifests: BTreeMap<String, crate::domain::development_mount::PreservedVolumeManifest>,
}

enum GeneratedServiceMount {
    Bind {
        source: String,
        target: String,
        read_only: bool,
    },
    Volume(GeneratedLongServiceMount),
}

impl GeneratedServiceMount {
    fn target(&self) -> &str {
        match self {
            Self::Bind { target, .. } => target,
            Self::Volume(mount) => &mount.target,
        }
    }
}

impl Serialize for GeneratedServiceMount {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Bind {
                source,
                target,
                read_only,
            } => serializer.serialize_str(&format!(
                "{source}:{target}:{}",
                if *read_only { "ro,z" } else { "z" }
            )),
            Self::Volume(mount) => mount.serialize(serializer),
        }
    }
}

#[derive(Serialize)]
struct GeneratedLongServiceMount {
    #[serde(rename = "type")]
    kind: &'static str,
    source: String,
    target: String,
    #[serde(skip_serializing_if = "is_false")]
    read_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    volume: Option<GeneratedVolumeOptions>,
}

#[derive(Serialize)]
struct GeneratedVolumeOptions {
    nocopy: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct GeneratedMetadata<'a> {
    project_id: String,
    target: &'a str,
    target_id: String,
    environment: &'a str,
    stack: &'a str,
}

/// The Compose profile of a `role: job` service: `compose up` leaves it
/// out, and `compose run` of a job starts it.
const JOB_ONLY_PROFILE: &str = "mm-job-only";

pub(super) struct GeneratedOverlayInput<'a> {
    pub document: &'a ConfigDocument,
    pub environment: &'a str,
    pub stack: &'a str,
    pub managed_services: &'a [StackServiceManifest],
    pub routes: &'a [StackRouteManifest],
    pub target: &'a NamedTarget,
    pub membership: &'a BTreeMap<(String, String), BTreeSet<String>>,
    pub source_files: &'a [CompiledComposeSource],
    /// Services that get mm's default restart policy: the ones a cluster
    /// node runs whose Compose files set none.
    pub restart_defaults: &'a BTreeSet<String>,
    /// The stack's shared volumes that a service of this node mounts.
    pub shared: &'a super::shared_volumes::NodeVolumes,
    /// What hardening and the host's runtime policy add, by service.
    pub security: &'a BTreeMap<String, super::hardening::GeneratedSecurity>,
    pub config_files: &'a super::config_files::StackConfigFiles,
    /// The services with user ids of their own on this host.
    pub user_ids: &'a super::user_namespace::StackUserIds,
    /// The services that may open ports below 1024 as any user.
    pub low_ports: &'a BTreeSet<String>,
}

pub(super) fn generated_overlay(
    input: GeneratedOverlayInput<'_>,
) -> Result<
    (
        Vec<u8>,
        Vec<crate::domain::development_mount::PreservedVolumeManifest>,
    ),
    MmError,
> {
    let labels = overlay_identity_labels(&input);
    let mut services = overlay_services(&input, &labels);
    attach_resolvers(&input, &mut services);
    for name in input.restart_defaults {
        services
            .entry(name.clone())
            .or_insert_with(|| empty_generated_service(None, &labels))
            .restart = Some(super::placement::DEFAULT_RESTART);
    }
    for (name, security) in input.security {
        services
            .entry(name.clone())
            .or_insert_with(|| empty_generated_service(None, &labels))
            .security = security.clone();
    }
    let keeps_host_ids =
        super::user_namespace::host_user_namespaces(input.document, input.environment, input.stack);
    for name in keeps_host_ids {
        let service = services
            .entry(name)
            .or_insert_with(|| empty_generated_service(None, &labels));
        service.userns_mode = Some("host".to_owned());
    }
    for (name, ids) in input.user_ids.iter() {
        let service = services
            .entry(name.clone())
            .or_insert_with(|| empty_generated_service(None, &labels));
        service.userns_mode = Some(ids.userns_mode());
    }
    for name in input.low_ports {
        services
            .entry(name.clone())
            .or_insert_with(|| empty_generated_service(None, &labels))
            .sysctls
            .insert(super::low_ports::UNPRIVILEGED_PORT_START, "0");
    }
    let uses_development_services = attach_mail_bindings(&input, &labels, &mut services);
    let networks = overlay_networks(&input, uses_development_services);
    let mut development = development_volumes(&input, &labels, &mut services)?;
    // A shared volume keeps its key; the overlay says where its files are.
    for (service, (label, value)) in input.shared.service_labels() {
        services
            .entry(service)
            .or_insert_with(|| empty_generated_service(None, &labels))
            .labels
            .insert(label, value);
    }
    for (service, source, target) in input.config_files.mounts() {
        let service = services.entry(service);
        let service = service.or_insert_with(|| empty_generated_service(None, &labels));
        service.volumes.push(GeneratedServiceMount::Bind {
            source,
            target,
            read_only: true,
        });
    }
    drain_labels(&input, &labels, &mut services)?;
    for volume in &input.shared.volumes {
        development.volumes.insert(
            volume.key.clone(),
            GeneratedVolume {
                name: volume.engine.name.clone(),
                labels: super::shared_volumes::NodeVolumes::volume_labels(
                    input.document,
                    input.environment,
                    input.stack,
                    &volume.key,
                ),
                driver: Some("local"),
                driver_opts: volume.engine.driver_opts.clone(),
            },
        );
    }
    let bytes = serde_yaml_ng::to_string(&GeneratedOverlay {
        services,
        networks,
        volumes: development.volumes,
        metadata: GeneratedMetadata {
            project_id: input.document.config.project.id.0.to_string(),
            target: &input.document.config.target,
            target_id: input.target.definition.id.0.to_string(),
            environment: input.environment,
            stack: input.stack,
        },
    })
    .map(String::into_bytes)
    .map_err(|error| MmError::Environment {
        message: format!("could not serialize generated Compose overlay: {error}"),
    })?;
    Ok((bytes, development.manifests.into_values().collect()))
}

fn overlay_identity_labels(input: &GeneratedOverlayInput<'_>) -> BTreeMap<String, String> {
    [
        (
            "io.mm.target-id".to_owned(),
            input.target.definition.id.0.to_string(),
        ),
        (
            "io.mm.project-id".to_owned(),
            input.document.config.project.id.0.to_string(),
        ),
        ("io.mm.environment".to_owned(), input.environment.to_owned()),
        ("io.mm.stack".to_owned(), input.stack.to_owned()),
        ("io.mm.resource-kind".to_owned(), "stack".to_owned()),
        ("io.mm.resource".to_owned(), input.stack.to_owned()),
        ("io.mm.generation".to_owned(), "1".to_owned()),
    ]
    .into_iter()
    .collect()
}

fn empty_generated_service(
    image: Option<String>,
    labels: &BTreeMap<String, String>,
) -> GeneratedService {
    GeneratedService {
        pull_policy: image
            .as_deref()
            .filter(|image| crate::domain::build::is_local_image_reference(image))
            .map(|_| "never"),
        image,
        labels: labels.clone(),
        deploy: GeneratedDeploy {
            labels: labels.clone(),
        },
        networks: BTreeMap::new(),
        volumes: Vec::new(),
        environment: BTreeMap::new(),
        restart: None,
        security: Default::default(),
        userns_mode: None,
        sysctls: BTreeMap::new(),
        profiles: Vec::new(),
    }
}

fn overlay_services(
    input: &GeneratedOverlayInput<'_>,
    labels: &BTreeMap<String, String>,
) -> BTreeMap<String, GeneratedService> {
    let mut services = BTreeMap::new();
    for managed in input.managed_services {
        let service = services
            .entry(managed.service.clone())
            .or_insert_with(|| empty_generated_service(managed.image.clone(), labels));
        if managed.is_job_only() {
            service.profiles.push(JOB_ONLY_PROFILE);
        }
    }
    for ((member_stack, service), logical_networks) in input.membership {
        if member_stack != input.stack {
            continue;
        }
        let service = services
            .entry(service.clone())
            .or_insert_with(|| empty_generated_service(None, labels));
        for logical in logical_networks {
            service
                .networks
                .entry(logical.clone())
                .or_insert_with(|| GeneratedServiceNetwork {
                    aliases: Vec::new(),
                });
        }
    }
    for route in input
        .routes
        .iter()
        .filter(|_| !served_by_cluster(input.document))
    {
        let service = services
            .entry(route.service.clone())
            .or_insert_with(|| empty_generated_service(None, labels));
        let aliases = &mut service
            .networks
            .entry("mm-ingress".to_owned())
            .or_insert_with(|| GeneratedServiceNetwork {
                aliases: Vec::new(),
            })
            .aliases;
        if !aliases.contains(&route.backend_alias) {
            aliases.push(route.backend_alias.clone());
        }
    }
    services
}

/// Where a container reads which resolver to ask.
const RESOLVER_CONFIGURATION: &str = "/etc/resolv.conf";

/// On a cluster node, every service on a network that spans the nodes asks
/// the node's own resolver for names, which knows the containers of every
/// node. The engine's built-in resolver knows only its own node's, so the
/// service gets the resolver file of one of its networks in place of the
/// engine's. Which network does not matter: the node's resolver answers a
/// container for all the networks it is on. `mmd` writes the file when it
/// stores the network's addresses, which is before a deployment starts a
/// container on the network.
fn attach_resolvers(
    input: &GeneratedOverlayInput<'_>,
    services: &mut BTreeMap<String, GeneratedService>,
) {
    for ((member_stack, service), logical_networks) in input.membership {
        if member_stack != input.stack {
            continue;
        }
        let file = logical_networks.iter().find_map(|logical| {
            input
                .document
                .config
                .networks
                .get(logical)
                .and_then(|network| network.resolver.as_ref())
        });
        if let (Some(file), Some(generated)) = (file, services.get_mut(service)) {
            generated.volumes.push(GeneratedServiceMount::Bind {
                source: file.to_string_lossy().into_owned(),
                target: RESOLVER_CONFIGURATION.to_owned(),
                read_only: true,
            });
        }
    }
}

/// A leave begun on the node finds the service's drain on the container.
fn drain_labels(
    input: &GeneratedOverlayInput<'_>,
    labels: &BTreeMap<String, String>,
    services: &mut BTreeMap<String, GeneratedService>,
) -> Result<(), MmError> {
    for managed in input.managed_services {
        let drain = service_config(input.document, &managed.module, &managed.service)?
            .drain
            .as_ref();
        if let Some(drain) = drain {
            services
                .entry(managed.service.clone())
                .or_insert_with(|| empty_generated_service(None, labels))
                .labels
                .insert(
                    crate::domain::leaving::DRAIN_LABEL.to_owned(),
                    drain.0.clone(),
                );
        }
    }
    Ok(())
}

fn attach_mail_bindings(
    input: &GeneratedOverlayInput<'_>,
    labels: &BTreeMap<String, String>,
    services: &mut BTreeMap<String, GeneratedService>,
) -> bool {
    let mut used = false;
    for managed in input.managed_services {
        if managed.mail_bindings.is_empty() {
            continue;
        }
        used = true;
        let generated = services
            .entry(managed.service.clone())
            .or_insert_with(|| empty_generated_service(managed.image.clone(), labels));
        generated
            .networks
            .entry("mm-services-development".to_owned())
            .or_insert_with(|| GeneratedServiceNetwork {
                aliases: Vec::new(),
            });
        for binding in &managed.mail_bindings {
            if matches!(
                binding.provenance,
                crate::domain::development_mail::MailBindingProvenance::InjectedDefault
                    | crate::domain::development_mail::MailBindingProvenance::InjectedConfigured
            ) {
                generated
                    .environment
                    .insert(binding.key.clone(), binding.value.value().to_owned());
            }
        }
    }
    used
}

fn overlay_networks(
    input: &GeneratedOverlayInput<'_>,
    uses_development_services: bool,
) -> BTreeMap<String, GeneratedNetwork> {
    let mut networks = BTreeMap::new();
    if !input.routes.is_empty() && !served_by_cluster(input.document) {
        networks.insert(
            "mm-ingress".to_owned(),
            GeneratedNetwork {
                external: true,
                name: ingress_network_name(input.target.definition.id),
            },
        );
    }
    if uses_development_services {
        networks.insert(
            "mm-services-development".to_owned(),
            GeneratedNetwork {
                external: true,
                name: crate::domain::service_stack::development_network_name(
                    input.target.definition.id,
                ),
            },
        );
    }
    for logical in input
        .membership
        .iter()
        .filter(|((stack, _), _)| stack == input.stack)
        .flat_map(|(_, names)| names.iter())
    {
        networks
            .entry(logical.clone())
            .or_insert_with(|| GeneratedNetwork {
                external: true,
                name: managed_network_name(
                    input.target.definition.id,
                    input.document.config.project.id,
                    input.environment,
                    logical,
                ),
            });
    }
    networks
}

fn development_volumes(
    input: &GeneratedOverlayInput<'_>,
    labels: &BTreeMap<String, String>,
    services: &mut BTreeMap<String, GeneratedService>,
) -> Result<DevelopmentVolumes, MmError> {
    let user_volume_keys = compose_volume_keys(input.source_files)?;
    let mut volumes = BTreeMap::new();
    let mut manifests = BTreeMap::new();
    for managed in input.managed_services {
        attach_development_mount(
            input,
            managed,
            labels,
            services,
            &user_volume_keys,
            &mut volumes,
            &mut manifests,
        )?;
    }
    Ok(DevelopmentVolumes { volumes, manifests })
}

fn attach_development_mount(
    input: &GeneratedOverlayInput<'_>,
    managed: &StackServiceManifest,
    labels: &BTreeMap<String, String>,
    services: &mut BTreeMap<String, GeneratedService>,
    user_volume_keys: &BTreeSet<String>,
    volumes: &mut BTreeMap<String, GeneratedVolume>,
    manifests: &mut BTreeMap<String, crate::domain::development_mount::PreservedVolumeManifest>,
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
    for (source, target) in mount.sources() {
        let source = dunce::canonicalize(input.document.root.join(&module.path).join(source))
            .map_err(|source| MmError::Io {
                context: format!(
                    "resolving development mount for module '{}'",
                    managed.module
                ),
                source,
            })?;
        generated.volumes.push(GeneratedServiceMount::Bind {
            source: source.to_string_lossy().into_owned(),
            target: target.to_owned(),
            read_only: mount.mode
                == crate::domain::development_mount::DevelopmentMountMode::ReadOnly,
        });
    }
    let mut preserved_paths = mount
        .preserve
        .iter()
        .map(|path| (path.path.clone(), false))
        .collect::<BTreeMap<_, _>>();
    for path in mount.excludes.iter().chain(&mount.protected) {
        preserved_paths.insert(
            crate::app::development_mount::join_container_path(&mount.target, path)?,
            true,
        );
    }
    for (container_path, nocopy) in preserved_paths {
        let manifest = preserved_volume_manifest(
            input.document,
            input.environment,
            input.target,
            &managed.module,
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
        let labels = preserved_volume_labels(input, &manifest);
        volumes.insert(
            manifest.logical_name.clone(),
            GeneratedVolume {
                name: manifest.physical_name.clone(),
                labels,
                driver: None,
                driver_opts: BTreeMap::new(),
            },
        );
        generated
            .volumes
            .push(GeneratedServiceMount::Volume(GeneratedLongServiceMount {
                kind: "volume",
                source: manifest.logical_name.clone(),
                target: container_path,
                read_only: false,
                volume: Some(GeneratedVolumeOptions { nocopy }),
            }));
        manifests.insert(manifest.physical_name.clone(), manifest);
    }
    generated
        .volumes
        .sort_by(|left, right| left.target().cmp(right.target()));
    Ok(())
}

fn preserved_volume_labels(
    input: &GeneratedOverlayInput<'_>,
    manifest: &crate::domain::development_mount::PreservedVolumeManifest,
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

const fn is_false(value: &bool) -> bool {
    !*value
}

fn preserved_volume_manifest(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
    module: &str,
    container_path: &str,
) -> crate::domain::development_mount::PreservedVolumeManifest {
    let identity = format!(
        "{}\0{}\0{}\0{}\0{}",
        target.definition.id.0, document.config.project.id.0, environment, module, container_path
    );
    let digest = blake3::hash(identity.as_bytes()).to_hex().to_string();
    let short = &digest[..16];
    crate::domain::development_mount::PreservedVolumeManifest {
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

pub(crate) fn managed_network_name(
    target_id: crate::domain::target::TargetId,
    project_id: ProjectId,
    environment: &str,
    logical: &str,
) -> String {
    format!(
        "mm-{}-{}-{}-{}",
        short_id(target_id.0),
        short_id(project_id.0),
        sanitize_identity(environment),
        sanitize_identity(logical)
    )
}

fn short_id(id: uuid::Uuid) -> String {
    id.as_simple().to_string()[..12].to_owned()
}

fn sanitize_identity(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches('-').to_owned();
    if sanitized.is_empty() {
        "net".to_owned()
    } else {
        sanitized
    }
}

mod intent_hash;
mod membership;
pub(super) use intent_hash::canonical_workload_intent_hash;
pub(super) use membership::resolve_network_membership;

#[cfg(test)]
mod tests;
