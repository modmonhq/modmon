use super::*;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn source(branch: Option<&str>, tags_at_head: &[&str], repository_tags: &[&str]) -> BuildSource {
    BuildSource {
        commit: COMMIT.to_owned(),
        branch: branch.map(str::to_owned),
        tags_at_head: tags_at_head.iter().map(ToString::to_string).collect(),
        repository_tags: repository_tags.iter().map(ToString::to_string).collect(),
        tags_complete: true,
        default_branch: branch == Some("main"),
        dirty: false,
    }
}

#[test]
fn stable_release_gets_safe_floating_aliases() {
    let source = source(Some("main"), &["v26.5.0"], &["v26.4.9", "v26.5.0"]);

    assert_eq!(
        automatic_tags(&source, BuildTagStrategy::Auto),
        Ok(vec![
            "v26.5.0".to_owned(),
            "26.5.0".to_owned(),
            "26.5".to_owned(),
            "latest".to_owned(),
            format!("sha-{COMMIT}"),
        ])
    );
}

#[test]
fn older_release_cannot_move_aliases_backwards() {
    let source = source(None, &["v26.5.0"], &["v26.5.1", "v26.6.0", "v26.5.0"]);

    assert_eq!(
        automatic_tags(&source, BuildTagStrategy::Auto),
        Ok(vec![
            "v26.5.0".to_owned(),
            "26.5.0".to_owned(),
            format!("sha-{COMMIT}"),
        ])
    );
}

#[test]
fn prerelease_never_gets_floating_aliases() {
    let source = source(None, &["v26.5.0-rc.1"], &["v26.5.0-rc.1"]);

    assert_eq!(
        automatic_tags(&source, BuildTagStrategy::Auto),
        Ok(vec![
            "v26.5.0-rc.1".to_owned(),
            "26.5.0-rc.1".to_owned(),
            format!("sha-{COMMIT}"),
        ])
    );
}

#[test]
fn release_and_default_branches_get_expected_alias() {
    assert_eq!(
        automatic_tags(&source(Some("26.5.x"), &[], &[]), BuildTagStrategy::Auto),
        Ok(vec!["26.5-current".to_owned(), format!("sha-{COMMIT}")])
    );
    assert_eq!(
        automatic_tags(&source(Some("main"), &[], &[]), BuildTagStrategy::Auto),
        Ok(vec!["edge".to_owned(), format!("sha-{COMMIT}")])
    );
}

#[test]
fn feature_branch_and_immutable_strategy_only_get_immutable_tags() {
    assert_eq!(
        automatic_tags(
            &source(Some("feature/build"), &[], &[]),
            BuildTagStrategy::Auto
        ),
        Ok(vec![format!("sha-{COMMIT}")])
    );
    assert_eq!(
        automatic_tags(&source(Some("main"), &[], &[]), BuildTagStrategy::Immutable),
        Ok(vec![format!("sha-{COMMIT}")])
    );
}

#[test]
fn dirty_local_build_has_no_movable_alias() {
    let mut source = source(Some("main"), &["v26.5.0"], &["v26.5.0"]);
    source.dirty = true;

    assert_eq!(
        automatic_tags(&source, BuildTagStrategy::Auto),
        Ok(vec![format!("sha-{COMMIT}-dirty")])
    );
}

#[test]
fn different_version_tags_at_head_are_rejected() {
    let source = source(None, &["v1.0.0", "v2.0.0"], &[]);

    assert_eq!(
        automatic_tags(&source, BuildTagStrategy::Auto),
        Err("HEAD has multiple different semantic version tags".to_owned())
    );
}

#[test]
fn development_variant_suffixes_every_tag() {
    let tags = vec!["v26.5.0".to_owned(), "latest".to_owned()];

    assert_eq!(
        suffixed_tags(&tags, "dev"),
        Ok(vec!["v26.5.0-dev".to_owned(), "latest-dev".to_owned()])
    );
}

#[test]
fn build_version_is_shared_across_release_units() {
    assert_eq!(
        build_version(&source(None, &["v26.5.0"], &[])),
        Ok("v26.5.0".to_owned())
    );
    assert_eq!(
        build_version(&source(Some("26.5.x"), &[], &[])),
        Ok("26.5.x".to_owned())
    );
    assert_eq!(
        build_version(&source(Some("feature/build"), &[], &[])),
        Ok("01234567".to_owned())
    );
}

#[test]
fn image_repositories_use_a_closed_oci_compatible_shape() {
    for valid in [
        "app",
        "example/app",
        "ghcr.io/example/app",
        "localhost:5000/example/app",
    ] {
        assert!(is_image_repository(valid), "{valid} should be accepted");
    }
    for invalid in [
        "Example/app",
        "example/app:tag",
        "example:5000",
        "host:bad/example/app",
        "host:5000:6000/example/app",
        "example//app",
        "example/-app",
    ] {
        assert!(
            !is_image_repository(invalid),
            "{invalid} should be rejected"
        );
    }
}

#[test]
fn canonical_identity_distinguishes_every_build_input() -> Result<(), String> {
    let hash_a = "a".repeat(64);
    let hash_b = "b".repeat(64);
    let platforms = vec!["linux/amd64".to_owned()];
    let unit = ImageBuildUnitPlan {
        module: "app".to_owned(),
        image: "ghcr.io/example/app".to_owned(),
        context: PathBuf::from("app"),
        dockerfile: PathBuf::from("app/Dockerfile"),
        context_hash: hash_a.clone(),
        dockerfile_hash: hash_b.clone(),
        args: BTreeMap::from([
            ("FEATURE".to_owned(), "one".to_owned()),
            ("VERSION".to_owned(), "01234567".to_owned()),
        ]),
        secret_ids: vec!["token".to_owned()],
        credential_secret_ids: Vec::new(),
        dependencies: Vec::new(),
        upstream_node_ids: Vec::new(),
        extra_inputs: vec![ImageBuildTrigger {
            path: PathBuf::from("shared/input"),
            digest: hash_a.clone(),
        }],
        version_embedded: false,
        reuse_policy: BuildReusePolicy::RevisionGated,
        input_fingerprint: String::new(),
        variants: Vec::new(),
    };
    let baseline = unit_input_fingerprint(&unit, &platforms)?;

    let mut changed = unit.clone();
    changed.context_hash = "c".repeat(64);
    assert_ne!(baseline, unit_input_fingerprint(&changed, &platforms)?);
    changed = unit.clone();
    changed.dockerfile_hash = "d".repeat(64);
    assert_ne!(baseline, unit_input_fingerprint(&changed, &platforms)?);
    changed = unit.clone();
    changed.args.insert("FEATURE".to_owned(), "two".to_owned());
    assert_ne!(baseline, unit_input_fingerprint(&changed, &platforms)?);
    changed = unit.clone();
    changed.secret_ids.push("second-token".to_owned());
    assert_ne!(baseline, unit_input_fingerprint(&changed, &platforms)?);
    // A credential-only secret is mounted but never changes the image, so
    // adding or rotating it keeps the fingerprint.
    changed = unit.clone();
    changed.secret_ids.push("composer_auth".to_owned());
    changed
        .credential_secret_ids
        .push("composer_auth".to_owned());
    assert_eq!(baseline, unit_input_fingerprint(&changed, &platforms)?);
    changed.credential_secret_ids.clear();
    assert_ne!(baseline, unit_input_fingerprint(&changed, &platforms)?);
    changed = unit.clone();
    changed.extra_inputs[0].digest = "e".repeat(64);
    assert_ne!(baseline, unit_input_fingerprint(&changed, &platforms)?);
    changed = unit.clone();
    changed.reuse_policy = BuildReusePolicy::Forbidden;
    assert_ne!(baseline, unit_input_fingerprint(&changed, &platforms)?);
    assert_ne!(
        baseline,
        unit_input_fingerprint(&unit, &["linux/arm64".to_owned()])?
    );

    let mut dependent = unit.clone();
    dependent.dependencies = vec![FirstPartyBuildEdge {
        producer_module: "base-a".to_owned(),
        dependent_module: "app-a".to_owned(),
        stage: None,
        reference: BuildDependencyReference::Dockerfile {
            original_reference: "example/base:current".to_owned(),
            instruction_index: 0,
            source_line: 1,
        },
    }];
    dependent.upstream_node_ids = vec![hash_a.clone()];
    let dependent_fingerprint = unit_input_fingerprint(&dependent, &platforms)?;
    let mut renamed = dependent.clone();
    renamed.dependencies[0].producer_module = "base-b".to_owned();
    renamed.dependencies[0].dependent_module = "app-b".to_owned();
    assert_eq!(
        dependent_fingerprint,
        unit_input_fingerprint(&renamed, &platforms)?
    );
    renamed.upstream_node_ids[0] = "f".repeat(64);
    assert_ne!(
        dependent_fingerprint,
        unit_input_fingerprint(&renamed, &platforms)?
    );

    let no_upstream = BTreeMap::new();
    let no_revisions = BTreeMap::new();
    let production = ImageBuildVariantPlan {
        target: Some("production".to_owned()),
        stage: None,
        tag_suffix: String::new(),
        tags: Vec::new(),
        args: BTreeMap::new(),
    };
    let execution =
        unit_execution_fingerprint(&baseline, "local", None, &no_upstream, &no_revisions)?;
    assert_ne!(
        execution,
        unit_execution_fingerprint(
            &baseline,
            "local",
            Some(&production),
            &no_upstream,
            &no_revisions,
        )?
    );
    assert_ne!(
        execution,
        unit_execution_fingerprint(&baseline, "integration", None, &no_upstream, &no_revisions,)?
    );
    assert_ne!(
        execution,
        unit_execution_fingerprint(
            &baseline,
            "local",
            None,
            &BTreeMap::from([("base:0".to_owned(), format!("sha256:{}", "f".repeat(64)))]),
            &no_revisions,
        )?
    );
    assert_ne!(
        execution,
        unit_execution_fingerprint(
            &baseline,
            "local",
            None,
            &no_upstream,
            &BTreeMap::from([("token".to_owned(), "revision-2".to_owned())]),
        )?
    );
    Ok(())
}

fn valid_unit(
    module: &str,
    image: &str,
    platforms: &[String],
) -> Result<ImageBuildUnitPlan, String> {
    let mut unit = ImageBuildUnitPlan {
        module: module.to_owned(),
        image: image.to_owned(),
        context: PathBuf::from(module),
        dockerfile: PathBuf::from(format!("{module}/Dockerfile")),
        context_hash: "a".repeat(64),
        dockerfile_hash: "b".repeat(64),
        args: BTreeMap::from([("VERSION".to_owned(), "01234567".to_owned())]),
        secret_ids: Vec::new(),
        credential_secret_ids: Vec::new(),
        dependencies: Vec::new(),
        upstream_node_ids: Vec::new(),
        extra_inputs: Vec::new(),
        version_embedded: false,
        reuse_policy: BuildReusePolicy::Allowed,
        input_fingerprint: String::new(),
        variants: vec![ImageBuildVariantPlan {
            target: None,
            tag_suffix: String::new(),
            tags: vec!["edge".to_owned(), format!("sha-{COMMIT}")],
            stage: None,
            args: BTreeMap::new(),
        }],
    };
    unit.input_fingerprint = unit_input_fingerprint(&unit, platforms)?;
    Ok(unit)
}

fn valid_plan() -> Result<ImageBuildPlan, String> {
    let platforms = vec!["linux/amd64".to_owned()];
    ImageBuildPlan::new(
        ImageBuildScope {
            project_id: ProjectId(uuid::Uuid::nil()),
            config_hash: "c".repeat(64),
            environment: "local".to_owned(),
        },
        source(Some("main"), &[], &[]),
        false,
        BuildTagStrategy::Auto,
        platforms.clone(),
        vec![valid_unit("app", "ghcr.io/example/app", &platforms)?],
    )
}

fn valid_image(plan: &ImageBuildPlan) -> Result<BuiltImage, String> {
    let unit = &plan.units[0];
    Ok(BuiltImage {
        module: unit.module.clone(),
        target: None,
        repository: unit.image.clone(),
        tags: unit.variants[0]
            .tags
            .iter()
            .map(|tag| format!("{}:{tag}", unit.image))
            .collect(),
        digest: format!("sha256:{}", "a".repeat(64)),
        execution_fingerprint: unit_execution_fingerprint(
            &unit.input_fingerprint,
            &plan.environment,
            None,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )?,
        secret_revisions: BTreeMap::new(),
        archive: Some(PathBuf::from("/tmp/app.oci.tar")),
        reused: false,
    })
}

fn result(plan: &ImageBuildPlan) -> Result<ImageBuildResult, String> {
    Ok(ImageBuildResult {
        operation_id: OperationId::new(),
        plan_hash: plan.plan_hash.clone(),
        images: vec![valid_image(plan)?],
        dependencies: Vec::new(),
    })
}

#[test]
fn plan_validation_rejects_every_top_level_identity_boundary() -> Result<(), String> {
    let plan = valid_plan()?;
    assert_eq!(plan.schema, PLAN_SCHEMA);
    plan.validate()?;

    macro_rules! rejects {
        ($change:expr, $message:literal) => {{
            let mut changed = plan.clone();
            $change(&mut changed);
            let error = match changed.validate() {
                Err(error) => error,
                Ok(()) => return Err("mutation must be rejected".to_owned()),
            };
            assert!(error.contains($message), "unexpected error: {error}");
        }};
    }

    rejects!(
        |plan: &mut ImageBuildPlan| plan.schema = "v2".to_owned(),
        "schema"
    );
    rejects!(
        |plan: &mut ImageBuildPlan| plan.source.commit = "short".to_owned(),
        "commit"
    );
    rejects!(
        |plan: &mut ImageBuildPlan| plan.config_hash = "short".to_owned(),
        "configuration hash"
    );
    rejects!(
        |plan: &mut ImageBuildPlan| plan.environment.clear(),
        "environment"
    );
    rejects!(
        |plan: &mut ImageBuildPlan| plan.environment = "bad\nname".to_owned(),
        "environment"
    );
    rejects!(
        |plan: &mut ImageBuildPlan| {
            plan.publish = true;
            plan.source.dirty = true
        },
        "dirty"
    );
    rejects!(
        |plan: &mut ImageBuildPlan| {
            plan.publish = true;
            plan.source.tags_complete = false
        },
        "tag inventory"
    );
    rejects!(
        |plan: &mut ImageBuildPlan| plan.units.clear(),
        "no build units"
    );
    rejects!(
        |plan: &mut ImageBuildPlan| plan.platforms.clear(),
        "no target platforms"
    );
    rejects!(
        |plan: &mut ImageBuildPlan| plan.platforms.push("linux/amd64".to_owned()),
        "duplicated"
    );
    rejects!(
        |plan: &mut ImageBuildPlan| plan.platforms[0] = "host".to_owned(),
        "invalid"
    );
    rejects!(
        |plan: &mut ImageBuildPlan| plan.source.tags_at_head =
            vec!["v1.0.0".to_owned(), "v2.0.0".to_owned()],
        "multiple different"
    );
    rejects!(
        |plan: &mut ImageBuildPlan| plan.plan_hash = "0".repeat(64),
        "plan hash"
    );
    Ok(())
}

#[test]
fn unit_identity_input_and_variant_validation_cover_each_rejected_class() -> Result<(), String> {
    let plan = valid_plan()?;
    let baseline = plan.units[0].clone();

    macro_rules! invalid_identity {
        ($change:expr, $message:literal) => {{
            let mut unit = baseline.clone();
            $change(&mut unit);
            let error = match validate_build_unit_identity(&unit, &mut BTreeSet::new(), "01234567")
            {
                Err(error) => error,
                Ok(()) => return Err("identity mutation must be rejected".to_owned()),
            };
            assert!(error.contains($message), "unexpected error: {error}");
        }};
    }
    invalid_identity!(
        |unit: &mut ImageBuildUnitPlan| unit.module = "Bad Name".to_owned(),
        "more than once"
    );
    let mut modules = BTreeSet::from([&baseline.module]);
    assert!(validate_build_unit_identity(&baseline, &mut modules, "01234567").is_err());
    invalid_identity!(
        |unit: &mut ImageBuildUnitPlan| unit.image = "repo:tag".to_owned(),
        "repository"
    );
    invalid_identity!(
        |unit: &mut ImageBuildUnitPlan| unit.context = PathBuf::from("../app"),
        "unsafe"
    );
    invalid_identity!(
        |unit: &mut ImageBuildUnitPlan| unit.context_hash = "short".to_owned(),
        "context hash"
    );
    invalid_identity!(
        |unit: &mut ImageBuildUnitPlan| unit.args.remove("VERSION"),
        "VERSION"
    );
    invalid_identity!(
        |unit: &mut ImageBuildUnitPlan| {
            unit.args.insert("bad-name".to_owned(), "x".to_owned());
        },
        "argument"
    );
    invalid_identity!(
        |unit: &mut ImageBuildUnitPlan| unit.secret_ids = vec!["bad name".to_owned()],
        "secret"
    );
    invalid_identity!(
        |unit: &mut ImageBuildUnitPlan| unit.credential_secret_ids = vec!["absent".to_owned()],
        "credential-only"
    );
    invalid_identity!(
        |unit: &mut ImageBuildUnitPlan| unit.secret_ids =
            vec!["token".to_owned(), "token".to_owned()],
        "secret"
    );

    let mut bad_input = baseline.clone();
    bad_input.extra_inputs.push(ImageBuildTrigger {
        path: PathBuf::from("../secret"),
        digest: "a".repeat(64),
    });
    assert!(plan.validate_build_inputs(&bad_input).is_err());
    bad_input = baseline.clone();
    bad_input.extra_inputs.push(ImageBuildTrigger {
        path: PathBuf::from("input"),
        digest: "short".to_owned(),
    });
    assert!(plan.validate_build_inputs(&bad_input).is_err());
    bad_input = baseline.clone();
    bad_input.input_fingerprint = "f".repeat(64);
    assert!(plan.validate_build_inputs(&bad_input).is_err());
    bad_input = baseline.clone();
    bad_input.variants.clear();
    bad_input.input_fingerprint = unit_input_fingerprint(&bad_input, &plan.platforms)?;
    assert!(plan.validate_build_inputs(&bad_input).is_err());

    let base_tags = automatic_tags(&plan.source, plan.tag_strategy)?;
    let expected = |variant: &ImageBuildVariantPlan| suffixed_tags(&base_tags, &variant.tag_suffix);
    let duplicated = |variant: &ImageBuildVariantPlan| {
        suffixed_tags(&["edge".to_owned(), "edge".to_owned()], &variant.tag_suffix)
    };
    let mut bad_variant = baseline.clone();
    bad_variant.variants[0].target = Some("bad target".to_owned());
    assert!(validate_build_variants(&bad_variant, &expected, &mut BTreeSet::new()).is_err());
    bad_variant = baseline.clone();
    bad_variant.variants.push(bad_variant.variants[0].clone());
    assert!(validate_build_variants(&bad_variant, &expected, &mut BTreeSet::new()).is_err());
    bad_variant = baseline.clone();
    bad_variant.variants[0].tags.clear();
    assert!(validate_build_variants(&bad_variant, &expected, &mut BTreeSet::new()).is_err());
    bad_variant = baseline.clone();
    bad_variant.variants[0].tags = vec!["other".to_owned()];
    assert!(validate_build_variants(&bad_variant, &expected, &mut BTreeSet::new()).is_err());
    bad_variant = baseline.clone();
    bad_variant.variants[0].tags = vec!["edge".to_owned(), "edge".to_owned()];
    assert!(validate_build_variants(&bad_variant, &duplicated, &mut BTreeSet::new()).is_err());
    let mut assigned = BTreeSet::from([(baseline.image.clone(), "edge".to_owned())]);
    assert!(validate_build_variants(&baseline, &expected, &mut assigned).is_err());
    Ok(())
}

#[test]
fn dependency_validation_rejects_identity_reference_order_and_ambiguity() -> Result<(), String> {
    let plan = valid_plan()?;
    let base = plan.units[0].clone();
    let mut web = valid_unit("web", "ghcr.io/example/web", &plan.platforms)?;
    let build_arg = BuildDependencyReference::BuildArgument {
        arg: "BASE_IMAGE".to_owned(),
        instruction_index: 7,
        source_line: 11,
    };
    let dockerfile = BuildDependencyReference::Dockerfile {
        original_reference: "ghcr.io/example/app:edge".to_owned(),
        instruction_index: 8,
        source_line: 12,
    };
    let valid_edge = FirstPartyBuildEdge {
        producer_module: "app".to_owned(),
        dependent_module: "web".to_owned(),
        stage: None,
        reference: build_arg.clone(),
    };
    assert_eq!(valid_edge.instruction_index(), 7);
    assert_eq!(valid_edge.source_line(), 11);
    let docker_edge = FirstPartyBuildEdge {
        reference: dockerfile,
        ..valid_edge.clone()
    };
    assert_eq!(docker_edge.instruction_index(), 8);
    assert_eq!(docker_edge.source_line(), 12);
    web.dependencies = vec![valid_edge.clone()];
    let mut graph = plan.clone();
    graph.units = vec![base, web.clone()];
    graph.validate_build_dependencies(&web, 1)?;

    macro_rules! rejects {
        ($change:expr) => {{
            let mut changed = web.clone();
            $change(&mut changed.dependencies[0]);
            assert!(graph.validate_build_dependencies(&changed, 1).is_err());
        }};
    }
    rejects!(|edge: &mut FirstPartyBuildEdge| edge.producer_module = "bad name".to_owned());
    rejects!(|edge: &mut FirstPartyBuildEdge| edge.producer_module = "web".to_owned());
    rejects!(|edge: &mut FirstPartyBuildEdge| edge.dependent_module = "other".to_owned());
    rejects!(|edge: &mut FirstPartyBuildEdge| edge.producer_module = "missing".to_owned());
    rejects!(|edge: &mut FirstPartyBuildEdge| edge.reference =
        BuildDependencyReference::BuildArgument {
            arg: "VERSION".to_owned(),
            instruction_index: 1,
            source_line: 1
        });
    rejects!(|edge: &mut FirstPartyBuildEdge| edge.reference =
        BuildDependencyReference::BuildArgument {
            arg: "BAD-NAME".to_owned(),
            instruction_index: 1,
            source_line: 1
        });
    rejects!(|edge: &mut FirstPartyBuildEdge| edge.reference =
        BuildDependencyReference::BuildArgument {
            arg: "BASE_IMAGE".to_owned(),
            instruction_index: 1,
            source_line: 0
        });
    rejects!(|edge: &mut FirstPartyBuildEdge| edge.reference =
        BuildDependencyReference::Dockerfile {
            original_reference: String::new(),
            instruction_index: 1,
            source_line: 1
        });
    rejects!(|edge: &mut FirstPartyBuildEdge| edge.reference =
        BuildDependencyReference::Dockerfile {
            original_reference: "base\nimage".to_owned(),
            instruction_index: 1,
            source_line: 1
        });
    rejects!(|edge: &mut FirstPartyBuildEdge| edge.reference =
        BuildDependencyReference::Dockerfile {
            original_reference: "base".to_owned(),
            instruction_index: 1,
            source_line: 0
        });
    assert!(graph.validate_build_dependencies(&web, 0).is_err());
    web.dependencies.push(valid_edge);
    assert!(graph.validate_build_dependencies(&web, 1).is_err());
    Ok(())
}

#[test]
fn build_results_validate_identity_output_revisions_and_execution_evidence() -> Result<(), String> {
    let plan = valid_plan()?;
    let valid = result(&plan)?;
    valid.validate_against(&plan)?;

    let mut changed = valid.clone();
    changed.plan_hash = "stale".to_owned();
    assert!(changed.validate_against(&plan).is_err());
    changed = valid.clone();
    changed.images.clear();
    assert!(changed.validate_against(&plan).is_err());

    macro_rules! rejects_image {
        ($change:expr) => {{
            let mut changed = valid.clone();
            $change(&mut changed.images[0]);
            assert!(changed.validate_images(&plan).is_err());
        }};
    }
    rejects_image!(|image: &mut BuiltImage| image.module = "unknown".to_owned());
    rejects_image!(|image: &mut BuiltImage| image.target = Some("unknown".to_owned()));
    rejects_image!(|image: &mut BuiltImage| image.repository = "other/repository".to_owned());
    rejects_image!(|image: &mut BuiltImage| image.tags.clear());
    rejects_image!(|image: &mut BuiltImage| image.digest = "mutable".to_owned());
    rejects_image!(|image: &mut BuiltImage| image.execution_fingerprint = "short".to_owned());
    rejects_image!(|image: &mut BuiltImage| image
        .secret_revisions
        .insert("unknown".to_owned(), "r1".to_owned()));
    rejects_image!(|image: &mut BuiltImage| image.archive = None);
    rejects_image!(|image: &mut BuiltImage| image.archive = Some(PathBuf::from("relative.tar")));
    rejects_image!(|image: &mut BuiltImage| image.archive = Some(PathBuf::new()));

    let mut duplicate_plan = plan.clone();
    duplicate_plan.units[0]
        .variants
        .push(ImageBuildVariantPlan {
            target: Some("other".to_owned()),
            tag_suffix: "other".to_owned(),
            tags: vec!["edge-other".to_owned(), format!("sha-{COMMIT}-other")],
            stage: None,
            args: BTreeMap::new(),
        });
    let mut duplicate = valid.clone();
    duplicate.images.push(duplicate.images[0].clone());
    assert!(duplicate.validate_images(&duplicate_plan).is_err());

    let mut revision_plan = plan.clone();
    revision_plan.units[0].secret_ids = vec!["token".to_owned()];
    revision_plan.units[0].reuse_policy = BuildReusePolicy::RevisionGated;
    let mut revision_image = valid.images[0].clone();
    assert!(
        ImageBuildResult {
            images: vec![revision_image.clone()],
            ..valid.clone()
        }
        .validate_images(&revision_plan)
        .is_err()
    );
    revision_image
        .secret_revisions
        .insert("token".to_owned(), "revision-1".to_owned());
    ImageBuildResult {
        images: vec![revision_image.clone()],
        ..valid.clone()
    }
    .validate_images(&revision_plan)?;
    // Credential-only secrets carry no revision evidence.
    let mut credential_plan = revision_plan.clone();
    credential_plan.units[0].secret_ids.push("auth".to_owned());
    credential_plan.units[0].credential_secret_ids = vec!["auth".to_owned()];
    ImageBuildResult {
        images: vec![revision_image.clone()],
        ..valid.clone()
    }
    .validate_images(&credential_plan)?;
    let mut credential_image = revision_image.clone();
    credential_image
        .secret_revisions
        .insert("auth".to_owned(), "revision-1".to_owned());
    assert!(
        ImageBuildResult {
            images: vec![credential_image],
            ..valid.clone()
        }
        .validate_images(&credential_plan)
        .is_err()
    );
    revision_image
        .secret_revisions
        .insert("token".to_owned(), "bad\nrevision".to_owned());
    assert!(
        ImageBuildResult {
            images: vec![revision_image],
            ..valid.clone()
        }
        .validate_images(&revision_plan)
        .is_err()
    );

    assert!(valid.validate_execution_fingerprints(&plan).is_ok());
    let mut bad_fingerprint = valid.clone();
    bad_fingerprint.images[0].execution_fingerprint = "f".repeat(64);
    assert!(
        bad_fingerprint
            .validate_execution_fingerprints(&plan)
            .is_err()
    );
    let mut unknown = valid.clone();
    unknown.images[0].module = "unknown".to_owned();
    assert!(unknown.validate_execution_fingerprints(&plan).is_err());
    let mut invalid_input = plan.clone();
    invalid_input.units[0].input_fingerprint = "short".to_owned();
    assert!(
        valid
            .validate_execution_fingerprints(&invalid_input)
            .is_err()
    );
    Ok(())
}

#[test]
fn dependency_evidence_requires_exact_unique_immutable_upstream_digests() -> Result<(), String> {
    let plan = valid_plan()?;
    let base = plan.units[0].clone();
    let mut web = valid_unit("web", "ghcr.io/example/web", &plan.platforms)?;
    let edge = FirstPartyBuildEdge {
        producer_module: "app".to_owned(),
        dependent_module: "web".to_owned(),
        stage: None,
        reference: BuildDependencyReference::Dockerfile {
            original_reference: "ghcr.io/example/app:edge".to_owned(),
            instruction_index: 1,
            source_line: 1,
        },
    };
    web.dependencies.push(edge.clone());
    let mut graph = plan.clone();
    graph.units = vec![base, web];
    let upstream = valid_image(&plan)?;
    let digest = upstream.digest.clone();
    let evidence = ResolvedBuildDependency {
        edge,
        dependent_target: None,
        digest: digest.clone(),
        immutable_reference: format!("ghcr.io/example/app@{digest}"),
    };
    let valid_result = ImageBuildResult {
        operation_id: OperationId::new(),
        plan_hash: graph.plan_hash.clone(),
        images: vec![upstream],
        dependencies: vec![evidence.clone()],
    };
    valid_result.validate_dependency_evidence(&graph)?;

    let mut changed = valid_result.clone();
    changed.dependencies.clear();
    assert!(changed.validate_dependency_evidence(&graph).is_err());
    changed = valid_result.clone();
    changed.dependencies[0].digest = "invalid".to_owned();
    assert!(changed.validate_dependency_evidence(&graph).is_err());
    changed = valid_result.clone();
    changed.dependencies[0].immutable_reference = "mutable:latest".to_owned();
    assert!(changed.validate_dependency_evidence(&graph).is_err());
    changed = valid_result.clone();
    changed.dependencies.push(evidence.clone());
    assert!(changed.validate_dependency_evidence(&graph).is_err());
    changed = valid_result.clone();
    changed.images[0].digest = format!("sha256:{}", "b".repeat(64));
    assert!(changed.validate_dependency_evidence(&graph).is_err());
    changed = valid_result.clone();
    changed.images.clear();
    assert!(changed.validate_dependency_evidence(&graph).is_err());

    let mut variant_graph = graph.clone();
    variant_graph.units[1].variants[0].target = Some("arm64".to_owned());
    let mut variants = valid_result;
    variants.images[0].target = Some("arm64".to_owned());
    variants.dependencies[0].dependent_target = Some("arm64".to_owned());
    let mut fallback = variants.images[0].clone();
    fallback.target = None;
    variants.images.push(fallback);
    variants.validate_dependency_evidence(&variant_graph)?;
    Ok(())
}

mod fingerprint_contracts;

/// The plan `environment` makes from `receipt`'s units after `change`.
fn plan_for(
    receipt: &BuildReceipt,
    environment: &str,
    change: impl Fn(&mut ImageBuildUnitPlan),
) -> Result<ImageBuildPlan, String> {
    let mut units = receipt.plan.units.clone();
    for unit in &mut units {
        change(unit);
        unit.input_fingerprint = unit_input_fingerprint(unit, &receipt.plan.platforms)?;
    }
    ImageBuildPlan::new(
        ImageBuildScope {
            project_id: receipt.plan.project_id,
            config_hash: "e".repeat(64),
            environment: environment.to_owned(),
        },
        receipt.plan.source.clone(),
        true,
        receipt.plan.tag_strategy,
        receipt.plan.platforms.clone(),
        units,
    )
}

/// `plan` published with every variant at one digest.
fn published(plan: ImageBuildPlan) -> Result<BuildReceipt, String> {
    let mut images = Vec::new();
    for unit in &plan.units {
        for variant in &unit.variants {
            images.push(BuiltImage {
                module: unit.module.clone(),
                target: variant.target.clone(),
                repository: unit.image.clone(),
                tags: variant
                    .tags
                    .iter()
                    .map(|tag| format!("{}:{tag}", unit.image))
                    .collect(),
                digest: format!("sha256:{}", "d".repeat(64)),
                execution_fingerprint: unit_execution_fingerprint(
                    &unit.input_fingerprint,
                    &plan.environment,
                    Some(variant),
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                )?,
                secret_revisions: BTreeMap::new(),
                archive: None,
                reused: false,
            });
        }
    }
    let result = ImageBuildResult {
        operation_id: OperationId::new(),
        plan_hash: plan.plan_hash.clone(),
        images,
        dependencies: Vec::new(),
    };
    BuildReceipt::new(plan, result)
}

#[test]
fn receipts_promote_to_environments_that_build_the_same_units() -> Result<(), String> {
    let project = ProjectId(uuid::Uuid::from_u128(4));
    let receipt = crate::test_support::build_receipt(project, "ci", COMMIT)?;

    let production = plan_for(&receipt, "production", |_| {})?;
    let promoted = receipt.promote(&production)?;
    assert_eq!(promoted.origin, receipt.origin());
    assert_eq!(promoted.origin.environment, "ci");
    let rebound = &promoted.receipt;
    rebound.validate()?;
    assert_eq!(rebound.plan.environment, "production");
    assert_eq!(rebound.plan.config_hash, production.config_hash);
    assert_eq!(rebound.plan.units, receipt.plan.units);
    assert_eq!(
        rebound.result.images[0].digest,
        receipt.result.images[0].digest
    );
    // Reuse is keyed by environment, so the promoted build has its own
    // execution fingerprint.
    assert_ne!(
        rebound.result.images[0].execution_fingerprint,
        receipt.result.images[0].execution_fingerprint
    );
    assert_eq!(
        receipt.unpromoted()?.receipt,
        receipt,
        "a receipt for its own environment is used unchanged"
    );

    // Credential-only secrets do not block promotion.
    let credential = published(plan_for(&receipt, "ci", |unit| {
        unit.secret_ids = vec!["composer_auth".to_owned()];
        unit.credential_secret_ids = vec!["composer_auth".to_owned()];
    })?)?;
    credential.promote(&plan_for(&credential, "production", |_| {})?)?;
    Ok(())
}

#[test]
fn receipts_do_not_promote_when_a_module_builds_differently() -> Result<(), String> {
    let project = ProjectId(uuid::Uuid::from_u128(4));
    let receipt = crate::test_support::build_receipt(project, "ci", COMMIT)?;
    let refused = |current: ImageBuildPlan, expected: &str| -> Result<(), String> {
        match receipt.promote(&current) {
            Ok(_) => Err(format!("promotion must be refused: {expected}")),
            Err(message) => {
                assert!(
                    message.contains("module 'api'")
                        && message.contains(expected)
                        && message
                            .contains("from environment 'ci' cannot be deployed to 'production'"),
                    "unexpected refusal: {message}"
                );
                Ok(())
            }
        }
    };
    refused(
        plan_for(&receipt, "production", |unit| {
            unit.args.insert("FEATURE".to_owned(), "on".to_owned());
        })?,
        "build argument FEATURE",
    )?;
    refused(
        plan_for(&receipt, "production", |unit| {
            unit.variants[0].target = Some("debug".to_owned());
        })?,
        "build variant debug here, default in the receipt",
    )?;
    refused(
        plan_for(&receipt, "production", |unit| {
            unit.context_hash = "f".repeat(64);
        })?,
        "build context contents",
    )?;
    refused(
        plan_for(&receipt, "production", |unit| {
            unit.image = "registry.example/other".to_owned();
        })?,
        "image repository",
    )?;
    let mut missing = plan_for(&receipt, "production", |_| {})?;
    missing.units[0].module = "worker".to_owned();
    assert!(
        receipt
            .check_promotion(&missing)
            .is_err_and(|message| message.contains("module 'worker' is not in the build receipt"))
    );
    let mut other_commit = plan_for(&receipt, "production", |_| {})?;
    other_commit.source.commit = "f".repeat(40);
    assert!(receipt.check_promotion(&other_commit).is_err());
    let mut other_project = plan_for(&receipt, "production", |_| {})?;
    other_project.project_id = ProjectId(uuid::Uuid::from_u128(5));
    assert!(receipt.check_promotion(&other_project).is_err());

    // A build secret's value can differ between environments.
    let secret = published(plan_for(&receipt, "ci", |unit| {
        unit.secret_ids = vec!["token".to_owned()];
        unit.reuse_policy = BuildReusePolicy::Forbidden;
    })?)?;
    let production = plan_for(&secret, "production", |_| {})?;
    assert!(secret.promote(&production).is_err_and(|message| {
        message.contains("mounts build secrets") && message.contains("token")
    }));
    Ok(())
}
