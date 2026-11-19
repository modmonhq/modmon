use super::*;

#[test]
fn every_splittable_key_has_a_type_check() {
    assert_eq!(CHECKED_CONFIG_KEYS, SPLITTABLE_CONFIG_KEYS);
    for key in SPLITTABLE_CONFIG_KEYS {
        let result = check_split_source(key, "[not, any, config, value]");
        assert!(result.is_err_and(|message| !message.contains("cannot be split")), "{key}");
    }
    assert!(check_split_source("schema-version", "1").is_err());
}

#[test]
fn split_type_errors_keep_their_line() {
    let error = check_split_source("stacks", "app:\n  unknown: true\n  files: {}\n")
        .err()
        .unwrap_or_default();
    assert!(error.contains("line 3"), "{error}");
    assert!(check_split_source("target", "local\n").is_ok());
    assert!(check_split_source("default-environment", "null\n").is_ok());
}

#[test]
fn anchors_are_type_checked_per_key() {
    assert!(serde_yaml_ng::from_str::<AnchorCheck>("schema-version: 1\ntarget: local\n").is_ok());
    let wrong = serde_yaml_ng::from_str::<AnchorCheck>("schema-version: 2\n")
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(wrong.contains("unsupported schema-version 2"), "{wrong}");
    assert!(serde_yaml_ng::from_str::<AnchorCheck>("routes: {}\n").is_ok());
    assert!(serde_yaml_ng::from_str::<AnchorCheck>("- not a mapping\n").is_err());
}

#[test]
fn split_paths_map_back_to_their_anchor() {
    let anchor = Path::new("/project/.config/mm.yaml");
    let split = split_config_path(anchor, "users");
    assert_eq!(split, Path::new("/project/.config/mm/users.yaml"));
    assert_eq!(anchor_for_source(&split), anchor);
    assert_eq!(anchor_for_source(anchor), anchor);
    assert!(has_yaml_content("--- \nkey: value\n"));
    assert!(!has_yaml_content("# only a comment\n\n---\n"));
}
