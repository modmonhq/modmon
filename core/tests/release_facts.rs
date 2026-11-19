//! Release facts: `MM.` keys mm knows about the release a
//! deployment runs, and `RELEASE.` inputs the deployer gives the plan.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use mm::adapters::config::{ConfigLoader, validate};
use mm::domain::config::{ConfigDocument, EnvironmentBinding};
use mm::domain::config_file::{BindingProblem, binding_text, render};
use mm::domain::release_facts::{ReleaseFacts, ReleaseRequest, ReleaseValue, release_value};
use mm::domain::rollout::{
    DeploymentRecord, DeploymentStatus, NodeDeploymentStatus, NodeStatusRow, redeploy_target,
    rollback_target,
};
use tempfile::TempDir;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn digest() -> String {
    format!("sha256:{}", "ab".repeat(32))
}

/// The minimal fixture project with `extra` appended to its configuration
/// and a config-file template `api/release.conf.tmpl` holding `template`.
fn project(extra: &str, template: &str) -> Result<TempDir, Box<dyn std::error::Error>> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal");
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path();
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("api"))?;
    for file in ["compose.yaml", "api/.env.example"] {
        fs::copy(fixture.join(file), root.join(file))?;
    }
    let config = fs::read_to_string(fixture.join(".config/mm.yaml"))?;
    let service = "      api:\n        stack: app\n";
    let config = config.replacen(
        service,
        &format!(
            "{service}        config-files:\n          /etc/release.conf: {{template: release.conf.tmpl}}\n"
        ),
        1,
    );
    fs::write(root.join(".config/mm.yaml"), format!("{config}{extra}"))?;
    fs::write(root.join("api/release.conf.tmpl"), template)?;
    Ok(temporary)
}

fn load(project: &TempDir) -> Result<ConfigDocument, Box<dyn std::error::Error>> {
    Ok(ConfigLoader.load_from(project.path())?)
}

fn released(inputs: &[(&str, &str)]) -> ReleaseFacts {
    ReleaseFacts::new(
        Some(COMMIT.to_owned()),
        ReleaseRequest {
            name: None,
            inputs: inputs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        },
    )
    .with_images(BTreeMap::from([(
        "api".to_owned(),
        format!("registry.example.com/shop/api@{}", digest()),
    )]))
}

const DECLARED: &str =
    "release-inputs:\n  BUILD_ID: {pattern: \"^[A-Za-z0-9._-]+$\"}\n  NODE_IMAGE: {}\n";

#[test]
fn a_config_file_template_renders_the_image_the_release_deploys() -> TestResult {
    let template =
        "image {{ MM.IMAGE.api }}\ndigest {{MM.IMAGE_DIGEST.api}}\nbuild {{ RELEASE.BUILD_ID }}\n";
    let project = project(DECLARED, template)?;
    let mut document = load(&project)?;
    let report = validate(&document, "local");
    assert!(report.is_valid(), "{:?}", report.diagnostics);

    document.config.release = Some(released(&[
        ("BUILD_ID", "4757.2.3-12"),
        ("NODE_IMAGE", "x"),
    ]));
    let rendered = render(template, |key| binding_text(&document.config, "local", key))
        .map_err(|problem| format!("{problem:?}"))?;
    assert_eq!(
        rendered,
        format!(
            "image registry.example.com/shop/api@{0}\ndigest {0}\nbuild 4757.2.3-12\n",
            digest()
        )
    );

    document.config.release = Some(ReleaseFacts::new(
        Some(COMMIT.to_owned()),
        ReleaseRequest::default(),
    ));
    match binding_text(&document.config, "local", "MM.IMAGE.api") {
        Err(BindingProblem::Unresolved(reason)) => {
            assert!(reason.contains("--receipt FILE"), "{reason}");
        }
        other => return Err(format!("expected a refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn facts_and_value_refs_resolve_from_the_release() -> TestResult {
    let project = project(
        &format!(
            "{DECLARED}bindings:\n  APP.VERSION: {{value-ref: MM.RELEASE}}\n  APP.BUILD: {{value-ref: RELEASE.BUILD_ID}}\n"
        ),
        "{{ APP.VERSION }}\n",
    )?;
    let mut document = load(&project)?;
    assert!(matches!(
        document.config.bindings.get("APP.VERSION"),
        Some(EnvironmentBinding::Reference(reference)) if reference.value_ref == "MM.RELEASE"
    ));
    let report = validate(&document, "local");
    assert!(report.is_valid(), "{:?}", report.diagnostics);

    assert_eq!(
        release_value(&document.config, "local", "MM.COMMIT"),
        Some(ReleaseValue::Pending)
    );
    assert_eq!(release_value(&document.config, "local", "DB.HOST"), None);

    document.config.release = Some(released(&[("BUILD_ID", "7"), ("NODE_IMAGE", "n")]));
    let value = |key: &str| release_value(&document.config, "local", key);
    let known = |text: &str| Some(ReleaseValue::Known(text.to_owned()));
    assert_eq!(value("MM.COMMIT"), known(COMMIT));
    assert_eq!(value("MM.COMMIT_SHORT"), known("0123456789ab"));
    assert_eq!(value("MM.RELEASE"), known("sha-0123456789ab"));
    assert_eq!(value("APP.VERSION"), known("sha-0123456789ab"));
    assert_eq!(value("APP.BUILD"), known("7"));
    assert_eq!(value("RELEASE.NODE_IMAGE"), known("n"));
    assert_eq!(
        value("RELEASE.UNDECLARED"),
        None,
        "only declared inputs are release keys"
    );

    let mut named = released(&[]);
    named.name = Some("v26.6.0".to_owned());
    document.config.release = Some(named);
    assert_eq!(
        release_value(&document.config, "local", "APP.VERSION"),
        known("v26.6.0")
    );
    Ok(())
}

#[test]
fn the_plan_refuses_undeclared_and_malformed_inputs() -> TestResult {
    let declared = serde_yaml_ng::from_str::<
        BTreeMap<String, mm::domain::release_facts::ReleaseInputConfig>,
    >("BUILD_ID: {pattern: \"[0-9]+\"}\nNOTE: {}\n")?;
    let check = |inputs: &[(&str, &str)]| released(inputs).check(&declared);

    assert_eq!(check(&[("BUILD_ID", "12"), ("NOTE", "any text")]), Ok(()));
    assert_eq!(
        check(&[("BUILD_ID", "12")]),
        Ok(()),
        "an input is required where a template reads it, which the environment review checks"
    );
    let undeclared = check(&[("BUILD_ID", "1"), ("NOTE", "n"), ("OTHER", "x")])
        .err()
        .unwrap_or_default();
    assert!(
        undeclared.contains("OTHER is not declared") && undeclared.contains("release-inputs:"),
        "{undeclared}"
    );
    let malformed = check(&[("BUILD_ID", "12a"), ("NOTE", "n")])
        .err()
        .unwrap_or_default();
    assert!(
        malformed.contains("does not match its pattern '[0-9]+'"),
        "the pattern matches the whole value: {malformed}"
    );
    let control = check(&[("BUILD_ID", "1"), ("NOTE", "two\nlines")])
        .err()
        .unwrap_or_default();
    assert!(control.contains("one line"), "{control}");

    let mut named = released(&[("BUILD_ID", "1"), ("NOTE", "n")]);
    named.name = Some("v1 beta".to_owned());
    let name = named.check(&declared).err().unwrap_or_default();
    assert!(name.contains("release name"), "{name}");
    Ok(())
}

#[test]
fn release_arguments_parse_and_print_back() -> TestResult {
    let request = ReleaseRequest::from_arguments(
        Some("v2".to_owned()),
        &["BUILD_ID=1=2".to_owned(), "NOTE=two words".to_owned()],
    )?;
    assert_eq!(
        request.inputs["BUILD_ID"], "1=2",
        "the value is what follows the first ="
    );
    assert_eq!(
        request.arguments(),
        " --release v2 --release-input BUILD_ID=1=2 --release-input 'NOTE=two words'"
    );
    let twice = ReleaseRequest::from_arguments(None, &["A=1".to_owned(), "A=2".to_owned()]);
    assert!(twice.is_err_and(|error| error.contains("twice")));
    let bare = ReleaseRequest::from_arguments(None, &["A".to_owned()]);
    assert!(bare.is_err_and(|error| error.contains("NAME=VALUE")));
    Ok(())
}

#[test]
fn bindings_cannot_take_the_keys_of_the_release() -> TestResult {
    for (bindings, expected) in [
        ("  MM.COMMIT: {value: x}\n", "facts mm supplies"),
        (
            "  RELEASE.BUILD_ID: {value: x}\n",
            "already reads release input",
        ),
        (
            "  APP.X: {value-ref: MM.NOTHING}\n",
            "names no release fact",
        ),
        (
            "  APP.X: {value-ref: domains.web}\n",
            "a domain needs `cast: url`",
        ),
    ] {
        let project = project(&format!("{DECLARED}bindings:\n{bindings}"), "x\n")?;
        let report = validate(&load(&project)?, "local");
        let found = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "MM_ENV_KEY_INVALID")
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>();
        assert!(
            found.iter().any(|message| message.contains(expected)),
            "{bindings}: {found:?}"
        );
    }
    for (inputs, expected) in [
        ("  build_id: {}\n", "one upper-case word"),
        (
            "  BUILD_ID: {pattern: \"[\"}\n",
            "is not a regular expression",
        ),
    ] {
        let project = project(&format!("release-inputs:\n{inputs}"), "x\n")?;
        let report = validate(&load(&project)?, "local");
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "MM_RELEASE_INPUT"
                    && diagnostic.message.contains(expected)),
            "{inputs}: {:?}",
            report.diagnostics
        );
    }
    Ok(())
}

fn record(release: &ReleaseFacts, at: i64, plan: char) -> DeploymentRecord {
    DeploymentRecord {
        commit: COMMIT.to_owned(),
        deployed_at_unix_ms: at,
        deployed_by: "ci".to_owned(),
        runs_services: true,
        plan_hash: Some(plan.to_string().repeat(64)),
        receipt: None,
        release: Some(release.clone()),
    }
}

fn row(node: &str, current: DeploymentRecord, previous: DeploymentRecord) -> NodeStatusRow {
    NodeStatusRow {
        node: node.to_owned(),
        problem: None,
        status: Some(NodeDeploymentStatus {
            current: Some(current),
            previous: Some(previous),
            rollout: None,
            operations: Vec::new(),
        }),
    }
}

#[test]
fn a_rollback_returns_to_the_inputs_the_deployment_before_ran_with() -> TestResult {
    let first = released(&[("BUILD_ID", "1")]);
    let second = released(&[("BUILD_ID", "2")]);
    assert!(
        !record(&first, 10, '1').same_release(&record(&second, 20, '2')),
        "the same commit and build with other inputs is another release"
    );
    assert_ne!(first.digest(), second.digest(), "and another plan");

    let status = DeploymentStatus {
        target: "production".to_owned(),
        environment: "production".to_owned(),
        nodes: vec![
            row("app-1", record(&second, 20, '2'), record(&first, 10, '1')),
            row("app-2", record(&second, 21, '2'), record(&first, 10, '1')),
        ],
        answered_unhealthy: Vec::new(),
    };
    let target = rollback_target(&status, None)?;
    assert_eq!(target.commit, COMMIT);
    assert_eq!(target.release, first.request());

    let mut split = status;
    if let Some(NodeStatusRow {
        status: Some(node), ..
    }) = split.nodes.get_mut(1)
    {
        node.previous = Some(record(&released(&[("BUILD_ID", "0")]), 5, '0'));
    }
    let refusal = rollback_target(&split, None).err().unwrap_or_default();
    assert!(
        refusal.contains("--release-input BUILD_ID=1 --nodes app-1")
            && refusal.contains("--release-input BUILD_ID=0 --nodes app-2"),
        "{refusal}"
    );
    Ok(())
}

#[test]
fn a_redeploy_keeps_the_release_the_hosts_run() -> TestResult {
    let first = released(&[("BUILD_ID", "1")]);
    let second = released(&[("BUILD_ID", "2")]);
    let status = DeploymentStatus {
        target: "production".to_owned(),
        environment: "production".to_owned(),
        nodes: vec![
            row("app-1", record(&second, 20, '2'), record(&first, 10, '1')),
            row("app-2", record(&second, 21, '2'), record(&first, 10, '1')),
        ],
        answered_unhealthy: Vec::new(),
    };
    let target = redeploy_target(&status, None)?;
    assert_eq!(target.commit, COMMIT);
    assert_eq!(target.release, second.request());
    assert_eq!(target.nodes, ["app-1", "app-2"]);

    let mut split = status;
    if let Some(NodeStatusRow {
        status: Some(node), ..
    }) = split.nodes.get_mut(1)
    {
        node.current = Some(record(&first, 21, '1'));
    }
    let refusal = redeploy_target(&split, None).err().unwrap_or_default();
    assert!(
        refusal.contains("--release-input BUILD_ID=2 --nodes app-1")
            && refusal.contains("--release-input BUILD_ID=1 --nodes app-2"),
        "{refusal}"
    );
    Ok(())
}
