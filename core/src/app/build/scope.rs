use super::*;

/// What `mm build` builds and where the images go.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuildRequest {
    pub publish: bool,
    /// Every module with a build, not only the ones the environment runs.
    pub all: bool,
}

impl BuildRequest {
    #[must_use]
    pub fn new(publish: bool, all: bool) -> Self {
        Self { publish, all }
    }
}

impl From<bool> for BuildRequest {
    fn from(publish: bool) -> Self {
        Self {
            publish,
            all: false,
        }
    }
}

/// The modules a build of `environment` covers: those with a service the
/// environment runs, and the first-party images they build FROM.
pub(super) fn modules_in_scope(document: &ConfigDocument, environment: &str) -> BTreeSet<String> {
    let producers = configured_producers(document);
    let mut wanted = document
        .config
        .active_modules(environment)
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let mut scope = BTreeSet::new();
    while let Some(name) = wanted.pop() {
        if !scope.insert(name.clone()) {
            continue;
        }
        if let Some(module) = document.config.modules.get(&name) {
            wanted.extend(build_bases(document, module, &producers));
        }
    }
    scope
}

fn configured_producers(document: &ConfigDocument) -> BTreeMap<String, Vec<String>> {
    let mut producers = BTreeMap::<String, Vec<String>>::new();
    for (name, module) in &document.config.modules {
        let Some(image) = module
            .build
            .as_ref()
            .filter(|build| build.enabled)
            .and_then(|build| build.image.as_deref())
        else {
            continue;
        };
        let Ok(image) = normalize_image_repository(image) else {
            continue;
        };
        producers.entry(image).or_default().push(name.clone());
    }
    producers
}

fn build_bases(
    document: &ConfigDocument,
    module: &ModuleConfig,
    producers: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    let configured = module.build.as_ref();
    if configured.is_some_and(|build| !build.enabled) {
        return Vec::new();
    }
    let mut bases = configured
        .map(|build| build.depends_on.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let dockerfile = document.root.join(
        module.path.join(
            configured
                .and_then(|build| build.dockerfile.as_deref())
                .unwrap_or(Path::new("Dockerfile")),
        ),
    );
    let inferred = match read_dockerfile(&dockerfile) {
        Ok(source) => match parse_dockerfile(&source, &dockerfile.display().to_string()) {
            Ok(parsed) => parsed.bases,
            Err(_) => Vec::new(),
        },
        Err(_) => Vec::new(),
    };
    for base in inferred {
        if let DockerfileBaseKind::Literal { repository } = base.kind
            && let Some(modules) = producers.get(&repository)
        {
            bases.extend(modules.iter().cloned());
        }
    }
    bases
}
