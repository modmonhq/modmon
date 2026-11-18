use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn yaml(source: &str) -> serde_yaml_ng::Value {
    let parsed = serde_yaml_ng::from_str(source);
    assert!(parsed.is_ok(), "invalid test YAML: {parsed:?}");
    parsed.unwrap_or(serde_yaml_ng::Value::Null)
}

#[test]
fn transitive_sources_and_yaml_extensions_are_rejected() -> Result<(), MmError> {
    let path = Path::new("compose.yaml");
    for source in [
        "[unterminated",
        "- services",
        "services:\n  web:\n    <<: invalid\n",
        "include: other.yaml\nservices: {}\n",
        "services:\n  web:\n    extends:\n      file: base.yaml\n      service: web\n",
        "services:\n  web: !custom {}\n",
    ] {
        assert!(reject_transitive_compose_files(path, source.as_bytes()).is_err());
    }
    reject_transitive_compose_files(
        path,
        b"x-default: &default {image: app}\nservices: {web: {<<: *default}}\n",
    )?;
    reject_transitive_compose_files(path, b"name: compose")?;
    Ok(())
}

#[test]
fn reserved_resource_identity_network_and_environment_contracts_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let root = crate::test_support::tempdir()?;
    let routed = BTreeSet::from(["web"]);
    let cases = [
        "networks: {mm-ingress-private: {}}",
        "networks: {app: {name: MM-INGRESS-private}}",
        "networks: {app: {external: {name: '$NETWORK'}}}",
        "volumes: {data: {driver_opts: {type: tmpfs}}}",
        "volumes: {data: {driver: nfs}}",
        "services: {mm-ingress-proxy: {}}",
        "services: {web: {container_name: '$DYNAMIC'}}",
        "services: {web: {hostname: MM-INGRESS-router}}",
        "services: {web: {links: ['db:mm-ingress-db']}}",
        "services: {web: {labels: {traefik.enable: 'true'}}}",
        "services: {web: {labels: ['io.mm.owner=test']}}",
        "services: {web: {networks: {mm-ingress-private: {}}}}",
        "services: {web: {networks: [mm-ingress-private]}}",
        "services: {web: {network_mode: bridge}}",
        "services: {worker: {network_mode: host}}",
        "services: {worker: {network_mode: '$MODE'}}",
        "services: {web: {environment: {A: B}}}",
        "services: {web: {env_file: [.env]}}",
    ];
    for source in cases {
        let result = reject_reserved_ingress_configuration(
            Path::new("compose.yaml"),
            source.as_bytes(),
            &routed,
            (80, 443, 8080),
            root.path(),
            root.path(),
        );
        assert!(result.is_err(), "must reject: {source}");
    }
    reject_reserved_ingress_configuration(
        Path::new("compose.yaml"),
        b"services: {web: image-only, worker: {network_mode: bridge}}",
        &BTreeSet::new(),
        (80, 443, 8080),
        root.path(),
        root.path(),
    )?;
    reject_reserved_ingress_configuration(
        Path::new("compose.yaml"),
        b"name: compose",
        &BTreeSet::new(),
        (80, 443, 8080),
        root.path(),
        root.path(),
    )?;
    reject_reserved_ingress_configuration(
        Path::new("compose.yaml"),
        b"services: {web: {labels: {example.owner: safe}, networks: [public]}}\nnetworks: {public: {name: public}}",
        &BTreeSet::new(),
        (80, 443, 8080),
        root.path(),
        root.path(),
    )?;
    for source in [
        "[unterminated",
        "services:\n  web:\n    <<: invalid\n",
        "services: {web: !custom {}}",
    ] {
        assert!(
            reject_reserved_ingress_configuration(
                Path::new("compose.yaml"),
                source.as_bytes(),
                &BTreeSet::new(),
                (80, 443, 8080),
                root.path(),
                root.path(),
            )
            .is_err()
        );
    }
    reject_reserved_ingress_configuration(
        Path::new("compose.yaml"),
        b"plain scalar",
        &BTreeSet::new(),
        (80, 443, 8080),
        root.path(),
        root.path(),
    )?;
    Ok(())
}

#[test]
fn unsafe_mount_and_proxy_port_forms_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let root = crate::test_support::tempdir()?;
    fs::create_dir(root.path().join("safe"))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink("/tmp", root.path().join("escape"))?;
    let sources = [
        "services: {web: {volumes_from: [other]}}",
        "services: {web: {volumes: ['mm-ingress-data:/data']}}",
        "services: {web: {volumes: ['$SOURCE:/data']}}",
        "services: {web: {volumes: ['~/data:/data']}}",
        "services: {web: {volumes: ['/tmp:/data']}}",
        "services: {web: {volumes: [{type: '$TYPE', source: data, target: /data}]}}",
        "services: {web: {ports: ['127.0.0.1:80:8000']}}",
        "services: {web: {ports: [{published: 443, target: 8443}]}}",
        "services: {web: {ports: [{published: '8079-8081', target: 9000}]}}",
    ];
    for source in sources {
        assert!(
            reject_reserved_ingress_configuration(
                Path::new("compose.yaml"),
                source.as_bytes(),
                &BTreeSet::new(),
                (80, 443, 8080),
                root.path(),
                root.path(),
            )
            .is_err()
        );
    }
    #[cfg(unix)]
    assert!(
        reject_reserved_ingress_configuration(
            Path::new("compose.yaml"),
            b"services: {web: {volumes: ['./escape:/data']}}",
            &BTreeSet::new(),
            (80, 443, 8080),
            root.path(),
            root.path(),
        )
        .is_err()
    );
    #[cfg(unix)]
    assert!(
        uses_unsafe_ingress_volume(
            &yaml("[{type: bind, source: './escape', target: /data}]"),
            root.path(),
            root.path(),
        )
        .is_err()
    );
    reject_reserved_ingress_configuration(
        Path::new("compose.yaml"),
        b"services: {web: {volumes: ['./safe:/data', 'cache:/cache'], ports: ['9000:9000']}}",
        &BTreeSet::new(),
        (80, 443, 8080),
        root.path(),
        root.path(),
    )?;
    Ok(())
}

#[test]
fn managed_network_declarations_and_attachments_are_rejected() -> Result<(), MmError> {
    let path = Path::new("compose.yaml");
    let managed = BTreeSet::from(["backend".to_owned()]);
    for source in [
        "networks: {backend: {}}",
        "services: {web: {networks: [backend]}}",
        "services: {web: {networks: {backend: {}}}}",
    ] {
        assert!(reject_nonportable_compose(path, source.as_bytes(), &managed).is_err());
    }
    assert!(reject_nonportable_compose(path, b"[invalid", &managed).is_err());
    reject_nonportable_compose(path, b"scalar", &managed)?;
    reject_nonportable_compose(path, b"services: {web: scalar}", &managed)?;
    reject_nonportable_compose(
        path,
        b"networks: {public: {}}\nservices: {web: {networks: [public]}}",
        &managed,
    )?;
    reject_nonportable_compose(path, b"name: compose", &managed)?;
    reject_nonportable_compose(path, b"services: {web: {networks: [public]}}", &managed)
}

#[test]
fn volume_path_resource_and_port_helpers_cover_portable_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    let root = crate::test_support::tempdir()?;
    let compose = root.path().join("compose");
    fs::create_dir(&compose)?;
    fs::create_dir(compose.join("data"))?;
    assert!(!uses_unsafe_ingress_volume(
        &yaml("not-a-list"),
        root.path(),
        &compose
    )?);
    assert!(!uses_unsafe_ingress_volume(
        &yaml("[42, 'data', './data:/data', {source: cache, target: /cache}]"),
        root.path(),
        &compose
    )?);
    assert!(uses_unsafe_ingress_volume(
        &yaml("[{type: bind, source: '../../outside', target: /data}]"),
        root.path(),
        &compose
    )?);
    assert!(!unsafe_volume_source(
        "cache",
        false,
        root.path(),
        &compose
    )?);
    assert!(unsafe_volume_source(
        "$CACHE",
        false,
        root.path(),
        &compose
    )?);
    assert!(unsafe_volume_source(
        "./data",
        true,
        root.path(),
        Path::new("relative-compose"),
    )?);
    assert!(unsafe_volume_source(
        "~/cache",
        true,
        root.path(),
        &compose
    )?);

    assert_eq!(normalize_absolute_path(Path::new("relative")), None);
    assert_eq!(
        normalize_absolute_path(Path::new("/one/./two/../three")),
        Some(PathBuf::from("/one/three"))
    );
    assert_eq!(normalize_absolute_path(Path::new("/../escape")), None);
    assert!(is_bind_source("./data"));
    assert!(!is_bind_source("named-volume"));

    assert!(uses_proxy_port(
        &yaml("['$PORT:9000', '127.0.0.1:79-81:9000/tcp']"),
        (80, 443, 8080)
    ));
    assert!(uses_proxy_port(
        &yaml("[{published: '443/tcp', target: 9000}]"),
        (80, 443, 8080)
    ));
    assert!(!uses_proxy_port(
        &yaml("[{published: 70000, target: 9000}, {published: broken}]"),
        (80, 443, 8080)
    ));
    assert!(port_expression_contains("79-81/tcp", 80));
    assert!(!port_expression_contains("broken-range", 80));
    assert!(port_expression_contains("[443]", 443));
    Ok(())
}

#[test]
fn resource_labels_links_and_yaml_tag_helpers_are_exact() -> Result<(), MmError> {
    assert_eq!(
        compose_resource_name(&yaml("{name: explicit}")),
        Some("explicit")
    );
    assert_eq!(
        compose_resource_name(&yaml("{external: {name: inherited}}")),
        Some("inherited")
    );
    assert_eq!(compose_resource_name(&yaml("external: true")), None);
    assert_eq!(compose_resource_name(&yaml("scalar")), None);
    assert!(is_reserved_label("Traefik.Enable"));
    assert!(is_reserved_label("io.mm.owner"));
    assert!(!is_reserved_label("example.owner"));
    assert!(is_reserved_ingress_name("MM-INGRESS-private"));
    assert!(uses_reserved_ingress_link(&yaml("[db:'$ALIAS']")));
    assert!(!uses_reserved_ingress_link(&yaml("[db:database]")));
    reject_compose_yaml_tags(Path::new("compose.yaml"), &yaml("{a: [one, two]}"))?;
    assert!(
        reject_compose_yaml_tags(Path::new("compose.yaml"), &yaml("{a: !custom value}")).is_err()
    );
    assert!(
        reject_compose_yaml_tags(Path::new("compose.yaml"), &yaml("{!custom key: value}")).is_err()
    );
    assert!(
        reject_compose_yaml_tags(Path::new("compose.yaml"), &yaml("[one, !custom two]")).is_err()
    );
    Ok(())
}
