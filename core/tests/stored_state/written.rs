use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use mm::adapters::mcp::AgentTrustStore;
use mm::adapters::remote::{DaemonConfig, DeployedState, HostAccessStore, StoredRollout};
use mm::adapters::services::FilesystemServiceStackStore;
use mm::adapters::state::SqliteStateStore;
use mm::adapters::target::{FilesystemTargetStore, ForwardShortcutStore, TargetPins};
use mm::adapters::user_namespace::FilesystemServiceUserIds;
use mm::app::ports::{
    EnvironmentValueStore, ExecAuditJournal, IngressClaimsReplacement, IngressRouteRegistry,
    McpAuditJournal, ServiceStackStore, ServiceUserIds, StateStore, TargetStore,
};
use mm::domain::build::{
    BuildReceipt, BuildReusePolicy, BuildSource, BuildTagStrategy, BuiltImage, ImageBuildPlan,
    ImageBuildResult, ImageBuildScope, ImageBuildUnitPlan, ImageBuildVariantPlan,
    unit_execution_fingerprint, unit_input_fingerprint,
};
use mm::domain::deployment::{DEPLOYMENT_PLAN_SCHEMA, DeploymentPlan, DeploymentRouteDiff};
use mm::domain::ingress::{
    CanonicalMiddleware, IngressCertificateReference, IngressRouteClaim, IngressRouteOwnership,
    IngressRouteProvenance,
};
use mm::domain::operation::{
    NewOperation, OperationEvent, OperationId, OperationKind, OperationPhase, OperationState,
};
use mm::domain::runtime::RuntimeKind;
use mm::domain::target::{TargetDefinition, TargetId, TargetTransport};
use mm::domain::traefik_dynamic::{PreparedTraefikDynamicConfig, TraefikDynamicConfigManifest};
use mm::domain::workspace::{ProjectId, WorkspaceRegistration};
use uuid::Uuid;

use super::sql_dump;

pub type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

pub const PROJECT: ProjectId =
    ProjectId(Uuid::from_u128(0x0001_0000_0000_4000_8000_0000_0000_0001));
pub const TARGET: TargetId = TargetId(Uuid::from_u128(0x0001_0000_0000_4000_8000_0000_0000_0002));
pub const BUILD: OperationId =
    OperationId(Uuid::from_u128(0x0001_0000_0000_4000_8000_0000_0000_0003));
pub const DEPLOY: OperationId =
    OperationId(Uuid::from_u128(0x0001_0000_0000_4000_8000_0000_0000_0004));
pub const ROOT: &str = "/work/web";
pub const ENVIRONMENT: &str = "production";
pub const COMMIT: &str = "3f9a1c2e7b4d5a6f8091a2b3c4d5e6f708192a3b";
pub const HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w";
pub const PIN: &str = "web:production";
pub const CONTAINER: &str = "4b7c2d0e9f1a3b5c7d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c";

/// Every file `mm` keeps on a machine, as this release writes it, under
/// `out/machine`; every file `mmd` keeps on a host under `out/host`; and a
/// build receipt.
pub fn all(out: &Path) -> TestResult {
    machine(&out.join("machine"))?;
    host(&out.join("host"))?;
    let receipt = build_receipt(OperationId::new())?;
    write(
        &out.join("build-receipt.json"),
        &serde_json::to_string_pretty(&receipt)?,
    )
}

fn machine(home: &Path) -> TestResult {
    private_directory(home)?;
    let database = home.join("state.sqlite3");
    state_database(&database)?;
    let connection = rusqlite::Connection::open(&database)?;
    write(&home.join("state.sql"), &sql_dump::dump(&connection)?)?;
    drop(connection);
    for suffix in ["", "-wal", "-shm"] {
        let path = PathBuf::from(format!("{}{suffix}", database.display()));
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    FilesystemServiceStackStore::new(home.join("services")).save_state(&services::state()?)?;
    let targets = FilesystemTargetStore::new(home.join("targets.yaml"));
    targets.insert_target("local", TargetDefinition::local(TARGET))?;
    let mut remote = TargetDefinition::local(TargetId(Uuid::from_u128(7)));
    remote.transport = TargetTransport::Ssh {
        host: "app.example.test".to_owned(),
        port: 22,
        user: "deploy".to_owned(),
        host_key_fingerprint: mm::domain::repo_target::ssh_fingerprint(HOST_KEY)?,
        host_key: Some(HOST_KEY.to_owned()),
        identity_file: None,
        remote_root: None,
        daemon_fingerprint: Some("b".repeat(64)),
    };
    remote.default_runtime = Some(RuntimeKind::DockerCompose);
    targets.insert_target("staging", remote)?;
    AgentTrustStore::new(home).trust(
        Path::new(ROOT),
        PROJECT,
        &mm::domain::agents::AgentsConfig::new(),
        1_000,
    )?;
    let mut pin = mm::domain::repo_target::TargetPin::default();
    pin.host_key = Some(HOST_KEY.to_owned());
    pin.daemon_fingerprint = Some("b".repeat(64));
    TargetPins::new(home.join("pinned-targets.json")).trust(PIN, &pin)?;
    ForwardShortcutStore::new(home.join("forwards.json")).save(
        &PROJECT.0.to_string(),
        "db",
        mm::domain::forward::ForwardShortcut {
            environment: ENVIRONMENT.to_owned(),
            service: "db".to_owned(),
            port: 5432,
            local_port: 15432,
            bind: mm::domain::forward::default_bind(),
            auto: true,
        },
    )?;
    Ok(())
}

fn state_database(path: &Path) -> TestResult {
    let store = SqliteStateStore::open(path)?;
    let workspace = store.register_workspace(
        &WorkspaceRegistration {
            project_id: PROJECT,
            root: PathBuf::from(ROOT),
            config_path: PathBuf::from(ROOT).join(".config/mm.yaml"),
        },
        1_000,
    )?;
    store.set_local_project_preference(
        PROJECT,
        Some(RuntimeKind::DockerCompose),
        Some("local"),
        1_000,
    )?;
    store.set_local_environment_preference(
        PROJECT,
        ENVIRONMENT,
        Some(RuntimeKind::PodmanCompose),
        None,
        1_000,
    )?;
    store.set_environment_override(workspace.id, ENVIRONMENT, "LOG.LEVEL", "debug", 1_100, None)?;

    let (plan, mut result) = build(BUILD)?;
    begin(
        &store,
        workspace.id,
        BUILD,
        OperationKind::Build,
        &plan.plan_hash,
    )?;
    store.save_image_build_plan(BUILD, &plan)?;
    result.operation_id = BUILD;
    store.save_image_build_result(BUILD, &result)?;
    store.finish_operation(BUILD, OperationState::Succeeded, 2_000, None)?;

    let mut route = claim();
    route.middlewares = vec!["mm-strip-api".to_owned()];
    let middlewares = BTreeMap::from([(
        "mm-strip-api".to_owned(),
        CanonicalMiddleware::ReplacePathRegex {
            regex: "^/api(.*)".to_owned(),
            replacement: "$1".to_owned(),
        },
    )]);
    let digest = "c".repeat(64);
    store.replace_owner_claims_cas(IngressClaimsReplacement {
        target_id: TARGET,
        project_id: PROJECT,
        environment: ENVIRONMENT,
        expected_generation: 0,
        claims: &[route],
        middlewares: &middlewares,
        preserve_gate_attachments: true,
        config_digest: Some(&digest),
        now_unix_ms: 2_100,
    })?;

    let plan = deployment_plan()?;
    begin(
        &store,
        workspace.id,
        DEPLOY,
        OperationKind::Deploy,
        &plan.plan_hash,
    )?;
    store.save_deployment_plan(DEPLOY, &plan)?;
    store.append_event(
        DEPLOY,
        3_100,
        &OperationEvent::PhaseChanged {
            operation_id: DEPLOY,
            phase: OperationPhase::Render,
        },
    )?;
    store.finish_operation(DEPLOY, OperationState::Succeeded, 3_200, None)?;

    store.append_exec_audit(&mm::domain::observability::ExecAuditRecord {
        id: Uuid::from_u128(11),
        principal: "developer".to_owned(),
        runtime: RuntimeKind::DockerCompose,
        project_id: PROJECT,
        environment: ENVIRONMENT.to_owned(),
        stack: "app".to_owned(),
        module: Some("web".to_owned()),
        service: "web".to_owned(),
        container_id: None,
        executable: "sh".to_owned(),
        argv_hash: "a".repeat(64),
        started_at_unix_ms: 4_000,
        finished_at_unix_ms: None,
        exit_code: None,
        error_code: None,
    })?;
    store.append_mcp_audit(&mm::domain::mcp::McpAuditRecord {
        id: Uuid::from_u128(12),
        request_id: Uuid::from_u128(13),
        principal: "local-agent".to_owned(),
        source: mm::domain::mcp::MCP_SOURCE.to_owned(),
        project_root: PathBuf::from(ROOT),
        environment: ENVIRONMENT.to_owned(),
        target: "local".to_owned(),
        operation: "deploy_plan".to_owned(),
        mutation: false,
        plan_hash: Some(plan.plan_hash.clone()),
        outcome_code: "ok".to_owned(),
        occurred_at_unix_ms: 4_100,
    })?;
    Ok(())
}

fn begin(
    store: &SqliteStateStore,
    workspace_id: mm::domain::workspace::WorkspaceId,
    id: OperationId,
    kind: OperationKind,
    plan_hash: &str,
) -> TestResult {
    store.begin_operation(&NewOperation {
        id,
        workspace_id,
        kind,
        environment: ENVIRONMENT.to_owned(),
        requested_at_unix_ms: 1_500,
        plan_hash: Some(plan_hash.to_owned()),
    })?;
    Ok(())
}

pub fn claim() -> IngressRouteClaim {
    IngressRouteClaim {
        target: "local".to_owned(),
        target_id: TARGET,
        project_id: PROJECT,
        project_name: "web".to_owned(),
        environment: ENVIRONMENT.to_owned(),
        stack: "app".to_owned(),
        route: "web".to_owned(),
        host: "web.example.test".to_owned(),
        path_prefix: "/".to_owned(),
        service: "web".to_owned(),
        port: 8080,
        tls: true,
        provenance: IngressRouteProvenance::ExplicitConfig,
        certificate_reference: IngressCertificateReference::LocalCa {
            host: "web.example.test".to_owned(),
        },
        ownership: IngressRouteOwnership::Project {
            project_id: PROJECT,
            environment: ENVIRONMENT.to_owned(),
            stack: "app".to_owned(),
            service: "web".to_owned(),
        },
        priority: 1,
        router_name: "mm-web".to_owned(),
        backend_alias: "mm-ingress-backend-web".to_owned(),
        artifact_hash: "e".repeat(64),
        middlewares: Vec::new(),
    }
}

/// A published one-image build of [`COMMIT`].
fn build(operation_id: OperationId) -> TestResult<(ImageBuildPlan, ImageBuildResult)> {
    let platforms = vec!["linux/amd64".to_owned()];
    let source = BuildSource {
        commit: COMMIT.to_owned(),
        branch: Some("main".to_owned()),
        tags_at_head: Vec::new(),
        repository_tags: Vec::new(),
        tags_complete: true,
        default_branch: true,
        dirty: false,
    };
    let version = mm::domain::build::build_version(&source)?;
    let tags = mm::domain::build::automatic_tags(&source, BuildTagStrategy::Auto)?;
    let mut unit = ImageBuildUnitPlan {
        module: "web".to_owned(),
        image: "registry.example.test/web".to_owned(),
        context: PathBuf::from("web"),
        dockerfile: PathBuf::from("web/Dockerfile"),
        context_hash: "a".repeat(64),
        dockerfile_hash: "b".repeat(64),
        args: BTreeMap::from([("VERSION".to_owned(), version)]),
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
            tags: tags.clone(),
            stage: None,
            args: BTreeMap::new(),
        }],
    };
    unit.input_fingerprint = unit_input_fingerprint(&unit, &platforms)?;
    let plan = ImageBuildPlan::new(
        ImageBuildScope {
            project_id: PROJECT,
            config_hash: "c".repeat(64),
            environment: ENVIRONMENT.to_owned(),
        },
        source,
        true,
        BuildTagStrategy::Auto,
        platforms,
        vec![unit],
    )?;
    let unit = &plan.units[0];
    let result = ImageBuildResult {
        operation_id,
        plan_hash: plan.plan_hash.clone(),
        images: vec![BuiltImage {
            module: unit.module.clone(),
            target: None,
            repository: unit.image.clone(),
            tags: tags
                .iter()
                .map(|tag| format!("{}:{tag}", unit.image))
                .collect(),
            digest: format!("sha256:{}", "d".repeat(64)),
            execution_fingerprint: unit_execution_fingerprint(
                &unit.input_fingerprint,
                ENVIRONMENT,
                None,
                &BTreeMap::new(),
                &BTreeMap::new(),
            )?,
            secret_revisions: BTreeMap::new(),
            archive: None,
            reused: false,
        }],
        dependencies: Vec::new(),
    };
    Ok((plan, result))
}

fn build_receipt(operation_id: OperationId) -> TestResult<BuildReceipt> {
    let (plan, result) = build(operation_id)?;
    Ok(BuildReceipt::new(plan, result)?)
}

fn deployment_plan() -> TestResult<DeploymentPlan> {
    let dynamic = PreparedTraefikDynamicConfig {
        root: "/srv/web/.mm/ingress/dynamic".into(),
        path: "/srv/web/.mm/ingress/dynamic/routes.yaml".into(),
        manifest: TraefikDynamicConfigManifest {
            target_id: TARGET,
            claim_count: 0,
            router_count: 0,
            service_count: 0,
            digest: "c".repeat(64),
        },
    };
    let mut plan = DeploymentPlan {
        excluded_services: Vec::new(),
        unknown_config_keys: Vec::new(),
        secret_file_owners: Vec::new(),
        healthcheck_arguments: Vec::new(),
        schema: DEPLOYMENT_PLAN_SCHEMA.to_owned(),
        project_id: PROJECT,
        project_name: "web".to_owned(),
        target_name: "local".to_owned(),
        target_id: TARGET,
        target_fingerprint: TargetDefinition::local(TARGET).canonical_fingerprint(),
        environment: ENVIRONMENT.to_owned(),
        runtime: Some(RuntimeKind::DockerCompose),
        runtime_source: None,
        workload_intent_hash: None,
        networks: Vec::new(),
        expected_route_generation: 1,
        expected_route_config_digest: None,
        expected_previous_plan_hash: None,
        stacks: Vec::new(),
        removed_stacks: Vec::new(),
        preserved_volumes: Vec::new(),
        removed_preserved_volumes: Vec::new(),
        previous_owner_claims: Vec::new(),
        adopted_route_claims: Vec::new(),
        desired_route_claims: Vec::new(),
        complete_route_claims: Vec::new(),
        desired_middlewares: BTreeMap::new(),
        environment_review: Default::default(),
        runtime_secret_plan: None,
        scheduler_reconciliation: None,
        environment_bundle_root: None,
        previous_dynamic_config: dynamic.clone(),
        dynamic_config: dynamic,
        local_certificates: Vec::new(),
        static_ingress: None,
        removed_ingress: None,
        route_diff: DeploymentRouteDiff::default(),
        ingress: Vec::new(),
        route_state_changed: false,
        build_receipt: None,
        local_build: None,
        registries: BTreeMap::new(),
        node: None,
        rollout: Default::default(),
        stack_after: Default::default(),
        config_files: Vec::new(),
        runtime_changes: Vec::new(),
        release: Some(mm::domain::release_facts::ReleaseFacts {
            commit: Some(COMMIT.to_owned()),
            name: Some("1.4.0".to_owned()),
            inputs: BTreeMap::new(),
            images: BTreeMap::new(),
        }),
        plan_hash: String::new(),
    };
    plan.plan_hash = plan.canonical_hash()?;
    Ok(plan)
}

mod services {
    use mm::domain::service_stack::{
        DASHBOARD_HOST, DashboardServiceConfig, MAILPIT_IMAGE, MAILPIT_VERSION,
        PreparedServiceStackArtifact, SERVICE_STACK_SCHEMA, SERVICE_STACK_VERSION,
        ServiceComponentDescriptor, ServiceComponentKind, ServiceStackAction,
        ServiceStackJournalState, ServiceStackPlan, ServiceStackState, development_network_name,
    };
    use mm::domain::target_ingress::{
        PreparedTargetIngressArtifact, TargetIngressArtifactManifest,
    };

    use super::{PathBuf, RuntimeKind, TARGET, TargetDefinition, TestResult, Uuid};

    pub fn state() -> TestResult<ServiceStackState> {
        let home = PathBuf::from("/srv/web/.mm");
        let root = home.join("services/artifacts/1");
        let ingress_root = home.join("ingress/1");
        let manifest = TargetIngressArtifactManifest {
            target_id: TARGET,
            target_name: "local".to_owned(),
            target_fingerprint: TargetDefinition::local(TARGET).canonical_fingerprint(),
            provider: "traefik".to_owned(),
            image: mm::domain::target::DEFAULT_TRAEFIK_IMAGE.to_owned(),
            acme_email: None,
            compose_project_name: "mm-ingress".to_owned(),
            network_name: mm::domain::target_ingress::ingress_network_name(TARGET),
            acme_volume_name: "mm-ingress-acme".to_owned(),
            dynamic_config_volume_name: "mm-ingress-dynamic".to_owned(),
            dynamic_config_directory: "/etc/traefik/dynamic".to_owned(),
            acme_storage_path: "/acme/acme.json".to_owned(),
            acme_file_mode: "0600".to_owned(),
            public_http_port: 80,
            public_https_port: 443,
            proxy_http_port: 18080,
            proxy_https_port: 18443,
            proxy_verify_port: 18081,
            certificate_resolver: "mm".to_owned(),
            target_compose_path: ingress_root.join("compose.yaml").display().to_string(),
            compose_digest: "a".repeat(64),
            support_files: Vec::new(),
            steps: Vec::new(),
            macos_host: None,
            artifact_hash: "b".repeat(64),
        };
        let ingress = PreparedTargetIngressArtifact {
            manifest_path: ingress_root.join("manifest.json"),
            compose_path: ingress_root.join("compose.yaml"),
            support_paths: Vec::new(),
            root: ingress_root,
            manifest,
        };
        let artifact = PreparedServiceStackArtifact {
            manifest_path: root.join("manifest.json"),
            compose_path: root.join("compose.yaml"),
            route_path: root.join("mailpit.dynamic.yaml"),
            compose_digest: "c".repeat(64),
            route_digest: "d".repeat(64),
            dashboard_unit_path: Some(root.join("mm-dashboard.service")),
            dashboard_unit_digest: Some("e".repeat(64)),
            root,
        };
        let mut plan = ServiceStackPlan {
            schema: SERVICE_STACK_SCHEMA,
            action: ServiceStackAction::Apply,
            stack_version: SERVICE_STACK_VERSION.to_owned(),
            runtime: RuntimeKind::DockerCompose,
            target_id: TARGET,
            target_fingerprint: TargetDefinition::local(TARGET).canonical_fingerprint(),
            components: vec![ServiceComponentDescriptor {
                kind: ServiceComponentKind::Mailpit,
                version: MAILPIT_VERSION.to_owned(),
                image: Some(MAILPIT_IMAGE.to_owned()),
                dependencies: Vec::new(),
                volumes: Vec::new(),
                engine_socket_mounted: false,
            }],
            dashboard_hosts: vec![DASHBOARD_HOST.to_owned()],
            dashboard_service: Some(DashboardServiceConfig {
                executable: PathBuf::from("/usr/local/bin/mm"),
                mm_home: home,
            }),
            development_network: development_network_name(TARGET),
            mailpit_data_volume: "mm-mailpit-data".to_owned(),
            cap_valkey_data_volume: None,
            cap_admin_key_file: None,
            previous_host_smtp_port: None,
            host_smtp_port: Some(1025),
            purge_data: false,
            previous_state_digest: String::new(),
            ingress: ingress.clone(),
            artifact: artifact.clone(),
            plan_hash: String::new(),
        };
        plan.plan_hash = plan.canonical_hash()?;
        plan.validate()?;
        Ok(ServiceStackState {
            schema: SERVICE_STACK_SCHEMA,
            installation_id: Uuid::from_u128(21),
            runtime: RuntimeKind::DockerCompose,
            target_id: TARGET,
            stack_version: SERVICE_STACK_VERSION.to_owned(),
            plan_hash: plan.plan_hash.clone(),
            artifact,
            ingress,
            development_network: plan.development_network.clone(),
            mailpit_data_volume: plan.mailpit_data_volume.clone(),
            cap_valkey_data_volume: None,
            host_smtp_port: Some(1025),
            journal_state: ServiceStackJournalState::Applied,
            operation_id: Uuid::from_u128(22),
            updated_at_unix_ms: 5_000,
            previous: None,
            failure_code: None,
            applied_plan: Box::new(plan),
        })
    }
}

/// A host's `/etc/mm` and `/var/lib/mm`, and the mm home `mmd` runs local
/// engines from, under `root`.
fn host(root: &Path) -> TestResult {
    let etc = root.join("etc/mm");
    let state = root.join("var/lib/mm");
    let mm_home = root.join("var/lib/mm-home");
    for directory in [&etc, &state, &mm_home] {
        private_directory(directory)?;
    }
    let config = daemon_config();
    write(
        &etc.join("mmd.json"),
        &serde_json::to_string_pretty(&config)?,
    )?;
    write(&etc.join("host.yaml"), HOST_POLICY)?;
    mm::adapters::remote::parse_host_policy(&etc.join("host.yaml"), HOST_POLICY.as_bytes())?;
    host_access(&state)?;
    host_cluster(&state)?;
    let range = format!(
        "{}:{}:{}\n",
        mm::domain::user_namespace::RANGE_OWNER,
        mm::domain::user_namespace::DEFAULT_FIRST,
        mm::domain::user_namespace::DEFAULT_COUNT
    );
    let ids = root.join("subordinate-ids");
    write(&ids, &range)?;
    FilesystemServiceUserIds::at(
        mm_home.join("service-user-ids.json"),
        ids.clone(),
        ids.clone(),
    )
    .reserve(&BTreeSet::from([service_owner()]))?;
    fs::remove_file(ids)?;
    Ok(())
}

pub const HOST_POLICY: &str = "schema-version: 1
break-glass:
  - name: ops
    key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w
projects:
  00010000-0000-4000-8000-000000000001:
    name: web
    delivery: push
    environments: [production]
";

pub fn daemon_config() -> DaemonConfig {
    DaemonConfig {
        schema_version: 1,
        socket_path: PathBuf::from("/run/mm/mm.sock"),
        state_root: PathBuf::from("/var/lib/mm"),
        mm_home: PathBuf::from("/var/lib/mm-home"),
        socket_uid: 0,
        socket_gid: 0,
        idle_timeout_ms: 30_000,
        host_policy_path: PathBuf::from("/etc/mm/host.yaml"),
        relay_uid: 991,
        authorized_keys_path: PathBuf::from("/etc/ssh/mm_authorized_keys"),
        relay_command: "/usr/bin/mm relay serve --stdio --socket /run/mm/mm.sock --daemon-uid 0"
            .to_owned(),
        projects_root: PathBuf::from("/srv/mm/projects"),
        node_env_path: PathBuf::from("/etc/mm/node.env"),
    }
}

pub fn service_owner() -> mm::domain::user_namespace::ServiceIdsOwner {
    mm::domain::user_namespace::ServiceIdsOwner {
        project: PROJECT,
        environment: ENVIRONMENT.to_owned(),
        stack: "app".to_owned(),
        service: "web".to_owned(),
    }
}

pub fn scope() -> mm::domain::remote::RemoteScope {
    mm::domain::remote::RemoteScope {
        project_id: PROJECT,
        environment: ENVIRONMENT.to_owned(),
        target: "production".to_owned(),
        target_fingerprint: "f".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: Some("app-1".to_owned()),
    }
}

fn host_access(state: &Path) -> TestResult {
    use mm::domain::host_policy::{
        AccessLevel, EmergencyBlock, EnvironmentAccess, EnvironmentAccessRecord, ReconciledKey,
        ReconciledUser,
    };
    let project = state.join("projects").join(PROJECT.0.to_string());
    let record = EnvironmentAccessRecord {
        schema_version: mm::domain::host_policy::ACCESS_RECORD_SCHEMA_VERSION,
        project_id: PROJECT,
        environment: ENVIRONMENT.to_owned(),
        commit: COMMIT.to_owned(),
        users: BTreeMap::from([(
            "ana".to_owned(),
            ReconciledUser {
                machine: false,
                keys: vec![ReconciledKey {
                    public_key: HOST_KEY.to_owned(),
                    fingerprint: mm::domain::repo_target::ssh_fingerprint(HOST_KEY)?,
                    expires_at_unix_ms: None,
                }],
                access: EnvironmentAccess::level(AccessLevel::Deploy),
                agent: None,
            },
        )]),
        deploy_from: Some(vec!["main".to_owned()]),
        reconciled_at_unix_ms: 6_000,
        reconciled_by: "ana".to_owned(),
    };
    write_json(&project.join("access/production.json"), &record)?;
    write_json(
        &project.join("blocks.json"),
        &[EmergencyBlock {
            project_id: PROJECT,
            environment: ENVIRONMENT.to_owned(),
            fingerprint: "SHA256:blocked".to_owned(),
            blocked_by: "ops".to_owned(),
            blocked_at_unix_ms: 6_100,
        }],
    )?;
    write_json(
        &project.join("deployed/production.json"),
        &DeployedState {
            project_id: PROJECT,
            environment: ENVIRONMENT.to_owned(),
            commit: COMMIT.to_owned(),
            root: PathBuf::from("/srv/mm/projects/web/production/3f9a1c2e7b4d"),
            deployed_at_unix_ms: 6_200,
            deployed_by: "ana".to_owned(),
            runs_services: true,
        },
    )?;
    write_json(
        &project.join(format!("bundles/production/{COMMIT}.json")),
        &mm::adapters::remote::BundleRecord {
            commit: COMMIT.to_owned(),
            bundle_hash: "a".repeat(64),
            bytes: 4096,
            files: 12,
            branches: vec!["main".to_owned()],
            delivered_by: "ana".to_owned(),
            delivered_at_unix_ms: 6_050,
        },
    )?;
    let mut config = daemon_config();
    config.state_root = state.to_path_buf();
    HostAccessStore::new(&config).update_deployments(PROJECT, ENVIRONMENT, |stored| {
        stored.finished(deployment_record());
        stored.rollout = Some(rollout());
        stored.once.insert(
            "migrate".to_owned(),
            mm::domain::scheduler::OnceRecord {
                node: Some("app-1".to_owned()),
                run: Some(Uuid::from_u128(31)),
                plan_hash: "a".repeat(64),
                at_unix_ms: 6_300,
            },
        );
        Ok(())
    })?;
    Ok(())
}

pub fn deployment_record() -> mm::domain::rollout::DeploymentRecord {
    mm::domain::rollout::DeploymentRecord {
        commit: COMMIT.to_owned(),
        deployed_at_unix_ms: 6_200,
        deployed_by: "ana".to_owned(),
        runs_services: true,
        plan_hash: Some("a".repeat(64)),
        receipt: None,
        release: Some(mm::domain::release_facts::ReleaseFacts {
            commit: Some(COMMIT.to_owned()),
            name: Some("1.4.0".to_owned()),
            inputs: BTreeMap::new(),
            images: BTreeMap::new(),
        }),
    }
}

fn rollout() -> StoredRollout {
    StoredRollout {
        plan_hash: "a".repeat(64),
        commit: COMMIT.to_owned(),
        principal_id: mm::domain::access::PrincipalId(Uuid::from_u128(41)),
        started_by: "ana".to_owned(),
        started_at_unix_ms: 6_000,
        heard_at_unix_ms: 6_150,
        expires_at_unix_ms: 9_000,
        state: mm::domain::rollout::RolloutState::Finished,
        stacks: vec!["app".to_owned()],
        failure: None,
        interrupted: None,
        interrupted_at_unix_ms: None,
        changes: None,
        begun_under: None,
    }
}

fn host_cluster(state: &Path) -> TestResult {
    use mm::adapters::healing::store::{ContainerHistory, History, keep_history, keep_watched};
    use mm::adapters::healing::{WatchedEnvironment, WatchedService};
    use mm::domain::autoscale::{Rule, ScaleChange, ScalePolicy, ScaledService, Want};
    use mm::domain::cluster_network::{
        NetworkAddresses, NetworkTable, NodeAddresses, NodeNetworks,
    };
    use mm::domain::healing::{RestartEvent, RestartPolicy};
    use mm::domain::metrics::{Metric, Percent};

    let checkout = PathBuf::from("/srv/mm/projects/web/production/3f9a1c2e7b4d");
    let health = state.join("health");
    keep_watched(
        &health,
        &WatchedEnvironment {
            scope: scope(),
            root: checkout.clone(),
            services: vec![WatchedService {
                stack: "app".to_owned(),
                service: "web".to_owned(),
                restart: Some(RestartPolicy::default()),
            }],
        },
    )?;
    keep_history(
        &health,
        &scope(),
        &History {
            containers: BTreeMap::from([(
                CONTAINER.to_owned(),
                ContainerHistory {
                    stack: "app".to_owned(),
                    service: "web".to_owned(),
                    restarts: vec![RestartEvent {
                        node: Some("app-1".to_owned()),
                        container: "web-production-app-web-1".to_owned(),
                        at_unix_ms: 7_000,
                        reason: "unhealthy for 3m".to_owned(),
                    }],
                    limit: None,
                },
            )]),
        },
    )?;
    let scale = state.join(mm::adapters::autoscale::SCALE_DIRECTORY);
    let rule = |hundredths, hold_seconds| Rule {
        threshold: Percent::from_hundredths(hundredths),
        hold_seconds,
    };
    let service = ScaledService {
        key: "app/web".to_owned(),
        stack: "app".to_owned(),
        service: "web".to_owned(),
        compose_project: "web-production-app".to_owned(),
        nodes: vec!["app-1".to_owned(), "app-2".to_owned()],
        replicas: 2,
        autoscale: Some(ScalePolicy {
            min: 2,
            max: 6,
            initial: 2,
            metric: Metric::Cpu,
            up: rule(7_500, 30),
            down: rule(2_500, 300),
        }),
        drain_ms: 10_000,
        now: Some(3),
    };
    let key = service.key();
    mm::adapters::autoscale::store::keep_deployed(
        &scale,
        &mm::adapters::autoscale::DeployedScaling {
            scope: scope(),
            root: checkout,
            plan_hash: "a".repeat(64),
            services: vec![service],
        },
    )?;
    mm::adapters::autoscale::store::keep_want(
        &scale,
        &key,
        &Want {
            count: 3,
            generation: 4,
            by: "app-1".to_owned(),
            change: Some(ScaleChange {
                from: 2,
                to: 3,
                reason: "cpu 82% above 75% for 30s".to_owned(),
                at_unix_ms: 7_100,
            }),
        },
    )?;
    mm::adapters::cluster_network::NetworkStore::new(state.join("networks")).write(
        &NodeNetworks {
            node: "app-1".to_owned(),
            space: "10.96.0.0/12".parse()?,
            mesh_interface: "mm0".to_owned(),
            mtu: 1420,
            table: NetworkTable {
                networks: BTreeMap::from([(
                    mm::domain::cluster_network::network_key(PROJECT, ENVIRONMENT, "default"),
                    NetworkAddresses {
                        block: "10.96.0.0/20".parse()?,
                        internal: false,
                        nodes: BTreeMap::from([(
                            "app-1".to_owned(),
                            NodeAddresses {
                                slice: "10.96.0.0/24".parse()?,
                                network: "mm-web-production-default".to_owned(),
                            },
                        )]),
                    },
                )]),
            },
        },
    )?;
    write_json(
        &state.join("mesh/mesh.json"),
        &mm::domain::mesh::MeshState {
            interface: "mm0".to_owned(),
            address: "10.90.0.1".parse()?,
            prefix: 24,
            port: 51820,
            mtu: 1420,
            peers: vec![mm::domain::mesh::MeshPeer {
                name: "app-2".to_owned(),
                public_key: "Zm9vYmFyYmF6cXV4cXV1eGNvcmdlZ3JhdWx0Z2FycGx5Zj0=".to_owned(),
                endpoint: "app-2.example.test:51820".to_owned(),
                allowed_ips: vec!["10.90.0.2/32".parse()?],
            }],
        },
    )?;
    write_json(
        &state.join(format!("ingress/routes/{}-{ENVIRONMENT}.json", PROJECT.0)),
        &mm::domain::cluster_ingress::IngressTable {
            network: "web-production-default".to_owned(),
            routes: vec![mm::domain::cluster_ingress::IngressRoute {
                router: "mm-web".to_owned(),
                host: "web.example.test".to_owned(),
                path: "/".to_owned(),
                priority: 1,
                stack: "app".to_owned(),
                service: "web".to_owned(),
                port: 8080,
                tls: true,
                balance: Default::default(),
                headers: BTreeMap::new(),
                allow: Vec::new(),
            }],
            certificate: None,
            engine_network: Some("mm-web-production-default".to_owned()),
            tunnel: None,
        },
    )?;
    write_json(
        &state.join(format!("leaving/{CONTAINER}.json")),
        &mm::domain::leaving::LeaveRecord {
            id: CONTAINER.to_owned(),
            name: "web-production-app-web-2".to_owned(),
            drain_ms: 10_000,
            marked_unix_ms: 7_200,
            confirmed_unix_ms: Some(7_250),
            restart: false,
        },
    )?;
    Ok(())
}

fn private_directory(path: &Path) -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> TestResult {
    write(path, &(serde_json::to_string_pretty(value)? + "\n"))
}

fn write(path: &Path, text: &str) -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    if let Some(parent) = path.parent() {
        private_directory(parent)?;
    }
    fs::write(path, text)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}
