use std::collections::BTreeSet;

use super::{EnvironmentBinding, MmConfig};
use crate::domain::config_resolve::effective_bindings;

impl MmConfig {
    /// The secrets `environment` uses, or every environment with `None`:
    /// those its bindings name, its active services mount, its registries,
    /// TLS and tunnel read, and its target's deploy key.
    #[must_use]
    pub fn used_secrets(&self, environment: Option<&str>) -> BTreeSet<String> {
        let mut used = BTreeSet::new();
        let bindings: Vec<&EnvironmentBinding> = match environment {
            Some(environment) => effective_bindings(self, environment)
                .into_values()
                .collect(),
            None => self
                .bindings
                .values()
                .chain(
                    self.environments
                        .values()
                        .flat_map(|config| config.bindings.values()),
                )
                .collect(),
        };
        used.extend(
            bindings
                .into_iter()
                .filter_map(EnvironmentBinding::secret_name)
                .map(str::to_owned),
        );
        used.extend(
            self.registries
                .values()
                .map(|registry| registry.password.secret.clone()),
        );
        for config in self
            .environments
            .iter()
            .filter(|(name, _)| environment.is_none_or(|wanted| wanted == name.as_str()))
            .map(|(_, config)| config)
        {
            if let Some(tls) = &config.tls {
                used.extend([tls.certificate.clone(), tls.key.clone()]);
            }
            if let Some((token, _)) = config.ingress.as_ref().and_then(|ingress| ingress.tunnel()) {
                used.insert(token);
            }
        }
        match environment {
            Some(environment) => used.extend(
                self.active_services(environment)
                    .flat_map(|(_, service)| service.secret_files_in(environment))
                    .map(|(_, file)| file.secret.clone()),
            ),
            None => used.extend(
                self.modules
                    .values()
                    .flat_map(|module| module.services.values())
                    .flat_map(|service| service.secret_files.values())
                    .map(|file| file.secret.clone()),
            ),
        }
        let targets: Vec<_> = match environment {
            Some(environment) => self
                .targets
                .get(self.target_of(environment))
                .into_iter()
                .collect(),
            None => self.targets.values().collect(),
        };
        used.extend(
            targets
                .into_iter()
                .filter_map(|target| target.setup.as_ref()?.deploy_key.as_ref())
                .map(|key| key.secret.clone()),
        );
        used
    }
}
