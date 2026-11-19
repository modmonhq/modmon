use super::{MmConfig, ModuleConfig, ServiceConfig};

impl MmConfig {
    pub fn active_services<'a>(
        &'a self,
        environment: &'a str,
    ) -> impl Iterator<Item = (&'a String, &'a ServiceConfig)> {
        self.modules
            .values()
            .flat_map(|module| &module.services)
            .filter(move |(_, service)| service.is_active(environment))
    }

    /// Modules with a service active in `environment`.
    pub fn active_modules<'a>(
        &'a self,
        environment: &'a str,
    ) -> impl Iterator<Item = (&'a String, &'a ModuleConfig)> {
        self.modules.iter().filter(move |(_, module)| {
            module
                .services
                .values()
                .any(|service| service.is_active(environment))
        })
    }
}
