use super::*;

#[test]
fn fingerprint_and_tag_lexical_boundaries_are_exact() -> Result<(), String> {
    let platforms = vec!["linux/amd64".to_owned()];
    let mut unit = valid_unit("app", "registry.example/app", &platforms)?;
    unit.dependencies.push(FirstPartyBuildEdge {
        producer_module: "base".to_owned(),
        dependent_module: "app".to_owned(),
        stage: None,
        reference: BuildDependencyReference::Dockerfile {
            original_reference: "base:edge".to_owned(),
            instruction_index: 1,
            source_line: 1,
        },
    });
    assert!(build_node_key(&unit, &platforms, &[]).is_err());

    unit.dependencies.clear();
    unit.dockerfile_hash.clear();
    assert_eq!(
        build_node_key(&unit, &platforms, &[])?.dockerfile_digest,
        unit.context_hash
    );
    assert!(is_oci_platform("linux/x86_64"));
    assert!(!is_oci_platform("linux/UPPER"));
    assert!(!is_registry("-registry.example"));
    assert!(!is_registry("registry.example:0"));
    assert!(!is_image_repository("registry.example/-component"));

    let mut invalid_commit = source(Some("main"), &[], &[]);
    invalid_commit.commit = "short".to_owned();
    assert!(automatic_tags(&invalid_commit, BuildTagStrategy::Auto).is_err());
    assert!(build_version(&invalid_commit).is_err());
    assert!(suffixed_tags(&["x".repeat(128)], "suffix").is_err());

    let mut conflicting = source(Some("main"), &["v1.0.0", "v2.0.0"], &[]);
    assert!(build_version(&conflicting).is_err());
    conflicting.tags_at_head = vec!["not-semver".to_owned()];
    conflicting.dirty = true;
    assert!(build_version(&conflicting)?.ends_with("-dirty"));
    assert!(parse_versions(&conflicting.tags_at_head).is_empty());
    assert_eq!(release_branch("01.2.x"), None);
    assert_eq!(release_branch("1.02.x"), None);
    assert_eq!(release_branch("1.x.x"), None);
    assert_eq!(release_branch("x.1.x"), None);
    assert!(oci_semver_tag(&"x".repeat(129)).is_err());
    Ok(())
}
