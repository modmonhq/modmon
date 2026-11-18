use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use tempfile::TempDir;

use super::*;
use crate::adapters::secrets::{VaultAppRoleCredential, VaultHttpResponse};
use crate::app::ports::{CommandOutput, CommandSpec, ProcessRunner};
use crate::domain::managed_service::{
    MANAGED_SERVICE_SCHEMA, ManagedServiceIdentity, ManagedServiceSpec, ManagedServiceStatus,
};
use crate::domain::runtime::RuntimeKind;
use crate::domain::target::TargetId;

const SENTINEL: &str = "mm-vault-recovery-sentinel-b81c";

struct QueueProcess {
    outputs: Mutex<VecDeque<CommandOutput>>,
    records: Mutex<Vec<String>>,
}
#[async_trait]
impl ProcessRunner for QueueProcess {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        self.records
            .lock()
            .map_err(|_| MmError::State {
                message: "process record lock poisoned".to_owned(),
            })?
            .push(format!("{:?}", specification.redacted_record()));
        self.outputs
            .lock()
            .map_err(|_| MmError::State {
                message: "process output lock poisoned".to_owned(),
            })?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "process output queue empty".to_owned(),
            })
    }
}

#[async_trait]
impl ManagedServiceRuntime for QueueProcess {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::DockerCompose
    }

    fn plan(&self, request: ManagedServicePlanRequest) -> Result<ManagedServicePlan, MmError> {
        fake_service_plan(
            &request.image,
            &request.provider_instance,
            &request.generation,
        )
    }

    async fn apply(&self, _plan: &ManagedServicePlan) -> Result<(), MmError> {
        self.runtime_effect("apply")
    }

    async fn observe(&self, plan: &ManagedServicePlan) -> Result<ManagedServiceStatus, MmError> {
        Ok(ManagedServiceStatus {
            running: true,
            healthy: true,
            image: Some(plan.service.image.clone()),
        })
    }

    async fn remove(&self, _plan: &ManagedServicePlan, _purge_data: bool) -> Result<(), MmError> {
        self.runtime_effect("remove")
    }
}

impl QueueProcess {
    fn runtime_effect(&self, operation: &str) -> Result<(), MmError> {
        self.records
            .lock()
            .map_err(|_| MmError::State {
                message: "runtime record lock poisoned".to_owned(),
            })?
            .push(operation.to_owned());
        let output = self
            .outputs
            .lock()
            .map_err(|_| MmError::State {
                message: "runtime output lock poisoned".to_owned(),
            })?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "runtime output queue empty".to_owned(),
            })?;
        if output.success() {
            Ok(())
        } else {
            Err(MmError::RuntimeResponse {
                message: "injected managed runtime failure".to_owned(),
            })
        }
    }
}

fn fake_service_plan(
    image: &str,
    instance: &str,
    generation: &str,
) -> Result<ManagedServicePlan, MmError> {
    let root = std::env::temp_dir().join("mm-managed-vault-fake");
    let mut plan = ManagedServicePlan {
        schema: MANAGED_SERVICE_SCHEMA,
        identity: ManagedServiceIdentity {
            target: "local".to_owned(),
            target_id: TargetId(uuid::Uuid::nil()),
            target_fingerprint: "a".repeat(64),
            runtime: RuntimeKind::DockerCompose,
            provider_instance: instance.to_owned(),
            component: "vault".to_owned(),
            project_name: format!("mm-managed-{instance}"),
            service_name: format!("mm-managed-{instance}"),
            data_volume: format!("mm-managed-data-{instance}"),
            config_volume: format!("mm-managed-config-{instance}"),
            ingress_network: "mm-ingress".to_owned(),
            ingress_config_volume: "mm-ingress-config".to_owned(),
        },
        service: ManagedServiceSpec {
            image: image.to_owned(),
            host_port: 18_210,
            container_port: 8200,
            user: "100:1000".to_owned(),
            entrypoint: "vault".to_owned(),
            command: vec!["server".to_owned()],
            healthcheck: vec!["CMD".to_owned(), "vault".to_owned()],
            config_mount: "/vault/config".to_owned(),
            data_mount: "/vault/file".to_owned(),
            config_files: vec![ManagedServiceFile {
                source: root.join("config.hcl"),
                name: "config.hcl".to_owned(),
                mode: 0o600,
                uid: 100,
                gid: 1000,
            }],
            ingress_files: [
                "managed-vault.dynamic.yaml",
                "managed-vault-ca.crt",
                "managed-vault.crt",
                "managed-vault.key",
            ]
            .into_iter()
            .map(|name| ManagedServiceFile {
                source: root.join("vault.dynamic.yaml"),
                name: name.to_owned(),
                mode: 0o600,
                uid: 0,
                gid: 0,
            })
            .collect(),
            generation: generation.to_owned(),
        },
        artifact_root: root.clone(),
        compose_path: root.join("compose.yaml"),
        plan_hash: String::new(),
    };
    plan.plan_hash = plan
        .canonical_hash()
        .map_err(|message| MmError::State { message })?;
    Ok(plan)
}

struct Healthy;
#[async_trait]
impl VaultHttpTransport for Healthy {
    async fn request(
        &self,
        _connection: &VaultConnection,
        _method: Method,
        path: &str,
        _token: Option<&str>,
        _body: Option<&serde_json::Value>,
    ) -> Result<VaultHttpResponse, MmError> {
        if path != "sys/health" {
            return Err(MmError::State {
                message: format!("unexpected fake Vault request {path}"),
            });
        }
        Ok(VaultHttpResponse { status: 200, headers: BTreeMap::new(), body: Zeroizing::new(serde_json::to_vec(&serde_json::json!({"initialized": true, "sealed": false, "standby": false, "version": "1.18.5"})).map_err(|error| MmError::State { message: error.to_string() })?) })
    }
    async fn request_bytes(
        &self,
        _connection: &VaultConnection,
        method: Method,
        path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<VaultHttpResponse, MmError> {
        if method != Method::GET || path != "sys/storage/raft/snapshot" {
            return Err(MmError::State {
                message: "unexpected byte request".to_owned(),
            });
        }
        Ok(VaultHttpResponse {
            status: 200,
            headers: BTreeMap::new(),
            body: Zeroizing::new(b"rollback-snapshot".to_vec()),
        })
    }
}

struct LifecycleTransport {
    sealed: Mutex<bool>,
}

struct FailingInitializationTransport {
    status: u16,
    body: Vec<u8>,
}

#[async_trait]
impl VaultHttpTransport for FailingInitializationTransport {
    async fn request(
        &self,
        _connection: &VaultConnection,
        _method: Method,
        path: &str,
        _token: Option<&str>,
        _body: Option<&serde_json::Value>,
    ) -> Result<VaultHttpResponse, MmError> {
        match path {
            "sys/health" => lifecycle_response(
                200,
                br#"{"initialized":false,"sealed":true,"standby":false,"version":"1.19.5"}"#
                    .to_vec(),
            ),
            "sys/init" => lifecycle_response(self.status, self.body.clone()),
            _ => Err(MmError::State {
                message: format!("unexpected initialization request to {path}"),
            }),
        }
    }

    async fn request_bytes(
        &self,
        _connection: &VaultConnection,
        _method: Method,
        path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<VaultHttpResponse, MmError> {
        Err(MmError::State {
            message: format!("unexpected initialization byte request to {path}"),
        })
    }
}

fn lifecycle_response(status: u16, body: Vec<u8>) -> Result<VaultHttpResponse, MmError> {
    Ok(VaultHttpResponse {
        status,
        headers: BTreeMap::new(),
        body: Zeroizing::new(body),
    })
}

#[async_trait]
impl VaultHttpTransport for LifecycleTransport {
    async fn request(
        &self,
        _connection: &VaultConnection,
        _method: Method,
        path: &str,
        _token: Option<&str>,
        _body: Option<&serde_json::Value>,
    ) -> Result<VaultHttpResponse, MmError> {
        if path == "sys/health" {
            let sealed = *self.sealed.lock().map_err(|_| MmError::State {
                message: "Vault lifecycle transport lock was poisoned".to_owned(),
            })?;
            return lifecycle_response(
                200,
                serde_json::to_vec(&serde_json::json!({
                    "initialized": true,
                    "sealed": sealed,
                    "standby": false,
                    "version": "1.19.5"
                }))
                .map_err(|error| MmError::State {
                    message: error.to_string(),
                })?,
            );
        }
        if path == "sys/init" {
            return lifecycle_response(
                200,
                serde_json::to_vec(&serde_json::json!({
                    "keys_base64": ["unseal-key"],
                    "root_token": "root-token"
                }))
                .map_err(|error| MmError::State {
                    message: error.to_string(),
                })?,
            );
        }
        if path == "sys/unseal" {
            *self.sealed.lock().map_err(|_| MmError::State {
                message: "Vault lifecycle transport lock was poisoned".to_owned(),
            })? = false;
            return lifecycle_response(204, Vec::new());
        }
        if path == "sys/seal" {
            *self.sealed.lock().map_err(|_| MmError::State {
                message: "Vault lifecycle transport lock was poisoned".to_owned(),
            })? = true;
            return lifecycle_response(204, Vec::new());
        }
        if path == "sys/mounts" {
            return lifecycle_response(
                200,
                br#"{"data":{"mm-kv/":{"type":"kv","options":{"version":"2"}}}}"#.to_vec(),
            );
        }
        if path == "sys/auth" {
            return lifecycle_response(
                200,
                br#"{"data":{"mm-approle/":{"type":"approle"}}}"#.to_vec(),
            );
        }
        if path.ends_with("/role-id") {
            return lifecycle_response(200, br#"{"data":{"role_id":"role-id"}}"#.to_vec());
        }
        if path.ends_with("/secret-id") {
            return lifecycle_response(200, br#"{"data":{"secret_id":"secret-id"}}"#.to_vec());
        }
        if path.ends_with("/login") {
            return lifecycle_response(
                200,
                br#"{"auth":{"client_token":"ephemeral","renewable":true,"lease_duration":900}}"#
                    .to_vec(),
            );
        }
        if path.contains("/metadata/") {
            return lifecycle_response(404, br#"{"errors":[]}"#.to_vec());
        }
        lifecycle_response(204, Vec::new())
    }

    async fn request_bytes(
        &self,
        _connection: &VaultConnection,
        method: Method,
        path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<VaultHttpResponse, MmError> {
        if method == Method::GET && path == "sys/storage/raft/snapshot" {
            return lifecycle_response(200, b"deterministic-vault-snapshot".to_vec());
        }
        if method == Method::POST && path == "sys/storage/raft/snapshot-force" {
            *self.sealed.lock().map_err(|_| MmError::State {
                message: "Vault lifecycle transport lock was poisoned".to_owned(),
            })? = true;
            return lifecycle_response(204, Vec::new());
        }
        Err(MmError::State {
            message: format!("unexpected Vault lifecycle byte request to {path}"),
        })
    }
}

#[tokio::test]
async fn failed_upgrade_restores_previous_vault_image_without_secret_arguments()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("vault");
    let connections = Arc::new(VaultConnectionStore::new(
        temp.path().join("vault.key"),
        temp.path().join("vault.age"),
    ));
    let project_id = ProjectId(uuid::Uuid::parse_str(
        "019c527b-6242-7b43-bfd4-bd60e27c1de3",
    )?);
    let ca_pem = "test-ca".to_owned();
    connections.save_connection(
        "managed",
        VaultConnection {
            mode: VaultMode::Managed,
            address: "https://127.0.0.1:18210".to_owned(),
            namespace: None,
            mount: DEFAULT_MOUNT.to_owned(),
            auth_mount: DEFAULT_AUTH_MOUNT.to_owned(),
            ca_sha256: ca_sha256(&ca_pem),
            ca_pem: ca_pem.clone(),
            project_id,
            credentials: BTreeMap::from([(
                "local".to_owned(),
                VaultAppRoleCredential {
                    role_id: Zeroizing::new("role".to_owned()),
                    secret_id: Zeroizing::new(SENTINEL.to_owned()),
                },
            )]),
            allowed_locations: BTreeMap::from([(
                "local".to_owned(),
                vec!["application/key".to_owned()],
            )]),
            managed_installation: Some("local-vault".to_owned()),
        },
    )?;
    connections.save_recovery(
        "local-vault",
        VaultRecoveryMaterial {
            root_token: Zeroizing::new(SENTINEL.to_owned()),
            unseal_key: Zeroizing::new(SENTINEL.to_owned()),
        },
    )?;
    let process = Arc::new(QueueProcess {
        outputs: Mutex::new(
            [output(0), output(1), output(0), output(0)]
                .into_iter()
                .collect(),
        ),
        records: Mutex::new(Vec::new()),
    });
    let manager = ManagedVaultManager::new(
        root.clone(),
        connections,
        Arc::new(Healthy),
        process.clone(),
    );
    let config_directory = root.join("installations/local-vault");
    fs::create_dir_all(&config_directory)?;
    fs::write(config_directory.join("ca.pem"), ca_pem)?;
    let previous = ManagedVaultState {
        schema: STATE_SCHEMA,
        installation: "local-vault".to_owned(),
        connection: "managed".to_owned(),
        project_id,
        environment: "local".to_owned(),
        locations: vec!["application/key".to_owned()],
        image: VAULT_1_18_IMAGE.to_owned(),
        host_port: 18_210,
        container: "mm-managed-managed".to_owned(),
        volume: "mm-managed-data-managed".to_owned(),
        config_volume: "mm-managed-config-managed".to_owned(),
        config_directory,
        ingress_host: managed_host("managed"),
        service: fake_service_plan(VAULT_1_18_IMAGE, "managed", &"a".repeat(64))?,
        plan_hash: "old".to_owned(),
        journal: Journal::Applied,
        previous: None,
    };
    manager.save_state(&previous)?;
    let request = ManagedVaultRequest {
        action: ManagedSecretBackendAction::Upgrade,
        installation: previous.installation.clone(),
        connection: previous.connection.clone(),
        project_id,
        environment: previous.environment.clone(),
        locations: previous.locations.clone(),
        image: VAULT_1_19_IMAGE.to_owned(),
        host_port: previous.host_port,
        purge_data: false,
        target: "local".to_owned(),
    };
    let plan = manager.plan(request)?;
    let apply = ManagedVaultRequest {
        action: ManagedSecretBackendAction::Upgrade,
        installation: previous.installation.clone(),
        connection: previous.connection.clone(),
        project_id,
        environment: previous.environment.clone(),
        locations: previous.locations.clone(),
        image: VAULT_1_19_IMAGE.to_owned(),
        host_port: previous.host_port,
        purge_data: false,
        target: "local".to_owned(),
    };
    let error = manager
        .execute(apply, &plan.plan_hash)
        .await
        .err()
        .ok_or("injected upgrade unexpectedly succeeded")?;
    assert!(error.to_string().contains("previous image was restored"));
    assert_eq!(
        manager.load_state()?.ok_or("restored state absent")?.image,
        VAULT_1_18_IMAGE
    );
    let records = process.records.lock().map_err(|_| "record lock")?;
    assert_eq!(records.len(), 4);
    assert!(records.iter().all(|record| !record.contains(SENTINEL)));
    Ok(())
}

fn planning_manager(temp: &TempDir) -> (ManagedVaultManager, Arc<QueueProcess>, ProjectId) {
    let process = Arc::new(QueueProcess {
        outputs: Mutex::new(VecDeque::new()),
        records: Mutex::new(Vec::new()),
    });
    let project_id = ProjectId(uuid::Uuid::from_u128(17));
    (
        ManagedVaultManager::new(
            temp.path().join("vault"),
            Arc::new(VaultConnectionStore::new(
                temp.path().join("vault.key"),
                temp.path().join("vault.age"),
            )),
            Arc::new(Healthy),
            process.clone(),
        ),
        process,
        project_id,
    )
}

fn planning_request(project_id: ProjectId) -> ManagedVaultRequest {
    ManagedVaultRequest {
        action: ManagedSecretBackendAction::Install,
        installation: "local-vault".to_owned(),
        connection: "managed".to_owned(),
        project_id,
        environment: "development".to_owned(),
        locations: vec!["zeta/key".to_owned(), "alpha/key".to_owned()],
        image: VAULT_1_19_IMAGE.to_owned(),
        host_port: 18_210,
        purge_data: false,
        target: "local".to_owned(),
    }
}

fn planning_state(
    manager: &ManagedVaultManager,
    request: &ManagedVaultRequest,
    image: &str,
    journal: Journal,
) -> Result<ManagedVaultState, MmError> {
    Ok(ManagedVaultState {
        schema: STATE_SCHEMA,
        installation: request.installation.clone(),
        connection: request.connection.clone(),
        project_id: request.project_id,
        environment: request.environment.clone(),
        locations: vec!["recorded/key".to_owned()],
        image: image.to_owned(),
        host_port: request.host_port,
        container: "mm-managed-managed".to_owned(),
        volume: "mm-managed-data-managed".to_owned(),
        config_volume: "mm-managed-config-managed".to_owned(),
        config_directory: manager
            .root
            .join("installations")
            .join(&request.installation),
        ingress_host: managed_host(&request.connection),
        service: fake_service_plan(image, &request.connection, &"d".repeat(64))?,
        plan_hash: "reviewed".to_owned(),
        journal,
        previous: None,
    })
}

#[test]
fn planning_validates_lifecycle_image_scope_and_review_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let (manager, _, project_id) = planning_manager(&temp);
    let request = planning_request(project_id);
    let plan = manager.plan(request.clone())?;
    assert_eq!(plan.locations, ["alpha/key", "zeta/key"]);
    assert_eq!(plan.previous_state_digest, "absent");
    assert!(!plan.plan_hash.is_empty());

    for action in [
        ManagedSecretBackendAction::Restart,
        ManagedSecretBackendAction::Upgrade,
        ManagedSecretBackendAction::Seal,
        ManagedSecretBackendAction::Unseal,
        ManagedSecretBackendAction::Remove,
    ] {
        let mut candidate = request.clone();
        candidate.action = action;
        assert!(manager.plan(candidate).is_err());
    }
    for mutate in ["installation", "connection", "environment", "image"] {
        let mut candidate = request.clone();
        match mutate {
            "installation" => candidate.installation = "../escape".to_owned(),
            "connection" => candidate.connection = "bad/name".to_owned(),
            "environment" => candidate.environment = "bad/name".to_owned(),
            _ => candidate.image = "vault:latest".to_owned(),
        }
        assert!(manager.plan(candidate).is_err());
    }
    let mut purge = request.clone();
    purge.purge_data = true;
    assert!(manager.plan(purge).is_err());

    manager.save_state(&planning_state(
        &manager,
        &request,
        VAULT_1_19_IMAGE,
        Journal::Applied,
    )?)?;
    assert!(manager.plan(request.clone()).is_err());

    let mut unsupported_upgrade = request.clone();
    unsupported_upgrade.action = ManagedSecretBackendAction::Upgrade;
    assert!(manager.plan(unsupported_upgrade).is_err());

    let mut changed_without_upgrade = request.clone();
    changed_without_upgrade.action = ManagedSecretBackendAction::Restart;
    changed_without_upgrade.image = VAULT_1_18_IMAGE.to_owned();
    assert!(manager.plan(changed_without_upgrade).is_err());

    let mut changed_scope = request;
    changed_scope.action = ManagedSecretBackendAction::Restart;
    changed_scope.host_port += 1;
    assert!(manager.plan(changed_scope).is_err());
    Ok(())
}

#[tokio::test]
async fn planning_accepts_reviewed_upgrade_and_reports_absent_and_retained_states()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let (manager, process, project_id) = planning_manager(&temp);
    let mut request = planning_request(project_id);
    let absent = manager.status().await?;
    assert!(!absent.installed);
    assert_eq!(absent.diagnostics.len(), 1);

    manager.save_state(&planning_state(
        &manager,
        &request,
        VAULT_1_18_IMAGE,
        Journal::Retained,
    )?)?;
    let retained = manager.status().await?;
    assert!(!retained.installed);
    assert_eq!(retained.journal.as_deref(), Some("retained"));
    assert!(!retained.recovery);

    request.action = ManagedSecretBackendAction::Upgrade;
    request.image = VAULT_1_19_IMAGE.to_owned();
    let upgrade = manager.plan(request.clone())?;
    assert_eq!(upgrade.previous_image.as_deref(), Some(VAULT_1_18_IMAGE));
    assert!(manager.execute(request, "stale-review").await.is_err());
    assert!(
        process
            .records
            .lock()
            .map_err(|_| "record lock")?
            .is_empty()
    );

    let digest_file = temp.path().join("digest.bin");
    fs::write(&digest_file, b"digest-input")?;
    assert!(manager.path_digest(&digest_file)?.is_some());
    assert!(manager.path_digest(&temp.path().join("missing"))?.is_none());
    assert!(manager.state_digest().is_err());
    Ok(())
}

#[tokio::test]
async fn installation_rejects_reserved_ports_and_malformed_initialization_responses()
-> Result<(), Box<dyn std::error::Error>> {
    let occupied = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let port = occupied.local_addr()?.port();
    let temp = crate::test_support::tempdir()?;
    let (manager, _, project_id) = planning_manager(&temp);
    let mut request = planning_request(project_id);
    request.host_port = port;
    let plan = manager.plan(request.clone())?;
    assert!(manager.execute(request, &plan.plan_hash).await.is_err());
    drop(occupied);

    for (status, body) in [
        (500, br#"{"errors":["failed"]}"#.to_vec()),
        (200, b"not-json".to_vec()),
        (200, br#"{"keys_base64":[],"root_token":"token"}"#.to_vec()),
    ] {
        let temp = crate::test_support::tempdir()?;
        let connections = Arc::new(VaultConnectionStore::new(
            temp.path().join("vault.key"),
            temp.path().join("vault.age"),
        ));
        let process = Arc::new(QueueProcess {
            outputs: Mutex::new(VecDeque::from([output(0), output(0)])),
            records: Mutex::new(Vec::new()),
        });
        let manager = ManagedVaultManager::new(
            temp.path().join("vault"),
            connections,
            Arc::new(FailingInitializationTransport { status, body }),
            process.clone(),
        );
        let request = planning_request(ProjectId(uuid::Uuid::new_v4()));
        let plan = manager.plan(request.clone())?;
        assert!(manager.execute(request, &plan.plan_hash).await.is_err());
        assert!(manager.load_state()?.is_none());
        assert_eq!(process.records.lock().map_err(|_| "record lock")?.len(), 2);
    }
    Ok(())
}

#[test]
fn managed_vault_configuration_and_plan_integrity_helpers_cover_fixed_contracts()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt as _;

    assert!(supported_image(VAULT_1_18_IMAGE));
    assert!(supported_image(VAULT_1_19_IMAGE));
    assert!(!supported_image("vault:latest"));
    assert_eq!(managed_host("team"), "team.vault.mm.test");

    let temp = crate::test_support::tempdir()?;
    let (manager, _, project_id) = planning_manager(&temp);
    let request = planning_request(project_id);
    let plan = manager.plan(request.clone())?;
    plan.validate()?;
    assert_eq!(plan.hash()?, plan.plan_hash);
    let generation = lifecycle_generation(&request, "absent")?;
    assert_eq!(generation.len(), 64);
    assert_ne!(generation, lifecycle_generation(&request, "changed")?);

    for mutation in 0..10 {
        let mut changed = plan.clone();
        match mutation {
            0 => changed.schema += 1,
            1 => changed.image = "vault:latest".to_owned(),
            2 => changed.host_port = 0,
            3 => changed.ingress_host = "wrong.test".to_owned(),
            4 => changed.management_access = "public".to_owned(),
            5 => changed.locations.clear(),
            6 => changed.service.identity.provider_instance = "other".to_owned(),
            7 => changed.service.identity.component = "openbao".to_owned(),
            8 => changed.service.identity.data_volume = "other".to_owned(),
            _ => changed.service.identity.config_volume = "other".to_owned(),
        }
        assert!(changed.validate().is_err());
    }
    let mut changed_hash = plan.clone();
    changed_hash.plan_hash = "0".repeat(64);
    assert!(changed_hash.validate().is_err());

    let config = temp.path().join("configuration");
    let ca_pem = prepare_configuration(
        &config,
        request.host_port,
        &request.installation,
        &managed_host(&request.connection),
        &plan.service.identity.service_name,
    )?;
    assert!(ca_pem.contains("BEGIN CERTIFICATE"));
    for name in ["ca.pem", "server.pem", "server-key.pem", "config.hcl"] {
        let path = config.join(name);
        assert!(path.is_file());
        assert_eq!(fs::metadata(path)?.permissions().mode() & 0o077, 0);
    }
    assert!(fs::read_to_string(config.join("config.hcl"))?.contains("disable_mlock = true"));

    let service = service_request(&request, &config, generation, RuntimeKind::DockerCompose);
    assert_eq!(service.component, "vault");
    assert_eq!(service.config_files.len(), 4);
    assert_eq!(service.ingress_files.len(), 4);
    prepare_ingress(&config, &plan.service, &plan.ingress_host)?;
    let route = fs::read_to_string(config.join("vault.dynamic.yaml"))?;
    assert!(route.contains(&plan.ingress_host));
    assert!(route.contains("ipAllowList"));
    let mut incomplete = plan.service.clone();
    incomplete.service.ingress_files.pop();
    assert!(prepare_ingress(&config, &incomplete, &plan.ingress_host).is_err());

    let state = planning_state(&manager, &request, VAULT_1_19_IMAGE, Journal::Applied)?;
    assert_eq!(state.digest()?.len(), 64);
    let primary = MmError::Secret {
        message: "primary".to_owned(),
    };
    assert!(matches!(
        with_cleanup_failures(primary, Vec::new(), "operation", false),
        MmError::Secret { .. }
    ));
    for restored in [false, true] {
        assert!(matches!(
            with_cleanup_failures(
                MmError::Secret {
                    message: "primary".to_owned()
                },
                vec!["cleanup".to_owned()],
                "operation",
                restored,
            ),
            MmError::OperationFailure { .. }
        ));
    }
    assert!(format!("{manager:?}").contains("ManagedVaultManager"));
    Ok(())
}

#[tokio::test]
async fn managed_vault_lifecycle_installs_backs_up_restores_upgrades_and_purges()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let project_id = ProjectId(uuid::Uuid::from_u128(91));
    let connections = Arc::new(VaultConnectionStore::new(
        temp.path().join("vault.key"),
        temp.path().join("vault.age"),
    ));
    let transport = Arc::new(LifecycleTransport {
        sealed: Mutex::new(true),
    });
    let process = Arc::new(QueueProcess {
        outputs: Mutex::new((0..8).map(|_| output(0)).collect()),
        records: Mutex::new(Vec::new()),
    });
    let manager = ManagedVaultManager::new(
        temp.path().join("vault"),
        connections.clone(),
        transport,
        process.clone(),
    );
    let mut request = ManagedVaultRequest {
        action: ManagedSecretBackendAction::Install,
        installation: "lifecycle-vault".to_owned(),
        connection: "lifecycle".to_owned(),
        project_id,
        environment: "development".to_owned(),
        locations: vec!["application/password".to_owned()],
        image: VAULT_1_18_IMAGE.to_owned(),
        host_port: 28_210,
        purge_data: false,
        target: "local".to_owned(),
    };
    let install = manager.plan(request.clone())?;
    assert_eq!(
        manager.execute(request.clone(), &install.plan_hash).await?,
        (false, false)
    );
    assert_eq!(manager.applied_state()?.journal, Journal::Applied);
    assert!(connections.has_connection("lifecycle")?);
    assert!(connections.has_recovery("lifecycle-vault")?);
    let status = manager.status().await?;
    assert!(status.installed);
    assert!(status.health.is_some_and(|health| health.healthy));
    assert!(status.diagnostics.is_empty());

    let backup = temp.path().join("vault-backup.age");
    let backup_digest = manager.backup(&backup).await?;
    assert_eq!(backup_digest.len(), 64);
    assert_eq!(manager.restore(&backup).await?, backup_digest);

    request.action = ManagedSecretBackendAction::Seal;
    let seal = manager.plan(request.clone())?;
    manager.execute(request.clone(), &seal.plan_hash).await?;
    request.action = ManagedSecretBackendAction::Unseal;
    let unseal = manager.plan(request.clone())?;
    manager.execute(request.clone(), &unseal.plan_hash).await?;

    request.action = ManagedSecretBackendAction::Restart;
    let restart = manager.plan(request.clone())?;
    manager.execute(request.clone(), &restart.plan_hash).await?;

    request.action = ManagedSecretBackendAction::Upgrade;
    request.image = VAULT_1_19_IMAGE.to_owned();
    let upgrade = manager.plan(request.clone())?;
    manager.execute(request.clone(), &upgrade.plan_hash).await?;
    assert_eq!(manager.applied_state()?.image, VAULT_1_19_IMAGE);

    request.action = ManagedSecretBackendAction::Remove;
    let retain = manager.plan(request.clone())?;
    assert_eq!(
        manager.execute(request.clone(), &retain.plan_hash).await?,
        (false, false)
    );
    assert_eq!(
        manager
            .load_state()?
            .ok_or("retained state missing")?
            .journal,
        Journal::Retained
    );

    request.action = ManagedSecretBackendAction::Install;
    let reinstall = manager.plan(request.clone())?;
    manager
        .execute(request.clone(), &reinstall.plan_hash)
        .await?;
    request.action = ManagedSecretBackendAction::Remove;
    request.purge_data = true;
    let purge = manager.plan(request.clone())?;
    assert_eq!(
        manager.execute(request, &purge.plan_hash).await?,
        (false, true)
    );
    assert!(manager.load_state()?.is_none());
    assert!(!connections.has_connection("lifecycle")?);
    assert!(!connections.has_recovery("lifecycle-vault")?);
    assert!(process.records.lock().map_err(|_| "record lock")?.len() >= 7);
    Ok(())
}

fn output(code: i32) -> CommandOutput {
    CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(String::new()),
        stderr: Zeroizing::new(String::new()),
        truncated: false,
    }
}
