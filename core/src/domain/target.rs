use std::collections::{BTreeMap, BTreeSet};

use std::path::{Component, Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::operation::OperationId;

/// Release-pinned Traefik OCI manifest used by new local targets.
pub const DEFAULT_TRAEFIK_IMAGE: &str = "docker.io/library/traefik@sha256:16acb89c6db341182970d6fdafece31303b0a380a8ed7aa51682e225229bf1d2";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct TargetId(pub Uuid);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct TargetCatalog {
    #[serde(default)]
    pub targets: BTreeMap<String, TargetDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct TargetDefinition {
    pub id: TargetId,
    pub transport: TargetTransport,
    pub bootstrap: TargetBootstrap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress: Option<TargetIngress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_runtime: Option<crate::domain::runtime::RuntimeKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_runtimes: Vec<crate::domain::runtime::RuntimeKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<TargetRuntimeEndpoint>,
    /// Every node of a cluster target, in declaration order. Empty for a
    /// target that names one host. `transport` then repeats the first node
    /// whose keys are recorded, so code that only asks "local or remote?"
    /// needs no cluster case.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<TargetNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct TargetRuntimeEndpoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<PathBuf>,
}

impl TargetDefinition {
    #[must_use]
    pub fn local(id: TargetId) -> Self {
        Self {
            id,
            transport: TargetTransport::Local,
            bootstrap: TargetBootstrap::Ansible,
            ingress: Some(TargetIngress::Traefik {
                image: DEFAULT_TRAEFIK_IMAGE.to_owned(),
                acme_email: None,
                acme_ca_server: None,
                acme_ca_certificate: None,
            }),
            default_runtime: None,
            available_runtimes: Vec::new(),
            runtime: None,
            nodes: Vec::new(),
        }
    }

    /// Whether the target lists nodes rather than one host.
    #[must_use]
    pub fn is_cluster(&self) -> bool {
        !self.nodes.is_empty()
    }

    #[must_use]
    pub fn supports_runtime(&self, kind: crate::domain::runtime::RuntimeKind) -> bool {
        self.available_runtimes.is_empty() || self.available_runtimes.contains(&kind)
    }

    pub fn canonical_fingerprint(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hash_field(&mut hasher, "mm-target");
        hash_field(&mut hasher, &self.id.0.to_string());
        match &self.transport {
            TargetTransport::Local => hash_field(&mut hasher, "local"),
            TargetTransport::Ssh {
                host,
                port,
                user,
                host_key_fingerprint,
                host_key,
                identity_file,
                remote_root,
                daemon_fingerprint,
            } => {
                hash_field(&mut hasher, "ssh");
                hash_field(&mut hasher, host);
                hash_field(&mut hasher, &port.to_string());
                hash_field(&mut hasher, user);
                hash_field(&mut hasher, host_key_fingerprint);
                hash_optional_path(&mut hasher, "host-key", host_key.as_deref());
                hash_optional_path(
                    &mut hasher,
                    "identity-file",
                    identity_file.as_ref().and_then(|path| path.to_str()),
                );
                hash_optional_path(
                    &mut hasher,
                    "remote-root",
                    remote_root.as_ref().and_then(|path| path.to_str()),
                );
                hash_optional_path(
                    &mut hasher,
                    "daemon-fingerprint",
                    daemon_fingerprint.as_deref(),
                );
            }
        }
        match self.bootstrap {
            TargetBootstrap::Ansible => hash_field(&mut hasher, "ansible"),
        }
        match &self.ingress {
            None => hash_field(&mut hasher, "no-ingress"),
            Some(TargetIngress::Traefik {
                image,
                acme_email,
                acme_ca_server,
                acme_ca_certificate,
            }) => {
                hash_field(&mut hasher, "traefik");
                hash_field(&mut hasher, image);
                match acme_email {
                    Some(acme_email) => {
                        hash_field(&mut hasher, "acme-email");
                        hash_field(&mut hasher, acme_email);
                    }
                    None => hash_field(&mut hasher, "no-acme-email"),
                }
                // Hashed only when set, so existing target fingerprints are unchanged.
                for (label, value) in [
                    ("acme-ca-server", acme_ca_server),
                    ("acme-ca-certificate", acme_ca_certificate),
                ] {
                    if let Some(value) = value {
                        hash_field(&mut hasher, label);
                        hash_field(&mut hasher, value);
                    }
                }
            }
        }
        match self.default_runtime {
            Some(runtime) => {
                hash_field(&mut hasher, "default-runtime");
                hash_field(&mut hasher, runtime_kind_name(runtime));
            }
            None => hash_field(&mut hasher, "no-default-runtime"),
        }
        if self.available_runtimes.is_empty() {
            hash_field(&mut hasher, "no-available-runtimes");
        } else {
            hash_field(&mut hasher, "available-runtimes");
            hasher.update(&(self.available_runtimes.len() as u64).to_le_bytes());
            for runtime in &self.available_runtimes {
                hash_field(&mut hasher, runtime_kind_name(*runtime));
            }
        }
        hash_runtime_endpoint(&mut hasher, self.runtime.as_ref());
        nodes::hash_nodes(&mut hasher, &self.nodes);
        hasher.finalize().to_hex().to_string()
    }
}

fn hash_runtime_endpoint(hasher: &mut blake3::Hasher, runtime: Option<&TargetRuntimeEndpoint>) {
    let Some(runtime) = runtime else {
        hash_field(hasher, "no-runtime-endpoint");
        return;
    };
    hash_field(hasher, "runtime-endpoint");
    hash_optional_display_path(hasher, "socket", runtime.socket.as_deref());
    hash_optional_path(hasher, "endpoint", runtime.endpoint.as_deref());
    hash_optional_display_path(hasher, "executable", runtime.executable.as_deref());
}

fn hash_optional_display_path(
    hasher: &mut blake3::Hasher,
    label: &str,
    value: Option<&std::path::Path>,
) {
    match value {
        Some(value) => {
            hash_field(hasher, label);
            hash_field(hasher, &value.display().to_string());
        }
        None => hash_field(hasher, &format!("no-{label}")),
    }
}

const fn runtime_kind_name(kind: crate::domain::runtime::RuntimeKind) -> &'static str {
    match kind {
        crate::domain::runtime::RuntimeKind::PodmanCompose => "podman-compose",
        crate::domain::runtime::RuntimeKind::DockerCompose => "docker-compose",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TargetTransport {
    Local,
    Ssh {
        host: String,
        #[serde(default = "default_ssh_port")]
        port: u16,
        user: String,
        host_key_fingerprint: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host_key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identity_file: Option<PathBuf>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remote_root: Option<PathBuf>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        daemon_fingerprint: Option<String>,
    },
}

const fn default_ssh_port() -> u16 {
    22
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "provider", rename_all = "kebab-case")]
pub enum TargetBootstrap {
    Ansible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "provider", rename_all = "kebab-case")]
pub enum TargetIngress {
    Traefik {
        image: String,
        #[serde(
            default,
            rename = "acme-email",
            skip_serializing_if = "Option::is_none"
        )]
        acme_email: Option<String>,
        /// ACME directory URL; defaults to Let's Encrypt production. Use it for
        /// Let's Encrypt staging or an internal ACME server.
        #[serde(
            default,
            rename = "acme-ca-server",
            skip_serializing_if = "Option::is_none"
        )]
        acme_ca_server: Option<String>,
        /// PEM CA bundle that signs the ACME server's own HTTPS certificate,
        /// for internal ACME servers that public roots do not trust.
        #[serde(
            default,
            rename = "acme-ca-certificate",
            skip_serializing_if = "Option::is_none"
        )]
        acme_ca_certificate: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NamedTarget {
    pub name: String,
    pub definition: TargetDefinition,
    pub fingerprint: String,
}

impl NamedTarget {
    #[must_use]
    pub fn new(name: String, definition: TargetDefinition) -> Self {
        let fingerprint = definition.canonical_fingerprint();
        Self {
            name,
            definition,
            fingerprint,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TargetCatalogSnapshot {
    pub targets: Vec<NamedTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BootstrapInputFile {
    pub path: PathBuf,
    pub digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapCapability {
    AnsiblePlaybook,
    SshTransport,
    Python3,
    Docker,
    Podman,
    ComposeProvider,
    Haproxy,
    RemoteAccess,
}

impl BootstrapCapability {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AnsiblePlaybook => "ansible-playbook",
            Self::SshTransport => "ssh-transport",
            Self::Python3 => "python3",
            Self::Docker => "docker-engine",
            Self::Podman => "podman",
            Self::ComposeProvider => "compose-provider",
            Self::Haproxy => "haproxy",
            Self::RemoteAccess => "remote-access",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapStep {
    ProvisionHost,
    EnrollRuntime,
    InstallAccess,
}

impl BootstrapStep {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ProvisionHost => "provision-host",
            Self::EnrollRuntime => "enroll-runtime",
            Self::InstallAccess => "install-access",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TargetBootstrapPlan {
    pub project_id: crate::domain::workspace::ProjectId,
    pub project_name: String,
    pub config_hash: String,
    pub environment: String,
    pub target: NamedTarget,
    pub selected_runtime: crate::domain::runtime::RuntimeKind,
    pub rootless: bool,
    pub input_files: Vec<BootstrapInputFile>,
    pub required_capabilities: Vec<BootstrapCapability>,
    pub steps: Vec<BootstrapStep>,
    /// The remote access layer, when the operator asked setup to install it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<BootstrapAccess>,
    pub plan_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TargetBootstrapResult {
    pub operation_id: OperationId,
    pub plan_hash: String,
    pub target: String,
    pub target_id: TargetId,
    pub verified_capabilities: Vec<BootstrapCapability>,
    pub next_command: String,
    /// Host facts printed after installing the remote access layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<BootstrapAccessReport>,
}

impl TargetBootstrapPlan {
    /// The command that applies this plan, with the selection it was made for.
    #[must_use]
    pub fn apply_command(&self) -> String {
        let mut command = format!(
            "mm --environment {} --target {} --runtime {} target setup apply --plan {}",
            self.environment,
            self.target.name,
            runtime_kind_name(self.selected_runtime),
            self.plan_hash
        );
        if let Some(access) = &self.access {
            for argument in access.arguments() {
                command.push(' ');
                command.push_str(&argument);
            }
        }
        command
    }

    pub fn validate(&self) -> Result<(), String> {
        let canonical = self.canonical_hash()?;
        if canonical != self.plan_hash {
            return Err("bootstrap plan hash does not match its contents".to_owned());
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<String, String> {
        self.validate_shape()?;
        let mut hasher = blake3::Hasher::new();
        hash_field(&mut hasher, "mm-target-bootstrap-plan");
        hash_field(&mut hasher, &self.project_id.0.to_string());
        hash_field(&mut hasher, &self.project_name);
        hash_field(&mut hasher, &self.config_hash);
        hash_field(&mut hasher, &self.environment);
        let target_fingerprint = self.target.definition.canonical_fingerprint();
        if self.target.fingerprint != target_fingerprint {
            return Err("bootstrap target fingerprint does not match its definition".to_owned());
        }
        hash_field(&mut hasher, &self.target.name);
        hash_field(&mut hasher, &target_fingerprint);
        hash_field(&mut hasher, runtime_kind_name(self.selected_runtime));
        hash_field(
            &mut hasher,
            if self.rootless { "rootless" } else { "rootful" },
        );
        hasher.update(&(self.input_files.len() as u64).to_le_bytes());
        for file in &self.input_files {
            let path = file
                .path
                .to_str()
                .ok_or_else(|| "bootstrap input paths must be valid Unicode".to_owned())?;
            hash_field(&mut hasher, path);
            hash_field(&mut hasher, &file.digest);
        }
        hasher.update(&(self.required_capabilities.len() as u64).to_le_bytes());
        for capability in &self.required_capabilities {
            hash_field(&mut hasher, capability.label());
        }
        hasher.update(&(self.steps.len() as u64).to_le_bytes());
        for step in &self.steps {
            hash_field(&mut hasher, step.label());
        }
        // Hashed only when set, so plans without remote access keep their hash.
        if let Some(access) = &self.access {
            access.hash(&mut hasher)?;
        }
        Ok(hasher.finalize().to_hex().to_string())
    }

    fn validate_shape(&self) -> Result<(), String> {
        if self.project_name.is_empty()
            || self.environment.is_empty()
            || self
                .project_name
                .chars()
                .chain(self.environment.chars())
                .any(char::is_control)
        {
            return Err("bootstrap project metadata is invalid".to_owned());
        }
        if self.config_hash.len() != 64
            || !self
                .config_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("bootstrap config hash is invalid".to_owned());
        }
        validate_target_name(&self.target.name)?;
        validate_target_definition(&self.target.definition)?;
        if self.input_files.len() > 256 {
            return Err("bootstrap input manifest contains too many files".to_owned());
        }
        let mut previous = None;
        let mut has_config = false;
        let mut has_playbook = false;
        for input in &self.input_files {
            if input.path.is_absolute()
                || input.path.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
                || !input.path.starts_with(Path::new(".config/ansible"))
                || input.digest.len() != 64
                || !input
                    .digest
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err("bootstrap input manifest contains an unsafe entry".to_owned());
            }
            if previous.as_ref().is_some_and(|path| path >= &input.path) {
                return Err("bootstrap input manifest must be sorted without duplicates".to_owned());
            }
            has_config |= input.path == Path::new(".config/ansible/ansible.cfg");
            has_playbook |= input.path == Path::new(".config/ansible/bootstrap.yml");
            previous = Some(input.path.clone());
        }
        if !has_config || !has_playbook {
            return Err("bootstrap input manifest is missing a required Ansible input".to_owned());
        }
        let requirements = bootstrap_requirements(
            &self.target.definition.transport,
            self.selected_runtime,
            self.target.definition.ingress.is_some(),
            self.access.is_some(),
        )?;
        if self.rootless != requirements.rootless {
            return Err(if requirements.rootless {
                "Podman Compose bootstrap without remote access requires rootless mode".to_owned()
            } else if self.access.is_some() {
                "bootstrap with remote access requires a rootful engine: mmd runs as root"
                    .to_owned()
            } else {
                "Docker bootstrap currently requires rootful mode".to_owned()
            });
        }
        if self.required_capabilities != requirements.capabilities {
            return Err("bootstrap capability manifest does not match the target".to_owned());
        }
        if self.steps != requirements.steps {
            return Err("bootstrap steps do not match the supported execution boundary".to_owned());
        }
        if let Some(access) = &self.access {
            access.validate()?;
            if access.apparmor_profiles != requirements.apparmor_profiles {
                return Err(
                    "bootstrap AppArmor profiles do not match the selected runtime".to_owned(),
                );
            }
        }
        Ok(())
    }
}

/// What setup installs and verifies for one target and runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapRequirements {
    pub rootless: bool,
    pub capabilities: Vec<BootstrapCapability>,
    pub steps: Vec<BootstrapStep>,
    /// The AppArmor profiles the access step installs for rootful Podman.
    pub apparmor_profiles: Vec<String>,
}

/// The capabilities and steps a setup plan must list, in order. The access
/// layer runs `mmd` as root, so it needs a rootful Docker runtime.
pub fn bootstrap_requirements(
    transport: &TargetTransport,
    runtime: crate::domain::runtime::RuntimeKind,
    ingress: bool,
    access: bool,
) -> Result<BootstrapRequirements, String> {
    use crate::domain::runtime::RuntimeKind;

    let mut capabilities = vec![BootstrapCapability::AnsiblePlaybook];
    if matches!(transport, TargetTransport::Ssh { .. }) {
        capabilities.push(BootstrapCapability::SshTransport);
    }
    capabilities.push(BootstrapCapability::Python3);
    let rootless = match runtime {
        RuntimeKind::DockerCompose => {
            capabilities.push(BootstrapCapability::Docker);
            false
        }
        // Remote access runs `mmd` as root, which drives rootful Podman;
        // without it the account that logs in drives its own.
        RuntimeKind::PodmanCompose => {
            capabilities.push(BootstrapCapability::Podman);
            !access
        }
    };
    capabilities.push(BootstrapCapability::ComposeProvider);
    if ingress {
        capabilities.push(BootstrapCapability::Haproxy);
    }
    let mut steps = vec![BootstrapStep::ProvisionHost, BootstrapStep::EnrollRuntime];
    let apparmor_profiles = if access && runtime == RuntimeKind::PodmanCompose {
        crate::domain::remote::CONTAINER_PROFILES
            .map(str::to_owned)
            .to_vec()
    } else {
        Vec::new()
    };
    if access {
        capabilities.push(BootstrapCapability::RemoteAccess);
        steps.push(BootstrapStep::InstallAccess);
    }
    Ok(BootstrapRequirements {
        rootless,
        capabilities,
        steps,
        apparmor_profiles,
    })
}

pub fn validate_target_catalog(catalog: &TargetCatalog) -> Result<(), String> {
    if catalog.targets.len() > 256 {
        return Err("target catalog may contain at most 256 targets".to_owned());
    }
    let mut ids = BTreeSet::new();
    for (name, definition) in &catalog.targets {
        validate_target_name(name)?;
        validate_target_definition(definition)?;
        if !ids.insert(definition.id) {
            return Err(format!(
                "target ID '{}' is assigned to more than one target name",
                definition.id.0
            ));
        }
    }
    Ok(())
}

pub fn validate_target_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 63
        || !name
            .bytes()
            .next()
            .is_some_and(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
        || !name.bytes().all(|value| {
            value.is_ascii_lowercase() || value.is_ascii_digit() || matches!(value, b'-' | b'_')
        })
    {
        return Err(format!(
            "target name '{name}' must be 1-63 characters, start with a lowercase letter or digit, and contain only lowercase letters, digits, '-' or '_'"
        ));
    }
    Ok(())
}

pub fn validate_target_definition(definition: &TargetDefinition) -> Result<(), String> {
    nodes::validate_nodes(&definition.transport, &definition.nodes)?;
    match &definition.transport {
        TargetTransport::Local => {}
        TargetTransport::Ssh {
            host,
            port,
            user,
            host_key_fingerprint,
            host_key,
            identity_file,
            remote_root,
            daemon_fingerprint,
        } => {
            if *port == 0 {
                return Err("SSH port must be greater than zero".to_owned());
            }
            validate_ssh_host(host)?;
            validate_ssh_user(user)?;
            validate_ssh_host_key_fingerprint(host_key_fingerprint)?;
            if let Some(host_key) = host_key {
                validate_ssh_host_key(host_key, host_key_fingerprint)?;
            }
            if let Some(identity_file) = identity_file
                && (!identity_file.is_absolute() || identity_file.as_os_str().is_empty())
            {
                return Err("SSH identity file must be an absolute path".to_owned());
            }
            if let Some(remote_root) = remote_root
                && (!remote_root.is_absolute() || remote_root.as_os_str().is_empty())
            {
                return Err("SSH remote project root must be an absolute path".to_owned());
            }
            if let Some(fingerprint) = daemon_fingerprint
                && (fingerprint.len() != 64
                    || !fingerprint
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')))
            {
                return Err("daemon fingerprint must be a lowercase BLAKE3 digest".to_owned());
            }
        }
    }
    if let Some(TargetIngress::Traefik {
        image,
        acme_email,
        acme_ca_server,
        acme_ca_certificate,
    }) = &definition.ingress
    {
        validate_digest_pinned_image(image)?;
        if let Some(acme_email) = acme_email {
            validate_acme_email(acme_email)?;
        }
        if let Some(server) = acme_ca_server {
            validate_acme_ca_server(server)?;
        }
        if let Some(certificate) = acme_ca_certificate {
            validate_acme_ca_certificate(certificate)?;
        }
    }
    if let Some(default_runtime) = definition.default_runtime
        && !definition.supports_runtime(default_runtime)
    {
        return Err("target default runtime is not listed in available-runtimes".to_owned());
    }
    if let Some(runtime) = &definition.runtime {
        if let Some(socket) = &runtime.socket
            && (socket.as_os_str().is_empty() || !socket.is_absolute())
        {
            return Err("target runtime socket must be an absolute path".to_owned());
        }
        if let Some(endpoint) = &runtime.endpoint
            && (endpoint.is_empty() || endpoint.chars().any(char::is_control))
        {
            return Err("target runtime endpoint is invalid".to_owned());
        }
        if let Some(executable) = &runtime.executable
            && (executable.as_os_str().is_empty() || executable.as_os_str().to_str().is_none())
        {
            return Err("target runtime executable path is invalid".to_owned());
        }
    }
    Ok(())
}

fn hash_field(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn hash_optional_path(hasher: &mut blake3::Hasher, label: &str, value: Option<&str>) {
    match value {
        Some(value) => {
            hash_field(hasher, label);
            hash_field(hasher, value);
        }
        None => hash_field(hasher, &format!("no-{label}")),
    }
}

#[path = "target/bootstrap_access.rs"]
mod bootstrap_access;
pub use bootstrap_access::{
    BootstrapAccess, BootstrapAccessReport, BootstrapExecution, MAX_BREAK_GLASS_KEYS,
};

#[path = "target/host_binaries.rs"]
mod host_binaries;
pub use host_binaries::{
    HOST_ARCHITECTURES, HostBinaries, HostFacts, binaries_for, host_cpu, linux_architecture,
    split_machine, validate_host_binaries,
};

#[path = "target/node_address.rs"]
mod node_address;
pub use node_address::{DeviceNodeAddress, NodeAddress, host_and_port};

#[path = "target/nodes.rs"]
mod nodes;
pub use nodes::TargetNode;

#[path = "target/ssh_validation.rs"]
mod ssh_validation;
use ssh_validation::*;

#[cfg(test)]
#[path = "target/tests.rs"]
mod tests;
