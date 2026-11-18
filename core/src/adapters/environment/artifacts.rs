use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use atomic_write_file::AtomicWriteFile;

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_owned_by_current_user, ensure_private_file,
    ensure_private_permissions_supported,
};
use crate::app::ports::{
    EnvironmentArtifactRequest, EnvironmentArtifactStore, SecretDeliveryCapabilityCatalog,
    SecretDeliveryDriver,
};
use crate::domain::environment::{
    CompiledEnvironmentBundle, PreparedEnvironmentBundle, PreparedServiceEnvironment,
};
use crate::domain::environment_delivery::EnvironmentDeliveryMode;
use crate::domain::runtime::RuntimeKind;
use crate::domain::secret_contract::{
    DeliveryCapabilityDescriptor, DeliveryDriverId, DeliveryEnvironmentScoping, DeliveryOperation,
    PlaintextPersistence, SecretMaterialKind, SecretWorkloadKind,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct FilesystemMaterializedEnvironmentWriter;

impl crate::app::ports::MaterializedEnvironmentWriter for FilesystemMaterializedEnvironmentWriter {
    fn write(&self, path: &Path, bytes: &[u8], force: bool) -> Result<(), MmError> {
        write_materialized_environment(path, bytes, force)
    }
}
use crate::domain::error::MmError;
use crate::domain::operation::OperationId;

#[derive(Clone)]
pub struct FilesystemEnvironmentArtifactStore {
    run_directory: PathBuf,
    drivers: BTreeMap<DeliveryDriverId, std::sync::Arc<dyn SecretDeliveryDriver>>,
    capabilities: BTreeMap<DeliveryDriverId, DeliveryCapabilityDescriptor>,
}

impl FilesystemEnvironmentArtifactStore {
    pub fn new(run_directory: PathBuf) -> Result<Self, MmError> {
        let drivers: Vec<std::sync::Arc<dyn SecretDeliveryDriver>> = vec![
            std::sync::Arc::new(DirectEnvironmentDelivery::new(run_directory.clone())?),
            std::sync::Arc::new(OperationEnvironmentFileDelivery::new(
                run_directory.clone(),
            )?),
        ];
        Self::from_drivers(run_directory, drivers)
    }

    pub fn from_drivers(
        run_directory: PathBuf,
        drivers: Vec<std::sync::Arc<dyn SecretDeliveryDriver>>,
    ) -> Result<Self, MmError> {
        let mut registered = BTreeMap::new();
        for driver in drivers {
            let descriptor = driver.descriptor();
            if descriptor.max_material_bytes == 0
                || descriptor.max_materialization_name_bytes == 0
                || descriptor.material_kinds.is_empty()
                || descriptor.runtimes.is_empty()
                || descriptor.workload_kinds.is_empty()
            {
                return Err(MmError::Security {
                    message: format!(
                        "secret delivery driver '{}' has empty capability limits",
                        descriptor.id
                    ),
                });
            }
            let id = descriptor.id.clone();
            if registered.insert(id.clone(), driver).is_some() {
                return Err(MmError::Secret {
                    message: format!("secret delivery driver '{id}' is registered more than once"),
                });
            }
        }
        let mut capabilities = registered
            .values()
            .map(|driver| (driver.descriptor().id.clone(), driver.descriptor().clone()))
            .collect::<BTreeMap<_, _>>();
        for descriptor in [
            crate::adapters::secrets::podman_native_secret_descriptor()?,
            crate::adapters::secrets::compose_file_secret_descriptor()?,
        ] {
            if capabilities
                .insert(descriptor.id.clone(), descriptor)
                .is_some()
            {
                return Err(MmError::Security {
                    message:
                        "runtime secret delivery capability collided with an environment driver"
                            .to_owned(),
                });
            }
        }
        Ok(Self {
            run_directory,
            drivers: registered,
            capabilities,
        })
    }

    #[must_use]
    pub fn capabilities(&self) -> Vec<DeliveryCapabilityDescriptor> {
        self.capabilities.values().cloned().collect()
    }
}

impl std::fmt::Debug for FilesystemEnvironmentArtifactStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FilesystemEnvironmentArtifactStore")
            .field("run_directory", &self.run_directory)
            .field("drivers", &self.drivers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl SecretDeliveryCapabilityCatalog for FilesystemEnvironmentArtifactStore {
    fn delivery_capabilities(&self) -> Vec<DeliveryCapabilityDescriptor> {
        self.capabilities()
    }

    fn delivery_capability(
        &self,
        driver: &DeliveryDriverId,
    ) -> Result<DeliveryCapabilityDescriptor, MmError> {
        self.drivers
            .get(driver)
            .map(|registered| registered.descriptor().clone())
            .or_else(|| self.capabilities.get(driver).cloned())
            .ok_or_else(|| MmError::Secret {
                message: format!("secret delivery driver '{driver}' is not registered"),
            })
    }
}

impl EnvironmentArtifactStore for FilesystemEnvironmentArtifactStore {
    fn bundle_root(&self, operation_id: OperationId) -> PathBuf {
        self.run_directory.join(operation_id.0.to_string())
    }

    fn publish(
        &self,
        request: EnvironmentArtifactRequest<'_>,
    ) -> Result<PreparedEnvironmentBundle, MmError> {
        let EnvironmentArtifactRequest {
            operation_id,
            delivery_driver,
            bundle,
        } = request;
        self.drivers
            .get(&delivery_driver)
            .ok_or_else(|| MmError::Secret {
                message: format!(
                    "secret delivery driver '{delivery_driver}' is not registered; supported drivers are {:?}",
                    self.drivers.keys().collect::<Vec<_>>()
                ),
            })?
            .publish(operation_id, bundle)
    }

    fn dispose(&self, bundle: &PreparedEnvironmentBundle) -> Result<(), MmError> {
        dispose_bundle(&self.run_directory, bundle)
    }

    fn reconcile(&self, active_operations: &HashSet<OperationId>) -> Result<(), MmError> {
        reconcile_bundles(&self.run_directory, active_operations)
    }

    fn limit_overlays(
        &self,
        bundle: &PreparedEnvironmentBundle,
        overlays: &[PathBuf],
        services: &std::collections::BTreeSet<String>,
    ) -> Result<Vec<PathBuf>, MmError> {
        super::restore::limited_overlays(&bundle.root, overlays, services)
    }
}

#[derive(Debug, Clone)]
pub struct DirectEnvironmentDelivery {
    run_directory: PathBuf,
    descriptor: DeliveryCapabilityDescriptor,
}

impl DirectEnvironmentDelivery {
    pub fn new(run_directory: PathBuf) -> Result<Self, MmError> {
        Ok(Self {
            run_directory,
            descriptor: direct_environment_descriptor()?,
        })
    }
}

impl SecretDeliveryDriver for DirectEnvironmentDelivery {
    fn descriptor(&self) -> &DeliveryCapabilityDescriptor {
        &self.descriptor
    }

    fn publish(
        &self,
        operation_id: OperationId,
        bundle: &CompiledEnvironmentBundle,
    ) -> Result<PreparedEnvironmentBundle, MmError> {
        publish_bundle(
            &self.run_directory,
            operation_id,
            EnvironmentDeliveryMode::Direct,
            bundle,
        )
    }
}

#[derive(Debug, Clone)]
pub struct OperationEnvironmentFileDelivery {
    run_directory: PathBuf,
    descriptor: DeliveryCapabilityDescriptor,
}

impl OperationEnvironmentFileDelivery {
    pub fn new(run_directory: PathBuf) -> Result<Self, MmError> {
        Ok(Self {
            run_directory,
            descriptor: operation_environment_file_descriptor()?,
        })
    }
}

impl SecretDeliveryDriver for OperationEnvironmentFileDelivery {
    fn descriptor(&self) -> &DeliveryCapabilityDescriptor {
        &self.descriptor
    }

    fn publish(
        &self,
        operation_id: OperationId,
        bundle: &CompiledEnvironmentBundle,
    ) -> Result<PreparedEnvironmentBundle, MmError> {
        publish_bundle(
            &self.run_directory,
            operation_id,
            EnvironmentDeliveryMode::EnvFile,
            bundle,
        )
    }
}

fn publish_bundle(
    run_directory: &Path,
    operation_id: OperationId,
    delivery_mode: EnvironmentDeliveryMode,
    bundle: &CompiledEnvironmentBundle,
) -> Result<PreparedEnvironmentBundle, MmError> {
    create_private_directory(run_directory)?;
    let final_root = run_directory.join(operation_id.0.to_string());
    if final_root.exists() {
        return Err(MmError::Environment {
            message: format!(
                "environment bundle already exists for operation {}",
                operation_id.0
            ),
        });
    }
    let staging = run_directory.join(format!(".{}.{}.tmp", operation_id.0, uuid::Uuid::new_v4()));
    create_private_directory(&staging)?;
    let result = (|| {
        let mut services = Vec::new();
        let mut interpolation_environment = crate::domain::runtime::RuntimeEnvironment::default();
        let mut direct_placeholders = BTreeMap::<(String, String), BTreeMap<String, String>>::new();
        let mut placeholder_index = 0_usize;
        for compiled in &bundle.services {
            validate_artifact_component(&compiled.stack, "stack")?;
            validate_artifact_component(&compiled.module, "module")?;
            validate_artifact_component(&compiled.service, "service")?;
            let path = match delivery_mode {
                EnvironmentDeliveryMode::EnvFile => {
                    let stack_directory = staging.join(&compiled.stack);
                    create_private_directory(&stack_directory)?;
                    let module_directory = stack_directory.join(&compiled.module);
                    create_private_directory(&module_directory)?;
                    let file_name = format!("{}.env", compiled.service);
                    write_new_private_file(&module_directory.join(&file_name), compiled.bytes())?;
                    Some(
                        final_root
                            .join(&compiled.stack)
                            .join(&compiled.module)
                            .join(file_name),
                    )
                }
                EnvironmentDeliveryMode::Direct => {
                    let service_placeholders = direct_placeholders
                        .entry((compiled.stack.clone(), compiled.service.clone()))
                        .or_default();
                    for (name, value) in compiled.direct_values.values() {
                        placeholder_index = placeholder_index.saturating_add(1);
                        let placeholder = placeholder_name(operation_id, placeholder_index);
                        if service_placeholders
                            .insert(name.to_owned(), placeholder.clone())
                            .is_some()
                            || interpolation_environment
                                .insert(placeholder, value.to_owned())
                                .is_some()
                        {
                            return Err(MmError::Security {
                                message: "direct environment placeholder identity collided"
                                    .to_owned(),
                            });
                        }
                    }
                    None
                }
            };
            services.push(PreparedServiceEnvironment {
                module: compiled.module.clone(),
                stack: compiled.stack.clone(),
                service: compiled.service.clone(),
                path,
            });
        }
        write_runtime_overlays(
            &staging,
            &final_root,
            delivery_mode,
            &services,
            &direct_placeholders,
        )?;
        fs::rename(&staging, &final_root).map_err(|source| MmError::Io {
            context: format!("publishing environment bundle {}", final_root.display()),
            source,
        })?;
        Ok(PreparedEnvironmentBundle {
            operation_id,
            root: final_root,
            manifest: bundle.manifest.clone(),
            services,
            interpolation_environment,
            runtime_secret_overlays: Vec::new(),
            runtime_secret_rollback_overlays: Vec::new(),
            registry_credentials: Vec::new(),
        })
    })();
    result.map_err(|primary| environment_publication_failure(&staging, operation_id, primary))
}

fn environment_publication_failure(
    staging: &Path,
    operation_id: OperationId,
    primary: MmError,
) -> MmError {
    match fs::remove_dir_all(staging) {
        Ok(()) => primary,
        Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => primary,
        Err(cleanup) => MmError::operation_failure(
            operation_id.0.to_string(),
            crate::domain::error::MutationStatus::NotStarted,
            primary,
            vec![cleanup.to_string()],
            "remove the private environment staging directory before retrying",
        ),
    }
}

fn dispose_bundle(run_directory: &Path, bundle: &PreparedEnvironmentBundle) -> Result<(), MmError> {
    if bundle.root.parent() != Some(run_directory)
        || bundle.root.file_name().and_then(|name| name.to_str())
            != Some(bundle.operation_id.0.to_string().as_str())
    {
        return Err(MmError::Environment {
            message: "refusing to dispose an environment bundle outside the run directory"
                .to_owned(),
        });
    }
    ensure_no_symlink_components(&bundle.root)?;
    match fs::remove_dir_all(&bundle.root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(MmError::Io {
            context: format!("disposing environment bundle {}", bundle.root.display()),
            source,
        }),
    }
}

fn reconcile_bundles(
    run_directory: &Path,
    active_operations: &HashSet<OperationId>,
) -> Result<(), MmError> {
    if !run_directory.exists() {
        return Ok(());
    }
    create_private_directory(run_directory)?;
    let entries = fs::read_dir(run_directory).map_err(|source| MmError::Io {
        context: "enumerating private environment operation directories".to_owned(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| MmError::Io {
            context: "enumerating a private environment operation directory".to_owned(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| MmError::Io {
            context: "inspecting a private environment operation entry".to_owned(),
            source,
        })?;
        if !file_type.is_dir() {
            return Err(MmError::Security {
                message: "environment operation storage contains a non-directory entry".to_owned(),
            });
        }
        ensure_owned_by_current_user(&path)?;
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(|| MmError::Security {
            message: "environment operation storage contains a non-Unicode entry".to_owned(),
        })?;
        let operation_id =
            OperationId(
                uuid::Uuid::parse_str(name).map_err(|error| MmError::Security {
                    message: format!("environment operation directory has an invalid ID: {error}"),
                })?,
            );
        let active = active_operations.contains(&operation_id);
        if !active {
            fs::remove_dir_all(&path).map_err(|source| MmError::Io {
                context: "removing an orphaned environment operation directory".to_owned(),
                source,
            })?;
        }
    }
    Ok(())
}

fn delivery_descriptor(
    id: &str,
    operation: DeliveryOperation,
    plaintext_persistence: PlaintextPersistence,
) -> Result<DeliveryCapabilityDescriptor, MmError> {
    let id =
        DeliveryDriverId::new(id.to_owned()).map_err(|message| MmError::Security { message })?;
    Ok(DeliveryCapabilityDescriptor {
        id,
        operation,
        environment_scoping: DeliveryEnvironmentScoping::Operation,
        material_kinds: vec![SecretMaterialKind::Text],
        max_material_bytes: crate::domain::secret_contract::MAX_SECRET_MATERIAL_BYTES,
        max_materialization_name_bytes: 255,
        runtimes: vec![RuntimeKind::PodmanCompose, RuntimeKind::DockerCompose],
        workload_kinds: vec![SecretWorkloadKind::Service],
        plaintext_persistence,
    })
}

pub fn direct_environment_descriptor() -> Result<DeliveryCapabilityDescriptor, MmError> {
    delivery_descriptor(
        "direct-environment",
        DeliveryOperation::DirectEnvironmentValue,
        PlaintextPersistence::None,
    )
}

pub fn operation_environment_file_descriptor() -> Result<DeliveryCapabilityDescriptor, MmError> {
    delivery_descriptor(
        "operation-env-file",
        DeliveryOperation::OperationScopedEnvironmentFile,
        PlaintextPersistence::OperationScopedPrivateFile,
    )
}

fn placeholder_name(operation_id: OperationId, index: usize) -> String {
    format!(
        "MM_ENV_{}_{}",
        operation_id.0.simple().to_string().to_ascii_uppercase(),
        index
    )
}

fn write_runtime_overlays(
    staging: &Path,
    final_root: &Path,
    mode: EnvironmentDeliveryMode,
    services: &[PreparedServiceEnvironment],
    direct_placeholders: &BTreeMap<(String, String), BTreeMap<String, String>>,
) -> Result<(), MmError> {
    let mut stacks = BTreeMap::<&str, Vec<&PreparedServiceEnvironment>>::new();
    for service in services {
        stacks.entry(&service.stack).or_default().push(service);
    }
    for (stack, services) in stacks {
        let mut service_map = serde_yaml_ng::Mapping::new();
        for service in services {
            let mut service_value = serde_yaml_ng::Mapping::new();
            match mode {
                EnvironmentDeliveryMode::EnvFile => {
                    let path = service.path.as_ref().ok_or_else(|| MmError::Security {
                        message: "env-file delivery omitted its private operation file".to_owned(),
                    })?;
                    service_value.insert(
                        serde_yaml_ng::Value::String("env_file".to_owned()),
                        serde_yaml_ng::Value::Sequence(vec![serde_yaml_ng::Value::String(
                            path.to_string_lossy().into_owned(),
                        )]),
                    );
                }
                EnvironmentDeliveryMode::Direct => {
                    let placeholders = direct_placeholders
                        .get(&(service.stack.clone(), service.service.clone()))
                        .ok_or_else(|| MmError::Security {
                            message: "direct environment service has no placeholder mapping"
                                .to_owned(),
                        })?;
                    let environment = placeholders
                        .iter()
                        .map(|(name, placeholder)| {
                            (
                                serde_yaml_ng::Value::String(name.clone()),
                                serde_yaml_ng::Value::String(format!(
                                    "${{{placeholder}?mm direct environment value unavailable}}"
                                )),
                            )
                        })
                        .collect();
                    service_value.insert(
                        serde_yaml_ng::Value::String("environment".to_owned()),
                        serde_yaml_ng::Value::Mapping(environment),
                    );
                }
            }
            service_map.insert(
                serde_yaml_ng::Value::String(service.service.clone()),
                serde_yaml_ng::Value::Mapping(service_value),
            );
        }
        let mut overlay = serde_yaml_ng::Mapping::new();
        overlay.insert(
            serde_yaml_ng::Value::String("services".to_owned()),
            serde_yaml_ng::Value::Mapping(service_map),
        );
        let bytes =
            serde_yaml_ng::to_string(&serde_yaml_ng::Value::Mapping(overlay)).map_err(|error| {
                MmError::Environment {
                    message: format!(
                        "could not serialize private runtime environment overlay: {error}"
                    ),
                }
            })?;
        let file_name = format!("{stack}.compose.yaml");
        write_new_private_file(&staging.join(&file_name), bytes.as_bytes())?;
        let final_path = final_root.join(file_name);
        if final_path.parent() != Some(final_root) {
            return Err(MmError::Security {
                message: "runtime environment overlay escaped its operation directory".to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_artifact_component(value: &str, kind: &str) -> Result<(), MmError> {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        Ok(())
    } else {
        Err(MmError::Security {
            message: format!("refusing unsafe environment artifact {kind} name"),
        })
    }
}

pub fn write_materialized_environment(
    path: &Path,
    contents: &[u8],
    force: bool,
) -> Result<(), MmError> {
    ensure_private_permissions_supported()?;
    let parent = path.parent().ok_or_else(|| MmError::Environment {
        message: format!("materialization path '{}' has no parent", path.display()),
    })?;
    ensure_no_symlink_components(parent)?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            return Err(MmError::Environment {
                message: format!("refusing to replace symlink {}", path.display()),
            });
        }
        if !force {
            return Err(MmError::Environment {
                message: format!(
                    "{} already exists; pass --force to replace it",
                    path.display()
                ),
            });
        }
        ensure_owned_by_current_user(path)?;
    }

    if !force {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| MmError::Environment {
                message: "materialization file name must be valid Unicode".to_owned(),
            })?;
        let temporary = parent.join(format!("{file_name}.{}.tmp", uuid::Uuid::new_v4()));
        if let Err(error) = write_new_private_file(&temporary, contents) {
            return Err(with_temporary_cleanup(error, &temporary));
        }
        let published = fs::hard_link(&temporary, path);
        if let Err(source) = fs::remove_file(&temporary)
            && source.kind() != std::io::ErrorKind::NotFound
        {
            return Err(MmError::operation_failure(
                temporary.display().to_string(),
                crate::domain::error::MutationStatus::Unknown,
                MmError::Io {
                    context: format!("removing temporary materialization {}", temporary.display()),
                    source,
                },
                Vec::new(),
                "remove the private temporary file and verify whether the destination was published",
            ));
        }
        return match published {
            Ok(()) => set_private_permissions(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(MmError::Environment {
                    message: format!(
                        "{} already exists; pass --force to replace it",
                        path.display()
                    ),
                })
            }
            Err(source) => Err(MmError::Io {
                context: format!("publishing materialized environment {}", path.display()),
                source,
            }),
        };
    }

    let mut options = AtomicWriteFile::options();
    configure_private_options(&mut options);
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: format!("opening {} for atomic materialization", path.display()),
        source,
    })?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing materialized environment {}", path.display()),
            source,
        })?;
    file.commit().map_err(|source| MmError::Io {
        context: format!("publishing materialized environment {}", path.display()),
        source,
    })?;
    set_private_permissions(path)
}

fn with_temporary_cleanup(primary: MmError, temporary: &Path) -> MmError {
    match fs::remove_file(temporary) {
        Ok(()) => primary,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => primary,
        Err(error) => MmError::operation_failure(
            temporary.display().to_string(),
            crate::domain::error::MutationStatus::NotStarted,
            primary,
            vec![error.to_string()],
            "remove the private temporary file before retrying",
        ),
    }
}

#[cfg(unix)]
fn configure_private_options(options: &mut atomic_write_file::OpenOptions) {
    use atomic_write_file::unix::OpenOptionsExt;
    use std::os::unix::fs::OpenOptionsExt as _;
    options
        .mode(0o600)
        .preserve_mode(false)
        .preserve_owner(false);
}

#[cfg(not(unix))]
fn configure_private_options(_options: &mut atomic_write_file::OpenOptions) {}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| MmError::Io {
        context: format!("setting private permissions on {}", path.display()),
        source,
    })?;
    ensure_private_file(path)
}

#[cfg(not(unix))]
fn set_private_permissions(path: &Path) -> Result<(), MmError> {
    Err(MmError::Security {
        message: format!(
            "cannot enforce private file permissions for '{}' on this platform",
            path.display()
        ),
    })
}

fn create_private_directory(path: &Path) -> Result<(), MmError> {
    ensure_private_permissions_supported()?;
    ensure_no_symlink_components(path)?;
    if path.exists() {
        ensure_owned_by_current_user(path)?;
    }
    create_directories(path).map_err(|source| MmError::Io {
        context: format!("creating private directory {}", path.display()),
        source,
    })?;
    set_private_directory_permissions(path)
}

#[cfg(unix)]
fn create_directories(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_directories(path: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(path)
}

pub(super) fn write_new_private_file(path: &Path, contents: &[u8]) -> Result<(), MmError> {
    ensure_private_permissions_supported()?;
    ensure_no_symlink_components(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    configure_private_open_options(&mut options);
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: format!("creating private file {}", path.display()),
        source,
    })?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing private file {}", path.display()),
            source,
        })?;
    set_private_permissions(path)
}

#[cfg(unix)]
fn configure_private_open_options(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn configure_private_open_options(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| MmError::Io {
        context: format!("setting private permissions on {}", path.display()),
        source,
    })
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;

#[cfg(not(unix))]
fn set_private_directory_permissions(path: &Path) -> Result<(), MmError> {
    Err(MmError::Security {
        message: format!(
            "cannot enforce private directory permissions for '{}' on this platform",
            path.display()
        ),
    })
}
