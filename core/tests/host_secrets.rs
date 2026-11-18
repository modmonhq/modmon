//! A deployment host that resolves secrets from OpenBao or Vault itself:
//! `mmd secrets connect` keeps the AppRole credential on the host, the
//! provider adapters use that connection only at the address the commit
//! declares, and a node of a cluster asks the store only for what its own
//! services use.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::MmError;
use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::environment::{
    EnvironmentIndexer, FilesystemEnvironmentArtifactStore, FilesystemMaterializedEnvironmentWriter,
};
use mm::adapters::secrets::{
    AgeSecretStore, HostProviderConnect, HostProviderDriver, OpenBaoConnection,
    OpenBaoConnectionStore, OpenBaoHttpResponse, OpenBaoHttpTransport, OpenBaoMode,
    OpenBaoSecretStore, SecretProviderRegistry, StorageProviderRegistration, VaultConnection,
    VaultConnectionStore, VaultHttpResponse, VaultHttpTransport, VaultMode, VaultSecretStore,
    age_storage_descriptor, approle_from_lines, ca_sha256, connect_host_openbao,
    connect_host_vault, disconnect_host_openbao, disconnect_host_vault, host_connection_name,
    openbao_storage_descriptor, podman_native_secret_descriptor,
};
use mm::adapters::state::SqliteStateStore;
use mm::app::ports::{
    SecretBatchReplacer, SecretCandidateLifecycle, SecretCreator, SecretDeliveryCapabilityCatalog,
    SecretMaterialResolver, SecretMetadataInspector, SecretRemover, SecretReplacer,
};
use mm::app::{
    EnvironmentDependencies, EnvironmentManager, NodeDocuments, SECRET_STORE_UNAVAILABLE,
    WorkspaceSelection,
};
use mm::domain::environment::EnvironmentReview;
use mm::domain::placement::NodeScope;
use mm::domain::repo_target::RepoTargetConfig;
use mm::domain::runtime::RuntimeKind;
use mm::domain::secret::{SecretReference, SecretScope, SecretStatus};
use mm::domain::secret_backend::ProviderInstanceName;
use mm::domain::secret_contract::{
    DeliveryCapabilityDescriptor, DeliveryDriverId, StorageProviderId,
};
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId};
use mm::domain::workspace::ProjectId;
use reqwest::Method;
use uuid::Uuid;
use zeroize::Zeroizing;

mod temp_dirs;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const PROJECT: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de3";
const OTHER_PROJECT: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de4";
const PROVIDER: &str = "team-secrets";
const ON_HOST: &str = "https://127.0.0.1:8200";
const CA_PEM: &str = "-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n";
/// Not credentials of anything: the fake store accepts exactly these.
const ROLE_ID: &str = "test-role-id-of-the-fake-store";
const SECRET_ID: &str = "test-secret-id-of-the-fake-store";
const FIRST_VALUE: &str = "host-secrets-sentinel-first";
const SECOND_VALUE: &str = "host-secrets-sentinel-second";

/// An OpenBao or Vault as the adapters see one: KV version 2 behind an
/// AppRole login that takes one role ID and secret ID.
#[derive(Default)]
struct Store {
    state: Mutex<StoreState>,
}

#[derive(Default)]
struct StoreState {
    values: BTreeMap<String, (u64, String)>,
    requests: Vec<String>,
    down: bool,
    vault: bool,
    /// The secret ID was destroyed in the store: no login with it works.
    revoked: bool,
}

impl Store {
    fn state(&self) -> Result<std::sync::MutexGuard<'_, StoreState>, MmError> {
        self.state.lock().map_err(|_| MmError::State {
            message: "fake store lock poisoned".to_owned(),
        })
    }

    /// Writes a new version of the value at `path`, as rotating it does.
    fn put(&self, path: &str, value: &str) -> TestResult {
        let mut state = self.state()?;
        let version = state.values.get(path).map_or(1, |(version, _)| version + 1);
        state
            .values
            .insert(path.to_owned(), (version, value.to_owned()));
        Ok(())
    }

    fn requests(&self) -> Result<Vec<String>, MmError> {
        Ok(self.state()?.requests.clone())
    }

    fn forget_requests(&self) -> TestResult {
        self.state()?.requests.clear();
        Ok(())
    }

    /// A store that reports a Vault release.
    fn vault() -> Self {
        Self {
            state: Mutex::new(StoreState {
                vault: true,
                ..StoreState::default()
            }),
        }
    }

    /// Destroys the secret ID, as `bao write auth/…/secret-id/destroy` does.
    fn revoke(&self) -> TestResult {
        self.state()?.revoked = true;
        Ok(())
    }

    fn set_down(&self, down: bool) -> TestResult {
        self.state()?.down = down;
        Ok(())
    }

    fn answer(
        &self,
        method: &Method,
        path: &str,
        token: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<(u16, serde_json::Value), MmError> {
        let mut state = self.state()?;
        state.requests.push(format!("{method} {path}"));
        if state.down {
            return Err(MmError::Secret {
                message: "provider is unavailable or its TLS identity could not be verified"
                    .to_owned(),
            });
        }
        if path == "sys/health" {
            return Ok((
                200,
                serde_json::json!({
                    "initialized": true,
                    "sealed": false,
                    "standby": false,
                    "version": if state.vault { "1.19.5" } else { "2.4.4" }
                }),
            ));
        }
        if path.ends_with("/login") {
            let given = |key: &str| {
                body.and_then(|body| body.get(key))
                    .and_then(serde_json::Value::as_str)
            };
            // What OpenBao 2.4.4 answered on the test cluster: 400, not 403.
            if given("role_id") != Some(ROLE_ID)
                || given("secret_id") != Some(SECRET_ID)
                || state.revoked
            {
                return Ok((
                    400,
                    serde_json::json!({"errors": ["invalid role or secret ID"]}),
                ));
            }
            return Ok((
                200,
                serde_json::json!({"auth": {
                    "client_token": "short-lived-test-token",
                    "renewable": true,
                    "lease_duration": 900
                }}),
            ));
        }
        if token.is_none() {
            return Ok((403, serde_json::json!({"errors": ["permission denied"]})));
        }
        if path.contains("/metadata/") {
            let data_path = path.replacen("/metadata/", "/data/", 1);
            return Ok(state.values.get(&data_path).map_or_else(
                || (404, serde_json::json!({"errors": []})),
                |(version, _)| {
                    (
                        200,
                        serde_json::json!({"data": {
                            "current_version": version,
                            "versions": {version.to_string(): {
                                "created_time": "2026-08-23T12:00:00Z",
                                "deletion_time": "",
                                "destroyed": false
                            }}
                        }}),
                    )
                },
            ));
        }
        if path.contains("/data/") && *method == Method::GET {
            return Ok(state.values.get(path).map_or_else(
                || (404, serde_json::json!({"errors": []})),
                |(version, value)| {
                    (
                        200,
                        serde_json::json!({"data": {
                            "data": {"value": value},
                            "metadata": {
                                "created_time": "2026-08-23T12:00:00Z",
                                "version": version
                            }
                        }}),
                    )
                },
            ));
        }
        Ok((404, serde_json::json!({"errors": []})))
    }
}

fn body_of(value: &serde_json::Value) -> Result<Zeroizing<Vec<u8>>, MmError> {
    serde_json::to_vec(value)
        .map(Zeroizing::new)
        .map_err(|error| MmError::State {
            message: format!("could not serialize a fake answer: {error}"),
        })
}

#[async_trait]
impl OpenBaoHttpTransport for Store {
    async fn request(
        &self,
        _connection: &OpenBaoConnection,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<OpenBaoHttpResponse, MmError> {
        let (status, value) = self.answer(&method, path, token, body)?;
        Ok(OpenBaoHttpResponse {
            status,
            headers: BTreeMap::new(),
            body: body_of(&value)?,
        })
    }

    async fn request_bytes(
        &self,
        _connection: &OpenBaoConnection,
        _method: Method,
        _path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<OpenBaoHttpResponse, MmError> {
        Err(MmError::State {
            message: "a host never sends the store a snapshot".to_owned(),
        })
    }
}

#[async_trait]
impl VaultHttpTransport for Store {
    async fn request(
        &self,
        _connection: &VaultConnection,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<VaultHttpResponse, MmError> {
        let (status, value) = self.answer(&method, path, token, body)?;
        Ok(VaultHttpResponse {
            status,
            headers: BTreeMap::new(),
            body: body_of(&value)?,
        })
    }

    async fn request_bytes(
        &self,
        _connection: &VaultConnection,
        _method: Method,
        _path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<VaultHttpResponse, MmError> {
        Err(MmError::State {
            message: "a host never sends the store a snapshot".to_owned(),
        })
    }
}

fn project(value: &str) -> Result<ProjectId, uuid::Error> {
    Uuid::parse_str(value).map(ProjectId)
}

fn provider() -> Result<ProviderInstanceName, String> {
    ProviderInstanceName::new(PROVIDER)
}

fn connect(
    project: ProjectId,
    environment: &str,
    address: &str,
) -> TestResult<HostProviderConnect> {
    Ok(HostProviderConnect {
        project,
        environment: environment.to_owned(),
        provider: provider()?,
        address: address.to_owned(),
        namespace: None,
        storage_mount: "mm-kv".to_owned(),
        auth_mount: "mm-approle".to_owned(),
        ca_pem: CA_PEM.to_owned(),
        role_id: Zeroizing::new(ROLE_ID.to_owned()),
        secret_id: Zeroizing::new(SECRET_ID.to_owned()),
    })
}

fn openbao_connections(home: &Path) -> Arc<OpenBaoConnectionStore> {
    Arc::new(OpenBaoConnectionStore::new(
        home.join("identities/openbao-connections.key"),
        home.join("secrets/openbao-connections.age"),
    ))
}

fn reference(location: &str) -> TestResult<SecretReference> {
    Ok(SecretReference::new(
        StorageProviderId::new(PROVIDER)?,
        location,
    )?)
}

fn data_path(project: &str, environment: &str, location: &str) -> String {
    format!("mm-kv/data/mm/{project}/{environment}/{location}")
}

/// Every regular file under `root`.
fn files_under(root: &Path) -> TestResult<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else {
                files.push(entry.path());
            }
        }
    }
    Ok(files)
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

#[tokio::test]
async fn a_host_keeps_the_approle_it_was_given_and_never_gives_it_back() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let connections = openbao_connections(home.path());
    let store = Arc::new(Store::default());
    let ours = project(PROJECT)?;
    store.put(
        &data_path(PROJECT, "production", "database/password"),
        FIRST_VALUE,
    )?;

    let request = connect(ours, "production", ON_HOST)?;
    let shown = format!("{request:?}");
    assert!(!shown.contains(ROLE_ID) && !shown.contains(SECRET_ID));
    let answer = connect_host_openbao(Arc::clone(&connections), store.clone(), request).await?;
    assert!(answer.authenticated);
    assert_eq!(answer.driver, HostProviderDriver::OpenBao);
    assert_eq!(answer.address, ON_HOST);
    assert_eq!(answer.trust_sha256, ca_sha256(CA_PEM));
    assert_eq!(answer.environments, ["production"]);
    let printed = serde_json::to_string(&answer)?;
    assert!(!printed.contains(ROLE_ID) && !printed.contains(SECRET_ID));

    // What is on disk is for the owner only and encrypted.
    let stored = files_under(home.path())?;
    assert!(!stored.is_empty());
    for path in &stored {
        let metadata = fs::metadata(path)?;
        assert_eq!(
            metadata.permissions().mode() & 0o077,
            0,
            "{} is readable by others",
            path.display()
        );
        let bytes = fs::read(path)?;
        assert!(!contains(&bytes, ROLE_ID) && !contains(&bytes, SECRET_ID));
    }
    // The credential went to the login and nowhere else.
    assert!(
        store
            .requests()?
            .iter()
            .all(|request| !request.contains(ROLE_ID) && !request.contains(SECRET_ID))
    );

    // The daemon's provider adapter finds the host's connection by project.
    let provider_store = OpenBaoSecretStore::new(
        provider()?,
        OpenBaoMode::External,
        ours,
        Arc::clone(&connections),
        store.clone(),
    );
    let production = SecretScope::new("production");
    let resolved = provider_store
        .resolve(&production, &reference("database/password")?)
        .await?;
    assert_eq!(resolved.metadata.revision.as_deref(), Some("1"));
    assert!(resolved.material.is_some());
    assert!(!format!("{:?}", resolved.material).contains(FIRST_VALUE));

    // Another project on the same host has no connection of its own, and
    // an environment the host was not connected for has none either. Both
    // refusals name the command.
    let theirs = OpenBaoSecretStore::new(
        provider()?,
        OpenBaoMode::External,
        project(OTHER_PROJECT)?,
        Arc::clone(&connections),
        store.clone(),
    );
    for refused in [
        theirs
            .inspect(&production, &reference("database/password")?)
            .await,
        provider_store
            .inspect(
                &SecretScope::new("staging"),
                &reference("database/password")?,
            )
            .await,
    ] {
        let message = refused
            .err()
            .ok_or("a missing connection was used")?
            .to_string();
        assert!(
            message.contains("mmd secrets connect --project"),
            "{message}"
        );
        assert!(message.contains("--provider team-secrets"), "{message}");
    }

    // A second project connects under the same provider name.
    let mut second = connect(project(OTHER_PROJECT)?, "production", ON_HOST)?;
    second.provider = provider()?;
    connect_host_openbao(Arc::clone(&connections), store.clone(), second).await?;
    assert!(connections.has_connection(&host_connection_name(PROVIDER, ours)?)?);
    assert!(connections.has_connection(&host_connection_name(PROVIDER, project(OTHER_PROJECT)?)?)?);
    assert_eq!(
        theirs
            .inspect(&production, &reference("database/password")?)
            .await?
            .status,
        SecretStatus::Unset
    );

    // Forgetting one project's connection leaves the other's.
    assert!(disconnect_host_openbao(&connections, &provider()?, ours)?);
    assert!(!disconnect_host_openbao(&connections, &provider()?, ours)?);
    assert!(
        provider_store
            .inspect(&production, &reference("database/password")?)
            .await
            .is_err()
    );
    assert!(
        theirs
            .inspect(&production, &reference("database/password")?)
            .await
            .is_ok()
    );
    Ok(())
}

#[tokio::test]
async fn a_connection_the_store_does_not_accept_is_not_kept() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let connections = openbao_connections(home.path());
    let store = Arc::new(Store::default());
    let ours = project(PROJECT)?;
    let name = host_connection_name(PROVIDER, ours)?;

    // A wrong secret ID.
    let mut wrong = connect(ours, "production", ON_HOST)?;
    wrong.secret_id = Zeroizing::new("not-the-secret-id".to_owned());
    let denied = connect_host_openbao(Arc::clone(&connections), store.clone(), wrong)
        .await
        .err()
        .ok_or("a refused login was stored")?
        .to_string();
    assert!(denied.contains("denied"), "{denied}");
    assert!(!denied.contains("not-the-secret-id"));
    assert!(!connections.has_connection(&name)?);

    // A store the host does not reach.
    store.set_down(true)?;
    let unreachable = connect_host_openbao(
        Arc::clone(&connections),
        store.clone(),
        connect(ours, "production", ON_HOST)?,
    )
    .await
    .err()
    .ok_or("an unreachable store was stored")?
    .to_string();
    assert!(unreachable.contains("nothing was stored"), "{unreachable}");
    assert!(unreachable.contains("--ca-file"), "{unreachable}");
    assert!(!connections.has_connection(&name)?);
    store.set_down(false)?;

    // A second environment joins the connection; a refused replacement of
    // the first leaves what worked.
    connect_host_openbao(
        Arc::clone(&connections),
        store.clone(),
        connect(ours, "production", ON_HOST)?,
    )
    .await?;
    let both = connect_host_openbao(
        Arc::clone(&connections),
        store.clone(),
        connect(ours, "staging", ON_HOST)?,
    )
    .await?;
    assert_eq!(both.environments, ["production", "staging"]);
    let mut replaced = connect(ours, "production", ON_HOST)?;
    replaced.role_id = Zeroizing::new("not-the-role-id".to_owned());
    assert!(
        connect_host_openbao(Arc::clone(&connections), store.clone(), replaced)
            .await
            .is_err()
    );
    let kept = connections.connection(&name)?;
    assert_eq!(kept.credentials["production"].role_id.as_str(), ROLE_ID);
    assert_eq!(kept.credentials.len(), 2);

    // Another address needs the old connection gone first.
    let moved = connect_host_openbao(
        Arc::clone(&connections),
        store.clone(),
        connect(ours, "production", "https://10.90.0.1:8200")?,
    )
    .await
    .err()
    .ok_or("a connection moved to another address in place")?
    .to_string();
    assert!(
        moved.contains("mmd secrets disconnect --project"),
        "{moved}"
    );
    assert_eq!(connections.connection(&name)?.address, ON_HOST);
    Ok(())
}

#[test]
fn the_credential_is_two_lines_on_stdin() -> TestResult {
    let (role_id, secret_id) =
        approle_from_lines(&Zeroizing::new(format!("{ROLE_ID}\n\n  {SECRET_ID}  \n")))?;
    assert_eq!(role_id.as_str(), ROLE_ID);
    assert_eq!(secret_id.as_str(), SECRET_ID);
    for malformed in ["", "only-one-line\n", "one\ntwo\nthree\n"] {
        let refused = approle_from_lines(&Zeroizing::new(malformed.to_owned()))
            .err()
            .ok_or("malformed input was read as a credential")?
            .to_string();
        assert!(refused.contains("two lines"), "{refused}");
        assert!(!refused.contains("only-one-line") && !refused.contains("three"));
    }
    Ok(())
}

#[tokio::test]
async fn a_host_connects_to_vault_the_same_way() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let connections = Arc::new(VaultConnectionStore::new(
        home.path().join("identities/vault-connections.key"),
        home.path().join("secrets/vault-connections.age"),
    ));
    let store = Arc::new(Store::vault());
    let ours = project(PROJECT)?;
    store.put(
        &data_path(PROJECT, "production", "database/password"),
        FIRST_VALUE,
    )?;
    let answer = connect_host_vault(
        Arc::clone(&connections),
        store.clone(),
        connect(ours, "production", ON_HOST)?,
    )
    .await?;
    assert_eq!(answer.driver, HostProviderDriver::Vault);
    assert!(!serde_json::to_string(&answer)?.contains(SECRET_ID));
    for path in files_under(home.path())? {
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o077, 0);
        assert!(!contains(&fs::read(&path)?, SECRET_ID));
    }

    let declared = declared_addresses(CONFIG)?;
    let vault = VaultSecretStore::new(
        provider()?,
        VaultMode::External,
        ours,
        Arc::clone(&connections),
        store.clone(),
    )
    .declared_at(declared.get(&provider()?).cloned());
    // The commit declares the mesh address for production; the host is
    // connected on loopback.
    let refused = vault
        .inspect(
            &SecretScope::new("production"),
            &reference("database/password")?,
        )
        .await
        .err()
        .ok_or("a host read a store the commit does not declare")?
        .to_string();
    assert!(refused.contains("https://10.90.0.1:8200"), "{refused}");
    assert!(refused.contains("targets.<target>.providers.team-secrets.address"));
    assert!(!refused.contains(ON_HOST), "{refused}");

    assert!(disconnect_host_vault(&connections, &provider()?, ours)?);
    connect_host_vault(
        Arc::clone(&connections),
        store.clone(),
        connect(ours, "production", "https://10.90.0.1:8200/")?,
    )
    .await?;
    let read = vault
        .resolve(
            &SecretScope::new("production"),
            &reference("database/password")?,
        )
        .await?;
    assert_eq!(read.metadata.status, SecretStatus::Set);
    Ok(())
}

/// A project on a cluster of two nodes: `api` on app-1 reads two secrets
/// from the store, `web` on app-2 reads none.
const CONFIG: &str = "schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: host-secrets
default-environment: production
target: local
runtime:
  driver: podman-compose
stacks:
  app:
    files: [compose.yaml]
modules:
  api:
    path: api
    env:
      template: .env.example
    services:
      api:
        stack: app
        placement: app-1
        secret-files:
          api-token: {secret: api.token, uid: \"0\", gid: \"0\", mode: 292}
  web:
    path: web
    services:
      web: {stack: app, placement: app-2}
providers:
  team-secrets:
    driver: openbao
    scope: project
    mode: external
    address: https://bao.example.test:8200
    storage-mount: mm-kv
    automation-auth-mount: mm-approle
    trust-bundle: bao-ca.pem
    expected-trust-sha256: TRUST_SHA256
secrets:
  database.password:
    storage: {provider: team-secrets, location: database/password}
  api.token:
    storage: {provider: team-secrets, location: api/token}
targets:
  cluster:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
    providers:
      team-secrets: {address: \"https://10.90.0.1:8200\"}
environments:
  production:
    target: cluster
    bindings:
      DB.PASSWORD: {secret: database.password}
  staging:
    bindings:
      DB.PASSWORD: {secret: database.password}
";

const COMPOSE: &str = "services:
  api:
    image: docker.io/library/alpine:3.22
    command: [\"sleep\", \"infinity\"]
  web:
    image: docker.io/library/alpine:3.22
    command: [\"sleep\", \"infinity\"]
";

fn write_project(root: &Path, config: &str) -> TestResult {
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("api"))?;
    fs::create_dir_all(root.join("web"))?;
    fs::write(
        root.join(".config/mm.yaml"),
        config.replace("TRUST_SHA256", &ca_sha256(CA_PEM)),
    )?;
    fs::write(root.join("compose.yaml"), COMPOSE)?;
    fs::write(root.join("bao-ca.pem"), CA_PEM)?;
    fs::write(
        root.join("api/.env.example"),
        "DB_PASSWORD= # mm:from=DB.PASSWORD required\n",
    )?;
    Ok(())
}

fn declared_addresses(
    config: &str,
) -> TestResult<BTreeMap<ProviderInstanceName, mm::domain::repo_target::DeclaredProviderAddress>> {
    let temporary = temp_dirs::tempdir()?;
    write_project(temporary.path(), config)?;
    Ok(ConfigLoader
        .load_from(temporary.path())?
        .config
        .declared_provider_addresses())
}

#[test]
fn a_target_declares_where_its_hosts_read_a_provider() -> TestResult {
    let declared = declared_addresses(CONFIG)?;
    let address = declared
        .get(&provider()?)
        .ok_or("the external provider declares no address")?;
    // The environment on the cluster target reads the mesh address; one on
    // another target reads the provider's own.
    assert_eq!(
        address.for_environment("production"),
        "https://10.90.0.1:8200"
    );
    assert_eq!(
        address.for_environment("staging"),
        "https://bao.example.test:8200"
    );
    assert!(address.matches("production", "https://10.90.0.1:8200/"));
    assert!(!address.matches("production", ON_HOST));

    let target = |address: &str| -> Result<(), String> {
        serde_yaml_ng::from_str::<RepoTargetConfig>(&format!(
            "host: 203.0.113.10\nhost-key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w\ndaemon-fingerprint: {}\nproviders:\n  team-secrets: {{address: \"{address}\"}}\n",
            "a".repeat(64)
        ))
        .map_err(|error| error.to_string())?
        .validate("production")
    };
    assert_eq!(target(ON_HOST), Ok(()));
    for (address, expected) in [
        ("http://127.0.0.1:8200", "must start with https://"),
        ("https://127.0.0.1:8200/v1", "without a path"),
        (
            "https://reader@bao.example.test",
            "may not contain credentials",
        ),
    ] {
        let problem = target(address).err().ok_or("a bad address was accepted")?;
        assert!(
            problem.contains("target 'production' providers.team-secrets.address")
                && problem.contains(expected),
            "{problem}"
        );
    }
    Ok(())
}

#[test]
fn validation_refuses_a_target_provider_the_project_does_not_declare() -> TestResult {
    let findings = |config: &str| -> TestResult<Vec<String>> {
        let temporary = temp_dirs::tempdir()?;
        let root = temporary.path().join("project");
        write_project(&root, config)?;
        let output = temp_dirs::mm()
            .arg("--root")
            .arg(&root)
            .arg("--mm-home")
            .arg(temporary.path().join("home"))
            .args(["--output", "json", "config", "validate"])
            .output()?;
        let report: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                format!(
                    "mm config validate printed no report ({error}): {}",
                    String::from_utf8_lossy(&output.stderr)
                )
            })?;
        Ok(report["validation"]["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|diagnostic| diagnostic["code"] == "MM_REPO_TARGET_INVALID")
            .filter_map(|diagnostic| diagnostic["message"].as_str().map(str::to_owned))
            .collect())
    };
    assert_eq!(findings(CONFIG)?, Vec::<String>::new());
    let unknown = CONFIG.replace(
        "      team-secrets: {address: \"https://10.90.0.1:8200\"}",
        "      other-secrets: {address: \"https://10.90.0.1:8200\"}",
    );
    let found = findings(&unknown)?;
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("'other-secrets', which is not an external provider under providers:"),
        "{found:?}"
    );
    Ok(())
}

struct Deliveries(Vec<DeliveryCapabilityDescriptor>);

impl SecretDeliveryCapabilityCatalog for Deliveries {
    fn delivery_capabilities(&self) -> Vec<DeliveryCapabilityDescriptor> {
        self.0.clone()
    }

    fn delivery_capability(
        &self,
        driver: &DeliveryDriverId,
    ) -> Result<DeliveryCapabilityDescriptor, MmError> {
        self.0
            .iter()
            .find(|descriptor| descriptor.id == *driver)
            .cloned()
            .ok_or_else(|| MmError::Secret {
                message: format!("test delivery driver '{driver}' is unavailable"),
            })
    }
}

/// One node of the cluster: its own mm home, its own connection store, and
/// the environment service `mmd` plans with, reading the configuration as
/// that node sees it.
struct Node {
    _home: tempfile::TempDir,
    connections: Arc<OpenBaoConnectionStore>,
    environments: EnvironmentManager,
    selection: WorkspaceSelection,
}

impl Node {
    fn new(name: &str, root: &Path, store: &Arc<Store>) -> TestResult<Self> {
        Self::planning(Some(name), "production", root, store)
    }

    /// `node`: the cluster node `mmd` plans for; `None` is the machine `mm`
    /// runs on, planning `environment` itself.
    fn planning(
        node: Option<&str>,
        environment: &str,
        root: &Path,
        store: &Arc<Store>,
    ) -> TestResult<Self> {
        let home = temp_dirs::tempdir()?;
        let connections = openbao_connections(home.path());
        let document = ConfigLoader.load_from(root)?;
        let instance = provider()?;
        let openbao = Arc::new(
            OpenBaoSecretStore::new(
                instance.clone(),
                OpenBaoMode::External,
                project(PROJECT)?,
                Arc::clone(&connections),
                store.clone(),
            )
            .declared_at(
                document
                    .config
                    .declared_provider_addresses()
                    .get(&instance)
                    .cloned(),
            ),
        );
        let age = Arc::new(AgeSecretStore::new(
            home.path().join("identities/age.key"),
            home.path().join("secrets/project.age"),
        ));
        let age_inspector: Arc<dyn SecretMetadataInspector> = age.clone();
        let age_resolver: Arc<dyn SecretMaterialResolver> = age.clone();
        let age_creator: Arc<dyn SecretCreator> = age.clone();
        let age_replacer: Arc<dyn SecretReplacer> = age.clone();
        let age_batch_replacer: Arc<dyn SecretBatchReplacer> = age.clone();
        let age_candidates: Arc<dyn SecretCandidateLifecycle> = age.clone();
        let age_remover: Arc<dyn SecretRemover> = age;
        let inspector: Arc<dyn SecretMetadataInspector> = openbao.clone();
        let resolver: Arc<dyn SecretMaterialResolver> = openbao.clone();
        let creator: Arc<dyn SecretCreator> = openbao.clone();
        let replacer: Arc<dyn SecretReplacer> = openbao.clone();
        let remover: Arc<dyn SecretRemover> = openbao;
        let registry = Arc::new(SecretProviderRegistry::new(vec![
            StorageProviderRegistration {
                descriptor: age_storage_descriptor()?,
                inspector: age_inspector,
                resolver: age_resolver,
                creator: Some(age_creator),
                replacer: Some(age_replacer),
                batch_replacer: Some(age_batch_replacer),
                candidates: Some(age_candidates),
                remover: Some(age_remover),
            },
            StorageProviderRegistration {
                descriptor: openbao_storage_descriptor(&instance)?,
                inspector,
                resolver,
                creator: Some(creator),
                replacer: Some(replacer),
                batch_replacer: None,
                candidates: None,
                remover: Some(remover),
            },
        ])?);
        let scope = node.map(|name| NodeScope {
            target: "cluster".to_owned(),
            node: name.to_owned(),
            environment: "production".to_owned(),
            resolver_files: None,
            mesh_addresses: [("app-1", [10, 90, 0, 1]), ("app-2", [10, 90, 0, 2])]
                .into_iter()
                .map(|(name, address)| (name.to_owned(), address.into()))
                .collect(),
        });
        let state = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
        let artifacts = FilesystemEnvironmentArtifactStore::new(home.path().join("run/env"))?;
        let mut deliveries = artifacts.delivery_capabilities();
        deliveries.push(podman_native_secret_descriptor()?);
        let environments = EnvironmentManager::new(EnvironmentDependencies {
            documents: match &scope {
                Some(scope) => Arc::new(NodeDocuments::new(Arc::new(ConfigLoader), scope.clone())),
                None => Arc::new(ConfigLoader),
            },
            sources: Arc::new(EnvironmentIndexer),
            writer: Arc::new(FilesystemMaterializedEnvironmentWriter),
            state: state.clone(),
            values: state,
            inspector: registry.clone(),
            resolver: registry.clone(),
            storage_capabilities: registry,
            delivery_capabilities: Arc::new(Deliveries(deliveries)),
            clock: Arc::new(FixedClock { unix_ms: 10_000 }),
        });
        let mut selection = WorkspaceSelection::new(root, Some(environment.to_owned()));
        selection.target = Some("local".to_owned());
        selection.runtime = Some(RuntimeKind::PodmanCompose);
        selection.node = scope;
        Ok(Self {
            _home: home,
            connections,
            environments,
            selection,
        })
    }

    /// The revision of every secret this node would plan with.
    async fn revisions(&self) -> Result<BTreeMap<String, Option<String>>, MmError> {
        Ok(self
            .environments
            .review(&self.selection)
            .await?
            .secret_revisions)
    }
}

#[tokio::test]
async fn a_node_asks_the_store_only_for_what_its_services_use_and_a_rotation_is_a_new_revision()
-> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    write_project(&root, CONFIG)?;
    let store = Arc::new(Store::default());
    let password = data_path(PROJECT, "production", "database/password");
    let token = data_path(PROJECT, "production", "api/token");
    store.put(&password, FIRST_VALUE)?;
    store.put(&token, FIRST_VALUE)?;
    let first = Node::new("app-1", &root, &store)?;
    let second = Node::new("app-2", &root, &store)?;

    // app-1 runs the service that reads the store and is not connected yet:
    // the plan is refused with the command to run on the host.
    let refused = first
        .revisions()
        .await
        .err()
        .ok_or("a node planned without a connection to the store")?
        .to_string();
    assert!(
        refused.contains(&format!(
            "mmd secrets connect --project {PROJECT} --environment production --provider team-secrets"
        )),
        "{refused}"
    );
    assert_eq!(store.requests()?, Vec::<String>::new());

    // app-2 runs nothing that reads the store: it needs no connection and
    // asks the store nothing.
    assert_eq!(second.revisions().await?, BTreeMap::new());
    assert_eq!(store.requests()?, Vec::<String>::new());

    connect_host_openbao(
        Arc::clone(&first.connections),
        store.clone(),
        connect(project(PROJECT)?, "production", "https://10.90.0.1:8200")?,
    )
    .await?;
    store.forget_requests()?;
    let before = first.revisions().await?;
    assert_eq!(
        before.values().cloned().collect::<Vec<_>>(),
        [Some("1".to_owned()), Some("1".to_owned())]
    );
    // Planning reads versions, never a value.
    assert!(
        store
            .requests()?
            .iter()
            .all(|request| !request.contains("/data/")),
        "{:?}",
        store.requests()?
    );
    let items = first.environments.validate_secret_delivery(
        &first.selection,
        RuntimeKind::PodmanCompose,
        &before,
    )?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].logical_name, "api.token");
    assert_eq!(items[0].backend, PROVIDER);
    assert_eq!(items[0].source_revision, "1");
    assert_eq!(
        items[0]
            .consumers
            .iter()
            .map(|consumer| consumer.service.as_str())
            .collect::<Vec<_>>(),
        ["api"]
    );

    // The token is rotated in the store; nothing was pushed to the host.
    store.put(&token, SECOND_VALUE)?;
    let after = first.revisions().await?;
    let changed = after
        .iter()
        .filter(|(reference, revision)| before.get(*reference) != Some(*revision))
        .map(|(reference, _)| reference.as_str())
        .collect::<Vec<_>>();
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert!(changed[0].ends_with("api/token"), "{changed:?}");
    let items = first.environments.validate_secret_delivery(
        &first.selection,
        RuntimeKind::PodmanCompose,
        &after,
    )?;
    assert_eq!(items[0].source_revision, "2");
    // The node that does not use it still plans the same, without asking.
    store.forget_requests()?;
    assert_eq!(second.revisions().await?, BTreeMap::new());
    assert_eq!(store.requests()?, Vec::<String>::new());

    // No revision, review or delivery item carries a value.
    let printed = format!("{before:?}{after:?}{items:?}");
    assert!(!printed.contains(FIRST_VALUE) && !printed.contains(SECOND_VALUE));
    Ok(())
}

// --- a store that cannot answer when a deployment is planned ---

/// The target `mmd` and a workstation plan a deployment on.
fn local_target() -> NamedTarget {
    NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(Uuid::from_u128(7))),
    )
}

/// Asserts that `planned` is the refusal for a store that did not answer,
/// naming the provider, the environment and what to run.
fn store_refusal(planned: Result<EnvironmentReview, MmError>, environment: &str) -> TestResult {
    let error = planned
        .err()
        .ok_or("a deployment was planned while the secret store did not answer")?;
    assert_eq!(error.code(), SECRET_STORE_UNAVAILABLE, "{error}");
    // The terminal UI shows an error as the command line's first line.
    let shown = mm::presentation::output::error_line(&error);
    assert!(
        shown.starts_with("error[MM_SECRET_STORE_UNAVAILABLE]: secret store 'team-secrets'"),
        "{shown}"
    );
    let message = error.to_string();
    for needle in [
        "secret store 'team-secrets' did not answer".to_owned(),
        format!("environment '{environment}'"),
        "nothing was planned".to_owned(),
        format!("mm secret status -e {environment}"),
        "mmd secrets connect".to_owned(),
    ] {
        assert!(message.contains(&needle), "{needle:?} is not in: {message}");
    }
    for secret in [ROLE_ID, SECRET_ID, FIRST_VALUE, "short-lived-test-token"] {
        assert!(!message.contains(secret), "{message}");
    }
    Ok(())
}

#[tokio::test]
async fn a_host_refuses_to_plan_a_deployment_while_the_store_does_not_answer() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    write_project(&root, CONFIG)?;
    let store = Arc::new(Store::default());
    for location in ["api/token", "database/password"] {
        store.put(&data_path(PROJECT, "production", location), FIRST_VALUE)?;
    }
    let node = Node::new("app-1", &root, &store)?;
    connect_host_openbao(
        Arc::clone(&node.connections),
        store.clone(),
        connect(project(PROJECT)?, "production", "https://10.90.0.1:8200")?,
    )
    .await?;
    let target = local_target();
    let review = node
        .environments
        .review_for_deployment(&node.selection, &target)
        .await?;
    assert!(
        review.secret_revisions.values().all(Option::is_some),
        "{review:?}"
    );

    store.set_down(true)?;
    store_refusal(
        node.environments
            .review_for_deployment(&node.selection, &target)
            .await,
        "production",
    )?;
    // An apply plans again before it changes anything; refused there, it
    // says so, and not that nothing was planned.
    let at_apply = mm::app::refused_at_apply(
        node.environments
            .review_for_deployment(&node.selection, &target)
            .await
            .err()
            .ok_or("an apply went ahead while the secret store did not answer")?,
    );
    assert_eq!(at_apply.code(), SECRET_STORE_UNAVAILABLE);
    let message = at_apply.to_string();
    assert!(
        message.contains("the apply planned again before changing anything and was refused, so nothing was changed")
            && message.contains("apply the same plan again")
            && !message.contains("nothing was planned"),
        "{message}"
    );
    // What `mm env review` shows still answers, without the version.
    assert_eq!(node.revisions().await?.values().flatten().count(), 0);

    store.set_down(false)?;
    node.environments
        .review_for_deployment(&node.selection, &target)
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_host_whose_secret_id_was_destroyed_refuses_to_plan_and_says_the_login_was_refused()
-> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    write_project(&root, CONFIG)?;
    let store = Arc::new(Store::default());
    for location in ["api/token", "database/password"] {
        store.put(&data_path(PROJECT, "production", location), FIRST_VALUE)?;
    }
    let node = Node::new("app-1", &root, &store)?;
    connect_host_openbao(
        Arc::clone(&node.connections),
        store.clone(),
        connect(project(PROJECT)?, "production", "https://10.90.0.1:8200")?,
    )
    .await?;
    let target = local_target();
    node.environments
        .review_for_deployment(&node.selection, &target)
        .await?;
    store.revoke()?;
    let error = node
        .environments
        .review_for_deployment(&node.selection, &target)
        .await
        .err()
        .ok_or("a deployment was planned with a destroyed secret ID")?;
    assert_eq!(error.code(), SECRET_STORE_UNAVAILABLE, "{error}");
    let message = error.to_string();
    for needle in [
        "secret store 'team-secrets' refused the login (access denied)",
        "nothing was planned",
        "mmd secrets connect",
    ] {
        assert!(message.contains(needle), "{needle:?} is not in: {message}");
    }
    for secret in [ROLE_ID, SECRET_ID, FIRST_VALUE] {
        assert!(!message.contains(secret), "{message}");
    }
    Ok(())
}

#[tokio::test]
async fn a_workstation_refuses_to_plan_a_local_deployment_while_the_store_does_not_answer()
-> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    write_project(&root, CONFIG)?;
    let store = Arc::new(Store::default());
    for location in ["api/token", "database/password"] {
        store.put(&data_path(PROJECT, "staging", location), FIRST_VALUE)?;
    }
    let workstation = Node::planning(None, "staging", &root, &store)?;
    connect_host_openbao(
        Arc::clone(&workstation.connections),
        store.clone(),
        connect(
            project(PROJECT)?,
            "staging",
            "https://bao.example.test:8200",
        )?,
    )
    .await?;
    let target = local_target();
    let review = workstation
        .environments
        .review_for_deployment(&workstation.selection, &target)
        .await?;
    assert!(
        !review.secret_revisions.is_empty()
            && review.secret_revisions.values().all(Option::is_some),
        "{review:?}"
    );
    store.set_down(true)?;
    store_refusal(
        workstation
            .environments
            .review_for_deployment(&workstation.selection, &target)
            .await,
        "staging",
    )
}

#[tokio::test]
async fn mm_dev_still_starts_while_the_store_of_the_development_environment_is_stopped()
-> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    write_project(
        &root,
        &CONFIG.replace(
            "  name: host-secrets\n",
            "  name: host-secrets\n  development-environment: staging\n",
        ),
    )?;
    let store = Arc::new(Store::default());
    for location in ["api/token", "database/password"] {
        store.put(&data_path(PROJECT, "staging", location), FIRST_VALUE)?;
    }
    let workstation = Node::planning(None, "staging", &root, &store)?;
    connect_host_openbao(
        Arc::clone(&workstation.connections),
        store.clone(),
        connect(
            project(PROJECT)?,
            "staging",
            "https://bao.example.test:8200",
        )?,
    )
    .await?;
    store.set_down(true)?;
    // `mm dev` deploys the development environment to this machine: it
    // plans without the version, and the environment status says why.
    let review = workstation
        .environments
        .review_for_deployment(&workstation.selection, &local_target())
        .await?;
    assert!(
        !review.secret_revisions.is_empty()
            && review.secret_revisions.values().all(Option::is_none),
        "{review:?}"
    );
    let status = workstation
        .environments
        .status(&workstation.selection, true)
        .await?;
    assert!(
        status
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_SECRET_PROVIDER_UNAVAILABLE"),
        "{:?}",
        status.diagnostics
    );
    Ok(())
}
