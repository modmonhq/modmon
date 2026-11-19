use std::collections::BTreeMap;

use mm::domain::config::{
    CastBinding, ConfigDocument, EnvironmentBinding, MmConfig, SecretBinding, SecretRef,
    ServiceRole, ValueCast, resolve_environment,
};
use mm::domain::development_mail::{
    DevelopmentMailPolicy, DevelopmentMailValue, configured_bindings,
};
use mm::domain::environment_delivery::{EnvironmentDeliveryMode, EnvironmentDeliveryProvenance};
use mm::domain::ingress::IngressRouteProvenance;
use mm::domain::managed_service::{
    MANAGED_SERVICE_SCHEMA, ManagedServiceFile, ManagedServiceIdentity, ManagedServicePlan,
    ManagedServiceSpec,
};
use mm::domain::operation::OperationId;
use mm::domain::runtime::RuntimeKind;
use mm::domain::secret::SecretReference;
use mm::domain::target::TargetId;
use mm::domain::target_ingress::{
    TargetIngressStep, ingress_acme_volume_name, ingress_dynamic_config_volume_name,
    ingress_network_name, ingress_project_name,
};
use uuid::Uuid;

mod temp_dirs;

#[cfg(unix)]
#[test]
fn relative_paths_report_an_unavailable_working_directory() -> Result<(), Box<dyn std::error::Error>>
{
    const CHILD: &str = "MM_PATH_POLICY_MISSING_CWD_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let temporary = temp_dirs::tempdir()?;
        std::env::set_current_dir(temporary.path())?;
        drop(temporary);
        let result =
            mm::domain::path_policy::ensure_safe_path_ancestors(std::path::Path::new("relative"));
        std::env::set_current_dir("/")?;
        assert!(matches!(
            result,
            Err(mm::domain::error::MmError::Io { context, .. })
                if context == "resolving the current directory for path validation"
        ));
        return Ok(());
    }

    let status = std::process::Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "relative_paths_report_an_unavailable_working_directory",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .status()?;
    assert!(status.success());
    Ok(())
}

#[cfg(unix)]
#[test]
fn private_files_reject_foreign_ownership_when_privilege_allows_the_fixture()
-> Result<(), Box<dyn std::error::Error>> {
    if !rustix::process::geteuid().is_root() {
        return Ok(());
    }
    let temporary = temp_dirs::tempdir()?;
    let file = temporary.path().join("private");
    std::fs::write(&file, b"value")?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
    rustix::fs::chown(
        &file,
        Some(rustix::process::Uid::from_raw(65_534)),
        Some(rustix::process::Gid::from_raw(65_534)),
    )?;

    assert!(matches!(
        mm::domain::path_policy::ensure_private_file(&file),
        Err(mm::domain::error::MmError::Security { .. })
    ));
    assert!(matches!(
        mm::domain::path_policy::ensure_owned_by_current_user(&file),
        Err(mm::domain::error::MmError::Security { .. })
    ));
    Ok(())
}

#[test]
fn small_domain_value_contracts_are_complete_and_stable() {
    let first = OperationId::new();
    let second = OperationId::default();
    assert_ne!(first, second);

    assert_eq!(
        IngressRouteProvenance::ExplicitConfig.to_string(),
        "explicit config"
    );
    assert_eq!(
        IngressRouteProvenance::AutomaticConvention.to_string(),
        "automatic convention"
    );
    assert_eq!(
        IngressRouteProvenance::MmManagedService.to_string(),
        "mm-managed service"
    );

    assert_eq!(EnvironmentDeliveryMode::Direct.as_str(), "direct");
    assert_eq!(
        EnvironmentDeliveryMode::Direct.driver_id(),
        "direct-environment"
    );
    assert!(
        EnvironmentDeliveryMode::Direct
            .required_runtime_capability()
            .is_some()
    );
    assert_eq!(EnvironmentDeliveryMode::EnvFile.as_str(), "env-file");
    assert_eq!(
        EnvironmentDeliveryMode::EnvFile.driver_id(),
        "operation-env-file"
    );
    assert_eq!(
        EnvironmentDeliveryMode::EnvFile.required_runtime_capability(),
        None
    );
    assert_eq!(
        EnvironmentDeliveryProvenance::EnvironmentOverride.as_str(),
        "environment-override"
    );
    assert_eq!(
        EnvironmentDeliveryProvenance::DevelopmentDefault.as_str(),
        "development-default"
    );
    assert_eq!(
        EnvironmentDeliveryProvenance::PolicyDefault.as_str(),
        "policy-default"
    );
    assert_eq!(
        EnvironmentDeliveryProvenance::BuiltInDefault.as_str(),
        "built-in-default"
    );
}

#[test]
fn configuration_value_accessors_and_environment_fallbacks_are_explicit()
-> Result<(), Box<dyn std::error::Error>> {
    for role in [
        ServiceRole::Database,
        ServiceRole::Queue,
        ServiceRole::Worker,
        ServiceRole::Scheduler,
        ServiceRole::Job,
    ] {
        assert!(role.excludes_automatic_ingress());
    }
    assert!(!ServiceRole::Http.excludes_automatic_ingress());

    let reference = "age://local/example".parse::<SecretReference>()?;
    let secret = EnvironmentBinding::Secret(SecretBinding {
        secret: "example".to_owned(),
    });
    assert_eq!(secret.value(), None);
    assert_eq!(secret.secret_name(), Some("example"));
    assert_eq!(secret.cast(), None);
    let cast = EnvironmentBinding::Cast(CastBinding {
        cast: ValueCast::Url,
        value_ref: "domains.web".to_owned(),
    });
    assert_eq!(cast.value(), None);
    assert_eq!(cast.secret_name(), None);
    assert!(cast.cast().is_some());
    let debug = format!("{:?}", SecretRef(reference));
    assert_eq!(debug, "SecretRef([REDACTED])");

    let config = serde_yaml_ng::from_str::<MmConfig>(
        "schema-version: 1\nproject:\n  id: 00000000-0000-0000-0000-000000000001\n  name: contracts\ntarget: local\nstacks: {}\nenvironments:\n  local:\n    bindings: {}\n",
    )?;
    let document = ConfigDocument {
        config,
        root: std::path::PathBuf::from("/workspace"),
        path: std::path::PathBuf::from("/workspace/mm.yaml"),
        sources: Default::default(),
        source_hash: "a".repeat(64),
        unknown_keys: Vec::new(),
    };
    assert_eq!(
        resolve_environment(&document, Some("explicit"))?,
        "explicit"
    );
    assert!(resolve_environment(&document, None).is_err());
    Ok(())
}

#[test]
fn development_mail_defaults_and_explicit_bindings_preserve_provenance() {
    let defaults = configured_bindings(&DevelopmentMailPolicy::default());
    assert_eq!(defaults.len(), 4);
    assert_eq!(
        defaults["MM_MAIL_SMTP_HOST"],
        (DevelopmentMailValue::SmtpHost, false)
    );
    assert_eq!(defaults["MM_MAIL_SMTP_PORT"].0.value(), "1025");
    assert_eq!(
        defaults["MM_MAIL_SMTP_URL"].0.value(),
        "smtp://mailpit:1025"
    );
    assert_eq!(defaults["MM_MAIL_UI_URL"].0.value(), "https://mail.mm.test");

    let explicit = DevelopmentMailPolicy {
        enabled: Some(true),
        bindings: BTreeMap::from([("MAILER_URL".to_owned(), DevelopmentMailValue::SmtpUrl)]),
    };
    assert!(!explicit.is_empty());
    assert_eq!(
        configured_bindings(&explicit)["MAILER_URL"],
        (DevelopmentMailValue::SmtpUrl, true)
    );
}

#[test]
fn target_ingress_names_and_step_labels_are_deterministic_and_namespaced() {
    let target = TargetId(Uuid::from_u128(42));
    let network = ingress_network_name(target);
    assert_eq!(network, ingress_project_name(target));
    assert!(network.starts_with("mm-ingress-"));
    assert!(ingress_acme_volume_name(target).starts_with("mm-ingress-acme-"));
    assert!(ingress_dynamic_config_volume_name(target).starts_with("mm-ingress-config-"));

    let steps = [
        TargetIngressStep::CreateExternalNetwork,
        TargetIngressStep::PreparePrivateIngressState,
        TargetIngressStep::ApplyIngressStack,
        TargetIngressStep::VerifyLoopbackIngress,
        TargetIngressStep::InstallRootlessAutostart,
        TargetIngressStep::InstallPrivilegedPortForwarder,
    ];
    let labels = steps.map(TargetIngressStep::label);
    assert_eq!(
        labels
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        steps.len()
    );
    assert!(labels.iter().all(|label| !label.is_empty()));
}

fn managed_plan() -> Result<ManagedServicePlan, String> {
    let file = ManagedServiceFile {
        source: std::path::PathBuf::from("/workspace/config.hcl"),
        name: "config.hcl".to_owned(),
        mode: 0o600,
        uid: 1000,
        gid: 1000,
    };
    let mut plan = ManagedServicePlan {
        schema: MANAGED_SERVICE_SCHEMA,
        identity: ManagedServiceIdentity {
            target: "local".to_owned(),
            target_id: TargetId(Uuid::from_u128(42)),
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
            config_files: vec![file],
            ingress_files: Vec::new(),
            generation: "v1".to_owned(),
        },
        artifact_root: std::path::PathBuf::from("/workspace/artifact"),
        compose_path: std::path::PathBuf::from("/workspace/artifact/compose.yaml"),
        plan_hash: String::new(),
    };
    plan.plan_hash = plan.canonical_hash()?;
    Ok(plan)
}

#[test]
fn managed_service_plans_reject_each_incomplete_unsafe_or_stale_contract() -> Result<(), String> {
    let valid = managed_plan()?;
    valid.validate()?;

    let mut wrong_schema = valid.clone();
    wrong_schema.schema += 1;
    assert!(wrong_schema.validate().is_err());

    let mutations: [fn(&mut ManagedServicePlan); 7] = [
        |plan| plan.service.host_port = 0,
        |plan| plan.service.container_port = 0,
        |plan| plan.service.command.clear(),
        |plan| plan.service.healthcheck.clear(),
        |plan| plan.service.config_files.clear(),
        |plan| plan.identity.provider_instance.clear(),
        |plan| plan.identity.component.clear(),
    ];
    for mutate in mutations {
        let mut candidate = valid.clone();
        mutate(&mut candidate);
        assert!(candidate.validate().is_err());
    }
    let mut outside = valid.clone();
    outside.compose_path = std::path::PathBuf::from("/outside/compose.yaml");
    assert!(outside.validate().is_err());

    for name in [
        "",
        ".",
        "..",
        "nested/file",
        "back\\slash",
        "control\n",
        &"a".repeat(256),
    ] {
        let mut unsafe_file = valid.clone();
        unsafe_file.service.config_files[0].name = name.to_owned();
        assert!(unsafe_file.validate().is_err(), "file name: {name:?}");
    }
    let mut relative_source = valid.clone();
    relative_source.service.config_files[0].source = std::path::PathBuf::from("relative");
    assert!(relative_source.validate().is_err());

    let mut stale = valid;
    stale.service.generation.push_str("-changed");
    assert!(stale.validate().is_err());
    Ok(())
}

#[tokio::test]
async fn operation_stream_cancellation_is_explicit_and_drop_safe() {
    use mm::app::ports::{ExecStream, LogStream};
    use mm::domain::observability::OperationCancellation;

    let log_cancellation = OperationCancellation::default();
    let (log_sender, log_receiver) = tokio::sync::mpsc::channel(1);
    let mut logs = LogStream::new(log_receiver, log_cancellation.clone());
    logs.cancel();
    assert!(log_cancellation.is_cancelled());
    drop(log_sender);
    assert!(logs.recv().await.is_none());

    let exec_cancellation = OperationCancellation::default();
    let (exec_sender, exec_receiver) = tokio::sync::mpsc::channel(1);
    let exec = ExecStream::new(exec_receiver, exec_cancellation.clone());
    exec.cancel();
    assert!(exec_cancellation.is_cancelled());
    drop(exec_sender);
    drop(exec);
    assert!(exec_cancellation.is_cancelled());
}
