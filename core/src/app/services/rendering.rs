use super::*;

pub(super) fn components(
    target: &NamedTarget,
    data_volume: &str,
    cap_data_volume: Option<&str>,
) -> Vec<ServiceComponentDescriptor> {
    let ingress_image = target
        .definition
        .ingress
        .as_ref()
        .map(|ingress| match ingress {
            crate::domain::target::TargetIngress::Traefik { image, .. } => image.clone(),
        });
    let mut components = vec![
        ServiceComponentDescriptor {
            kind: ServiceComponentKind::LocalIngress,
            version: "3".to_owned(),
            image: ingress_image,
            dependencies: Vec::new(),
            volumes: vec![
                PersistentVolumeOwner {
                    volume: ingress_acme_volume_name(target.definition.id),
                    uid: 0,
                    gid: 0,
                    retained_by_default: true,
                },
                PersistentVolumeOwner {
                    volume: ingress_dynamic_config_volume_name(target.definition.id),
                    uid: 0,
                    gid: 0,
                    retained_by_default: true,
                },
            ],
            engine_socket_mounted: false,
        },
        ServiceComponentDescriptor {
            kind: ServiceComponentKind::MmControlApi,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            image: None,
            dependencies: vec![ServiceComponentKind::LocalIngress],
            volumes: Vec::new(),
            engine_socket_mounted: false,
        },
        ServiceComponentDescriptor {
            kind: ServiceComponentKind::Mailpit,
            version: MAILPIT_VERSION.to_owned(),
            image: Some(MAILPIT_IMAGE.to_owned()),
            dependencies: vec![ServiceComponentKind::LocalIngress],
            volumes: vec![PersistentVolumeOwner {
                volume: data_volume.to_owned(),
                uid: 0,
                gid: 0,
                retained_by_default: true,
            }],
            engine_socket_mounted: false,
        },
    ];
    if let Some(cap_data_volume) = cap_data_volume {
        components.push(ServiceComponentDescriptor {
            kind: ServiceComponentKind::CapValkey,
            version: VALKEY_VERSION.to_owned(),
            image: Some(VALKEY_IMAGE.to_owned()),
            dependencies: Vec::new(),
            volumes: vec![PersistentVolumeOwner {
                volume: cap_data_volume.to_owned(),
                uid: 999,
                gid: 1000,
                retained_by_default: true,
            }],
            engine_socket_mounted: false,
        });
        components.push(ServiceComponentDescriptor {
            kind: ServiceComponentKind::Cap,
            version: CAP_VERSION.to_owned(),
            image: Some(CAP_IMAGE.to_owned()),
            dependencies: vec![
                ServiceComponentKind::CapValkey,
                ServiceComponentKind::LocalIngress,
            ],
            volumes: Vec::new(),
            engine_socket_mounted: false,
        });
    }
    components
}

#[derive(Serialize)]
pub(super) struct MailpitCompose {
    name: String,
    services: BTreeMap<String, ServiceDefinition>,
    networks: BTreeMap<String, ExternalResource>,
    volumes: BTreeMap<String, ExternalResource>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    secrets: BTreeMap<String, FileSecret>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum ServiceDefinition {
    Mailpit(MailpitService),
    Cap(CapService),
    Valkey(ValkeyService),
}

#[derive(Serialize)]
pub(super) struct MailpitService {
    image: &'static str,
    restart: &'static str,
    environment: BTreeMap<&'static str, &'static str>,
    volumes: Vec<&'static str>,
    networks: BTreeMap<&'static str, NetworkAttachment>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    ports: Vec<String>,
    labels: BTreeMap<String, String>,
    read_only: bool,
    tmpfs: Vec<&'static str>,
    cap_drop: Vec<&'static str>,
    security_opt: Vec<&'static str>,
    cpus: f32,
    mem_limit: &'static str,
    pids_limit: u16,
}

#[derive(Serialize)]
pub(super) struct CapService {
    image: &'static str,
    restart: &'static str,
    entrypoint: Vec<&'static str>,
    environment: BTreeMap<&'static str, String>,
    depends_on: BTreeMap<&'static str, DependencyCondition>,
    secrets: Vec<&'static str>,
    networks: BTreeMap<&'static str, NetworkAttachment>,
    labels: BTreeMap<String, String>,
    read_only: bool,
    tmpfs: Vec<&'static str>,
    cap_drop: Vec<&'static str>,
    security_opt: Vec<&'static str>,
    cpus: f32,
    mem_limit: &'static str,
    pids_limit: u16,
}

#[derive(Serialize)]
pub(super) struct ValkeyService {
    image: &'static str,
    user: &'static str,
    restart: &'static str,
    command: Vec<&'static str>,
    healthcheck: Healthcheck,
    volumes: Vec<&'static str>,
    networks: BTreeMap<&'static str, NetworkAttachment>,
    labels: BTreeMap<String, String>,
    read_only: bool,
    tmpfs: Vec<&'static str>,
    cap_drop: Vec<&'static str>,
    security_opt: Vec<&'static str>,
    cpus: f32,
    mem_limit: &'static str,
    pids_limit: u16,
}

#[derive(Serialize)]
pub(super) struct DependencyCondition {
    condition: &'static str,
}

#[derive(Serialize)]
pub(super) struct Healthcheck {
    test: Vec<&'static str>,
    interval: &'static str,
    timeout: &'static str,
    retries: u8,
}

#[derive(Serialize)]
pub(super) struct FileSecret {
    file: String,
}

#[derive(Serialize)]
pub(super) struct NetworkAttachment {
    aliases: Vec<&'static str>,
}

#[derive(Serialize)]
pub(super) struct ExternalResource {
    external: bool,
    name: String,
}

pub(super) fn render_compose(
    target: &NamedTarget,
    development_network: &str,
    data_volume: &str,
    host_smtp_port: Option<u16>,
    cap_data_volume: Option<&str>,
    cap_admin_key_file: Option<&Path>,
) -> Result<Vec<u8>, MmError> {
    let ownership = ResourceOwnership::for_named_target(
        target,
        ResourceKind::Stack,
        services_project_name(target.definition.id),
        "services",
    );
    let service = mailpit_service(&ownership, host_smtp_port);
    let mut services =
        BTreeMap::from([("mailpit".to_owned(), ServiceDefinition::Mailpit(service))]);
    let mut volumes = BTreeMap::from([(
        "mailpit-data".to_owned(),
        ExternalResource {
            external: true,
            name: data_volume.to_owned(),
        },
    )]);
    let mut secrets = BTreeMap::new();
    if let (Some(cap_data_volume), Some(cap_admin_key_file)) = (cap_data_volume, cap_admin_key_file)
    {
        services.insert(
            "valkey".to_owned(),
            ServiceDefinition::Valkey(valkey_service(&ownership)),
        );
        services.insert(
            "cap".to_owned(),
            ServiceDefinition::Cap(cap_service(&ownership)),
        );
        volumes.insert(
            "cap-valkey-data".to_owned(),
            ExternalResource {
                external: true,
                name: cap_data_volume.to_owned(),
            },
        );
        secrets.insert(
            "cap-admin-key".to_owned(),
            FileSecret {
                file: cap_admin_key_file.display().to_string(),
            },
        );
    }
    serde_yaml_ng::to_string(&MailpitCompose {
        name: services_project_name(target.definition.id),
        services,
        networks: [
            (
                "ingress".to_owned(),
                ExternalResource {
                    external: true,
                    name: ingress_network_name(target.definition.id),
                },
            ),
            (
                "development".to_owned(),
                ExternalResource {
                    external: true,
                    name: development_network.to_owned(),
                },
            ),
        ]
        .into_iter()
        .collect(),
        volumes,
        secrets,
    })
    .map(String::into_bytes)
    .map_err(|error| MmError::State {
        message: format!("could not render service Compose artifact: {error}"),
    })
}

fn mailpit_service(ownership: &ResourceOwnership, host_smtp_port: Option<u16>) -> MailpitService {
    let mut labels = ownership.labels();
    labels.insert("io.mm.component".to_owned(), "mailpit".to_owned());
    labels.insert(
        "io.mm.component-version".to_owned(),
        MAILPIT_VERSION.to_owned(),
    );
    MailpitService {
        image: MAILPIT_IMAGE,
        restart: "unless-stopped",
        environment: [
            ("MP_DATABASE", "/data/mailpit.db"),
            ("MP_DISABLE_VERSION_CHECK", "true"),
            ("MP_SMTP_DISABLE_RDNS", "true"),
        ]
        .into_iter()
        .collect(),
        volumes: vec!["mailpit-data:/data"],
        networks: [
            (
                "ingress",
                NetworkAttachment {
                    aliases: vec!["mailpit"],
                },
            ),
            (
                "development",
                NetworkAttachment {
                    aliases: vec!["mailpit"],
                },
            ),
        ]
        .into_iter()
        .collect(),
        ports: host_smtp_port
            .map(|port| vec![format!("127.0.0.1:{port}:{MAILPIT_SMTP_PORT}")])
            .unwrap_or_default(),
        labels,
        read_only: true,
        tmpfs: vec!["/tmp"],
        cap_drop: vec!["ALL"],
        security_opt: vec!["no-new-privileges:true"],
        cpus: 1.0,
        mem_limit: "256m",
        pids_limit: 128,
    }
}

fn valkey_service(ownership: &ResourceOwnership) -> ValkeyService {
    let mut labels = ownership.labels();
    labels.insert("io.mm.component".to_owned(), "cap-valkey".to_owned());
    labels.insert(
        "io.mm.component-version".to_owned(),
        VALKEY_VERSION.to_owned(),
    );
    ValkeyService {
        image: VALKEY_IMAGE,
        user: "999:1000",
        restart: "unless-stopped",
        command: vec![
            "valkey-server",
            "--save",
            "60",
            "1",
            "--loglevel",
            "warning",
            "--maxmemory-policy",
            "noeviction",
        ],
        healthcheck: Healthcheck {
            test: vec!["CMD", "valkey-cli", "ping"],
            interval: "5s",
            timeout: "3s",
            retries: 5,
        },
        volumes: vec!["cap-valkey-data:/data"],
        networks: [(
            "ingress",
            NetworkAttachment {
                aliases: vec!["valkey"],
            },
        )]
        .into_iter()
        .collect(),
        labels,
        read_only: true,
        tmpfs: vec!["/tmp"],
        cap_drop: vec!["ALL"],
        security_opt: vec!["no-new-privileges:true"],
        cpus: 0.5,
        mem_limit: "256m",
        pids_limit: 128,
    }
}

fn cap_service(ownership: &ResourceOwnership) -> CapService {
    let mut labels = ownership.labels();
    labels.insert("io.mm.component".to_owned(), "cap".to_owned());
    labels.insert("io.mm.component-version".to_owned(), CAP_VERSION.to_owned());
    CapService {
        image: CAP_IMAGE,
        restart: "unless-stopped",
        entrypoint: vec![
            "/bin/sh",
            "-eu",
            "-c",
            "export ADMIN_KEY=\"$$(cat /run/secrets/cap-admin-key)\"; exec bun run ./src/index.js",
        ],
        environment: [
            ("CORS_ORIGIN", "*".to_owned()),
            ("REDIS_URL", "redis://valkey:6379".to_owned()),
            ("REDIS_PREFIX", "mm:cap:".to_owned()),
            ("RATELIMIT_IP_HEADER", "x-forwarded-for".to_owned()),
        ]
        .into_iter()
        .collect(),
        depends_on: [(
            "valkey",
            DependencyCondition {
                condition: "service_healthy",
            },
        )]
        .into_iter()
        .collect(),
        secrets: vec!["cap-admin-key"],
        networks: [(
            "ingress",
            NetworkAttachment {
                aliases: vec!["cap"],
            },
        )]
        .into_iter()
        .collect(),
        labels,
        read_only: true,
        tmpfs: vec!["/tmp", "/usr/src/app/data"],
        cap_drop: vec!["ALL"],
        security_opt: vec!["no-new-privileges:true"],
        cpus: 1.0,
        mem_limit: "512m",
        pids_limit: 128,
    }
}

pub(super) fn render_route(
    dashboard_hosts: &BTreeSet<String>,
    dashboard_ingress_token: &str,
    gate_enabled: bool,
) -> Vec<u8> {
    let dashboard_certificates = dashboard_hosts
        .iter()
        .map(|host| {
            format!(
                "    - certFile: /etc/traefik/dynamic/{host}.crt\n      keyFile: /etc/traefik/dynamic/{host}.key\n"
            )
        })
        .collect::<String>();
    let cap_routes = if gate_enabled {
        format!(
            "    mm-cap-secure:\n      rule: Host(`{CAP_HOST}`) && PathRegexp(`^/[A-Za-z0-9_-]+/(challenge|redeem)$`)\n      entryPoints: [websecure]\n      service: mm-cap\n      tls: {{}}\n    mm-cap-redirect:\n      rule: Host(`{CAP_HOST}`) && PathRegexp(`^/[A-Za-z0-9_-]+/(challenge|redeem)$`)\n      entryPoints: [web]\n      service: mm-cap\n      middlewares: [mm-managed-https]\n    mm-cap-verify:\n      rule: Host(`{CAP_HOST}`) && (Path(`/`) || PathRegexp(`^/[A-Za-z0-9_-]+/siteverify$`))\n      entryPoints: [{TRAEFIK_VERIFY_ENTRYPOINT}]\n      service: mm-cap\n      middlewares: [{TRAEFIK_VERIFY_MIDDLEWARE}]\n    mm-cap-admin:\n      rule: Host(`{CAP_HOST}`) && (Path(`/auth/login`) || PathPrefix(`/server/keys`))\n      entryPoints: [{TRAEFIK_VERIFY_ENTRYPOINT}]\n      service: mm-cap\n"
        )
    } else {
        String::new()
    };
    let cap_service = if gate_enabled {
        format!(
            "    mm-cap:\n      loadBalancer:\n        servers:\n          - url: http://cap:{CAP_PORT}\n"
        )
    } else {
        String::new()
    };
    let cap_certificate = if gate_enabled {
        format!(
            "    - certFile: /etc/traefik/dynamic/{CAP_HOST}.crt\n      keyFile: /etc/traefik/dynamic/{CAP_HOST}.key\n"
        )
    } else {
        String::new()
    };
    format!(
        "http:\n  routers:\n    mm-dashboard-secure:\n      rule: Host(`{DASHBOARD_HOST}`) || HostRegexp(`^mm\\.[a-z0-9-]+\\.test$`)\n      entryPoints: [websecure]\n      service: mm-dashboard\n      middlewares: [mm-dashboard-auth]\n      tls: {{}}\n    mm-dashboard-redirect:\n      rule: Host(`{DASHBOARD_HOST}`) || HostRegexp(`^mm\\.[a-z0-9-]+\\.test$`)\n      entryPoints: [web]\n      service: mm-dashboard\n      middlewares: [mm-managed-https]\n    mm-dashboard-verify:\n      rule: Host(`{DASHBOARD_HOST}`)\n      entryPoints: [{TRAEFIK_VERIFY_ENTRYPOINT}]\n      service: mm-dashboard\n      middlewares: [mm-dashboard-auth, {TRAEFIK_VERIFY_MIDDLEWARE}]\n{cap_routes}    mm-mailpit-secure:\n      rule: Host(`{MAILPIT_HOST}`)\n      entryPoints: [websecure]\n      service: mm-mailpit\n      tls: {{}}\n    mm-mailpit-redirect:\n      rule: Host(`{MAILPIT_HOST}`)\n      entryPoints: [web]\n      service: mm-mailpit\n      middlewares: [mm-managed-https]\n    mm-mailpit-verify:\n      rule: Host(`{MAILPIT_HOST}`)\n      entryPoints: [{TRAEFIK_VERIFY_ENTRYPOINT}]\n      service: mm-mailpit\n      middlewares: [{TRAEFIK_VERIFY_MIDDLEWARE}]\n  middlewares:\n    mm-dashboard-auth:\n      headers:\n        customRequestHeaders:\n          X-MM-Ingress-Auth: \"{dashboard_ingress_token}\"\n    mm-gate-ingress-auth:\n      headers:\n        customRequestHeaders:\n          X-MM-Ingress-Auth: \"{dashboard_ingress_token}\"\n    mm-gate-ingress-clear:\n      headers:\n        customRequestHeaders:\n          X-MM-Ingress-Auth: \"\"\n    mm-managed-https:\n      redirectScheme:\n        scheme: https\n        permanent: true\n    {TRAEFIK_VERIFY_MIDDLEWARE}:\n      headers:\n        customResponseHeaders:\n          {TRAEFIK_VERIFY_HEADER}: \"{TRAEFIK_VERIFY_HEADER_VALUE}\"\n  services:\n    mm-dashboard:\n      loadBalancer:\n        servers:\n          - url: http://mm-control-host:{DASHBOARD_BACKEND_PORT}\n    mm-mailpit:\n      loadBalancer:\n        servers:\n          - url: http://mailpit:{MAILPIT_UI_PORT}\n{cap_service}tls:\n  certificates:\n{dashboard_certificates}{cap_certificate}    - certFile: /etc/traefik/dynamic/{MAILPIT_HOST}.crt\n      keyFile: /etc/traefik/dynamic/{MAILPIT_HOST}.key\n"
    )
    .into_bytes()
}

/// systemd user unit on Linux; launchd user agent when the local ingress is macOS.
pub(crate) fn render_dashboard_service_unit(
    service: &DashboardServiceConfig,
    runtime: RuntimeKind,
    macos_host: Option<&crate::domain::target_ingress::MacosIngressHost>,
) -> Result<Vec<u8>, MmError> {
    let runtime = match runtime {
        RuntimeKind::PodmanCompose => "podman-compose",
        RuntimeKind::DockerCompose => "docker-compose",
    };
    if let Some(host) = macos_host {
        if runtime != "docker-compose" {
            return Err(MmError::Target {
                message:
                    "local services on macOS require the docker-compose runtime with Docker Desktop"
                        .to_owned(),
            });
        }
        return Ok(crate::app::target_ingress::render_dashboard_plist(
            host,
            absolute_utf8(&service.executable)?,
            absolute_utf8(&service.mm_home)?,
            runtime,
        )
        .into_bytes());
    }
    let executable = systemd_quote_path(&service.executable)?;
    let mm_home = systemd_quote_path(&service.mm_home)?;
    let inherited = runtime_environment(runtime).join(" ");
    // Mount-namespace sandboxing in a user unit implicitly remaps unmapped
    // owners to nobody. Preserve host UIDs so mm's path ownership checks remain
    // authoritative; the service still runs unprivileged with no new privileges.
    Ok(format!(
        "[Unit]\nDescription=mm local dashboard control API\nAfter=network-online.target mm-ingress.service\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={executable} --mm-home {mm_home} --runtime {runtime} dev dashboard serve --listen 0.0.0.0:{DASHBOARD_BACKEND_PORT}\nRestart=on-failure\nRestartSec=2s\nTimeoutStopSec=30\nUMask=0077\nPassEnvironment={inherited}\nNoNewPrivileges=true\nRestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX\nRestrictRealtime=true\nLockPersonality=true\n\n[Install]\nWantedBy=default.target\n"
    )
    .into_bytes())
}

fn absolute_utf8(path: &Path) -> Result<&str, MmError> {
    path.to_str()
        .filter(|_| path.is_absolute())
        .ok_or_else(|| MmError::Security {
            message: "dashboard service paths must be absolute UTF-8 paths".to_owned(),
        })
}

pub(super) fn runtime_environment(runtime: &str) -> &'static [&'static str] {
    if runtime == "podman-compose" {
        crate::domain::runtime::PODMAN_INHERITED_ENVIRONMENT
    } else {
        crate::domain::runtime::DOCKER_INHERITED_ENVIRONMENT
    }
}

pub(super) fn systemd_quote_path(path: &Path) -> Result<String, MmError> {
    if !path.is_absolute() {
        return Err(MmError::Security {
            message: "dashboard service paths must be absolute".to_owned(),
        });
    }
    let value = path.to_str().ok_or_else(|| MmError::Security {
        message: "dashboard service paths must be valid UTF-8".to_owned(),
    })?;
    if value.chars().any(char::is_control) {
        return Err(MmError::Security {
            message: "dashboard service paths must not contain control characters".to_owned(),
        });
    }
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('$', "$$");
    Ok(format!("\"{escaped}\""))
}

pub(super) fn validate_dashboard_ingress_token(token: &str) -> Result<(), MmError> {
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

pub(super) fn artifact_identity(
    request: ServicePlanRequest,
    runtime: RuntimeKind,
    target: &NamedTarget,
    previous: &str,
    artifacts: [&[u8]; 3],
    ingress_hash: &str,
) -> String {
    let [compose, route, dashboard_unit] = artifacts;
    let value = format!(
        "{}|{:?}|{}|{}|{}|{}|{}|{}|{:?}|{}|{}",
        request.action.as_str(),
        runtime,
        target.definition.id.0,
        previous,
        blake3::hash(compose).to_hex(),
        blake3::hash(route).to_hex(),
        blake3::hash(dashboard_unit).to_hex(),
        ingress_hash,
        request.host_smtp_port,
        request.purge_data,
        request.include_gate
    );
    blake3::hash(value.as_bytes()).to_hex().to_string()
}

pub(super) fn require_hash(plan: &ServiceStackPlan, expected: &str) -> Result<(), MmError> {
    if plan.plan_hash == expected {
        Ok(())
    } else {
        Err(MmError::State {
            message: format!(
                "service stack plan changed (expected {expected}, current {}); review the new plan",
                plan.plan_hash
            ),
        })
    }
}

pub(super) fn status_from_state(
    state: &ServiceStackState,
    components: Vec<crate::domain::service_stack::ServiceComponentStatus>,
    diagnostics: Vec<Diagnostic>,
) -> ServiceStackStatus {
    ServiceStackStatus {
        installed: true,
        stack_version: Some(state.stack_version.clone()),
        runtime: Some(state.runtime),
        target_id: Some(state.target_id),
        route: format!("https://{MAILPIT_HOST}"),
        internal_smtp: format!("mailpit:{MAILPIT_SMTP_PORT}"),
        host_smtp_port: state.host_smtp_port,
        components,
        diagnostics,
    }
}
