use super::*;

fn source() -> BuildSource {
    BuildSource {
        commit: "a".repeat(40),
        branch: None,
        tags_at_head: Vec::new(),
        repository_tags: Vec::new(),
        tags_complete: true,
        default_branch: false,
        dirty: false,
    }
}

#[test]
fn tag_helpers_cover_release_lexing_semver_conversion_and_deduplication() {
    assert_eq!(release_branch("1.2.x"), Some((1, 2)));
    for invalid in [
        "1.2", "1.x", "1.2.3.x", "01.2.x", "1.02.x", "a.2.x", "1.b.x",
    ] {
        assert_eq!(release_branch(invalid), None, "{invalid}");
    }

    assert_eq!(
        oci_semver_tag("v1.2.3+build"),
        Ok("v1.2.3-build".to_owned())
    );
    assert!(oci_semver_tag(&format!("1.2.3+{}", "a".repeat(128))).is_err());
    assert_eq!(
        parse_versions(&["invalid".to_owned(), "v1.2.3".to_owned()]),
        vec![("v1.2.3".to_owned(), Version::new(1, 2, 3))]
    );

    let mut tags = vec!["existing".to_owned()];
    push_unique(&mut tags, "existing".to_owned());
    push_unique(&mut tags, "new".to_owned());
    assert_eq!(tags, ["existing", "new"]);
}

#[test]
fn public_tag_derivation_rejects_invalid_identity_and_oci_suffixes() {
    let mut value = source();
    value.commit = "invalid".to_owned();
    assert!(automatic_tags(&value, BuildTagStrategy::Auto).is_err());
    assert!(build_version(&value).is_err());

    let mut value = source();
    value.tags_at_head = vec!["1.2.3".to_owned()];
    assert_eq!(build_version(&value), Ok("1.2.3".to_owned()));
    assert_eq!(
        automatic_tags(&value, BuildTagStrategy::Immutable),
        Ok(vec!["1.2.3".to_owned(), format!("sha-{}", value.commit)])
    );

    assert_eq!(
        suffixed_tags(&["edge".to_owned()], "development"),
        Ok(vec!["edge-development".to_owned()])
    );
    assert_eq!(
        suffixed_tags(&["edge".to_owned()], ""),
        Ok(vec!["edge".to_owned()])
    );
    assert!(suffixed_tags(&["a".repeat(128)], "suffix").is_err());
}
