use super::*;
use crate::domain::development_mail::{
    DevelopmentMailValue, MailBindingManifest, MailBindingProvenance,
};
use crate::domain::development_mount::{
    DevelopmentMountConfig, DevelopmentMountMode, PreservedContainerPath, PreservedVolumeManifest,
};
use crate::domain::ingress::{
    IngressCertificateReference, IngressRouteOwnership, IngressRouteProvenance, ServiceHttpEvidence,
};
use uuid::Uuid;

fn manifest() -> StackArtifactManifest {
    StackArtifactManifest {
        project_id: ProjectId(Uuid::from_u128(1)),
        project_name: "project".to_owned(),
        target: "local".to_owned(),
        target_id: TargetId(Uuid::from_u128(2)),
        environment: "development".to_owned(),
        stack: "web".to_owned(),
        compose_project_name: "mm-project-web".to_owned(),
        user_files: vec![ComposeSourceManifest {
            path: PathBuf::from("compose.yaml"),
            digest: "a".repeat(64),
        }],
        generated_overlay_digest: "b".repeat(64),
        services: Vec::new(),
        routes: Vec::new(),
        middleware_inputs: Vec::new(),
        preserved_volumes: Vec::new(),
        security: Vec::new(),
        low_ports: Vec::new(),
        artifact_hash: String::new(),
    }
}

#[test]
fn canonical_hash_binds_mount_mail_route_middleware_and_volume_contracts() -> Result<(), String> {
    let mut value = manifest();
    let base = value.canonical_hash()?;
    value.services.push(StackServiceManifest {
        module: "app".to_owned(),
        service: "web".to_owned(),
        image: Some(format!("registry.example/app@sha256:{}", "c".repeat(64))),
        role: None,
        http_evidence: ServiceHttpEvidence::default(),
        development_mount: Some(DevelopmentMountConfig {
            additional: Vec::new(),
            source: PathBuf::from("."),
            target: "/app".to_owned(),
            mode: DevelopmentMountMode::ReadWrite,
            preserve: vec![PreservedContainerPath {
                path: "/app/vendor".to_owned(),
            }],
            excludes: Vec::new(),
            protected: Vec::new(),
            safe_exceptions: Vec::new(),
            services: vec!["web".to_owned()],
            preset: None,
        }),
        mail_bindings: [
            MailBindingProvenance::InjectedDefault,
            MailBindingProvenance::InjectedConfigured,
            MailBindingProvenance::PreservedProjectValue,
            MailBindingProvenance::PreservedComposeValue,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, provenance)| MailBindingManifest {
            key: format!("MAIL_{index}"),
            value: DevelopmentMailValue::SmtpHost,
            provenance,
        })
        .collect(),
    });
    value.routes.push(StackRouteManifest {
        name: "web".to_owned(),
        host: "app.test".to_owned(),
        path_prefix: "/".to_owned(),
        service: "web".to_owned(),
        port: 8080,
        tls: true,
        provenance: IngressRouteProvenance::ExplicitConfig,
        certificate_reference: IngressCertificateReference::None,
        ownership: IngressRouteOwnership::Project {
            project_id: value.project_id,
            environment: value.environment.clone(),
            stack: value.stack.clone(),
            service: "web".to_owned(),
        },
        priority: 1,
        router_name: "web".to_owned(),
        backend_alias: "web".to_owned(),
        middlewares: vec!["security".to_owned()],
        balance: crate::domain::cluster_ingress::Balance::LocalFirst,
        headers: std::collections::BTreeMap::new(),
        allow: Vec::new(),
    });
    value.middleware_inputs.push(StackMiddlewareInput {
        bundle: "security".to_owned(),
        path: PathBuf::from("middleware.yaml"),
        digest: "d".repeat(64),
    });
    value.preserved_volumes.push(PreservedVolumeManifest {
        module: "app".to_owned(),
        container_path: "/app/vendor".to_owned(),
        logical_name: "vendor".to_owned(),
        physical_name: "mm-vendor".to_owned(),
        resource: "volume".to_owned(),
    });
    let complete = value.canonical_hash()?;
    assert_ne!(complete, base);

    value.services[0].image = None;
    assert_ne!(value.canonical_hash()?, complete);
    value.services[0].image = Some("app:latest".to_owned());
    assert!(value.canonical_hash().is_err());
    value.services[0].image = Some(format!("Bad Repo@sha256:{}", "a".repeat(64)));
    assert!(value.canonical_hash().is_err());
    Ok(())
}

#[test]
fn canonical_hash_rejects_non_unicode_source_and_middleware_paths() {
    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let invalid = PathBuf::from(OsString::from_vec(vec![0xff]));
        let mut value = manifest();
        value.user_files[0].path = invalid.clone();
        assert!(value.canonical_hash().is_err());
        value.user_files[0].path = PathBuf::from("compose.yaml");
        value.middleware_inputs.push(StackMiddlewareInput {
            bundle: "security".to_owned(),
            path: invalid,
            digest: "a".repeat(64),
        });
        assert!(value.canonical_hash().is_err());
    }
}

#[test]
fn compiled_artifact_debug_reports_manifests_and_sizes_without_rendered_bytes() {
    let manifest = manifest();
    let source = CompiledComposeSource {
        manifest: manifest.user_files[0].clone(),
        bytes: b"secret compose content".to_vec(),
    };
    let source_debug = format!("{source:?}");
    assert!(source_debug.contains("byte_count: 22"));
    assert!(!source_debug.contains("secret compose content"));
    let artifact = CompiledStackArtifact {
        manifest,
        user_files: vec![source],
        generated_overlay: b"secret overlay".to_vec(),
        config_files: Vec::new(),
    };
    let debug = format!("{artifact:?}");
    assert!(debug.contains("generated_overlay_bytes: 14"));
    assert!(!debug.contains("secret overlay"));
}
