use std::fs;
use std::path::PathBuf;

use super::*;
use crate::app::stacks::tests::support::{StackFixture, candidate, route, service, source};
use crate::domain::development_mount::{
    DevelopmentMountConfig, DevelopmentMountMode, PreservedContainerPath,
};

#[test]
fn generated_overlay_contains_owned_services_routes_networks_and_mail()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture =
        StackFixture::new("services: {web: {expose: [8080]}, sidecar: {image: sidecar}}")?;
    let mut web = service("web", true, vec![candidate(8080, TransportProtocol::Tcp)]);
    web.mail_bindings = development_mail_bindings(
        &fixture.document,
        "development",
        &fixture.target,
        "app",
        &fixture.document.config.modules["app"].services["web"],
    );
    let route = route(
        &fixture.document,
        "web",
        "web.stack-fixture.test",
        "/",
        true,
    );
    let membership = resolve_network_membership(&fixture.document)?;
    let mut membership = membership;
    membership.insert(
        ("other".to_owned(), "ignored".to_owned()),
        BTreeSet::from(["backend".to_owned()]),
    );
    let generated = generated_overlay(GeneratedOverlayInput {
        document: &fixture.document,
        environment: "development",
        stack: "app",
        managed_services: &[web],
        routes: &[route.clone(), route],
        target: &fixture.target,
        membership: &membership,
        source_files: &[source(
            "compose.yaml",
            "services: {web: {expose: [8080]}, sidecar: {image: sidecar}}",
        )],
        restart_defaults: &BTreeSet::new(),
        shared: &Default::default(),
        security: &Default::default(),
        config_files: &Default::default(),
        user_ids: &Default::default(),
        low_ports: &Default::default(),
        host_userns: &Default::default(),
    })?;
    assert!(generated.preserved_volumes.is_empty());
    let overlay: serde_yaml_ng::Value = serde_yaml_ng::from_slice(&generated.bytes)?;
    assert_eq!(
        overlay["services"]["web"]["networks"]["mm-ingress"]["aliases"]
            .as_sequence()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        overlay["services"]["web"]["environment"]["MM_MAIL_SMTP_PORT"],
        "1025"
    );
    assert!(overlay["services"]["sidecar"]["networks"]["backend"].is_mapping());
    assert!(overlay["networks"]["backend"]["external"].as_bool() == Some(true));
    assert!(overlay["networks"]["mm-services-development"].is_mapping());
    assert_eq!(overlay["x-mm"]["stack"], "app");
    Ok(())
}

#[test]
fn development_mount_overlay_is_owned_sorted_and_rejects_invalid_inputs()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    fs::create_dir(fixture.root.path().join("app/storage"))?;
    let membership = resolve_network_membership(&fixture.document)?;
    let mut web = service("web", true, vec![candidate(8080, TransportProtocol::Tcp)]);
    web.development_mount = Some(DevelopmentMountConfig {
        additional: Vec::new(),
        source: PathBuf::from("."),
        target: "/app".to_owned(),
        mode: DevelopmentMountMode::ReadOnly,
        preserve: vec![PreservedContainerPath {
            path: "/app/vendor".to_owned(),
        }],
        excludes: vec![PathBuf::from("storage")],
        protected: Vec::new(),
        safe_exceptions: Vec::new(),
        services: vec!["web".to_owned()],
        preset: None,
    });
    macro_rules! overlay {
        ($managed_services:expr, $source_files:expr) => {
            generated_overlay(GeneratedOverlayInput {
                document: &fixture.document,
                environment: "development",
                stack: "app",
                managed_services: $managed_services,
                routes: &[],
                target: &fixture.target,
                membership: &membership,
                source_files: $source_files,
                restart_defaults: &BTreeSet::new(),
                shared: &Default::default(),
                security: &Default::default(),
                config_files: &Default::default(),
                user_ids: &Default::default(),
                low_ports: &Default::default(),
                host_userns: &Default::default(),
            })
        };
    }
    let compose = [source("compose.yaml", "services: {web: {expose: [8080]}}")];
    let generated = overlay!(std::slice::from_ref(&web), &compose)?;
    let manifests = generated.preserved_volumes;
    assert_eq!(manifests.len(), 2);
    assert_eq!(manifests[0].module, "app");
    let overlay: serde_yaml_ng::Value = serde_yaml_ng::from_slice(&generated.bytes)?;
    let mounts = overlay["services"]["web"]["volumes"]
        .as_sequence()
        .ok_or("missing generated mounts")?;
    assert_eq!(mounts.len(), 3);
    assert!(
        mounts[0]
            .as_str()
            .is_some_and(|mount| mount.ends_with(":ro,z"))
    );

    let collision_source = format!(
        "services: {{web: {{}}}}\nvolumes: {{{}: {{}}}}\n",
        manifests[0].logical_name
    );
    assert!(
        overlay!(
            std::slice::from_ref(&web),
            &[source("compose.yaml", &collision_source)]
        )
        .is_err()
    );

    let mut missing_module = web.clone();
    missing_module.module = "missing".to_owned();
    assert!(overlay!(std::slice::from_ref(&missing_module), &compose).is_err());

    let mut missing_source = web.clone();
    missing_source
        .development_mount
        .as_mut()
        .ok_or("missing mount")?
        .source = PathBuf::from("missing");
    assert!(overlay!(std::slice::from_ref(&missing_source), &compose).is_err());

    let mut invalid_path = web.clone();
    invalid_path
        .development_mount
        .as_mut()
        .ok_or("missing mount")?
        .target = "/app\nbad".to_owned();
    assert!(overlay!(&[invalid_path], &compose).is_err());
    Ok(())
}

#[test]
fn network_membership_and_canonical_intent_are_stable_and_source_aware()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture =
        StackFixture::new("services: {web: {expose: [8080]}, sidecar: {image: sidecar}}")?;
    let membership = resolve_network_membership(&fixture.document)?;
    assert_eq!(
        membership[&("app".to_owned(), "web".to_owned())],
        BTreeSet::from(["backend".to_owned(), "service-net".to_owned()])
    );
    assert_eq!(
        membership[&("app".to_owned(), "sidecar".to_owned())],
        BTreeSet::from(["backend".to_owned()])
    );
    let networks = canonical_networks(
        &fixture.document,
        &fixture.target,
        "development",
        &membership,
    )?;
    assert_eq!(networks.len(), 2);
    assert!(
        networks
            .iter()
            .any(|network| { network.logical_name == "backend" && network.members.len() == 2 })
    );

    let images = BTreeMap::from([("app".to_owned(), "example/web@sha256:one".to_owned())]);
    let first = canonical_workload_intent_hash(
        fixture.document.config.project.id,
        "development",
        &images,
        &networks,
    );
    let second = canonical_workload_intent_hash(
        fixture.document.config.project.id,
        "development",
        &images,
        &networks,
    );
    let changed = canonical_workload_intent_hash(
        fixture.document.config.project.id,
        "production",
        &images,
        &networks,
    );
    assert_eq!(first, second);
    assert_ne!(first, changed);

    let name = managed_network_name(
        fixture.target.definition.id,
        fixture.document.config.project.id,
        "Feature / ONE",
        "---",
    );
    assert!(name.ends_with("feature---one-net"), "{name}");
    Ok(())
}

#[test]
fn network_source_and_overlay_parsing_failures_are_propagated()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    fs::write(fixture.root.path().join("compose.yaml"), "services: [")?;
    assert!(resolve_network_membership(&fixture.document).is_err());

    fs::remove_file(fixture.root.path().join("compose.yaml"))?;
    let missing = resolve_network_membership(&fixture.document)?;
    assert_eq!(
        missing[&("app".to_owned(), "web".to_owned())],
        BTreeSet::from(["backend".to_owned(), "service-net".to_owned()])
    );

    fs::write(
        fixture.root.path().join("compose.yaml"),
        "services: {42: {}, web: {}}",
    )?;
    let numeric_service = resolve_network_membership(&fixture.document)?;
    assert!(!numeric_service.keys().any(|(_, service)| service == "42"));

    fs::write(fixture.root.path().join("compose.yaml"), "name: compose")?;
    let no_services = resolve_network_membership(&fixture.document)?;
    assert!(no_services.contains_key(&("app".to_owned(), "web".to_owned())));

    assert!(
        generated_overlay(GeneratedOverlayInput {
            document: &fixture.document,
            environment: "development",
            stack: "app",
            managed_services: &[],
            routes: &[],
            target: &fixture.target,
            membership: &BTreeMap::new(),
            source_files: &[source("invalid.yaml", "[")],
            restart_defaults: &BTreeSet::new(),
            shared: &Default::default(),
            security: &Default::default(),
            config_files: &Default::default(),
            user_ids: &Default::default(),
            low_ports: &Default::default(),
            host_userns: &Default::default(),
        })
        .is_err()
    );
    Ok(())
}

#[test]
fn a_service_on_a_network_with_a_resolver_file_mounts_it() -> Result<(), Box<dyn std::error::Error>>
{
    // web is on backend and service-net, sidecar on backend.
    let compose = "services: {web: {expose: [8080]}, sidecar: {image: sidecar}}";
    let fixture = StackFixture::new(compose)?;
    let membership = resolve_network_membership(&fixture.document)?;
    let volumes = |document: &ConfigDocument,
                   service: &str|
     -> Result<serde_yaml_ng::Value, Box<dyn std::error::Error>> {
        let generated = generated_overlay(GeneratedOverlayInput {
            document,
            environment: "development",
            stack: "app",
            managed_services: &[],
            routes: &[],
            target: &fixture.target,
            membership: &membership,
            source_files: &[source("compose.yaml", compose)],
            restart_defaults: &BTreeSet::new(),
            shared: &Default::default(),
            security: &Default::default(),
            config_files: &Default::default(),
            user_ids: &Default::default(),
            low_ports: &Default::default(),
            host_userns: &Default::default(),
        })?;
        let overlay: serde_yaml_ng::Value = serde_yaml_ng::from_slice(&generated.bytes)?;
        Ok(overlay["services"][service]["volumes"].clone())
    };
    let mount = |file: &str| {
        serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&format!(
            "['/var/lib/mm/networks/resolv/{file}:/etc/resolv.conf:ro,z']"
        ))
    };
    // Off a cluster node nothing names a resolver file.
    assert!(volumes(&fixture.document, "web")?.is_null());

    // `mmd` on a cluster node names one per network that spans the nodes.
    let mut document = fixture.document.clone();
    let mut name = |network: &str, file: &str| {
        document
            .config
            .networks
            .entry(network.to_owned())
            .or_default()
            .resolver = Some(PathBuf::from("/var/lib/mm/networks/resolv").join(file));
    };
    name("service-net", "service-net.conf");
    assert_eq!(volumes(&document, "web")?, mount("service-net.conf")?);
    assert!(
        volumes(&document, "sidecar")?.is_null(),
        "not on that network"
    );
    let mut name = |network: &str, file: &str| {
        document
            .config
            .networks
            .entry(network.to_owned())
            .or_default()
            .resolver = Some(PathBuf::from("/var/lib/mm/networks/resolv").join(file));
    };
    name("backend", "backend.conf");
    // One file is enough: the node answers a container for all its networks.
    assert_eq!(volumes(&document, "web")?, mount("backend.conf")?);
    assert_eq!(volumes(&document, "sidecar")?, mount("backend.conf")?);
    Ok(())
}
