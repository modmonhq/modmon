use super::*;
use crate::adapters::config::ConfigLoader;
use crate::domain::stack::ComposeSourceManifest;

fn parsed<T>(result: Result<T, String>) -> Result<T, MmError> {
    result.map_err(|message| MmError::State { message })
}

fn yaml(source: &str) -> Result<serde_yaml_ng::Value, MmError> {
    serde_yaml_ng::from_str(source).map_err(|error| MmError::State {
        message: format!("test YAML is invalid: {error}"),
    })
}

fn source(path: &str, bytes: &str) -> CompiledComposeSource {
    CompiledComposeSource {
        manifest: ComposeSourceManifest {
            path: PathBuf::from(path),
            digest: blake3::hash(bytes.as_bytes()).to_hex().to_string(),
        },
        bytes: bytes.as_bytes().to_vec(),
    }
}

fn document() -> Result<ConfigDocument, MmError> {
    ConfigLoader.load_from(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal"))
}

#[test]
fn scalar_port_parsers_accept_only_static_bounded_compose_forms() -> Result<(), MmError> {
    assert_eq!(parsed(parse_port_number("1"))?, 1);
    assert_eq!(parsed(parse_port_number("65535"))?, 65_535);
    assert_eq!(parsed(parse_port_range("80-82"))?, vec![80, 81, 82]);
    for invalid in ["", "0", "x", "65536", "82-80", "1-2-3"] {
        assert!(parse_port_range(invalid).is_err(), "accepted {invalid}");
    }
    assert_eq!(parsed(valid_target_port(443))?, 443);
    assert!(valid_target_port(0).is_err());
    assert!(valid_target_port(65_536).is_err());

    assert_eq!(parsed(parse_protocol("tcp"))?, TransportProtocol::Tcp);
    assert_eq!(parsed(parse_protocol("udp"))?, TransportProtocol::Udp);
    assert_eq!(parsed(parse_protocol("sctp"))?, TransportProtocol::Sctp);
    assert_eq!(protocol_label(TransportProtocol::Tcp), "tcp");
    assert_eq!(protocol_label(TransportProtocol::Udp), "udp");
    assert_eq!(protocol_label(TransportProtocol::Sctp), "sctp");
    assert!(parse_protocol("http").is_err());

    assert_eq!(
        parsed(split_port_protocol("8080/udp"))?,
        ("8080", Some(TransportProtocol::Udp))
    );
    assert_eq!(parsed(split_port_protocol("8080"))?, ("8080", None));
    for invalid in ["", " 80", "8 0", "$PORT", "80/tcp/extra", "80/http"] {
        assert!(split_port_protocol(invalid).is_err(), "accepted {invalid}");
    }
    Ok(())
}

#[test]
fn short_and_long_published_ports_validate_prefixes_targets_and_host_ips() -> Result<(), MmError> {
    for (value, expected) in [
        ("80", vec![80]),
        ("8080:80", vec![80]),
        ("127.0.0.1:8080:80-81", vec![80, 81]),
        ("[::1]:8080:80", vec![80]),
    ] {
        assert_eq!(parsed(parse_short_port_without_protocol(value))?, expected);
    }
    for invalid in [
        ":80",
        ":8080:80",
        "localhost:8080:80",
        "::1:8080:80",
        "8080:0",
        "0:80",
        "127.0.0.1:0:80",
    ] {
        assert!(
            parse_short_port_without_protocol(invalid).is_err(),
            "accepted {invalid}"
        );
    }
    parsed(validate_short_host_ip("127.0.0.1"))?;
    parsed(validate_short_host_ip("[::1]"))?;
    assert!(validate_short_host_ip("::1").is_err());
    assert!(validate_short_host_ip("localhost").is_err());

    assert_eq!(parsed(parse_long_target_port(&yaml("8080")?))?, 8080);
    assert_eq!(parsed(parse_long_target_port(&yaml("'8081'")?))?, 8081);
    for value in ["'80-81'", "'80/tcp'", "{}"] {
        assert!(parse_long_target_port(&yaml(value)?).is_err());
    }
    Ok(())
}

#[test]
fn expose_and_ports_collection_preserve_protocol_and_evidence() -> Result<(), MmError> {
    let path = Path::new("compose.yaml");
    let mut targets = BTreeMap::new();
    parsed(collect_exposed_ports(
        &yaml("[80, '81-82/udp']")?,
        path,
        "web",
        &mut targets,
    ))?;
    assert!(targets[&(80, TransportProtocol::Tcp)].contains(&HttpEvidenceSource::ComposeExpose));
    assert!(targets.contains_key(&(81, TransportProtocol::Udp)));
    assert!(targets.contains_key(&(82, TransportProtocol::Udp)));

    parsed(collect_published_target_ports(
        &yaml("[443, '127.0.0.1:8443:8443/tcp', {target: '9000', protocol: sctp}]")?,
        path,
        "web",
        &mut targets,
    ))?;
    assert!(
        targets[&(443, TransportProtocol::Tcp)].contains(&HttpEvidenceSource::ComposePublishedPort)
    );
    assert!(targets.contains_key(&(8443, TransportProtocol::Tcp)));
    assert!(targets.contains_key(&(9000, TransportProtocol::Sctp)));

    for invalid in [
        "{}",
        "[{}]",
        "[0]",
        "['$PORT']",
        "[':80']",
        "[{target: '80-81'}]",
        "[{target: 80, protocol: http}]",
        "[true]",
    ] {
        assert!(
            collect_published_target_ports(&yaml(invalid)?, path, "web", &mut BTreeMap::new())
                .is_err()
        );
    }
    for invalid in ["{}", "[{}]", "[0]", "['$PORT']", "['0']"] {
        assert!(collect_exposed_ports(&yaml(invalid)?, path, "web", &mut BTreeMap::new()).is_err());
    }
    Ok(())
}

#[test]
fn inference_merges_ordered_sources_and_rejects_ambiguous_compose_shapes() -> Result<(), MmError> {
    let document = document()?;
    assert!(infer_compose_target_ports(&document, "app", &BTreeSet::new(), &[])?.is_empty());
    let services = ["api".to_owned(), "worker".to_owned()]
        .into_iter()
        .collect();
    let sources = [
        source("base.yaml", "services:\n  api:\n    expose: ['8080/tcp']\n"),
        source(
            "compose.yaml",
            "services:\n  api:\n    ports:\n      - target: 9090\n        protocol: udp\n",
        ),
    ];
    let inferred = infer_compose_target_ports(&document, "app", &services, &sources)?;
    assert!(inferred["api"].declared_in_compose);
    assert_eq!(inferred["api"].candidates.len(), 2);
    assert!(!inferred["worker"].declared_in_compose);
    assert!(inferred["worker"].candidates.is_empty());

    let without_services = infer_compose_target_ports(
        &document,
        "app",
        &["api".to_owned()].into_iter().collect(),
        &[source("empty.yaml", "name: compose\n")],
    )?;
    assert!(!without_services["api"].declared_in_compose);

    for invalid in [
        "[]",
        "services: []\n",
        "services:\n  api: image\n",
        "services:\n  api:\n    extends: base\n",
        "services:\n  api:\n    expose: 80\n",
        "services:\n  api:\n    ports: 80\n",
        "services: [",
        "services:\n  api:\n    <<: invalid\n",
    ] {
        assert!(
            infer_compose_target_ports(
                &document,
                "app",
                &["api".to_owned()].into_iter().collect(),
                &[source("invalid.yaml", invalid)],
            )
            .is_err(),
            "accepted {invalid}"
        );
    }
    assert!(
        port_inference_error("app", "api", "ambiguous")
            .to_string()
            .contains("ambiguous")
    );
    Ok(())
}
