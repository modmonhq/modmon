use super::*;

impl EnvironmentManager {
    pub(super) fn mark_defaults(
        &self,
        document: &ConfigDocument,
        environment: &str,
        metadata: &BTreeMap<String, SecretMetadata>,
        secrets: &mut [SecretBindingStatus],
    ) {
        if secrets
            .iter()
            .all(|secret| secret.status != SecretStatus::Unset)
        {
            return;
        }
        let index = self.sources.index(document, environment);
        let covered = secrets_covered_by_defaults(document, environment, &index, metadata);
        for secret in secrets {
            secret.defaulted = covered.contains(&secret.key);
        }
    }
}

/// The secrets every variable reading them in `environment` would take
/// from a non-empty `.env.example` default while the secret is not stored.
pub fn template_defaulted_secrets(
    document: &ConfigDocument,
    environment: &str,
    index: &EnvironmentIndex,
) -> BTreeSet<String> {
    let unset = document
        .config
        .secrets
        .keys()
        .map(|name| {
            let metadata = SecretMetadata {
                status: SecretStatus::Unset,
                revision: None,
                created_at_unix_ms: None,
                candidate_revision: None,
            };
            (name.clone(), metadata)
        })
        .collect();
    secrets_covered_by_defaults(document, environment, index, &unset)
}

/// The unset secrets that every variable reading them in `environment`
/// resolves from a non-empty `.env.example` default, as `mm env status` does.
pub(super) fn secrets_covered_by_defaults(
    document: &ConfigDocument,
    environment: &str,
    index: &EnvironmentIndex,
    metadata: &BTreeMap<String, SecretMetadata>,
) -> BTreeSet<String> {
    let mut by_reference = BTreeMap::new();
    for (name, metadata) in metadata {
        if let Some(reference) = document
            .config
            .secrets
            .get(name)
            .and_then(|declaration| configured_reference(declaration).ok())
        {
            by_reference.insert(reference.canonical(), metadata.clone());
        }
    }
    let modules = status_modules(
        document,
        environment,
        index,
        &BTreeMap::new(),
        &by_reference,
        true,
        &mut Vec::new(),
    );
    let mut coverage = BTreeMap::<String, bool>::new();
    for variable in modules
        .iter()
        .flat_map(|module| &module.services)
        .flat_map(|service| &service.variables)
        .filter(|variable| {
            matches!(
                variable.source,
                ValueSource::Secret | ValueSource::TemplateDefault
            )
        })
    {
        let Some(secret) = variable
            .global_key
            .as_deref()
            .and_then(|key| effective_binding(&document.config, environment, key))
            .and_then(EnvironmentBinding::secret_name)
        else {
            continue;
        };
        let defaulted = variable.status == ResolutionStatus::Defaulted
            && variable
                .value
                .as_deref()
                .is_some_and(|value| !value.is_empty());
        *coverage.entry(secret.to_owned()).or_insert(true) &= defaulted;
    }
    let Ok(other_consumers) = delivered_secret_references(
        document,
        environment,
        &EnvironmentIndex {
            environment: environment.to_owned(),
            input_hash: String::new(),
            modules: Vec::new(),
            diagnostics: Vec::new(),
        },
    ) else {
        return BTreeSet::new();
    };
    coverage
        .into_iter()
        .filter(|(name, covered)| {
            *covered
                && metadata
                    .get(name)
                    .is_some_and(|metadata| metadata.status == SecretStatus::Unset)
                && document
                    .config
                    .secrets
                    .get(name)
                    .and_then(|declaration| configured_reference(declaration).ok())
                    .is_some_and(|reference| !other_consumers.contains_key(&reference.canonical()))
        })
        .map(|(name, _)| name)
        .collect()
}
