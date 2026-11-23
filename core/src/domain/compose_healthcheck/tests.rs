use super::{HealthcheckArgumentNote, healthcheck_argument_notes};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn notes(files: &[(&str, &str)]) -> Result<Vec<HealthcheckArgumentNote>, serde_yaml_ng::Error> {
    let files = files
        .iter()
        .map(|(name, text)| Ok(((*name).to_owned(), serde_yaml_ng::from_str(text)?)))
        .collect::<Result<Vec<_>, serde_yaml_ng::Error>>()?;
    Ok(healthcheck_argument_notes("app", &files, |service| {
        service == "skipped"
    }))
}

fn services(found: &[HealthcheckArgumentNote]) -> Vec<&str> {
    found.iter().map(|note| note.service.as_str()).collect()
}

#[test]
fn only_exec_checks_with_whitespace_or_empty_arguments_are_noted() -> TestResult {
    let found = notes(&[(
        "compose.yaml",
        r#"services:
  spaced: {healthcheck: {test: ["CMD", "sh", "-c", "exit 0"]}}
  empty: {healthcheck: {test: ["CMD", "check", ""]}}
  plain: {healthcheck: {test: ["CMD", "curl", "-f", "http://127.0.0.1/health"]}}
  shell: {healthcheck: {test: ["CMD-SHELL", "curl -f http://127.0.0.1/health"]}}
  string: {healthcheck: {test: "curl -f http://127.0.0.1/health"}}
  none: {healthcheck: {test: ["NONE"]}}
  disabled: {healthcheck: {test: ["CMD", "sh", "-c", "exit 0"], disable: true}}
  skipped: {healthcheck: {test: ["CMD", "sh", "-c", "exit 0"]}}
  bare: {image: example/bare}
"#,
    )])?;
    assert_eq!(services(&found), ["spaced", "empty"]);
    assert_eq!(found[0].shell, "sh -c 'exit 0'");
    assert_eq!(found[1].shell, "check ''");
    assert_eq!(found[0].interpolated, None);
    assert_eq!(
        found[0].help(),
        r#"write `test: ["CMD-SHELL", "sh -c 'exit 0'"]` (it runs with /bin/sh in the container), or run Podman 5.8 or later"#
    );
    Ok(())
}

#[test]
fn later_files_override_the_check_and_name_the_file_that_sets_it() -> TestResult {
    let base = r#"services:
  shell: {healthcheck: {test: ["CMD", "sh", "-c", "exit 0"]}}
  disabled: {healthcheck: {test: ["CMD", "sh", "-c", "exit 0"]}}
  both: {healthcheck: {test: ["CMD", "sh", "-c", "exit 0"]}}
  timing: {healthcheck: {test: ["CMD", "sh", "-c", "exit 0"]}}
  added: {image: example/added}
"#;
    let override_file = r#"services:
  shell: {healthcheck: {test: ["CMD-SHELL", "exit 0"]}}
  disabled: {healthcheck: {disable: true}}
  both: {healthcheck: {test: ["CMD", "sh", "-c", "exit 1"]}}
  timing: {healthcheck: {interval: 5s}}
  added: {healthcheck: {test: ["CMD", "sh", "-c", "exit 0"]}}
"#;
    let found = notes(&[
        ("compose.yaml", base),
        ("compose.override.yaml", override_file),
    ])?;
    assert_eq!(services(&found), ["both", "timing", "added"]);
    assert_eq!(found[0].file, "compose.override.yaml");
    assert_eq!(found[0].shell, "sh -c 'exit 1'");
    assert_eq!(found[1].file, "compose.yaml");
    assert_eq!(found[2].file, "compose.override.yaml");
    Ok(())
}

#[test]
fn a_check_inherited_through_extends_is_noted_unless_overridden() -> TestResult {
    let found = notes(&[(
        "compose.yaml",
        r#"services:
  base: {healthcheck: {test: ["CMD", "sh", "-c", "exit 0"]}}
  child: {extends: base}
  mapped: {extends: {service: base}, healthcheck: {interval: 5s}}
  replaced: {extends: base, healthcheck: {test: ["CMD-SHELL", "exit 0"]}}
  looped: {extends: looped}
"#,
    )])?;
    assert_eq!(services(&found), ["base", "child", "mapped"]);
    Ok(())
}

#[test]
fn interpolation_is_noted_only_where_its_value_could_hold_whitespace() -> TestResult {
    let found = notes(&[(
        "compose.yaml",
        r#"services:
  whole: {healthcheck: {test: ["CMD", "sh", "-c", "${CHECK}"]}}
  bare: {healthcheck: {test: ["CMD", "sh", "-c", "$CHECK"]}}
  fallback: {healthcheck: {test: ["CMD", "sh", "-c", "${CHECK:-exit 0}"]}}
  embedded: {healthcheck: {test: ["CMD", "curl", "-f", "http://127.0.0.1:${PORT}/health"]}}
  defaulted: {healthcheck: {test: ["CMD", "curl", "http://127.0.0.1:${PORT:-8080}/"]}}
  escaped: {healthcheck: {test: ["CMD", "echo", "$${HOME}"]}}
  literal: {healthcheck: {test: ["CMD", "sh", "-c", "test ${A}"]}}
"#,
    )])?;
    assert_eq!(services(&found), ["whole", "bare", "fallback", "literal"]);
    assert_eq!(found[0].interpolated.as_deref(), Some("${CHECK}"));
    assert_eq!(found[1].interpolated.as_deref(), Some("$CHECK"));
    assert_eq!(found[2].interpolated.as_deref(), Some("${CHECK:-exit 0}"));
    assert_eq!(found[3].interpolated, None);
    assert!(
        found[0]
            .line()
            .contains("with an argument from `${CHECK}`, whose value mm does not know"),
        "{}",
        found[0].line()
    );
    Ok(())
}

#[test]
fn podman_keeps_arguments_from_5_8() {
    for version in ["5.8.0", "podman version 5.8.9", "6.0.0-rc1", "5.10.1"] {
        assert!(super::podman_keeps_arguments(version), "{version}");
    }
    for version in ["5.7.1", "podman version 5.4.2", "4.9.4", "unknown", ""] {
        assert!(!super::podman_keeps_arguments(version), "{version}");
    }
}
