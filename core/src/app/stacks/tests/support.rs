use std::fs;

use crate::adapters::config::ConfigLoader;
use crate::domain::target::{TargetDefinition, TargetId, TargetTransport};

use super::*;

pub(in crate::app::stacks) struct StackFixture {
    pub(in crate::app::stacks) root: tempfile::TempDir,
    pub(in crate::app::stacks) document: ConfigDocument,
    pub(in crate::app::stacks) target: NamedTarget,
}

impl StackFixture {
    pub(in crate::app::stacks) fn new(compose: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let root = crate::test_support::tempdir()?;
        fs::create_dir_all(root.path().join(".config"))?;
        fs::create_dir_all(root.path().join("app"))?;
        fs::write(root.path().join("compose.yaml"), compose)?;
        fs::write(
            root.path().join(".config/mm.yaml"),
            "schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: Stack_Fixture
  development-environment: development
default-environment: development
target: local
runtime:
  driver: podman-compose
networks:
  backend: {}
  service-net: {}
stacks:
  app:
    files: [compose.yaml]
    networks: [backend]
modules:
  app:
    path: app
    networks: [backend]
    services:
      web:
        stack: app
        networks: [service-net]
environments:
  development:
    domains:
      public: public.example.test
    bindings: {}
  production:
    domains:
      public: production.example.test
    bindings: {}
",
        )?;
        let document = ConfigLoader.load_from(root.path())?;
        let target = NamedTarget::new(
            "local".to_owned(),
            TargetDefinition::local(TargetId(uuid::Uuid::parse_str(
                "319c527b-6242-7b43-bfd4-bd60e27c1de3",
            )?)),
        );
        Ok(Self {
            root,
            document,
            target,
        })
    }

    pub(in crate::app::stacks) fn remote_target(&self) -> NamedTarget {
        let mut definition = self.target.definition.clone();
        definition.transport = TargetTransport::Ssh {
            host: "host.example.test".to_owned(),
            port: 22,
            user: "deploy".to_owned(),
            host_key_fingerprint: "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU".to_owned(),
            host_key: None,
            identity_file: None,
            remote_root: None,
            daemon_fingerprint: None,
        };
        NamedTarget::new("remote".to_owned(), definition)
    }
}

pub(in crate::app::stacks) fn candidate(
    port: u16,
    protocol: TransportProtocol,
) -> HttpPortCandidate {
    HttpPortCandidate {
        port,
        protocol,
        sources: vec![HttpEvidenceSource::ComposeExpose],
    }
}

pub(in crate::app::stacks) fn source(path: &str, bytes: &str) -> CompiledComposeSource {
    CompiledComposeSource {
        manifest: ComposeSourceManifest {
            path: PathBuf::from(path),
            digest: blake3::hash(bytes.as_bytes()).to_hex().to_string(),
        },
        bytes: bytes.as_bytes().to_vec(),
    }
}

pub(in crate::app::stacks) fn service(
    name: &str,
    declared: bool,
    candidates: Vec<HttpPortCandidate>,
) -> StackServiceManifest {
    StackServiceManifest {
        module: "app".to_owned(),
        service: name.to_owned(),
        image: None,
        role: None,
        http_evidence: ServiceHttpEvidence {
            declared_in_compose: declared,
            candidates,
            problem: None,
        },
        development_mount: None,
        mail_bindings: Vec::new(),
    }
}

pub(in crate::app::stacks) fn route(
    document: &ConfigDocument,
    service: &str,
    host: &str,
    path: &str,
    tls: bool,
) -> StackRouteManifest {
    StackRouteManifest {
        name: ingress_route_name(host, path),
        host: host.to_owned(),
        path_prefix: path.to_owned(),
        service: service.to_owned(),
        port: 8080,
        tls,
        provenance: IngressRouteProvenance::ExplicitConfig,
        certificate_reference: certificate_reference(host, tls),
        ownership: project_route_ownership(document, "development", "app", service),
        priority: route_priority(path),
        router_name: "fixture-router".to_owned(),
        backend_alias: "fixture-backend".to_owned(),
        middlewares: Vec::new(),
        balance: crate::domain::cluster_ingress::Balance::LocalFirst,
        headers: std::collections::BTreeMap::new(),
        allow: Vec::new(),
    }
}

pub(in crate::app::stacks) fn prepared_stack(
    document: &ConfigDocument,
    services: Vec<StackServiceManifest>,
    routes: Vec<StackRouteManifest>,
) -> PreparedStackArtifact {
    PreparedStackArtifact {
        root: document.root.join("artifact"),
        manifest_path: document.root.join("artifact/manifest.json"),
        generated_overlay_path: document.root.join("artifact/generated.yaml"),
        compose_files: Vec::new(),
        manifest: StackArtifactManifest {
            project_id: document.config.project.id,
            project_name: document.config.project.name.clone(),
            target: document.config.target.clone(),
            target_id: TargetId(uuid::Uuid::nil()),
            environment: "development".to_owned(),
            stack: "app".to_owned(),
            compose_project_name: "fixture".to_owned(),
            user_files: Vec::new(),
            generated_overlay_digest: String::new(),
            services,
            routes,
            middleware_inputs: Vec::new(),
            preserved_volumes: Vec::new(),
            security: Vec::new(),
            low_ports: Vec::new(),
            host_network: Vec::new(),
            masked_paths: Vec::new(),
            artifact_hash: String::new(),
        },
    }
}
