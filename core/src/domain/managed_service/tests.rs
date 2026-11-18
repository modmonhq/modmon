use super::{
    MANAGED_SERVICE_SCHEMA, ManagedServiceFile, ManagedServiceIdentity, ManagedServicePlan,
    ManagedServiceSpec, safe_file_name,
};
use crate::domain::runtime::RuntimeKind;
use crate::domain::target::TargetId;
use std::path::PathBuf;
use uuid::Uuid;

fn plan() -> Result<ManagedServicePlan, String> {
    let mut plan = ManagedServicePlan {
        schema: MANAGED_SERVICE_SCHEMA,
        identity: ManagedServiceIdentity {
            target: "local".to_owned(),
            target_id: TargetId(Uuid::from_u128(1)),
            target_fingerprint: "a".repeat(64),
            runtime: RuntimeKind::DockerCompose,
            provider_instance: "vault".to_owned(),
            component: "server".to_owned(),
            project_name: "mm-vault".to_owned(),
            service_name: "vault".to_owned(),
            data_volume: "data".to_owned(),
            config_volume: "config".to_owned(),
            ingress_network: "ingress".to_owned(),
            ingress_config_volume: "ingress-config".to_owned(),
        },
        service: ManagedServiceSpec {
            image: "vault@sha256:abc".to_owned(),
            host_port: 8200,
            container_port: 8200,
            user: "1000:1000".to_owned(),
            entrypoint: "vault".to_owned(),
            command: vec!["server".to_owned()],
            healthcheck: vec!["vault".to_owned(), "status".to_owned()],
            config_mount: "/config".to_owned(),
            data_mount: "/data".to_owned(),
            config_files: vec![ManagedServiceFile {
                source: PathBuf::from("/workspace/config.hcl"),
                name: "config.hcl".to_owned(),
                mode: 0o600,
                uid: 1000,
                gid: 1000,
            }],
            ingress_files: Vec::new(),
            generation: "v1".to_owned(),
        },
        artifact_root: PathBuf::from("/workspace/artifact"),
        compose_path: PathBuf::from("/workspace/artifact/compose.yaml"),
        plan_hash: String::new(),
    };
    plan.plan_hash = plan.canonical_hash()?;
    Ok(plan)
}

#[test]
fn canonical_plan_validation_and_file_name_boundaries_are_executable_in_the_unit_target()
-> Result<(), String> {
    let valid = plan()?;
    valid.validate()?;
    assert!(safe_file_name("config.hcl"));
    assert!(!safe_file_name(&"a".repeat(256)));
    let mut stale = valid;
    stale.service.generation.push_str("-changed");
    assert!(stale.validate().is_err());
    Ok(())
}
