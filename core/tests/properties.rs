use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use chrono::{TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use mm::adapters::config::MmConfig;
use mm::adapters::remote::{decode_frame, encode_frame};
use mm::app::ports::CommandSpec;
use mm::domain::access::{
    AccessAction, AccessPolicy, AccessRequest, AccessResourceKind, BuiltInRole, Capability, Denial,
    Grant, Principal, PrincipalId, PrincipalKey, RoleRef, ScopeMatcher,
};
use mm::domain::ca::normalize_hostname;
use mm::domain::config::{EnvScalar, EnvironmentBinding, ValueBinding};
use mm::domain::config_resolve::{
    effective_binding, effective_bindings, route_path_contains, route_paths_overlap,
};
use mm::domain::development_mount::HostPlatform;
use mm::domain::local_ingress::ProjectSlug;
use mm::domain::remote::{PROTOCOL_VERSION, RemotePlanBinding};
use mm::domain::runtime::RuntimeKind;
use mm::domain::scheduler::{apply_jitter, definition_hash, next_occurrence, parse_duration};
use mm::domain::secret::SecretMaterial;
use mm::domain::workspace::ProjectId;
use proptest::collection::{btree_map, vec};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed, TestCaseError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct TestFrame {
    name: String,
    values: Vec<u16>,
}

fn minimal_config() -> Result<MmConfig, String> {
    serde_yaml_ng::from_str(
        "schema-version: 1\nproject:\n  id: 00000000-0000-0000-0000-000000000001\n  name: properties\ntarget: local\nstacks: {}\nenvironments:\n  local:\n    bindings: {}\n",
    )
    .map_err(|error| error.to_string())
}

fn value_binding(value: u16) -> EnvironmentBinding {
    EnvironmentBinding::Value(ValueBinding {
        value: EnvScalar::Integer(i64::from(value)),
    })
}

fn remote_binding() -> RemotePlanBinding {
    RemotePlanBinding {
        protocol_version: PROTOCOL_VERSION,
        daemon_version: "0.1.0".to_owned(),
        daemon_fingerprint: "a".repeat(64),
        target_fingerprint: "b".repeat(64),
        target: "production".to_owned(),
        node: None,
        kind: mm::domain::remote::RemotePlanKind::Deployment,
        project_id: ProjectId(Uuid::from_u128(1)),
        environment: "production".to_owned(),
        runtime: RuntimeKind::DockerCompose,
        commit: "c".repeat(40),
        deployment_plan_hash: "c".repeat(64),
        access_change_hash: "c".repeat(64),
        principal_id: PrincipalId(Uuid::from_u128(2)),
        authorization_revision: 1,
        authorization_hash: "d".repeat(64),
        plan_nonce: Uuid::from_u128(3),
        expires_at_unix_ms: 1_000_000,
        bundle_hash: None,
        plan_hash: String::new(),
    }
}

fn scoped_policy(environments: BTreeSet<String>, denied: bool) -> AccessPolicy {
    let principal_id = PrincipalId(Uuid::from_u128(1));
    let scope = ScopeMatcher {
        project_id: ProjectId(Uuid::from_u128(2)),
        environments,
        services: BTreeSet::new(),
        jobs: BTreeSet::new(),
        gates: BTreeSet::new(),
    };
    let capability = Capability::new(AccessAction::Status, AccessResourceKind::Project);
    AccessPolicy {
        schema_version: mm::domain::access::ACCESS_POLICY_SCHEMA_VERSION,
        revision: 1,
        principals: vec![Principal {
            id: principal_id,
            display_name: "Property Principal".to_owned(),
            project_id: None,
            environment: None,
            disabled: false,
            keys: vec![PrincipalKey {
                fingerprint: "SHA256:property".to_owned(),
                public_key: String::new(),
                disabled: false,
                expires_at_unix_ms: None,
            }],
        }],
        roles: BTreeMap::new(),
        grants: vec![Grant {
            id: Uuid::from_u128(10),
            principal_id,
            role: RoleRef::BuiltIn(BuiltInRole::Viewer),
            scope: scope.clone(),
        }],
        denials: denied
            .then(|| Denial {
                id: Uuid::from_u128(11),
                principal_id,
                capabilities: BTreeSet::from([capability]),
                scope,
            })
            .into_iter()
            .collect(),
    }
}

proptest! {
    #![proptest_config(Config {
        cases: 256,
        rng_seed: RngSeed::Fixed(0x4d4f_444d_4f4e_3235),
        failure_persistence: None,
        ..Config::default()
    })]

    #[test]
    fn protocol_frames_round_trip(frame in ("[ -~]{0,256}", vec(any::<u16>(), 0..128))
        .prop_map(|(name, values)| TestFrame { name, values })) {
        let encoded = encode_frame(&frame).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let decoded: TestFrame = decode_frame(&encoded).map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(decoded, frame);
    }

    #[test]
    fn arbitrary_protocol_input_is_canonical_when_valid(bytes in vec(any::<u8>(), 0..2048)) {
        if let Ok(value) = decode_frame::<serde_json::Value>(&bytes) {
            let canonical = encode_frame(&value).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let decoded: serde_json::Value = decode_frame(&canonical).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(decoded, value);
        }
    }

    #[test]
    fn slug_normalization_is_idempotent(value in "[ -~]{0,96}") {
        if let Ok(slug) = ProjectSlug::normalize(&value) {
            let normalized = slug.as_str();
            prop_assert!(normalized.len() <= 63);
            prop_assert!(normalized.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'));
            prop_assert!(!normalized.starts_with('-'));
            prop_assert!(!normalized.ends_with('-'));
            prop_assert!(!normalized.contains("--"));
            prop_assert_ne!(normalized, "mm");
            let repeated = ProjectSlug::normalize(normalized)
                .map_err(TestCaseError::fail)?;
            prop_assert_eq!(repeated.as_str(), normalized);
        }
    }

    #[test]
    fn host_path_and_route_canonicalization_is_idempotent(
        labels in vec("[a-z0-9]{1,12}", 1..6),
        path_segments in vec("[a-z0-9]{1,12}", 1..8),
        extra_segments in vec("[a-z0-9]{1,12}", 0..5),
    ) {
        let host = labels.join(".");
        let decorated = format!("  {}. ", host.to_ascii_uppercase());
        let normalized = normalize_hostname(&decorated).map_err(TestCaseError::fail)?;
        prop_assert_eq!(&normalized, &host);
        prop_assert_eq!(normalize_hostname(&normalized).map_err(TestCaseError::fail)?, host);

        let portable = path_segments.join("/");
        let windows_spelling = portable.replace('/', "\\");
        let normalized_path = HostPlatform::Windows
            .normalize_relative(&windows_spelling)
            .map_err(TestCaseError::fail)?;
        prop_assert_eq!(&normalized_path, &PathBuf::from(&portable));
        prop_assert_eq!(
            HostPlatform::Unix.normalize_relative(&portable).map_err(TestCaseError::fail)?,
            normalized_path,
        );

        let prefix = format!("/{portable}");
        let candidate = if extra_segments.is_empty() {
            prefix.clone()
        } else {
            format!("{prefix}/{}", extra_segments.join("/"))
        };
        prop_assert!(route_path_contains(&prefix, &candidate));
        prop_assert_eq!(
            route_paths_overlap(&prefix, &candidate),
            route_paths_overlap(&candidate, &prefix),
        );
        let sibling = format!("{prefix}-sibling");
        prop_assert!(!route_path_contains(&sibling, &candidate));
    }

    #[test]
    fn environment_bindings_have_exact_override_precedence(
        project in btree_map("[A-Z][A-Z0-9_]{0,12}", any::<u16>(), 0..32),
        environment in btree_map("[A-Z][A-Z0-9_]{0,12}", any::<u16>(), 0..32),
    ) {
        let mut config = minimal_config().map_err(TestCaseError::fail)?;
        config.bindings = project.iter().map(|(key, value)| (key.clone(), value_binding(*value))).collect();
        config.environments.get_mut("local").ok_or_else(|| TestCaseError::fail("missing local fixture"))?.bindings =
            environment.iter().map(|(key, value)| (key.clone(), value_binding(*value))).collect();

        let effective = effective_bindings(&config, "local");
        let expected_keys = project.keys().chain(environment.keys()).collect::<BTreeSet<_>>();
        prop_assert_eq!(effective.len(), expected_keys.len());
        for key in expected_keys {
            let expected = environment.get(key).or_else(|| project.get(key)).copied();
            let actual = effective_binding(&config, "local", key)
                .and_then(EnvironmentBinding::value)
                .map(ToString::to_string);
            prop_assert_eq!(actual, expected.map(|value| value.to_string()));
        }
    }

    #[test]
    fn access_scope_expansion_is_monotonic_and_explicit_deny_wins(
        environment in "[a-z][a-z0-9-]{0,20}",
        additional in proptest::collection::btree_set("[a-z][a-z0-9-]{0,20}", 0..16),
    ) {
        let capability = Capability::new(AccessAction::Status, AccessResourceKind::Project);
        let request = AccessRequest {
            principal_id: PrincipalId(Uuid::from_u128(1)),
            capability,
            project_id: ProjectId(Uuid::from_u128(2)),
            environment: &environment,
            service: None,
            job: None,
            gate: None,
        };
        let minimum = scoped_policy(BTreeSet::from([environment.clone()]), false);
        let mut expanded_scope = additional;
        expanded_scope.insert(environment.clone());
        let expanded = scoped_policy(expanded_scope.clone(), false);
        let minimum_authorization = minimum.authorize(&request).map_err(TestCaseError::fail)?;
        let expanded_authorization = expanded.authorize(&request).map_err(TestCaseError::fail)?;
        prop_assert_eq!(minimum_authorization.authorization_hash, expanded_authorization.authorization_hash);

        let denied = scoped_policy(expanded_scope, true);
        let denial = denied.authorize(&request);
        prop_assert_eq!(
            denial.as_ref().map_err(String::as_str),
            Err("an explicit denial matches this capability and scope"),
        );
    }

    #[test]
    fn every_remote_authority_field_changes_the_plan_binding(field in 0_usize..14) {
        let original = remote_binding();
        let original_hash = original.canonical_hash().map_err(TestCaseError::fail)?;
        let mut changed = original;
        match field {
            0 => changed.daemon_version.push_str("+changed"),
            1 => changed.daemon_fingerprint = "e".repeat(64),
            2 => changed.target_fingerprint = "f".repeat(64),
            3 => changed.target.push_str("-changed"),
            4 => changed.commit = "d".repeat(40),
            5 => changed.project_id = ProjectId(Uuid::from_u128(10)),
            6 => changed.environment.push_str("-changed"),
            7 => changed.runtime = RuntimeKind::PodmanCompose,
            8 => changed.deployment_plan_hash = "1".repeat(64),
            9 => changed.principal_id = PrincipalId(Uuid::from_u128(20)),
            10 => changed.authorization_revision += 1,
            11 => changed.authorization_hash = "2".repeat(64),
            12 => changed.plan_nonce = Uuid::from_u128(30),
            _ => changed.expires_at_unix_ms += 1,
        }
        prop_assert_ne!(changed.canonical_hash().map_err(TestCaseError::fail)?, original_hash);
    }

    #[test]
    fn unknown_configuration_mutations_are_rejected(field in "x-[a-z]{1,24}") {
        let source = format!(
            "schema-version: 1\ndefault-environment: local\nproject:\n  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3\n  name: properties\ntarget: local\nstacks: {{}}\nenvironments:\n  local:\n    bindings: {{}}\n{field}: true\n"
        );
        prop_assert!(serde_yaml_ng::from_str::<MmConfig>(&source).is_err());
    }

    #[test]
    fn definition_hashes_are_stable_and_content_sensitive(
        entries in btree_map("[a-z]{1,16}", any::<u64>(), 0..64),
    ) {
        let forward = entries.clone();
        let reverse: BTreeMap<_, _> = entries.iter().rev().map(|(key, value)| (key.clone(), *value)).collect();
        let forward_hash = definition_hash(&forward).map_err(TestCaseError::fail)?;
        let reverse_hash = definition_hash(&reverse).map_err(TestCaseError::fail)?;
        prop_assert_eq!(&forward_hash, &reverse_hash);

        let mut changed = forward.clone();
        changed.insert("property-sentinel".to_owned(), u64::MAX);
        let changed_hash = definition_hash(&changed).map_err(TestCaseError::fail)?;
        prop_assert_ne!(forward_hash, changed_hash);
    }

    #[test]
    fn durations_and_jitter_remain_bounded(
        amount in 0_u64..=1_000_000,
        unit in prop::sample::select(vec![("ms", 1_u64), ("s", 1_000), ("m", 60_000), ("h", 3_600_000), ("d", 86_400_000)]),
        identity in "[ -~]{0,64}",
        due in 0_i64..=i64::MAX / 2,
        maximum_ms in 0_u64..=86_400_000,
    ) {
        let duration = parse_duration(&format!("{amount}{}", unit.0))
            .map_err(TestCaseError::fail)?;
        let expected = amount * unit.1;
        prop_assert_eq!(duration, Duration::from_millis(expected));

        let jittered = apply_jitter(&identity, due, Duration::from_millis(maximum_ms));
        let repeated = apply_jitter(&identity, due, Duration::from_millis(maximum_ms));
        prop_assert!(jittered >= due);
        let maximum_ms = i64::try_from(maximum_ms)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert!(jittered <= due + maximum_ms);
        prop_assert_eq!(jittered, repeated);
    }

    #[test]
    fn stepped_minute_schedules_follow_an_independent_timezone_arithmetic_oracle(
        after_minute in -10_000_000_i64..10_000_000,
        second_offset in 0_i64..60_000,
        step in 1_u8..=30,
        timezone in prop::sample::select(vec![
            "UTC",
            "America/New_York",
            "Europe/London",
            "Pacific/Kiritimati",
        ]),
    ) {
        let after = after_minute * 60_000 + second_offset;
        let expression = format!("*/{step} * * * *");
        let actual = next_occurrence(&expression, timezone, after, None, None)
            .map_err(TestCaseError::fail)?
            .ok_or_else(|| TestCaseError::fail("bounded minute schedule had no occurrence"))?;

        let zone = timezone.parse::<Tz>().map_err(|error| TestCaseError::fail(error.to_string()))?;
        let mut expected = after.div_euclid(60_000) * 60_000 + 60_000;
        while Utc
            .timestamp_millis_opt(expected)
            .single()
            .ok_or_else(|| TestCaseError::fail("generated timestamp left the calendar range"))?
            .with_timezone(&zone)
            .minute()
            % u32::from(step)
            != 0
        {
            expected += 60_000;
        }
        prop_assert_eq!(actual, expected);
        prop_assert!(actual > after);
        prop_assert_eq!(actual.rem_euclid(60_000), 0);
    }

    #[test]
    fn secret_material_never_appears_in_debug(value in "[ -~]{1,512}") {
        let material = SecretMaterial::new(value.clone()).map_err(TestCaseError::fail)?;
        let debug = format!("{material:?}");
        prop_assert_eq!(&debug, "SecretMaterial([REDACTED])");
        prop_assert!(!debug.contains(&value));
        prop_assert_eq!(material.expose(str::to_owned), value);
    }

    #[test]
    fn sensitive_command_records_never_emit_the_sentinel(value in "[!-~]{1,512}") {
        let record = CommandSpec::new("tool")
            .sensitive_arg(value.clone())
            .sensitive_environment("TOKEN", value.clone())
            .redacted_record();
        let encoded = serde_json::to_string(&record).map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert!(!encoded.contains(&value));
        prop_assert_eq!(record.arguments, vec!["[REDACTED]".to_owned()]);
        prop_assert_eq!(record.environment_keys, vec!["TOKEN".to_owned()]);
    }
}
