use super::*;

#[test]
fn extraction_keeps_comments_and_leaves_the_next_key_alone() {
    let source = "schema-version: 1\n# Stacks\nstacks: # compose stacks\n  app:\n    files: [a.yaml]\n\n  # second stack\n  web:\n    files: [b.yaml]\n\n# Modules\nmodules: {}\n";
    let (remaining, value) = extract_top_level(source, "stacks").unwrap_or_default();
    assert_eq!(
        value,
        "# Stacks\n# compose stacks\napp:\n  files: [a.yaml]\n\n# second stack\nweb:\n  files: [b.yaml]\n"
    );
    assert_eq!(remaining, "schema-version: 1\n\n# Modules\nmodules: {}\n");

    let (remaining, value) = extract_top_level(source, "modules").unwrap_or_default();
    assert_eq!(value, "# Modules\n{}\n");
    assert!(remaining.ends_with("files: [b.yaml]\n\n"), "{remaining}");
    assert!(extract_top_level(source, "jobs").is_none());
}

#[test]
fn joining_indents_blocks_and_inlines_single_values() {
    assert_eq!(
        append_top_level(
            "schema-version: 1",
            "stacks",
            "# main\napp:\n  files: [a]\n\nweb: {}\n"
        ),
        "schema-version: 1\nstacks:\n  # main\n  app:\n    files: [a]\n\n  web: {}\n"
    );
    assert_eq!(
        append_top_level("a: 1\n", "target", "# where\nlocal\n"),
        "a: 1\ntarget: local\n"
    );
    assert_eq!(append_top_level("", "networks", "{}"), "networks: {}\n");
}

#[test]
fn dedent_uses_the_shallowest_content_line() {
    assert_eq!(
        dedent("    a: 1\n  # note\n      b: 2\n\n"),
        "a: 1\n# note\n  b: 2\n\n"
    );
    assert_eq!(dedent(""), "");
}
