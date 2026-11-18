use super::{route_paths_overlap, route_rule};

#[test]
fn route_overlap_respects_path_segment_boundaries() {
    assert!(route_paths_overlap("/", "/api"));
    assert!(route_paths_overlap("/api", "/api"));
    assert!(route_paths_overlap("/api", "/api/v1"));
    assert!(!route_paths_overlap("/api", "/apix"));
}

#[test]
fn route_rule_does_not_use_a_broad_path_prefix() {
    assert_eq!(route_rule("example.test", "/"), "Host(`example.test`)");
    assert_eq!(
        route_rule("example.test", "/api"),
        "Host(`example.test`) && (Path(`/api`) || PathPrefix(`/api/`))"
    );
}
