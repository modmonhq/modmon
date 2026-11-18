use super::*;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed, TestCaseError};

#[test]
fn parses_supported_from_forms_and_stage_references() -> Result<(), MmError> {
    let source = "# syntax=docker/dockerfile:1\nFrOm --platform=$BUILDPLATFORM \\\n+  registry.example:5000/team/base:old AS Build\nFROM build as final\nFROM registry.example/team/tool@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n";
    let source = source.replace("+  registry", "  registry");
    let parsed = parse(&source, "Dockerfile")?;
    assert_eq!(parsed.bases.len(), 3);
    assert!(matches!(
        parsed.bases[0].kind,
        DockerfileBaseKind::Literal { .. }
    ));
    assert_eq!(parsed.bases[0].stage.as_deref(), Some("Build"));
    assert_eq!(parsed.bases[0].source_line, 2);
    assert_eq!(parsed.bases[1].kind, DockerfileBaseKind::Stage);
    assert!(matches!(
        parsed.bases[2].kind,
        DockerfileBaseKind::Literal { .. }
    ));
    Ok(())
}

#[test]
fn comments_are_lines_not_inline_suffixes() {
    assert!(parse("# FROM ignored\nFROM scratch\n", "Dockerfile").is_ok());
    let error = parse("FROM alpine # not-an-inline-comment\n", "Dockerfile");
    assert!(error.is_err());
}

#[test]
fn variables_and_duplicate_stages_fail_actionably() {
    let variable = parse("FROM ${BASE:-alpine}\n", "Dockerfile")
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(variable.contains("use exactly $ARG or ${ARG}"));
    let duplicate = parse(
        "FROM scratch AS build\nFROM scratch AS BUILD\n",
        "Dockerfile",
    )
    .err()
    .map(|error| error.to_string())
    .unwrap_or_default();
    assert!(duplicate.contains("duplicated"));
    let digest = parse("FROM example/base@sha256:nope\n", "Dockerfile")
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(digest.contains("invalid OCI digest"));
    let escape = parse("# EsCaPe=`\nFROM scratch\n", "Dockerfile")
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(escape.contains("default backslash"));
    for source in [
        "FROM --platform= scratch\n",
        "FROM --unknown=value scratch\n",
        "FROM --platform=linux --platform=linux scratch\n",
    ] {
        let error = parse(source, "Dockerfile")
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(error.contains("only --platform=<value> is supported"));
    }
}

#[test]
fn malformed_instructions_fail_at_their_exact_boundary() {
    for (source, expected) in [
        ("RUN <<EOF\n", "heredoc syntax"),
        ("FROM scratch \\\n\n", "blank lines inside continued"),
        ("FROM scratch \\", "ends during a continued instruction"),
        ("ARG\n", "exactly one variable declaration"),
        ("ARG FIRST SECOND\n", "exactly one variable declaration"),
        ("ARG lower\n", "not a supported variable name"),
        ("FROM\n", "has no image reference"),
        ("FROM scratch AS invalid/name\n", "stage alias"),
        ("FROM scratch trailing\n", "must use 'FROM"),
        ("FROM exämple/base\n", "must be ASCII"),
        ("FROM -invalid\n", "not OCI-compatible"),
    ] {
        let error = parse(source, "Dockerfile")
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn rewrite_changes_only_the_reviewed_from_token() -> Result<(), MmError> {
    let source = "# ghcr.io/example/base\nFROM ghcr.io/example/base AS app\n";
    let edge = crate::domain::build::FirstPartyBuildEdge {
        producer_module: "base".to_owned(),
        dependent_module: "app".to_owned(),
        stage: Some("app".to_owned()),
        reference: BuildDependencyReference::Dockerfile {
            original_reference: "ghcr.io/example/base".to_owned(),
            instruction_index: 0,
            source_line: 2,
        },
    };
    let rewritten = rewrite_dependencies(
        source,
        "Dockerfile",
        &[ResolvedBuildDependency {
            edge,
            dependent_target: None,
            digest: format!("sha256:{}", "a".repeat(64)),
            immutable_reference: format!("ghcr.io/example/base@sha256:{}", "a".repeat(64)),
        }],
    )?;
    assert!(rewritten.starts_with("# ghcr.io/example/base\n"));
    assert!(rewritten.contains("FROM ghcr.io/example/base@sha256:"));
    Ok(())
}

#[test]
fn rewrite_rejects_each_stale_review_field_independently() {
    let source = "FROM ghcr.io/example/base AS app\n";
    for (original_reference, source_line) in
        [("ghcr.io/example/changed", 1), ("ghcr.io/example/base", 2)]
    {
        let edge = crate::domain::build::FirstPartyBuildEdge {
            producer_module: "base".to_owned(),
            dependent_module: "app".to_owned(),
            stage: Some("app".to_owned()),
            reference: BuildDependencyReference::Dockerfile {
                original_reference: original_reference.to_owned(),
                instruction_index: 0,
                source_line,
            },
        };
        let result = rewrite_dependencies(
            source,
            "Dockerfile",
            &[ResolvedBuildDependency {
                edge,
                dependent_target: None,
                digest: format!("sha256:{}", "a".repeat(64)),
                immutable_reference: format!("ghcr.io/example/base@sha256:{}", "a".repeat(64)),
            }],
        );
        assert!(matches!(
            result,
            Err(error) if error.to_string().contains("changed before execution")
        ));
    }
}

#[test]
fn rewrite_ignores_build_arguments_and_rejects_missing_from_instructions() -> Result<(), MmError> {
    let dependency = |reference| ResolvedBuildDependency {
        edge: crate::domain::build::FirstPartyBuildEdge {
            producer_module: "base".to_owned(),
            dependent_module: "app".to_owned(),
            stage: None,
            reference,
        },
        dependent_target: None,
        digest: format!("sha256:{}", "a".repeat(64)),
        immutable_reference: format!("example/base@sha256:{}", "a".repeat(64)),
    };
    let build_argument = dependency(BuildDependencyReference::BuildArgument {
        arg: "BASE".to_owned(),
        instruction_index: 0,
        source_line: 1,
    });
    assert_eq!(
        rewrite_dependencies("FROM scratch\n", "Dockerfile", &[build_argument])?,
        "FROM scratch\n"
    );

    let missing = dependency(BuildDependencyReference::Dockerfile {
        original_reference: "example/base".to_owned(),
        instruction_index: 1,
        source_line: 1,
    });
    let error = rewrite_dependencies("FROM scratch\n", "Dockerfile", &[missing])
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(error.contains("missing from the snapshot"));
    Ok(())
}

#[test]
fn rewrite_tracks_offsets_across_instructions_and_blank_lines() -> Result<(), MmError> {
    let source = "FROM scratch AS base\n\nFROM ghcr.io/example/base AS app\n";
    let edge = crate::domain::build::FirstPartyBuildEdge {
        producer_module: "base".to_owned(),
        dependent_module: "app".to_owned(),
        stage: Some("app".to_owned()),
        reference: BuildDependencyReference::Dockerfile {
            original_reference: "ghcr.io/example/base".to_owned(),
            instruction_index: 1,
            source_line: 3,
        },
    };
    let immutable = format!("ghcr.io/example/base@sha256:{}", "a".repeat(64));
    let rewritten = rewrite_dependencies(
        source,
        "Dockerfile",
        &[ResolvedBuildDependency {
            edge,
            dependent_target: None,
            digest: format!("sha256:{}", "a".repeat(64)),
            immutable_reference: immutable.clone(),
        }],
    )?;
    assert_eq!(
        rewritten,
        format!("FROM scratch AS base\n\nFROM {immutable} AS app\n")
    );
    Ok(())
}

proptest! {
    #![proptest_config(Config {
        cases: 256,
        rng_seed: RngSeed::Fixed(0x444f_434b_4552_3235),
        failure_persistence: None,
        ..Config::default()
    })]

    #[test]
    fn comment_text_cannot_create_dockerfile_stages(comment in "[ -~]{0,256}") {
        let comment = comment.replace(['\r', '\n'], " ");
        let source = format!("# {comment}\nFROM scratch AS final\n");
        let parsed = parse(&source, "Dockerfile")
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(parsed.bases.len(), 1);
        prop_assert_eq!(parsed.bases[0].stage.as_deref(), Some("final"));
    }
}
