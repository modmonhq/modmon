use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

use crate::app::ports::{TargetIngressArtifactStore, TargetStore};

mod macos;

use crate::domain::error::MmError;
use crate::domain::runtime::RuntimeKind;
use crate::domain::target::{NamedTarget, TargetIngress, TargetTransport};
use crate::domain::target_ingress::{
    CompiledTargetIngressArtifact, CompiledTargetIngressSupportFile, INGRESS_PUBLIC_HTTP_PORT,
    INGRESS_PUBLIC_HTTPS_PORT, IngressHostProfile, TARGET_INGRESS_COMPOSE_PATH,
    TRAEFIK_ACME_FILE_MODE, TRAEFIK_ACME_STORAGE_PATH, TRAEFIK_CERTIFICATE_RESOLVER,
    TRAEFIK_DYNAMIC_CONFIG_DIRECTORY, TargetIngressArtifactManifest, TargetIngressPlan,
    TargetIngressStep, TargetIngressSupportFileManifest, ingress_acme_volume_name,
    ingress_dynamic_config_volume_name, ingress_loopback_ports, ingress_network_name,
    ingress_project_name,
};
pub(crate) use macos::render_dashboard_plist;

#[derive(Clone)]
pub struct TargetIngressManager {
    targets: Arc<dyn TargetStore>,
    artifacts: Arc<dyn TargetIngressArtifactStore>,
    local_host: IngressHostProfile,
}

impl std::fmt::Debug for TargetIngressManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TargetIngressManager")
            .finish_non_exhaustive()
    }
}

impl TargetIngressManager {
    #[must_use]
    pub fn new(
        targets: Arc<dyn TargetStore>,
        artifacts: Arc<dyn TargetIngressArtifactStore>,
    ) -> Self {
        Self {
            targets,
            artifacts,
            local_host: IngressHostProfile::LinuxSystemd,
        }
    }

    /// Selects how the local target's ingress integrates with this workstation.
    /// Remote targets are always provisioned as Linux/systemd hosts.
    #[must_use]
    pub fn with_local_host_profile(mut self, profile: IngressHostProfile) -> Self {
        self.local_host = profile;
        self
    }

    pub fn plan(&self, target_name: &str) -> Result<TargetIngressPlan, MmError> {
        let target = self.targets.resolve_target(target_name)?;
        let runtime = target
            .definition
            .default_runtime
            .unwrap_or(RuntimeKind::PodmanCompose);
        self.plan_resolved_for_runtime(&target, runtime)
    }

    pub fn plan_for_runtime(
        &self,
        target_name: &str,
        runtime: RuntimeKind,
    ) -> Result<TargetIngressPlan, MmError> {
        self.plan_resolved_for_runtime(&self.targets.resolve_target(target_name)?, runtime)
    }

    pub(crate) fn plan_resolved_for_runtime(
        &self,
        target: &NamedTarget,
        runtime: RuntimeKind,
    ) -> Result<TargetIngressPlan, MmError> {
        let (image, acme_email) = traefik_settings(target)?;
        let target_fingerprint = target.definition.canonical_fingerprint();
        if target_fingerprint != target.fingerprint {
            return Err(MmError::Security {
                message: "resolved target fingerprint does not match its definition".to_owned(),
            });
        }
        let compose_project_name = ingress_project_name(target.definition.id);
        let network_name = ingress_network_name(target.definition.id);
        let acme_volume_name = ingress_acme_volume_name(target.definition.id);
        let dynamic_config_volume_name = ingress_dynamic_config_volume_name(target.definition.id);
        let (proxy_http_port, proxy_https_port, proxy_verify_port) =
            ingress_loopback_ports(target.definition.id);
        let macos_host = self.macos_host(target)?;
        let trusted_proxy_peers = macos_host.as_ref().map_or_else(
            || LINUX_TRUSTED_PROXY_PEERS.to_owned(),
            crate::domain::target_ingress::MacosIngressHost::trusted_proxy_peer,
        );
        let compose = render_compose(&IngressRender {
            target_id: target.definition.id.0.to_string(),
            image,
            acme_email: acme_email.as_deref(),
            compose_project_name: &compose_project_name,
            network_name: &network_name,
            acme_volume_name: &acme_volume_name,
            dynamic_config_volume_name: &dynamic_config_volume_name,
            proxy_http_port,
            proxy_https_port,
            proxy_verify_port,
            trusted_proxy_peers: &trusted_proxy_peers,
        })?;
        let HostIntegration {
            steps,
            support_files,
            target_compose_path,
        } = host_integration(
            macos_host.as_ref(),
            runtime,
            &dynamic_config_volume_name,
            &compose_project_name,
            (proxy_http_port, proxy_https_port),
        );
        let support_manifests = support_files
            .iter()
            .map(|file| file.manifest.clone())
            .collect();
        let mut manifest = TargetIngressArtifactManifest {
            target_id: target.definition.id,
            target_name: target.name.clone(),
            target_fingerprint,
            provider: "traefik".to_owned(),
            image: image.clone(),
            acme_email: acme_email.clone(),
            compose_project_name,
            network_name,
            acme_volume_name,
            dynamic_config_volume_name,
            dynamic_config_directory: TRAEFIK_DYNAMIC_CONFIG_DIRECTORY.to_owned(),
            acme_storage_path: TRAEFIK_ACME_STORAGE_PATH.to_owned(),
            acme_file_mode: TRAEFIK_ACME_FILE_MODE.to_owned(),
            public_http_port: INGRESS_PUBLIC_HTTP_PORT,
            public_https_port: INGRESS_PUBLIC_HTTPS_PORT,
            proxy_http_port,
            proxy_https_port,
            proxy_verify_port,
            certificate_resolver: TRAEFIK_CERTIFICATE_RESOLVER.to_owned(),
            target_compose_path,
            compose_digest: blake3::hash(&compose).to_hex().to_string(),
            support_files: support_manifests,
            steps,
            macos_host,
            artifact_hash: String::new(),
        };
        manifest.artifact_hash = manifest.canonical_hash();
        let artifact = self.artifacts.publish(&CompiledTargetIngressArtifact {
            manifest: manifest.clone(),
            compose,
            support_files,
        })?;
        Ok(TargetIngressPlan {
            target_name: target.name.clone(),
            target_id: target.definition.id,
            plan_hash: manifest.artifact_hash,
            artifact,
        })
    }
}

impl TargetIngressManager {
    /// Remote targets are always Linux; the local target follows this workstation.
    fn macos_host(
        &self,
        target: &NamedTarget,
    ) -> Result<Option<crate::domain::target_ingress::MacosIngressHost>, MmError> {
        let profile = match target.definition.transport {
            TargetTransport::Local => self.local_host.clone(),
            TargetTransport::Ssh { .. } => IngressHostProfile::LinuxSystemd,
        };
        match profile {
            IngressHostProfile::LinuxSystemd => Ok(None),
            IngressHostProfile::MacosLaunchd(facts) => Ok(Some(
                crate::domain::target_ingress::MacosIngressHost::for_target(
                    facts,
                    target.definition.id,
                ),
            )),
            IngressHostProfile::Unavailable { reason } => Err(MmError::Target { message: reason }),
        }
    }
}

fn traefik_settings(target: &NamedTarget) -> Result<(&String, &Option<String>), MmError> {
    match target.definition.ingress.as_ref() {
        Some(TargetIngress::Traefik {
            image, acme_email, ..
        }) => Ok((image, acme_email)),
        None => Err(MmError::Target {
            message: format!(
                "target '{}' has no ingress configuration; run `mm target ingress configure`",
                target.name
            ),
        }),
    }
}

/// The platform-specific half of an ingress artifact.
struct HostIntegration {
    steps: Vec<TargetIngressStep>,
    support_files: Vec<CompiledTargetIngressSupportFile>,
    target_compose_path: String,
}

fn host_integration(
    macos_host: Option<&crate::domain::target_ingress::MacosIngressHost>,
    runtime: RuntimeKind,
    dynamic_config_volume_name: &str,
    compose_project_name: &str,
    (proxy_http_port, proxy_https_port): (u16, u16),
) -> HostIntegration {
    let mut steps = vec![
        TargetIngressStep::CreateExternalNetwork,
        TargetIngressStep::PreparePrivateIngressState,
        TargetIngressStep::ApplyIngressStack,
        TargetIngressStep::VerifyLoopbackIngress,
    ];
    match macos_host {
        None => {
            steps.extend([
                TargetIngressStep::InstallRootlessAutostart,
                TargetIngressStep::InstallPrivilegedPortForwarder,
            ]);
            HostIntegration {
                steps,
                support_files: render_support_files(
                    runtime,
                    dynamic_config_volume_name,
                    compose_project_name,
                    proxy_http_port,
                    proxy_https_port,
                ),
                target_compose_path: TARGET_INGRESS_COMPOSE_PATH.to_owned(),
            }
        }
        Some(host) => {
            steps.extend([
                TargetIngressStep::InstallLaunchdAutostart,
                TargetIngressStep::InstallLaunchdForwarder,
            ]);
            HostIntegration {
                steps,
                support_files: macos::render_support_files(
                    host,
                    dynamic_config_volume_name,
                    compose_project_name,
                    proxy_http_port,
                    proxy_https_port,
                ),
                target_compose_path: host
                    .ingress_path(crate::domain::target_ingress::MACOS_COMPOSE_FILE),
            }
        }
    }
}

fn render_support_files(
    runtime: RuntimeKind,
    dynamic_config_volume_name: &str,
    compose_project_name: &str,
    proxy_http_port: u16,
    proxy_https_port: u16,
) -> Vec<CompiledTargetIngressSupportFile> {
    let files = [
        (
            "dynamic.yaml",
            format!("podman-volume://{dynamic_config_volume_name}/dynamic.yaml"),
            "0644",
            crate::domain::traefik_dynamic::EMPTY_DYNAMIC_CONFIG.to_owned(),
        ),
        (
            "haproxy.cfg",
            "/etc/mm/ingress/haproxy.cfg".to_owned(),
            "0644",
            haproxy_config(proxy_http_port, proxy_https_port),
        ),
        (
            "mm-ingress-forwarder.service",
            "/etc/systemd/system/mm-ingress-forwarder.service".to_owned(),
            "0644",
            haproxy_service_unit(),
        ),
        (
            "mm-ingress.service",
            "%h/.config/systemd/user/mm-ingress.service".to_owned(),
            "0644",
            rootless_autostart_unit(compose_project_name, runtime),
        ),
    ];
    files
        .into_iter()
        .map(|(name, destination, mode, contents)| {
            compile_support_file(name, destination, mode, contents)
        })
        .collect()
}

fn compile_support_file(
    name: &str,
    destination: String,
    mode: &str,
    contents: String,
) -> CompiledTargetIngressSupportFile {
    let bytes = contents.into_bytes();
    CompiledTargetIngressSupportFile {
        manifest: TargetIngressSupportFileManifest {
            name: name.to_owned(),
            destination,
            mode: mode.to_owned(),
            digest: blake3::hash(&bytes).to_hex().to_string(),
        },
        bytes,
    }
}

fn rootless_autostart_unit(compose_project_name: &str, runtime: RuntimeKind) -> String {
    let (start, stop) = match runtime {
        RuntimeKind::PodmanCompose | RuntimeKind::DockerCompose => {
            let engine = if runtime == RuntimeKind::DockerCompose {
                "/usr/bin/docker"
            } else {
                "/usr/bin/podman"
            };
            let base = format!(
                "{engine} compose --file {TARGET_INGRESS_COMPOSE_PATH} --project-directory %h/.local/share/mm/ingress/current --project-name {compose_project_name}"
            );
            (
                format!("{base} up -d --remove-orphans"),
                format!("{base} down --remove-orphans"),
            )
        }
    };
    format!(
        "[Unit]\nDescription=mm target ingress\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=oneshot\nRemainAfterExit=true\nExecStart={start}\nExecStop={stop}\nTimeoutStartSec=120\nTimeoutStopSec=120\n\n[Install]\nWantedBy=default.target\n"
    )
}

/// The ingress network has fixed addressing on macOS so Traefik can trust the
/// Docker Desktop gateway that forwards loopback-published ports.
pub(crate) async fn ensure_ingress_network(
    runtime: &dyn crate::app::ports::LocalDeploymentRuntime,
    target: &NamedTarget,
    manifest: &crate::domain::target_ingress::TargetIngressArtifactManifest,
    resource: &str,
) -> Result<crate::domain::runtime::ResourceEnsureResult, MmError> {
    match &manifest.macos_host {
        None => {
            runtime
                .ensure_network(target, &manifest.network_name, resource)
                .await
        }
        Some(host) => {
            runtime
                .ensure_network_with_subnet(
                    target,
                    &manifest.network_name,
                    resource,
                    &host.network_subnet,
                    &host.network_gateway,
                )
                .await
        }
    }
}

pub(crate) fn haproxy_config(proxy_http_port: u16, proxy_https_port: u16) -> String {
    format!(
        "global\n  log stdout format raw local0\n\ndefaults\n  mode tcp\n  log global\n  timeout connect 5s\n  timeout client 60s\n  timeout server 60s\n\nfrontend mm-http\n  bind 0.0.0.0:{INGRESS_PUBLIC_HTTP_PORT}\n  bind [::]:{INGRESS_PUBLIC_HTTP_PORT} v6only\n  default_backend mm-http-traefik\n\nbackend mm-http-traefik\n  server ingress 127.0.0.1:{proxy_http_port} check send-proxy-v2\n\nfrontend mm-https\n  bind 0.0.0.0:{INGRESS_PUBLIC_HTTPS_PORT}\n  bind [::]:{INGRESS_PUBLIC_HTTPS_PORT} v6only\n  default_backend mm-https-traefik\n\nbackend mm-https-traefik\n  server ingress 127.0.0.1:{proxy_https_port} check send-proxy-v2\n"
    )
}

pub(crate) fn haproxy_service_unit() -> String {
    "[Unit]\nDescription=mm ingress privileged port forwarder\nAfter=network-online.target\nWants=network-online.target\nConditionPathExists=/etc/mm/ingress/haproxy.cfg\n\n[Service]\nType=simple\nExecStart=/usr/sbin/haproxy -W -db -f /etc/mm/ingress/haproxy.cfg\nRestart=on-failure\nRestartSec=2s\nDynamicUser=true\nAmbientCapabilities=CAP_NET_BIND_SERVICE\nCapabilityBoundingSet=CAP_NET_BIND_SERVICE\nNoNewPrivileges=true\nPrivateDevices=true\nPrivateTmp=true\nProtectControlGroups=true\nProtectHome=true\nProtectKernelModules=true\nProtectKernelTunables=true\nProtectSystem=strict\nRestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX\nRestrictRealtime=true\nLockPersonality=true\n\n[Install]\nWantedBy=multi-user.target\n"
        .to_owned()
}

#[derive(Serialize)]
struct IngressCompose<'a> {
    name: &'a str,
    services: BTreeMap<&'static str, TraefikService<'a>>,
    networks: BTreeMap<&'static str, ExternalResource<'a>>,
    volumes: BTreeMap<&'static str, ExternalResource<'a>>,
    #[serde(rename = "x-mm")]
    metadata: IngressMetadata,
}

#[derive(Serialize)]
struct TraefikService<'a> {
    image: &'a str,
    restart: &'static str,
    command: Vec<String>,
    ports: Vec<String>,
    volumes: Vec<String>,
    networks: Vec<&'static str>,
    extra_hosts: Vec<&'static str>,
    read_only: bool,
    tmpfs: Vec<&'static str>,
    cap_drop: Vec<&'static str>,
    security_opt: Vec<&'static str>,
    labels: BTreeMap<String, String>,
    cpus: f32,
    mem_limit: &'static str,
    pids_limit: u16,
}

#[derive(Serialize)]
struct ExternalResource<'a> {
    external: bool,
    name: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct IngressMetadata {
    target_id: String,
    certificate_resolver: &'static str,
}

struct IngressRender<'a> {
    target_id: String,
    image: &'a str,
    acme_email: Option<&'a str>,
    compose_project_name: &'a str,
    network_name: &'a str,
    acme_volume_name: &'a str,
    dynamic_config_volume_name: &'a str,
    proxy_http_port: u16,
    proxy_https_port: u16,
    proxy_verify_port: u16,
    trusted_proxy_peers: &'a str,
}

/// Linux publishes Traefik's ports through rootless port forwarding, so the
/// PROXY protocol peer is the host loopback.
const LINUX_TRUSTED_PROXY_PEERS: &str = "127.0.0.1/32,::1/128";

fn render_compose(input: &IngressRender<'_>) -> Result<Vec<u8>, MmError> {
    let command = traefik_command(input.acme_email, input.trusted_proxy_peers);
    let labels = [
        ("io.mm.target-id".to_owned(), input.target_id.clone()),
        ("io.mm.resource-kind".to_owned(), "stack".to_owned()),
        ("io.mm.resource".to_owned(), "ingress".to_owned()),
        ("io.mm.stack".to_owned(), "ingress".to_owned()),
        ("io.mm.generation".to_owned(), "1".to_owned()),
        ("io.mm.component".to_owned(), "local-ingress".to_owned()),
    ]
    .into_iter()
    .collect();
    let service = TraefikService {
        image: input.image,
        restart: "unless-stopped",
        command,
        ports: vec![
            format!("127.0.0.1:{}:8080", input.proxy_http_port),
            format!("127.0.0.1:{}:8443", input.proxy_https_port),
            format!("127.0.0.1:{}:8082", input.proxy_verify_port),
        ],
        volumes: vec![
            format!("mm-traefik-dynamic:{TRAEFIK_DYNAMIC_CONFIG_DIRECTORY}:ro"),
            "mm-traefik-acme:/var/lib/traefik".to_owned(),
        ],
        networks: vec!["mm-ingress"],
        extra_hosts: vec!["mm-control-host:host-gateway"],
        read_only: true,
        tmpfs: vec!["/tmp"],
        cap_drop: vec!["ALL"],
        security_opt: vec!["no-new-privileges:true"],
        labels,
        cpus: 1.0,
        mem_limit: "256m",
        pids_limit: 128,
    };
    serde_yaml_ng::to_string(&IngressCompose {
        name: input.compose_project_name,
        services: [("traefik", service)].into_iter().collect(),
        networks: [(
            "mm-ingress",
            ExternalResource {
                external: true,
                name: input.network_name,
            },
        )]
        .into_iter()
        .collect(),
        volumes: [
            (
                "mm-traefik-acme",
                ExternalResource {
                    external: true,
                    name: input.acme_volume_name,
                },
            ),
            (
                "mm-traefik-dynamic",
                ExternalResource {
                    external: true,
                    name: input.dynamic_config_volume_name,
                },
            ),
        ]
        .into_iter()
        .collect(),
        metadata: IngressMetadata {
            target_id: input.target_id.clone(),
            certificate_resolver: TRAEFIK_CERTIFICATE_RESOLVER,
        },
    })
    .map(String::into_bytes)
    .map_err(|error| MmError::Target {
        message: format!("could not serialize target ingress Compose artifact: {error}"),
    })
}

fn traefik_command(acme_email: Option<&str>, trusted_proxy_peers: &str) -> Vec<String> {
    let mut command = vec![
        "--api.dashboard=false".to_owned(),
        "--ping=true".to_owned(),
        "--ping.entrypoint=ping".to_owned(),
        format!("--providers.file.directory={TRAEFIK_DYNAMIC_CONFIG_DIRECTORY}"),
        "--providers.file.watch=true".to_owned(),
        "--entrypoints.web.address=:8080".to_owned(),
        "--entrypoints.websecure.address=:8443".to_owned(),
        "--entrypoints.ping.address=:8081".to_owned(),
        "--entrypoints.mm-verify.address=:8082".to_owned(),
        format!("--entrypoints.web.proxyprotocol.trustedips={trusted_proxy_peers}"),
        format!("--entrypoints.websecure.proxyprotocol.trustedips={trusted_proxy_peers}"),
        format!("--entrypoints.web.forwardedheaders.trustedips={trusted_proxy_peers}"),
        format!("--entrypoints.websecure.forwardedheaders.trustedips={trusted_proxy_peers}"),
    ];
    if let Some(acme_email) = acme_email {
        command.push(format!(
            "--certificatesresolvers.{TRAEFIK_CERTIFICATE_RESOLVER}.acme.email={acme_email}"
        ));
    }
    command.extend([
        format!(
            "--certificatesresolvers.{TRAEFIK_CERTIFICATE_RESOLVER}.acme.storage={TRAEFIK_ACME_STORAGE_PATH}"
        ),
        format!(
            "--certificatesresolvers.{TRAEFIK_CERTIFICATE_RESOLVER}.acme.httpchallenge.entrypoint=web"
        ),
    ]);
    command
}
