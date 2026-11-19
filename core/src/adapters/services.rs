use std::fs;
use std::path::{Path, PathBuf};

use atomic_write_file::AtomicWriteFile;

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_owned_by_current_user, ensure_private_file,
    ensure_private_permissions_supported, ensure_user_controlled_directory, read_regular_file,
};
use crate::app::ports::{ServiceStackArtifactInput, ServiceStackStore};
use crate::domain::error::MmError;
use crate::domain::service_stack::{
    PreparedServiceStackArtifact, SERVICE_STACK_SCHEMA, ServiceStackState, newer_stack,
};
use crate::domain::stored_state::StoredOn::ThisMachine;
use crate::domain::stored_state::{check_schema, stored_version, unreadable, written_by_newer_mm};

#[derive(Debug, Clone)]
pub struct FilesystemServiceStackStore {
    root: PathBuf,
}

impl FilesystemServiceStackStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }

    pub fn load_or_create_dashboard_token(&self) -> Result<String, MmError> {
        const TOKEN_BYTES: usize = 32;
        const TOKEN_LENGTH: usize = TOKEN_BYTES * 2;

        prepare_directory(&self.root)?;
        let path = self.root.join("dashboard-ingress.token");
        ensure_no_symlink_components(&path)?;
        if path.exists() {
            ensure_private_file(&path)?;
            let bytes = read_regular_file(&path, TOKEN_LENGTH)?.ok_or_else(|| MmError::State {
                message: "dashboard ingress credential disappeared".to_owned(),
            })?;
            let token = String::from_utf8(bytes).map_err(|_| MmError::Security {
                message: "dashboard ingress credential is not valid UTF-8".to_owned(),
            })?;
            validate_dashboard_token(&token)?;
            return Ok(token);
        }

        let mut random = [0_u8; TOKEN_BYTES];
        getrandom::fill(&mut random).map_err(|error| MmError::Security {
            message: format!("could not generate dashboard ingress credential: {error}"),
        })?;
        let token = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        write_private(&path, token.as_bytes())?;
        Ok(token)
    }

    pub fn cap_admin_key_file(&self) -> PathBuf {
        self.root.join("cap-admin.key")
    }
}

fn validate_dashboard_token(token: &str) -> Result<(), MmError> {
    if token.len() == 64
        && token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(MmError::Security {
            message: "dashboard ingress credential is invalid".to_owned(),
        })
    }
}

impl ServiceStackStore for FilesystemServiceStackStore {
    fn load_state(&self) -> Result<Option<ServiceStackState>, MmError> {
        let path = self.state_path();
        ensure_no_symlink_components(&path)?;
        if !path.exists() {
            return Ok(None);
        }
        ensure_owned_by_current_user(&path)?;
        let bytes = fs::read(&path).map_err(|source| MmError::Io {
            context: format!("reading service stack state {}", path.display()),
            source,
        })?;
        if let Some(schema) = stored_version(&bytes, "schema") {
            check_schema(&path, schema, SERVICE_STACK_SCHEMA, ThisMachine)?;
        }
        let state = serde_json::from_slice::<ServiceStackState>(&bytes)
            .map_err(|error| unreadable(&path, "service stack state", &error, ThisMachine))?;
        if state.schema != SERVICE_STACK_SCHEMA {
            return Err(MmError::State {
                message: format!("unsupported service stack state schema {}", state.schema),
            });
        }
        if newer_stack(&state.stack_version).map_err(|message| MmError::State { message })? {
            return Err(written_by_newer_mm(
                &path,
                &format!("service stack {}", state.stack_version),
                ThisMachine,
            ));
        }
        state
            .applied_plan
            .validate()
            .map_err(|message| MmError::Security { message })?;
        Ok(Some(state))
    }

    fn save_state(&self, state: &ServiceStackState) -> Result<(), MmError> {
        prepare_directory(&self.root)?;
        let path = self.state_path();
        ensure_no_symlink_components(&path)?;
        let bytes = serde_json::to_vec_pretty(state).map_err(|error| MmError::State {
            message: format!("could not serialize service stack state: {error}"),
        })?;
        write_private(&path, &bytes)
    }

    fn remove_state(&self) -> Result<(), MmError> {
        let path = self.state_path();
        ensure_no_symlink_components(&path)?;
        if path.exists() {
            ensure_owned_by_current_user(&path)?;
            fs::remove_file(&path).map_err(|source| MmError::Io {
                context: format!("removing service stack state {}", path.display()),
                source,
            })?;
        }
        Ok(())
    }

    fn ensure_cap_admin_key_file(&self) -> Result<PathBuf, MmError> {
        const KEY_BYTES: usize = 32;
        const KEY_LENGTH: usize = KEY_BYTES * 2;

        prepare_directory(&self.root)?;
        let path = self.cap_admin_key_file();
        ensure_no_symlink_components(&path)?;
        if path.exists() {
            ensure_private_file(&path)?;
            let key = read_regular_file(&path, KEY_LENGTH)?.ok_or_else(|| MmError::State {
                message: "Cap administration credential disappeared".to_owned(),
            })?;
            if key.len() != KEY_LENGTH || key.iter().any(|byte| !byte.is_ascii_hexdigit()) {
                return Err(MmError::Security {
                    message: "Cap administration credential is invalid".to_owned(),
                });
            }
            return Ok(path);
        }
        let mut random = [0_u8; KEY_BYTES];
        getrandom::fill(&mut random).map_err(|_| MmError::Security {
            message: "could not generate Cap administration credential".to_owned(),
        })?;
        let key = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        write_private(&path, key.as_bytes())?;
        Ok(path)
    }

    fn publish(
        &self,
        artifact: ServiceStackArtifactInput<'_>,
    ) -> Result<PreparedServiceStackArtifact, MmError> {
        if artifact.plan_identity.len() != 64
            || !artifact
                .plan_identity
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(MmError::Security {
                message: "service artifact identity is invalid".to_owned(),
            });
        }
        let root = self.root.join("artifacts").join(artifact.plan_identity);
        prepare_directory(&root)?;
        let manifest_path = root.join("manifest.json");
        let compose_path = root.join("compose.yaml");
        let route_path = root.join("mailpit.dynamic.yaml");
        let dashboard_unit_path = root.join("mm-dashboard.service");
        publish_exact(&manifest_path, artifact.manifest)?;
        publish_exact(&compose_path, artifact.compose)?;
        publish_exact(&route_path, artifact.route)?;
        publish_exact(&dashboard_unit_path, artifact.dashboard_unit)?;
        Ok(PreparedServiceStackArtifact {
            root,
            manifest_path,
            compose_path,
            route_path,
            compose_digest: blake3::hash(artifact.compose).to_hex().to_string(),
            route_digest: blake3::hash(artifact.route).to_hex().to_string(),
            dashboard_unit_path: Some(dashboard_unit_path),
            dashboard_unit_digest: Some(blake3::hash(artifact.dashboard_unit).to_hex().to_string()),
        })
    }
}

impl crate::app::ports::InstallationIngressOwnership for FilesystemServiceStackStore {
    fn owns_ingress(&self, target_id: crate::domain::target::TargetId) -> Result<bool, MmError> {
        Ok(self
            .load_state()?
            .is_some_and(|state| state.target_id == target_id))
    }
}

fn prepare_directory(path: &Path) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    fs::create_dir_all(path).map_err(|source| MmError::Io {
        context: format!("creating service stack directory {}", path.display()),
        source,
    })?;
    ensure_user_controlled_directory(path)
}

fn publish_exact(path: &Path, expected: &[u8]) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    if path.exists() {
        ensure_owned_by_current_user(path)?;
        let observed = fs::read(path).map_err(|source| MmError::Io {
            context: format!("reading existing service artifact {}", path.display()),
            source,
        })?;
        if observed != expected {
            return Err(MmError::Security {
                message: format!(
                    "service artifact collision at {}; refusing to replace reviewed content",
                    path.display()
                ),
            });
        }
        return Ok(());
    }
    write_private(path, expected)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), MmError> {
    use std::io::Write;

    ensure_private_permissions_supported()?;
    let mut options = AtomicWriteFile::options();
    configure_private_options(&mut options);
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: format!("opening private service file {}", path.display()),
        source,
    })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing private service file {}", path.display()),
            source,
        })?;
    file.commit().map_err(|source| MmError::Io {
        context: format!("committing private service file {}", path.display()),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| {
            MmError::Io {
                context: format!("setting private service file mode {}", path.display()),
                source,
            }
        })?;
    }
    ensure_private_file(path)
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

#[cfg(all(test, unix))]
#[path = "services/tests.rs"]
mod tests;
