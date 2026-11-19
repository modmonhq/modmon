//! Whether a service is part of an environment, as the reader of the
//! configuration sees it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ServiceConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServiceSecretFileConfig {
    pub secret: String,
    #[serde(default = "default_secret_uid")]
    pub uid: String,
    #[serde(default = "default_secret_gid")]
    pub gid: String,
    #[serde(default = "default_secret_mode")]
    pub mode: u32,
    /// Environments in which the file is mounted and its secret required;
    /// an empty list means all of the service's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
}

fn default_secret_uid() -> String {
    "0".to_owned()
}

fn default_secret_gid() -> String {
    "0".to_owned()
}

const fn default_secret_mode() -> u32 {
    0o444
}

/// A file the service ships with, mounted read-only at the path that is
/// its key under `config-files`. Exactly one of `file` and `template`
/// says where it comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServiceConfigFileConfig {
    /// A file of the module, shipped as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<std::path::PathBuf>,
    /// A file of the module in which every `{{ KEY }}` is replaced by the
    /// environment's binding of that key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<std::path::PathBuf>,
    /// The numeric user that owns the file. Without it the file belongs
    /// to the user mm runs as on the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gid: Option<String>,
    #[serde(default = "default_secret_mode")]
    pub mode: u32,
    /// Environments in which the file is mounted; an empty list means all
    /// of the service's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
}

fn applies_in(environments: &[String], environment: &str) -> bool {
    environments.is_empty() || environments.iter().any(|name| name == environment)
}

impl ServiceConfig {
    /// Whether the service runs in `environment` here. "Here" matters on a
    /// cluster node: `mmd` reads the configuration as that node sees it
    /// (`domain::placement::view_from_node`), where a service placed on
    /// other nodes is not active. Everywhere else every service of the
    /// environment is.
    #[must_use]
    pub fn is_active(&self, environment: &str) -> bool {
        self.placed_elsewhere.is_none() && self.is_in_environment(environment)
    }

    /// Whether the service belongs to `environment` on some node of the
    /// target, placed here or not.
    #[must_use]
    pub fn is_in_environment(&self, environment: &str) -> bool {
        applies_in(&self.environments, environment) && self.off_locally_in(environment).is_none()
    }

    /// Why the service does not run in `environment` on this machine, when
    /// that is the development environment and its `development:` says so.
    #[must_use]
    pub fn off_locally_in(
        &self,
        environment: &str,
    ) -> Option<&crate::domain::development_units::OffLocally> {
        self.off_locally
            .as_ref()
            .filter(|off| off.environment == environment)
    }

    /// Whether the service exists only for its jobs (`role: job`): a
    /// deployment starts no container of it.
    #[must_use]
    pub fn is_job_only(&self) -> bool {
        self.role == Some(super::ServiceRole::Job)
    }

    /// Whether the development environment gives the service an automatic
    /// local route. A service on the host's network gets none: mm routes
    /// over networks of its own.
    #[must_use]
    pub fn takes_automatic_local_ingress(&self) -> bool {
        self.automatic_local_ingress != Some(false)
            && self.host_network.is_none()
            && !self
                .role
                .is_some_and(super::ServiceRole::excludes_automatic_ingress)
    }

    /// The secret files mounted in `environment`.
    pub fn secret_files_in<'a>(
        &'a self,
        environment: &'a str,
    ) -> impl Iterator<Item = (&'a String, &'a ServiceSecretFileConfig)> + 'a {
        self.secret_files
            .iter()
            .filter(move |(_, file)| applies_in(&file.environments, environment))
    }

    /// The config files mounted in `environment`.
    pub fn config_files_in<'a>(
        &'a self,
        environment: &'a str,
    ) -> impl Iterator<Item = (&'a String, &'a ServiceConfigFileConfig)> + 'a {
        self.config_files
            .iter()
            .filter(move |(_, file)| applies_in(&file.environments, environment))
    }
}
