use super::*;

#[test]
fn registry_hosts_are_host_names_with_optional_ports() {
    for host in ["ghcr.io", "registry.example.com:5000", "localhost:5000"] {
        assert!(validate_registry_host(host).is_ok(), "{host}");
    }
    for host in [
        "",
        "GHCR.io",
        "ghcr.io/org",
        "https://ghcr.io",
        "host:0",
        "host:x",
        "-a.io",
    ] {
        assert!(validate_registry_host(host).is_err(), "{host}");
    }
}

#[test]
fn well_known_registries_get_a_token_link() {
    let ghcr = registry_token_hint("ghcr.io", "shop", "uat").unwrap_or_default();
    assert!(ghcr.contains("scopes=read:packages"), "{ghcr}");
    assert!(
        ghcr.contains("description=mm%20shop%20uat%20pull"),
        "{ghcr}"
    );
    assert!(ghcr.contains("No expiration"), "{ghcr}");
    assert!(registry_token_hint("registry.gitlab.com", "a", "b").is_some());
    assert!(registry_token_hint("registry.example.com", "a", "b").is_none());
}
