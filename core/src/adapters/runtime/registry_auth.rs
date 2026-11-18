//! A private client configuration holding registry logins for one command.

use std::io::Write;

use crate::app::ports::CommandSpec;
use crate::domain::error::MmError;
use crate::domain::runtime::{RegistryCredential, registry_auth_config};

/// Owns the directory until the command finishes; dropping it removes the
/// logins from disk.
pub(crate) struct RegistryAuth {
    directory: Option<tempfile::TempDir>,
}

impl RegistryAuth {
    /// Writes `config.json` (0600) in a new private directory, or nothing
    /// when there are no logins.
    pub(crate) fn prepare(credentials: &[RegistryCredential]) -> Result<Self, MmError> {
        if credentials.is_empty() {
            return Ok(Self { directory: None });
        }
        let io = |context: &str| {
            let context = context.to_owned();
            move |source| MmError::Io { context, source }
        };
        let directory = tempfile::Builder::new()
            .prefix("mm-registry-")
            .tempdir()
            .map_err(io("creating a private registry configuration directory"))?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
                .map_err(io("securing the registry configuration directory"))?;
            options.mode(0o600);
        }
        let config = registry_auth_config(credentials);
        options
            .open(directory.path().join("config.json"))
            .and_then(|mut file| file.write_all(config.as_bytes()))
            .map_err(io("writing the registry configuration"))?;
        Ok(Self {
            directory: Some(directory),
        })
    }

    /// Points the Docker CLI and Compose plugin at the logins.
    pub(crate) fn docker(&self, specification: CommandSpec) -> CommandSpec {
        match &self.directory {
            Some(directory) => {
                specification.environment("DOCKER_CONFIG", directory.path().to_string_lossy())
            }
            None => specification,
        }
    }

    /// Points Podman, and the Compose provider it runs, at the logins.
    pub(crate) fn podman(&self, specification: CommandSpec) -> CommandSpec {
        match &self.directory {
            Some(directory) => specification
                .environment(
                    "REGISTRY_AUTH_FILE",
                    directory.path().join("config.json").to_string_lossy(),
                )
                .environment("DOCKER_CONFIG", directory.path().to_string_lossy()),
            None => specification,
        }
    }

    pub(crate) fn for_runtime(
        &self,
        runtime: crate::domain::runtime::RuntimeKind,
        specification: CommandSpec,
    ) -> CommandSpec {
        match runtime {
            crate::domain::runtime::RuntimeKind::DockerCompose => self.docker(specification),
            crate::domain::runtime::RuntimeKind::PodmanCompose => self.podman(specification),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logins_live_in_a_private_file_that_disappears_with_the_guard()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(RegistryAuth::prepare(&[])?.directory.is_none());
        let auth = RegistryAuth::prepare(&[RegistryCredential {
            registry: "ghcr.io".to_owned(),
            username: "bot".to_owned(),
            password: zeroize::Zeroizing::new("token".to_owned()),
        }])?;
        let directory = auth
            .directory
            .as_ref()
            .map(|directory| directory.path().to_owned())
            .ok_or("no directory")?;
        let config = directory.join("config.json");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&config)?.permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(std::fs::read_to_string(&config)?.contains("ghcr.io"));
        drop(auth);
        assert!(!directory.exists());
        Ok(())
    }

    #[test]
    fn podman_and_its_compose_provider_both_get_the_logins()
    -> Result<(), Box<dyn std::error::Error>> {
        let auth = RegistryAuth::prepare(&[RegistryCredential {
            registry: "ghcr.io".to_owned(),
            username: "bot".to_owned(),
            password: zeroize::Zeroizing::new("token".to_owned()),
        }])?;
        let directory = auth
            .directory
            .as_ref()
            .map(|directory| directory.path().to_string_lossy().into_owned())
            .ok_or("no directory")?;
        let podman = auth.for_runtime(
            crate::domain::runtime::RuntimeKind::PodmanCompose,
            CommandSpec::new("podman"),
        );
        let values = podman
            .environment_values()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(values.get("DOCKER_CONFIG"), Some(&directory));
        assert_eq!(
            values.get("REGISTRY_AUTH_FILE"),
            Some(&format!("{directory}/config.json"))
        );
        let docker = auth.for_runtime(
            crate::domain::runtime::RuntimeKind::DockerCompose,
            CommandSpec::new("docker"),
        );
        assert!(
            docker
                .environment_values()
                .any(|(key, value)| key == "DOCKER_CONFIG" && value == directory)
        );
        Ok(())
    }
}
