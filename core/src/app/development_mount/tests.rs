use super::*;

mod policy_contracts;

#[test]
fn platform_paths_are_portable_and_traversal_is_rejected() {
    assert_eq!(
        HostPlatform::Windows.normalize_relative(r"src\web").ok(),
        Some(PathBuf::from("src/web"))
    );
    assert!(HostPlatform::Unix.normalize_relative("../secret").is_err());
    assert!(
        HostPlatform::Windows
            .normalize_relative(r"C:\code")
            .is_err()
    );
    assert!(
        HostPlatform::Windows
            .normalize_relative(r"\\server\code")
            .is_err()
    );
}

#[test]
fn sensitive_patterns_cover_private_project_material() {
    for path in [
        ".git/config",
        ".config/mm.yaml",
        ".env",
        ".env.local",
        ".ssh/id_ed25519",
        "certs/server.pem",
    ] {
        assert!(sensitive_path(path, &[]), "{path}");
    }
    assert!(!sensitive_path(".env.example", &[]));
    assert!(sensitive_path(
        "private/token.txt",
        &["private/*".to_owned()]
    ));
}

#[test]
fn framework_presets_keep_platform_specific_outputs_visible() {
    let laravel = preset_preserves(Some(DevelopmentMountPreset::Laravel));
    let node = preset_preserves(Some(DevelopmentMountPreset::Node));
    let dotnet = preset_preserves(Some(DevelopmentMountPreset::Dotnet));
    let rust = preset_preserves(Some(DevelopmentMountPreset::Rust));
    assert!(laravel.iter().any(|path| path.path == "/app/vendor"));
    assert!(node.iter().any(|path| path.path == "/app/node_modules"));
    assert!(dotnet.iter().any(|path| path.path == "/app/obj"));
    assert!(rust.iter().any(|path| path.path == "/app/target"));
}

#[test]
fn framework_preset_fixtures_are_detected() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::tempdir()?;
    let cases = [
        ("laravel", DevelopmentMountPreset::Laravel),
        ("node", DevelopmentMountPreset::Node),
        ("dotnet", DevelopmentMountPreset::Dotnet),
        ("rust", DevelopmentMountPreset::Rust),
    ];
    for (directory, _) in cases {
        fs::create_dir(fixture.path().join(directory))?;
    }
    fs::write(fixture.path().join("laravel/artisan"), "")?;
    fs::write(fixture.path().join("laravel/composer.json"), "{}")?;
    fs::write(fixture.path().join("node/package.json"), "{}")?;
    fs::write(fixture.path().join("dotnet/App.csproj"), "<Project />")?;
    fs::write(fixture.path().join("rust/Cargo.toml"), "[package]")?;

    for (directory, expected) in cases {
        assert_eq!(
            discover_preset(&fixture.path().join(directory))?,
            Some(expected)
        );
    }
    Ok(())
}

#[test]
fn policy_helpers_reject_nonportable_paths_and_unknown_services()
-> Result<(), Box<dyn std::error::Error>> {
    let module = serde_yaml_ng::from_str::<ModuleConfig>(
        "path: app\nservices:\n  web: {stack: app}\n  worker: {stack: app}\n",
    )?;
    assert!(validate_services("app", &module, &[]).is_err());
    assert!(validate_services("app", &module, &["missing".to_owned()]).is_err());
    assert!(validate_services("app", &module, &["web".to_owned(), "web".to_owned()]).is_err());
    validate_services("app", &module, &["web".to_owned(), "worker".to_owned()])?;

    for path in ["", "/absolute", "../parent", "nested/../parent"] {
        assert!(validate_subpath("app", Path::new(path)).is_err(), "{path}");
    }
    validate_subpath("app", Path::new("nested/./child"))?;
    assert_eq!(
        join_container_path("/app/", Path::new("src/./web"))?,
        "/app/src/web"
    );
    assert_eq!(
        join_container_path("relative", Path::new("src"))?,
        "/relative/src"
    );
    assert_eq!(
        normalize_relative_for_match(Path::new("src/../web")),
        "src/web"
    );
    Ok(())
}

#[test]
fn sensitive_and_wildcard_matching_cover_anchors_gaps_and_suffixes() {
    for (value, pattern, expected) in [
        ("exact", "exact", true),
        ("other", "exact", false),
        ("prefix-value", "prefix-*", true),
        ("other-prefix-value", "prefix-*", false),
        ("value-suffix", "*-suffix", true),
        ("value-suffix-more", "*-suffix", false),
        ("one-middle-two", "one*two", true),
        ("one-middle", "one*two", false),
        ("anything", "*", true),
    ] {
        assert_eq!(
            wildcard_matches(value, pattern),
            expected,
            "{value} {pattern}"
        );
    }
    for sensitive in [
        ".ssh/config",
        "keys/id_rsa",
        "keys/id_ed25519",
        "cert/server.key",
        "nested/secrets/token",
        ".config/mm/private",
    ] {
        assert!(sensitive_path(sensitive, &[]), "{sensitive}");
    }
}

#[test]
fn infrastructure_sorting_and_bounded_reads_are_explicit() -> Result<(), Box<dyn std::error::Error>>
{
    assert!(infrastructure_role(ServiceRole::Database));
    assert!(infrastructure_role(ServiceRole::Queue));
    assert!(!infrastructure_role(ServiceRole::Http));
    for name in ["postgres", "redis-cache", "RabbitMQ"] {
        assert!(infrastructure_name(name), "{name}");
    }
    assert!(!infrastructure_name("application"));
    assert_eq!(
        sorted_unique(&["b".to_owned(), "a".to_owned(), "b".to_owned()]),
        ["a", "b"]
    );

    let temporary = crate::test_support::tempdir()?;
    let file = temporary.path().join("input");
    fs::write(&file, b"bounded")?;
    assert_eq!(read_bounded(&file)?, b"bounded");
    assert!(read_bounded(temporary.path()).is_err());
    fs::write(&file, vec![0_u8; (MAX_DISCOVERY_FILE_BYTES + 1) as usize])?;
    assert!(read_bounded(&file).is_err());

    let value = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("{name: value}")?;
    let mapping = value.as_mapping().ok_or("mapping missing")?;
    assert_eq!(
        mapping_value(mapping, "name").and_then(|v| v.as_str()),
        Some("value")
    );
    assert!(mapping_value(mapping, "missing").is_none());
    Ok(())
}
