//! The planning helpers behind the build manager. The manager's plans are
//! tested through its public API in `tests/units/build_manager.rs`.

use std::fs;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use tempfile::TempDir;

use super::*;

fn graph_unit(module: String, predecessors: Vec<String>) -> ImageBuildUnitPlan {
    let dependencies = predecessors
        .into_iter()
        .enumerate()
        .map(|(index, producer_module)| FirstPartyBuildEdge {
            producer_module,
            dependent_module: module.clone(),
            stage: None,
            reference: BuildDependencyReference::Dockerfile {
                original_reference: "example/base".to_owned(),
                instruction_index: u32::try_from(index).unwrap_or(u32::MAX),
                source_line: 1,
            },
        })
        .collect();
    ImageBuildUnitPlan {
        module,
        image: "example/image".to_owned(),
        context: PathBuf::from("."),
        dockerfile: PathBuf::from("Dockerfile"),
        context_hash: "a".repeat(64),
        dockerfile_hash: "b".repeat(64),
        args: BTreeMap::new(),
        secret_ids: Vec::new(),
        credential_secret_ids: Vec::new(),
        dependencies,
        upstream_node_ids: Vec::new(),
        extra_inputs: Vec::new(),
        version_embedded: false,
        reuse_policy: BuildReusePolicy::Allowed,
        input_fingerprint: "c".repeat(64),
        variants: Vec::new(),
    }
}

proptest! {
    #![proptest_config(Config {
        cases: 128,
        rng_seed: RngSeed::Fixed(0x4255_494c_4447_5241),
        failure_persistence: None,
        ..Config::default()
    })]

    #[test]
    fn generated_build_graphs_are_stable_topological_and_reject_cycles(
        choices in proptest::collection::vec(any::<u8>(), 1..16),
        duplicate_edges in any::<bool>(),
    ) {
        let mut units = Vec::new();
        for (position, choice) in choices.iter().enumerate() {
            let module = format!("module-{position}");
            let predecessors = if position == 0 {
                Vec::new()
            } else {
                let predecessor = format!("module-{}", usize::from(*choice) % position);
                if duplicate_edges {
                    vec![predecessor.clone(), predecessor]
                } else {
                    vec![predecessor]
                }
            };
            units.push(graph_unit(module, predecessors));
        }
        units.reverse();

        let first = topological_build_units(units.clone())?;
        let second = topological_build_units(units.clone())?;
        prop_assert_eq!(&first, &second);
        let names = first.iter().map(|unit| unit.module.as_str()).collect::<BTreeSet<_>>();
        prop_assert_eq!(names.len(), units.len());
        let positions = first
            .iter()
            .enumerate()
            .map(|(position, unit)| (unit.module.as_str(), position))
            .collect::<BTreeMap<_, _>>();
        for unit in &first {
            for predecessor in unit_predecessors(unit) {
                prop_assert!(positions[predecessor] < positions[unit.module.as_str()]);
            }
        }

        if units.len() > 1 {
            let first_name = first[0].module.clone();
            let last_name = first[first.len() - 1].module.clone();
            let first_unit = units.iter_mut().find(|unit| unit.module == first_name)
                .ok_or_else(|| TestCaseError::fail("sorted module disappeared"))?;
            first_unit.dependencies.push(FirstPartyBuildEdge {
                producer_module: last_name,
                dependent_module: first_name,
                stage: None,
                reference: BuildDependencyReference::Dockerfile {
                    original_reference: "example/cycle".to_owned(),
                    instruction_index: 99,
                    source_line: 1,
                },
            });
            let error = topological_build_units(units).err()
                .ok_or_else(|| TestCaseError::fail("generated cycle was accepted"))?;
            prop_assert!(error.to_string().contains("form a cycle"));
        }
    }
}

fn project() -> Result<TempDir, Box<dyn std::error::Error>> {
    let project = crate::test_support::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir(project.path().join("website"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-test
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  website:
    path: website
    services:
      app:
        stack: app
    build:
      image: ghcr.io/acme/web
      targets:
        production: ""
        development: dev
      args:
        SENTRY_ORG: acme
        SENTRY_PROJECT: web
      secrets:
        SENTRY_AUTH_TOKEN: build.sentry-auth-token
        composer_auth: build.composer-auth
        gh_pat: build.github-token
secrets:
  build.sentry-auth-token:
    storage:
      provider: env
      location: ACME_SENTRY_AUTH_TOKEN
  build.composer-auth:
    storage:
      provider: env
      location: ACME_COMPOSER_AUTH
  build.github-token:
    storage:
      provider: env
      location: ACME_GITHUB_TOKEN
environments:
  local:
    bindings: {}
"#,
    )?;
    fs::write(
        project.path().join("compose.yaml"),
        "services:\n  app:\n    image: ghcr.io/acme/web:old\n",
    )?;
    fs::write(
        project.path().join("website/Dockerfile"),
        "FROM scratch AS production\nFROM scratch AS development\n",
    )?;
    fs::write(project.path().join("website/app.txt"), "content\n")?;
    Ok(project)
}

#[test]
fn immutable_snapshot_rejects_a_changed_context() -> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let context = project.path().join("website");
    let dockerfile = context.join("Dockerfile");
    let hash = hash_context(&context, &dockerfile)?;
    fs::write(context.join("app.txt"), "changed\n")?;
    let snapshot = project.path().join("snapshot");

    assert!(matches!(
        snapshot_build_context(project.path(), &context, &dockerfile, &snapshot, &hash),
        Err(MmError::State { .. })
    ));
    Ok(())
}

#[test]
fn graph_helpers_report_acyclic_fallback_external_bases_and_unknown_predecessors()
-> Result<(), Box<dyn std::error::Error>> {
    let standalone = graph_unit("standalone".to_owned(), Vec::new());
    let units = vec![standalone.clone()];
    let index = BTreeMap::from([("standalone", 0)]);
    assert_eq!(cycle_path(&units, &index), "unknown cycle");

    let units = vec![
        graph_unit("producer".to_owned(), Vec::new()),
        graph_unit("dependent".to_owned(), vec!["producer".to_owned()]),
    ];
    let index = BTreeMap::from([("producer", 0), ("dependent", 1)]);
    assert_eq!(cycle_path(&units, &index), "unknown cycle");

    let project = crate::test_support::tempdir()?;
    fs::write(
        project.path().join("Dockerfile"),
        "FROM example/external:latest\n",
    )?;
    let mut external = vec![standalone];
    infer_first_party_bases(project.path(), &mut external)?;
    assert!(external[0].dependencies.is_empty());

    let mut unknown = vec![graph_unit(
        "dependent".to_owned(),
        vec!["absent".to_owned()],
    )];
    assert_eq!(topological_build_units(unknown.clone())?.len(), 1);
    assert!(assign_canonical_fingerprints(&mut unknown, &["linux/amd64".to_owned()]).is_err());
    Ok(())
}

#[test]
fn module_planning_rejects_reserved_snapshots_and_missing_authoritative_images()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let document = crate::test_project_documents().load_from(project.path())?;
    let module = document
        .config
        .modules
        .get("website")
        .cloned()
        .ok_or("website module missing")?;
    let tags = vec!["edge".to_owned()];

    let mut disabled = module.clone();
    disabled.build.as_mut().ok_or("build missing")?.enabled = false;
    assert!(
        plan_module(
            &document,
            ("website", &disabled),
            None,
            &tags,
            "version",
            &[]
        )?
        .is_none()
    );

    let mut automatic = module.clone();
    automatic.build = None;
    fs::remove_file(project.path().join("website/Dockerfile"))?;
    assert!(
        plan_module(
            &document,
            ("website", &automatic),
            None,
            &tags,
            "version",
            &[]
        )?
        .is_none()
    );
    fs::write(project.path().join("website/Dockerfile"), "FROM scratch\n")?;
    assert!(matches!(
        plan_module(
            &document,
            ("website", &automatic),
            None,
            &tags,
            "version",
            &[]
        ),
        Err(MmError::Build { .. })
    ));

    fs::write(
        project.path().join("website/.mm-external.Dockerfile"),
        "reserved",
    )?;
    assert!(matches!(
        plan_module(&document, ("website", &module), None, &tags, "version", &[]),
        Err(MmError::Security { .. })
    ));
    fs::remove_file(project.path().join("website/.mm-external.Dockerfile"))?;

    let mut invalid_image = module.clone();
    invalid_image.build.as_mut().ok_or("build missing")?.image = Some("UPPER/image".to_owned());
    assert!(matches!(
        plan_module(
            &document,
            ("website", &invalid_image),
            None,
            &tags,
            "version",
            &[]
        ),
        Err(MmError::Build { .. })
    ));

    let mut missing_context = module.clone();
    missing_context
        .build
        .as_mut()
        .ok_or("build missing")?
        .context = Some(PathBuf::from("missing"));
    assert!(matches!(
        plan_module(
            &document,
            ("website", &missing_context),
            None,
            &tags,
            "version",
            &[]
        ),
        Err(MmError::Io { .. })
    ));

    fs::remove_file(project.path().join("website/Dockerfile"))?;
    assert!(matches!(
        plan_module(&document, ("website", &module), None, &tags, "version", &[]),
        Err(MmError::Io { .. })
    ));
    fs::write(project.path().join("website/Dockerfile"), [0xff])?;
    assert!(matches!(
        plan_module(&document, ("website", &module), None, &tags, "version", &[]),
        Err(MmError::Build { .. })
    ));
    fs::write(project.path().join("website/Dockerfile"), "FROM ${\n")?;
    assert!(matches!(
        plan_module(&document, ("website", &module), None, &tags, "version", &[]),
        Err(MmError::Build { .. })
    ));

    fs::write(project.path().join("website/Dockerfile"), "FROM scratch\n")?;
    fs::write(project.path().join(".env"), "SECRET=value\n")?;
    let mut sensitive_input = module.clone();
    sensitive_input
        .build
        .as_mut()
        .ok_or("build missing")?
        .extra_inputs
        .push(PathBuf::from(".env"));
    assert!(
        plan_module(
            &document,
            ("website", &sensitive_input),
            None,
            &tags,
            "version",
            &[]
        )
        .is_err_and(|error| error.code() == "MM_SECURITY")
    );
    Ok(())
}

#[test]
fn typed_dependency_planning_requires_declared_and_consumed_from_arguments()
-> Result<(), Box<dyn std::error::Error>> {
    let dockerfile = Path::new("Dockerfile");
    let parsed = parse_dockerfile("ARG BASE\nFROM ${BASE}\n", "Dockerfile")?;
    let missing = plan_dependencies(None, "web", dockerfile, &parsed);
    assert!(matches!(missing, Err(MmError::Build { .. })));

    let mut configured = ModuleBuildConfig::default();
    configured
        .depends_on
        .insert("base".to_owned(), "BASE".to_owned());
    let undeclared = parse_dockerfile("FROM ${BASE}\n", "Dockerfile")?;
    assert!(matches!(
        plan_dependencies(Some(&configured), "web", dockerfile, &undeclared),
        Err(MmError::Build { .. })
    ));

    let unused = parse_dockerfile("ARG BASE\nFROM scratch\n", "Dockerfile")?;
    assert!(matches!(
        plan_dependencies(Some(&configured), "web", dockerfile, &unused),
        Err(MmError::Build { .. })
    ));
    let undeclared_unused = parse_dockerfile("FROM scratch\n", "Dockerfile")?;
    assert!(matches!(
        plan_dependencies(Some(&configured), "web", dockerfile, &undeclared_unused),
        Err(MmError::Build { .. })
    ));

    let dependencies = plan_dependencies(Some(&configured), "web", dockerfile, &parsed)?;
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].producer_module, "base");
    assert_eq!(reuse_policy_for(&configured), BuildReusePolicy::Allowed);
    configured
        .secrets
        .insert("token".to_owned(), "build.token".to_owned());
    assert_eq!(reuse_policy_for(&configured), BuildReusePolicy::Forbidden);
    Ok(())
}

#[test]
fn graph_inference_propagates_dockerfile_read_and_parse_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let project = crate::test_support::tempdir()?;
    let mut units = vec![graph_unit("app".to_owned(), Vec::new())];
    assert!(matches!(
        infer_first_party_bases(project.path(), &mut units),
        Err(MmError::Io { .. })
    ));
    fs::write(project.path().join("Dockerfile"), "FROM ${\n")?;
    assert!(matches!(
        infer_first_party_bases(project.path(), &mut units),
        Err(MmError::Build { .. })
    ));
    Ok(())
}
