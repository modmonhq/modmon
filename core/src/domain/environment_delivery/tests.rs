use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{
    EnvironmentDeliveryMode, EnvironmentDeliveryPolicy, EnvironmentDeliveryProvenance,
    resolve_environment_delivery,
};
use crate::domain::config::{
    EnvironmentConfig, MmConfig, ProjectConfig, RuntimeConfig, StackConfig,
};
use crate::domain::workspace::ProjectId;

fn config(
    development: Option<&str>,
    policy: EnvironmentDeliveryPolicy,
    environments: &[(&str, Option<EnvironmentDeliveryMode>)],
) -> MmConfig {
    MmConfig {
        schema_version: crate::domain::config::CONFIG_SCHEMA_VERSION,
        project: ProjectConfig {
            id: ProjectId(uuid::Uuid::nil()),
            name: "example".to_owned(),
            development_environment: development.map(str::to_owned),
        },
        default_environment: Some("local".to_owned()),
        target: "local".to_owned(),
        runtime: RuntimeConfig::default(),
        build: crate::domain::config::BuildConfig::default(),
        networks: BTreeMap::new(),
        stacks: [(
            "app".to_owned(),
            StackConfig {
                project_name: None,
                files: vec![PathBuf::from("compose.yaml")],
                networks: Vec::new(),
                ingress: Default::default(),
                shared_volumes: Vec::new(),
                after: Vec::new(),
                storage: None,
            },
        )]
        .into_iter()
        .collect(),
        modules: BTreeMap::new(),
        providers: BTreeMap::new(),
        secrets: BTreeMap::new(),
        bindings: BTreeMap::new(),
        environment_delivery: policy,
        environments: environments
            .iter()
            .map(|(name, delivery)| {
                (
                    (*name).to_owned(),
                    EnvironmentConfig {
                        environment_delivery: *delivery,
                        ..EnvironmentConfig::default()
                    },
                )
            })
            .collect(),
        ingress: Default::default(),
        development_mounts: Default::default(),
        development_mail: Default::default(),
        jobs: Default::default(),
        schedules: Default::default(),
        gates: Default::default(),
        users: Default::default(),
        agents: Default::default(),
        targets: Default::default(),
        registries: Default::default(),
        hardening: None,
        release_inputs: BTreeMap::new(),
        host_runtimes: None,
        release: None,
        development: Default::default(),
    }
}

#[test]
fn unspecified_policy_uses_the_current_direct_default() {
    let config = config(
        None,
        EnvironmentDeliveryPolicy::default(),
        &[("local", None), ("production", None)],
    );
    let local = resolve_environment_delivery(&config, "local");
    let production = resolve_environment_delivery(&config, "production");
    assert_eq!(local.mode, EnvironmentDeliveryMode::Direct);
    assert_eq!(
        local.provenance,
        EnvironmentDeliveryProvenance::BuiltInDefault
    );
    assert_eq!(production.mode, EnvironmentDeliveryMode::Direct);
    assert_eq!(
        production.provenance,
        EnvironmentDeliveryProvenance::BuiltInDefault
    );
}

#[test]
fn development_identity_defaults_to_env_file_and_others_to_direct() {
    let config = config(
        Some("workstation"),
        EnvironmentDeliveryPolicy::default(),
        &[("workstation", None), ("production", None)],
    );
    let development = resolve_environment_delivery(&config, "workstation");
    let production = resolve_environment_delivery(&config, "production");
    assert_eq!(development.mode, EnvironmentDeliveryMode::EnvFile);
    assert_eq!(
        development.provenance,
        EnvironmentDeliveryProvenance::BuiltInDefault
    );
    assert_eq!(production.mode, EnvironmentDeliveryMode::Direct);
    assert_eq!(
        production.provenance,
        EnvironmentDeliveryProvenance::BuiltInDefault
    );
}

#[test]
fn renaming_development_environment_moves_policy_by_identity() {
    let before = config(
        Some("local"),
        EnvironmentDeliveryPolicy::default(),
        &[("local", None), ("preview", None)],
    );
    let after = config(
        Some("preview"),
        EnvironmentDeliveryPolicy::default(),
        &[("local", None), ("preview", None)],
    );
    assert_eq!(
        resolve_environment_delivery(&before, "local").mode,
        EnvironmentDeliveryMode::EnvFile
    );
    assert_eq!(
        resolve_environment_delivery(&before, "preview").mode,
        EnvironmentDeliveryMode::Direct
    );
    assert_eq!(
        resolve_environment_delivery(&after, "local").mode,
        EnvironmentDeliveryMode::Direct
    );
    assert_eq!(
        resolve_environment_delivery(&after, "preview").mode,
        EnvironmentDeliveryMode::EnvFile
    );
}

#[test]
fn explicit_environment_override_wins() {
    let config = config(
        Some("local"),
        EnvironmentDeliveryPolicy {
            default: Some(EnvironmentDeliveryMode::Direct),
            development_default: Some(EnvironmentDeliveryMode::EnvFile),
        },
        &[
            ("local", Some(EnvironmentDeliveryMode::Direct)),
            ("ci", Some(EnvironmentDeliveryMode::EnvFile)),
        ],
    );
    let local = resolve_environment_delivery(&config, "local");
    let ci = resolve_environment_delivery(&config, "ci");
    assert_eq!(local.mode, EnvironmentDeliveryMode::Direct);
    assert_eq!(
        local.provenance,
        EnvironmentDeliveryProvenance::EnvironmentOverride
    );
    assert_eq!(ci.mode, EnvironmentDeliveryMode::EnvFile);
    assert_eq!(
        ci.provenance,
        EnvironmentDeliveryProvenance::EnvironmentOverride
    );
}

#[test]
fn policy_defaults_apply_in_documented_order() {
    let config = config(
        Some("local"),
        EnvironmentDeliveryPolicy {
            default: Some(EnvironmentDeliveryMode::Direct),
            development_default: Some(EnvironmentDeliveryMode::EnvFile),
        },
        &[("local", None), ("integration", None)],
    );
    let local = resolve_environment_delivery(&config, "local");
    let integration = resolve_environment_delivery(&config, "integration");
    assert_eq!(local.mode, EnvironmentDeliveryMode::EnvFile);
    assert_eq!(
        local.provenance,
        EnvironmentDeliveryProvenance::DevelopmentDefault
    );
    assert_eq!(integration.mode, EnvironmentDeliveryMode::Direct);
    assert_eq!(
        integration.provenance,
        EnvironmentDeliveryProvenance::PolicyDefault
    );
}

#[test]
fn global_policy_precedes_the_built_in_development_fallback() {
    let config = config(
        Some("workstation"),
        EnvironmentDeliveryPolicy {
            default: Some(EnvironmentDeliveryMode::Direct),
            development_default: None,
        },
        &[("workstation", None)],
    );
    let delivery = resolve_environment_delivery(&config, "workstation");
    assert_eq!(delivery.mode, EnvironmentDeliveryMode::Direct);
    assert_eq!(
        delivery.provenance,
        EnvironmentDeliveryProvenance::PolicyDefault
    );
}

#[test]
fn hard_coded_names_are_not_special() {
    let config = config(
        Some("staging"),
        EnvironmentDeliveryPolicy::default(),
        &[("local", None), ("dev", None), ("staging", None)],
    );
    assert_eq!(
        resolve_environment_delivery(&config, "local").mode,
        EnvironmentDeliveryMode::Direct
    );
    assert_eq!(
        resolve_environment_delivery(&config, "dev").mode,
        EnvironmentDeliveryMode::Direct
    );
    assert_eq!(
        resolve_environment_delivery(&config, "staging").mode,
        EnvironmentDeliveryMode::EnvFile
    );
}
