//! The secret store mm runs on a cluster environment's nodes: the members'
//! files, the authority and recovery copy on the operator's machine, the
//! first initialisation and a second member joining, mmd unsealing a
//! restarted member, and a value staged on the hosts becoming one new
//! version once its deployment succeeded.

#![cfg(unix)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::MmError;
use mm::adapters::secrets::{
    AgeSecretStore, FileStoreKeeper, MemberRecord, NodeStore, OpenBaoConnection,
    OpenBaoConnectionStore, OpenBaoHttpResponse, OpenBaoHttpTransport, OpenBaoMode,
    OpenBaoSecretStore, StagedSlot, StoreChange, StoreLayout, StoreStagedChange, StoreUnsealer,
    render_config, render_unit,
};
use mm::app::ports::{
    ClusterStoreKeeper, CommandOutput, CommandSpec, ProcessRunner, SecretMaterialResolver,
    SecretMetadataInspector, StoreScope,
};
use mm::domain::secret::{SecretReference, SecretScope, SecretStatus};
use mm::domain::secret_backend::ProviderInstanceName;
use mm::domain::secret_contract::StorageProviderId;
use mm::domain::secret_store::{
    ClusterStoreReport, StoreFacts, StoreInitRequest, StoreMember, StoreMemberSpec, StoreNodes,
    StorePeer, StoreRecovery, StoreRestorePlan, StoreRestoreStep, StoreSnapshot, StoreStatus,
    rejoining, removal_through, store_diagnostics, store_id, store_members,
};
use mm::domain::workspace::ProjectId;
use reqwest::Method;
use zeroize::Zeroizing;

mod temp_dirs;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const PROJECT: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de7";
const ENVIRONMENT: &str = "production";
const PROVIDER: &str = "deploy-secrets";
/// Not keys or tokens of anything: the fake store hands out these.
const UNSEAL_KEY: &str = "unseal-key-of-the-fake-store";
const ROOT_TOKEN: &str = "root-token-of-the-fake-store";
const VALUE: &str = "managed-store-sentinel-value";
const HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn project() -> Result<ProjectId, uuid::Error> {
    Ok(ProjectId(uuid::Uuid::parse_str(PROJECT)?))
}

fn members() -> Vec<StoreMember> {
    ["app-1", "app-2", "app-3"]
        .iter()
        .enumerate()
        .map(|(index, name)| StoreMember {
            name: (*name).to_owned(),
            address: std::net::Ipv4Addr::new(10, 90, 0, u8::try_from(index + 1).unwrap_or(1)),
        })
        .collect()
}

/// An OpenBao as the member, mmd and setup see one: initialisation, the
/// seal, AppRole logins, policies, Raft membership, and
/// KV version 2 with check-and-set.
#[derive(Default)]
struct Bao {
    state: Mutex<BaoState>,
}

#[derive(Default)]
struct BaoState {
    initialized: bool,
    sealed: bool,
    tokens: BTreeMap<String, String>,
    mounts: BTreeSet<String>,
    auths: BTreeSet<String>,
    audits: BTreeSet<String>,
    policies: BTreeMap<String, String>,
    roles: BTreeMap<String, Vec<String>>,
    kv: BTreeMap<String, Vec<(serde_json::Value, bool)>>,
    /// The Raft servers and whether each leads; app-1 leading app-2 when unset.
    raft: Option<Vec<(String, bool)>>,
    requests: Vec<String>,
    issued: u64,
}

impl BaoState {
    fn servers(&mut self) -> &mut Vec<(String, bool)> {
        self.raft
            .get_or_insert_with(|| vec![("app-1".to_owned(), true), ("app-2".to_owned(), false)])
    }
}

impl Bao {
    fn state(&self) -> Result<std::sync::MutexGuard<'_, BaoState>, MmError> {
        self.state.lock().map_err(|_| MmError::State {
            message: "fake store lock poisoned".to_owned(),
        })
    }

    fn seal(&self) -> TestResult {
        self.state()?.sealed = true;
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
        let field = |key: &str| {
            body.and_then(|body| body.get(key))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let seal = |state: &BaoState| serde_json::json!({"initialized": state.initialized, "sealed": state.sealed, "version": "2.4.4", "standby": false});
        match (method.as_str(), path) {
            ("GET", "sys/seal-status") => return Ok((200, seal(&state))),
            ("GET", health) if health.starts_with("sys/health") => {
                let code = if state.initialized && !state.sealed {
                    200
                } else {
                    503
                };
                return Ok((code, seal(&state)));
            }
            ("POST", "sys/init") => {
                if state.initialized {
                    return Ok((400, serde_json::json!({"errors": ["already initialized"]})));
                }
                state.initialized = true;
                state.sealed = true;
                state
                    .tokens
                    .insert(ROOT_TOKEN.to_owned(), "root".to_owned());
                return Ok((
                    200,
                    serde_json::json!({"keys_base64": [UNSEAL_KEY], "root_token": ROOT_TOKEN}),
                ));
            }
            ("POST", "sys/unseal") => {
                if field("key") != UNSEAL_KEY || !state.initialized {
                    return Ok((400, serde_json::json!({"errors": ["bad key"]})));
                }
                state.sealed = false;
                return Ok((200, seal(&state)));
            }
            ("POST", "auth/mm-approle/login") => {
                let (role_id, secret_id) = (field("role_id"), field("secret_id"));
                let Some(role) = role_id.strip_prefix("role-id-").map(str::to_owned) else {
                    return Ok((400, serde_json::json!({})));
                };
                if !state
                    .roles
                    .get(&role)
                    .is_some_and(|ids| ids.contains(&secret_id))
                {
                    return Ok((400, serde_json::json!({})));
                }
                state.issued += 1;
                let token = format!("token-{role}-{}", state.issued);
                state.tokens.insert(token.clone(), role);
                return Ok((
                    200,
                    serde_json::json!({"auth": {"client_token": token, "renewable": true, "lease_duration": 900}}),
                ));
            }
            _ => {}
        }
        if state.sealed {
            return Ok((503, serde_json::json!({"errors": ["sealed"]})));
        }
        let Some(holder) = token.and_then(|token| state.tokens.get(token)).cloned() else {
            return Ok((403, serde_json::json!({"errors": ["permission denied"]})));
        };
        if let Some(kv) = path.strip_prefix("mm-kv/") {
            if holder != "root" && holder != "mm-deploy" {
                return Ok((403, serde_json::json!({})));
            }
            return Ok(kv_answer(&mut state, method, kv, body));
        }
        if holder == "mm-operations" && path == "sys/storage/raft/snapshot" {
            let snapshot = serde_json::json!({"kv": state.kv, "roles": state.roles});
            return Ok((200, snapshot));
        }
        if holder == "mm-operations" && path == "sys/storage/raft/configuration" {
            let servers = state
                .servers()
                .iter()
                .map(|(name, leader)| {
                    serde_json::json!({"node_id": name, "address": format!("{name}:8201"), "leader": leader, "voter": true})
                })
                .collect::<Vec<_>>();
            return Ok((
                200,
                serde_json::json!({"data": {"config": {"servers": servers}}}),
            ));
        }
        if holder == "mm-operations" && path == "sys/storage/raft/remove-peer" {
            let peer = field("server_id");
            state.servers().retain(|(name, _)| *name != peer);
            return Ok((204, serde_json::Value::Null));
        }
        if holder != "root" {
            return Ok((403, serde_json::json!({})));
        }
        Ok(root_answer(&mut state, method, path, token, body))
    }
}

fn root_answer(
    state: &mut BaoState,
    method: &Method,
    path: &str,
    token: Option<&str>,
    body: Option<&serde_json::Value>,
) -> (u16, serde_json::Value) {
    let listing = |names: &BTreeSet<String>| serde_json::json!({"data": names.iter().map(|name| (format!("{name}/"), serde_json::json!({"type": "x"}))).collect::<serde_json::Map<_, _>>()});
    match (method.as_str(), path) {
        ("GET", "sys/mounts") => (200, listing(&state.mounts)),
        ("GET", "sys/auth") => (200, listing(&state.auths)),
        ("GET", "sys/audit") => (200, listing(&state.audits)),
        ("POST", "auth/token/revoke-self") => {
            if let Some(token) = token {
                state.tokens.remove(token);
            }
            (204, serde_json::Value::Null)
        }
        _ => {
            if let Some(name) = path.strip_prefix("sys/mounts/") {
                state.mounts.insert(name.to_owned());
            } else if let Some(name) = path.strip_prefix("sys/auth/") {
                state.auths.insert(name.to_owned());
            } else if let Some(name) = path.strip_prefix("sys/audit/") {
                state.audits.insert(name.to_owned());
            } else if let Some(name) = path.strip_prefix("sys/policies/acl/") {
                let policy = body
                    .and_then(|body| body["policy"].as_str())
                    .unwrap_or_default();
                state.policies.insert(name.to_owned(), policy.to_owned());
            } else if let Some(role) = path.strip_prefix("auth/mm-approle/role/") {
                return role_answer(state, role);
            }
            (204, serde_json::Value::Null)
        }
    }
}

fn role_answer(state: &mut BaoState, role: &str) -> (u16, serde_json::Value) {
    if let Some(name) = role.strip_suffix("/role-id") {
        return (
            200,
            serde_json::json!({"data": {"role_id": format!("role-id-{name}")}}),
        );
    }
    if let Some(name) = role.strip_suffix("/secret-id") {
        state.issued += 1;
        let secret = format!("secret-id-{name}-{}", state.issued);
        state
            .roles
            .entry(name.to_owned())
            .or_default()
            .push(secret.clone());
        return (200, serde_json::json!({"data": {"secret_id": secret}}));
    }
    state.roles.entry(role.to_owned()).or_default();
    (204, serde_json::Value::Null)
}

fn kv_answer(
    state: &mut BaoState,
    method: &Method,
    path: &str,
    body: Option<&serde_json::Value>,
) -> (u16, serde_json::Value) {
    let (kind, key) = path.split_once('/').unwrap_or((path, ""));
    if let Some(folder) = key.strip_suffix("?list=true") {
        let keys = state
            .kv
            .keys()
            .filter_map(|known| known.strip_prefix(folder))
            .map(|rest| match rest.split_once('/') {
                Some((first, _)) => format!("{first}/"),
                None => rest.to_owned(),
            })
            .collect::<BTreeSet<_>>();
        if keys.is_empty() {
            return (404, serde_json::json!({}));
        }
        return (200, serde_json::json!({"data": {"keys": keys}}));
    }
    let versions = state.kv.entry(key.to_owned()).or_default();
    let current = versions.len();
    match (method.as_str(), kind) {
        ("GET", "data") => match versions.last() {
            Some((data, false)) => (
                200,
                serde_json::json!({"data": {"data": data, "metadata": {"version": current, "created_time": "2026-11-18T10:00:00Z"}}}),
            ),
            _ => (404, serde_json::json!({})),
        },
        ("GET", "metadata") if current > 0 => (
            200,
            serde_json::json!({"data": {"current_version": current, "versions": versions
                .iter()
                .enumerate()
                .map(|(index, (_, deleted))| (
                    (index + 1).to_string(),
                    serde_json::json!({
                        "created_time": format!("2026-11-18T10:0{index}:00Z"),
                        "deletion_time": if *deleted { "2026-11-18T11:00:00Z" } else { "" },
                        "destroyed": false,
                    }),
                ))
                .collect::<serde_json::Map<_, _>>()}}),
        ),
        ("GET", "metadata") => (404, serde_json::json!({})),
        ("POST", "data") => {
            let body = body.cloned().unwrap_or_default();
            if body["options"]["cas"].as_u64() != Some(current as u64) {
                return (
                    400,
                    serde_json::json!({"errors": ["check-and-set mismatch"]}),
                );
            }
            versions.push((body["data"].clone(), false));
            (
                200,
                serde_json::json!({"data": {"version": versions.len()}}),
            )
        }
        ("DELETE", "data") => {
            if let Some(last) = versions.last_mut() {
                last.1 = true;
            }
            (204, serde_json::Value::Null)
        }
        _ => (405, serde_json::json!({})),
    }
}

#[async_trait]
impl OpenBaoHttpTransport for Bao {
    async fn request(
        &self,
        _connection: &OpenBaoConnection,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<OpenBaoHttpResponse, MmError> {
        let (status, json) = self.answer(&method, path, token, body)?;
        Ok(OpenBaoHttpResponse {
            status,
            headers: BTreeMap::new(),
            body: Zeroizing::new(if json.is_null() {
                Vec::new()
            } else {
                serde_json::to_vec(&json).map_err(|error| MmError::State {
                    message: error.to_string(),
                })?
            }),
        })
    }

    async fn request_bytes(
        &self,
        _connection: &OpenBaoConnection,
        method: Method,
        path: &str,
        token: &str,
        body: Option<&[u8]>,
    ) -> Result<OpenBaoHttpResponse, MmError> {
        let mut state = self.state()?;
        state.requests.push(format!("{method} {path}"));
        let operations = state.tokens.get(token).map(String::as_str) == Some("mm-operations");
        let status = if path == "sys/storage/raft/snapshot-force" && operations && !state.sealed {
            let snapshot: serde_json::Value = serde_json::from_slice(body.unwrap_or_default())
                .map_err(|error| MmError::State {
                    message: error.to_string(),
                })?;
            state.kv = serde_json::from_value(snapshot["kv"].clone()).unwrap_or_default();
            state.roles = serde_json::from_value(snapshot["roles"].clone()).unwrap_or_default();
            state.tokens.clear();
            state.sealed = true;
            204
        } else {
            403
        };
        Ok(OpenBaoHttpResponse {
            status,
            headers: BTreeMap::new(),
            body: Zeroizing::new(Vec::new()),
        })
    }
}

/// systemd as a member sees it: every command succeeds and is recorded.
#[derive(Default)]
struct Systemd {
    commands: Mutex<Vec<String>>,
}

#[async_trait]
impl ProcessRunner for Systemd {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let record = specification.redacted_record();
        self.commands
            .lock()
            .map_err(|_| MmError::State {
                message: "poisoned".to_owned(),
            })?
            .push(format!("{} {}", record.program, record.arguments.join(" ")));
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(String::new()),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

/// One node: its layout under a temporary root, its mmd's stores.
struct Node {
    name: &'static str,
    root: PathBuf,
    store: NodeStore,
    systemd: Arc<Systemd>,
}

impl Node {
    fn new(root: &Path, name: &'static str, bao: &Arc<Bao>) -> TestResult<Self> {
        let root = root.join(name);
        let layout = StoreLayout {
            config_root: root.join("etc/mm/openbao"),
            data_root: root.join("var/lib/mm-openbao"),
            bin_root: root.join("usr/libexec/mm-openbao"),
            unit_root: root.join("etc/systemd/system"),
        };
        let id = store_id(project()?, ENVIRONMENT);
        for directory in [
            layout.config(&id),
            layout.data(&id),
            layout.bin_root.join("2.4.4"),
            layout.unit_root.clone(),
        ] {
            fs::create_dir_all(directory)?;
        }
        fs::write(layout.binary("2.4.4"), "not a real bao")?;
        let home = root.join("mm-home");
        let systemd = Arc::new(Systemd::default());
        let host_store = AgeSecretStore::new(
            home.join("identities/age.key"),
            home.join(format!("secrets/{PROJECT}.age")),
        );
        host_store.ensure_recipient()?;
        let transport: Arc<dyn OpenBaoHttpTransport> = Arc::clone(bao) as _;
        Ok(Self {
            name,
            store: NodeStore {
                layout,
                connections: Arc::new(connections(&home)),
                transport,
                process: Arc::clone(&systemd) as _,
                host_store,
                project: project()?,
                environment: ENVIRONMENT.to_owned(),
            },
            root,
            systemd,
        })
    }

    fn home(&self) -> PathBuf {
        self.root.join("mm-home")
    }

    fn recipient(&self) -> TestResult<String> {
        Ok(self.store.host_store.ensure_recipient()?)
    }

    fn age(&self) -> AgeSecretStore {
        AgeSecretStore::new(
            self.home().join("identities/age.key"),
            self.home().join(format!("secrets/{PROJECT}.age")),
        )
    }

    /// Setup's two calls: the node asks for a certificate, then takes it.
    async fn member(
        &self,
        keeper: &FileStoreKeeper,
    ) -> TestResult<mm::domain::secret_store::StoreMemberAnswer> {
        let scope = scope()?;
        let mut spec = StoreMemberSpec {
            provider: PROVIDER.to_owned(),
            node: self.name.to_owned(),
            version: "2.4.4".to_owned(),
            members: members(),
            ca_pem: keeper.authority(scope)?,
            certificate_pem: None,
        };
        let first = self.store.member(spec.clone()).await?;
        let request = first.csr.ok_or("a new member asks for a certificate")?;
        let address = members()
            .into_iter()
            .find(|member| member.name == self.name)
            .ok_or("not a member")?
            .address;
        spec.certificate_pem = Some(keeper.sign(scope, &request, (self.name, address))?);
        Ok(self.store.member(spec).await?)
    }
}

fn connections(home: &Path) -> OpenBaoConnectionStore {
    OpenBaoConnectionStore::new(
        home.join("identities/openbao-connections.key"),
        home.join("secrets/openbao-connections.age"),
    )
}

fn scope() -> Result<StoreScope<'static>, uuid::Error> {
    Ok(StoreScope {
        project: project()?,
        environment: ENVIRONMENT,
        provider: PROVIDER,
    })
}

fn keeper(root: &Path) -> FileStoreKeeper {
    FileStoreKeeper::new(
        root.join("operator/identities/age.key"),
        root.join("operator/secrets"),
    )
}

fn files_under(root: &Path) -> TestResult<Vec<(PathBuf, Vec<u8>)>> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push((path.clone(), fs::read(&path)?));
            }
        }
    }
    Ok(files)
}

#[test]
fn members_are_three_or_five_nodes_and_a_refusal_names_the_fix() {
    let five = ["a", "b", "c", "d", "e"];
    assert_eq!(
        store_members(None, &five, PROVIDER).map(|found| found.len()),
        Ok(5)
    );
    assert_eq!(
        store_members(None, &five[..3], PROVIDER).map(|found| found.len()),
        Ok(3)
    );
    let four = store_members(None, &five[..4], PROVIDER)
        .err()
        .unwrap_or_default();
    assert!(
        four.contains("the target has 4 node(s)")
            && four.contains("providers.deploy-secrets.nodes")
            && four.contains("[a, b, c]"),
        "{four}"
    );
    let named = StoreNodes::Named(vec!["e".to_owned(), "a".to_owned(), "c".to_owned()]);
    assert_eq!(
        store_members(Some(&named), &five, PROVIDER),
        Ok(vec!["a".to_owned(), "c".to_owned(), "e".to_owned()]),
        "members keep the target's order"
    );
    let unknown = StoreNodes::Named(vec!["a".to_owned(), "b".to_owned(), "x".to_owned()]);
    assert!(
        store_members(Some(&unknown), &five, PROVIDER)
            .err()
            .unwrap_or_default()
            .contains("names 'x', which is not a node of the target")
    );
}

#[test]
fn a_member_listens_on_loopback_and_its_mesh_address_only_and_writes_only_its_data() -> TestResult {
    let layout = StoreLayout::host();
    let record = MemberRecord {
        version: "2.4.4".to_owned(),
        initialized: false,
        provider: PROVIDER.to_owned(),
        project_id: project()?,
        environment: ENVIRONMENT.to_owned(),
        node: "app-2".to_owned(),
        address: std::net::Ipv4Addr::new(10, 90, 0, 2),
        members: members(),
    };
    let id = store_id(project()?, ENVIRONMENT);
    let config = render_config(&layout, &id, &record);
    let addresses = config
        .lines()
        .filter_map(|line| line.trim().strip_prefix("address = "))
        .collect::<Vec<_>>();
    assert_eq!(
        addresses,
        ["\"127.0.0.1:8200\"", "\"10.90.0.2:8200\""],
        "{config}"
    );
    for expected in [
        "node_id = \"app-2\"",
        "leader_api_addr = \"https://10.90.0.1:8200\"",
        "leader_api_addr = \"https://10.90.0.3:8200\"",
        "cluster_address = \"10.90.0.2:8201\"",
        "api_addr = \"https://10.90.0.2:8200\"",
        "tls_min_version = \"tls13\"",
        "disable_mlock = true",
        &format!(
            "audit \"file\" \"mm\" {{\n  options {{\n    file_path = \"/var/lib/mm-openbao/{id}/audit.log\"\n"
        ),
    ] {
        assert!(config.contains(expected), "{expected}:\n{config}");
    }
    assert!(
        !config.contains("leader_api_addr = \"https://10.90.0.2:8200\""),
        "a member does not join itself"
    );
    assert!(!config.contains("0.0.0.0"), "{config}");
    let unit = render_unit(&layout, &id, &record);
    for expected in [
        "User=mm-openbao",
        "ExecStart=/usr/libexec/mm-openbao/2.4.4/bao server",
        "ProtectSystem=strict",
        &format!("ReadWritePaths=/var/lib/mm-openbao/{id}\n"),
        "CapabilityBoundingSet=\n",
        "NoNewPrivileges=yes",
        "After=network-online.target mm-mesh.service",
    ] {
        assert!(unit.contains(expected), "{expected}:\n{unit}");
    }
    Ok(())
}

#[test]
fn the_authority_and_the_recovery_copy_stay_sealed_on_the_operators_machine() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let keeper = keeper(temporary.path());
    let scope = scope()?;
    let authority = keeper.authority(scope)?;
    assert_eq!(keeper.authority(scope)?, authority, "made once");
    assert!(keeper.recovery(scope)?.is_none());
    for (path, bytes) in files_under(&temporary.path().join("operator/secrets"))? {
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains("PRIVATE KEY") && !text.contains("BEGIN CERTIFICATE"),
            "{} holds the authority in the clear",
            path.display()
        );
    }
    let recovery = StoreRecovery {
        project_id: project()?,
        environment: ENVIRONMENT.to_owned(),
        provider: PROVIDER.to_owned(),
        unseal_keys: vec![Zeroizing::new(UNSEAL_KEY.to_owned())],
        deploy: mm::domain::secret_store::StoreCredential {
            role_id: Zeroizing::new("role-id-mm-deploy".to_owned()),
            secret_id: Zeroizing::new("secret-id-mm-deploy-1".to_owned()),
        },
        operations: mm::domain::secret_store::StoreCredential {
            role_id: Zeroizing::new("role-id-mm-operations".to_owned()),
            secret_id: Zeroizing::new("secret-id-mm-operations-2".to_owned()),
        },
        ca_pem: authority.clone(),
        members: members(),
    };
    let sealed = mm::adapters::secrets::seal_to_recipient(
        &keeper.operator_recipient(project()?)?,
        &serde_json::to_vec(&recovery)?,
    )?;
    keeper.keep_recovery(scope, &sealed)?;
    assert_eq!(keeper.open_recovery(scope)?, recovery);

    // A second operator gets the copy sealed to their own identity, and
    // can then sign for a node as the first one could.
    let second = FileStoreKeeper::new(
        temporary.path().join("second/identities/age.key"),
        temporary.path().join("second/secrets"),
    );
    let exported = keeper.export(scope, Some(&second.operator_recipient(project()?)?))?;
    assert!(
        !exported.contains(UNSEAL_KEY)
            && exported.starts_with("-----BEGIN AGE ENCRYPTED FILE-----")
    );
    assert!(
        keeper.import(scope, &exported).is_err(),
        "a copy sealed to someone else does not open here"
    );
    second.import(scope, &exported)?;
    assert_eq!(second.authority(scope)?, authority);
    assert_eq!(second.open_recovery(scope)?, recovery);
    let other = StoreScope {
        environment: "staging",
        ..scope
    };
    let refused = second
        .import(
            other,
            &keeper.export(scope, Some(&second.operator_recipient(project()?)?))?,
        )
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        refused.contains("of environment 'production', not of"),
        "{refused}"
    );
    Ok(())
}

#[tokio::test]
async fn setup_initialises_the_store_once_and_every_member_unseals_with_its_own_copy() -> TestResult
{
    let temporary = temp_dirs::tempdir()?;
    let bao = Arc::new(Bao::default());
    let keeper = keeper(temporary.path());
    let first = Node::new(temporary.path(), "app-1", &bao)?;
    let second = Node::new(temporary.path(), "app-2", &bao)?;
    let id = store_id(project()?, ENVIRONMENT);

    let answer = first.member(&keeper).await?;
    assert!(!answer.initialized && !answer.has_keys && answer.changed);
    let systemd = first
        .systemd
        .commands
        .lock()
        .map_err(|_| "poisoned")?
        .clone();
    assert_eq!(
        systemd,
        [
            "systemctl daemon-reload".to_owned(),
            format!("systemctl enable --quiet mm-openbao-{id}.service"),
            format!("systemctl restart mm-openbao-{id}.service"),
        ]
    );
    let again = first
        .store
        .member(StoreMemberSpec {
            provider: PROVIDER.to_owned(),
            node: "app-1".to_owned(),
            version: "2.4.4".to_owned(),
            members: members(),
            ca_pem: keeper.authority(scope()?)?,
            certificate_pem: None,
        })
        .await?;
    assert!(
        again.csr.is_none() && !again.changed,
        "a member with a fitting certificate asks for no other and changes nothing"
    );
    second.member(&keeper).await?;

    let request = StoreInitRequest {
        recipients: BTreeMap::from([
            ("app-1".to_owned(), first.recipient()?),
            ("app-2".to_owned(), second.recipient()?),
        ]),
        operator: keeper.operator_recipient(project()?)?,
    };
    let init = first.store.init(request.clone()).await?;
    assert_eq!(init.sealed.keys().collect::<Vec<_>>(), ["app-2"]);
    keeper.keep_recovery(scope()?, &init.recovery)?;
    let recovery = keeper.open_recovery(scope()?)?;
    assert_eq!(recovery.unseal_keys.len(), 1);
    {
        let state = bao.state()?;
        assert!(
            !state.tokens.contains_key(ROOT_TOKEN),
            "the root token is revoked after the bootstrap"
        );
        assert_eq!(
            state.policies.get("mm-deploy").map(String::as_str),
            Some(format!(
                "path \"mm-kv/data/mm/{PROJECT}/{ENVIRONMENT}/*\" {{ capabilities = [\"create\", \"read\", \"update\", \"delete\"] }}\npath \"mm-kv/metadata/mm/{PROJECT}/{ENVIRONMENT}/*\" {{ capabilities = [\"read\", \"list\", \"delete\"] }}\npath \"mm-kv/delete/mm/{PROJECT}/{ENVIRONMENT}/*\" {{ capabilities = [\"update\"] }}\n"
            ).as_str()),
            "mmd reads and writes the environment's path and nothing else"
        );
        assert!(state.mounts.contains("mm-kv") && state.auths.contains("mm-approle"));
    }
    assert!(
        first.store.init(request).await.is_err(),
        "a store is initialised once"
    );
    assert!(
        first
            .store
            .layout
            .record(&id)?
            .is_some_and(|record| record.initialized)
    );

    let joined = second.store.join(init.sealed["app-2"].as_bytes()).await?;
    assert!(joined.member && !joined.sealed);
    assert!(
        first
            .store
            .join(init.sealed["app-2"].as_bytes())
            .await
            .is_err(),
        "a document sealed to another node does not open here"
    );

    let status = first.store.status().await?;
    assert!(status.initialized && !status.sealed && status.has_keys);
    assert_eq!(status.leader.as_deref(), Some("app-1"));
    assert_eq!(status.peers.len(), 2);

    // Neither the files of a node nor what the commands print hold a key,
    // a token or a login.
    let printed = serde_json::to_string(&(status, &init.sealed, &init.recovery))?;
    for (path, bytes) in files_under(&first.root)?
        .into_iter()
        .chain(files_under(&second.root)?)
        .chain([(PathBuf::from("printed"), printed.into_bytes())])
    {
        let text = String::from_utf8_lossy(&bytes);
        for secret in [UNSEAL_KEY, ROOT_TOKEN, "secret-id-mm-", "role-id-mm-"] {
            assert!(!text.contains(secret), "{} holds {secret}", path.display());
        }
    }
    Ok(())
}

#[tokio::test]
async fn mmd_unseals_a_member_that_restarted_sealed() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let bao = Arc::new(Bao::default());
    let keeper = keeper(temporary.path());
    let node = Node::new(temporary.path(), "app-1", &bao)?;
    node.member(&keeper).await?;
    node.store
        .init(StoreInitRequest {
            recipients: BTreeMap::from([("app-1".to_owned(), node.recipient()?)]),
            operator: keeper.operator_recipient(project()?)?,
        })
        .await?;
    bao.seal()?;
    let unsealer = StoreUnsealer::new(
        node.store.layout.clone(),
        Arc::new(connections(&node.home())),
        Arc::clone(&bao) as _,
    );
    assert_eq!(unsealer.round().await?, [store_id(project()?, ENVIRONMENT)]);
    assert!(!bao.state()?.sealed);
    assert!(
        unsealer.round().await?.is_empty(),
        "an unsealed member is left alone"
    );
    Ok(())
}

#[tokio::test]
async fn a_staged_value_is_planned_first_and_becomes_one_version_whichever_nodes_make_it_live()
-> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let bao = Arc::new(Bao::default());
    let keeper = keeper(temporary.path());
    let first = Node::new(temporary.path(), "app-1", &bao)?;
    let second = Node::new(temporary.path(), "app-2", &bao)?;
    first.member(&keeper).await?;
    second.member(&keeper).await?;
    let init = first
        .store
        .init(StoreInitRequest {
            recipients: BTreeMap::from([
                ("app-1".to_owned(), first.recipient()?),
                ("app-2".to_owned(), second.recipient()?),
            ]),
            operator: keeper.operator_recipient(project()?)?,
        })
        .await?;
    second.store.join(init.sealed["app-2"].as_bytes()).await?;

    let reference = SecretReference::new(StorageProviderId::new(PROVIDER)?, "database/password")?;
    let scope = SecretScope::new(ENVIRONMENT);
    let slot = StagedSlot::Secret {
        environment: ENVIRONMENT.to_owned(),
        reference: reference.clone(),
    };
    assert!(slot.kept_elsewhere());
    let revision = uuid::Uuid::new_v4().to_string();
    let mut stores = Vec::new();
    for node in [&first, &second] {
        let age = Arc::new(node.age());
        age.stage_values(
            uuid::Uuid::new_v4(),
            vec![StoreStagedChange {
                slot: slot.clone(),
                change: StoreChange::Set {
                    revision: revision.clone(),
                    value: Zeroizing::new(VALUE.to_owned()),
                },
            }],
            "alice",
            1,
        )?;
        let store = OpenBaoSecretStore::new(
            ProviderInstanceName::new(PROVIDER)?,
            OpenBaoMode::Managed,
            project()?,
            Arc::new(connections(&node.home())),
            Arc::clone(&bao) as _,
        );
        stores.push((store.clone().staged_in(Arc::clone(&age)), store, age));
    }
    let (planned, live, age) = &stores[0];
    let metadata = planned.inspect(&scope, &reference).await?;
    assert_eq!(metadata.status, SecretStatus::Set);
    assert_eq!(
        metadata.revision.as_deref(),
        Some(revision.as_str()),
        "a plan binds the staged revision"
    );
    assert_eq!(
        live.live(&scope, &reference).await?.status,
        SecretStatus::Unset
    );
    let resolved = planned.resolve(&scope, &reference).await?;
    assert_eq!(
        resolved
            .material
            .map(|material| material.expose(|value| value == VALUE)),
        Some(true),
        "the deployment delivers the staged value"
    );

    for (_, live, age) in &stores {
        let change = age
            .staged_change(&slot, &revision)?
            .ok_or("the change is staged")?
            .ok_or("it sets a value")?;
        live.write_live(&scope, &reference, (&change, &revision), 2)
            .await?;
        assert!(age.settle_staged(&slot, &revision)?);
    }
    let versions = bao.state()?.kv.values().map(Vec::len).collect::<Vec<_>>();
    assert_eq!(
        versions,
        [1],
        "two nodes made the change live as one version"
    );
    let after = planned.inspect(&scope, &reference).await?;
    assert_eq!(
        after.revision.as_deref(),
        Some(revision.as_str()),
        "the version keeps the revision it was planned with, so no container is replaced again"
    );
    assert!(age.staged_metadata(&scope, &reference)?.is_none());

    let removal = uuid::Uuid::new_v4().to_string();
    let changed = live
        .write_live(
            &scope,
            &reference,
            (
                &Zeroizing::new("second-value-of-the-test".to_owned()),
                &removal,
            ),
            3,
        )
        .await?;
    assert!(changed);
    assert!(live.remove_live(&scope, &reference).await?);
    assert!(!live.remove_live(&scope, &reference).await?, "removed once");
    assert_eq!(
        live.live(&scope, &reference).await?.status,
        SecretStatus::Unset
    );
    Ok(())
}

#[test]
fn a_managed_store_on_a_cluster_is_validated_with_the_fix_named() -> TestResult {
    let findings = |nodes: usize, store: &str| -> TestResult<Vec<String>> {
        let temporary = temp_dirs::tempdir()?;
        let root = temporary.path().join("project");
        fs::create_dir_all(root.join(".config"))?;
        fs::create_dir_all(root.join("api"))?;
        let node_lines = (1..=nodes)
            .map(|index| format!("      - {{name: app-{index}, host: 203.0.113.{index}}}\n"))
            .collect::<String>();
        fs::write(
            root.join(".config/mm.yaml"),
            format!(
                "schema-version: 1\nproject:\n  id: {PROJECT}\n  name: store\ntarget: local\nstacks:\n  app:\n    files: [compose.yaml]\nmodules:\n  api:\n    path: api\n    services:\n      api: {{stack: app}}\nproviders:\n{store}secrets:\n  database.password:\n    storage: {{provider: deploy-secrets, location: database/password}}\ntargets:\n  cluster:\n    nodes:\n{node_lines}environments:\n  production:\n    target: cluster\n"
            ),
        )?;
        fs::write(
            root.join("compose.yaml"),
            "services:\n  api:\n    image: docker.io/library/alpine:3.22\n",
        )?;
        let output = temp_dirs::mm()
            .arg("--root")
            .arg(&root)
            .arg("--mm-home")
            .arg(temporary.path().join("home"))
            .args(["--output", "json", "-e", "production", "config", "validate"])
            .output()?;
        let report: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                format!(
                    "no report ({error}): {}",
                    String::from_utf8_lossy(&output.stderr)
                )
            })?;
        Ok(report["validation"]["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|diagnostic| diagnostic["code"] == "MM_SECRET_STORE_INVALID")
            .filter_map(|diagnostic| diagnostic["message"].as_str().map(str::to_owned))
            .collect())
    };
    let store = "  deploy-secrets:\n    driver: openbao\n    mode: managed\n    scope: environment\n    release: \"2.4\"\n";
    assert_eq!(
        findings(3, store)?,
        Vec::<String>::new(),
        "the smallest form is valid"
    );
    let four = findings(4, store)?.join("\n");
    assert!(
        four.contains("environment 'production': the target has 4 node(s)"),
        "{four}"
    );
    let old = findings(3, &store.replace("\"2.4\"", "\"2.3\""))?.join("\n");
    assert!(old.contains("release is '2.3'"), "{old}");
    let two = findings(
        3,
        &format!(
            "{store}{}",
            store.replace("deploy-secrets", "other-secrets")
        ),
    )?
    .join("\n");
    assert!(
        two.contains("an environment has one managed store"),
        "{two}"
    );
    Ok(())
}

fn member_status(node: &str, version: &str, sealed: bool, voters: &[&str]) -> StoreStatus {
    StoreStatus {
        provider: PROVIDER.to_owned(),
        node: node.to_owned(),
        unit: format!("mm-openbao-{}.service", node),
        version: Some(version.to_owned()),
        running: true,
        initialized: true,
        sealed,
        has_keys: true,
        leader: Some("app-1".to_owned()),
        peers: voters
            .iter()
            .map(|name| StorePeer {
                name: (*name).to_owned(),
                address: "10.90.0.1:8201".to_owned(),
                leader: *name == "app-1",
                voter: true,
            })
            .collect(),
        problems: Vec::new(),
    }
}

#[test]
fn doctor_names_a_sealed_member_one_out_of_the_raft_cluster_and_releases_that_differ() {
    let whole = ["app-1", "app-2", "app-3"];
    let healthy = [
        member_status("app-1", "2.4.4", false, &whole),
        member_status("app-2", "2.4.4", false, &whole),
        member_status("app-3", "2.4.4", false, &whole),
    ];
    let reports = |statuses: &[StoreStatus]| {
        statuses
            .iter()
            .map(|status| (status.node.clone(), status.clone()))
            .collect::<Vec<_>>()
    };
    let codes = |statuses: &[StoreStatus]| {
        let owned = reports(statuses);
        let members = owned
            .iter()
            .map(|(name, status)| (name.as_str(), status))
            .collect::<Vec<_>>();
        store_diagnostics("cluster", &members)
            .into_iter()
            .map(|diagnostic| (diagnostic.code.to_string(), diagnostic.message))
            .collect::<Vec<_>>()
    };
    assert!(codes(&healthy).is_empty(), "{:?}", codes(&healthy));

    let mut sealed = healthy.clone();
    sealed[1].sealed = true;
    let found = codes(&sealed);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "MM_SECRET_STORE_SEALED");
    assert!(found[0].1.contains("on app-2 of 'cluster'"), "{found:?}");

    let out = [
        member_status("app-1", "2.4.4", false, &["app-1", "app-2"]),
        member_status("app-2", "2.4.4", false, &["app-1", "app-2"]),
        member_status("app-3", "2.4.4", false, &[]),
    ];
    let found = codes(&out);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "MM_SECRET_STORE_DEGRADED");
    assert!(found[0].1.contains("app-3: not a voter"), "{found:?}");

    let mut skew = healthy.clone();
    skew[2].version = Some("2.5.5".to_owned());
    let found = codes(&skew);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "MM_SECRET_STORE_VERSION_SKEW");
    assert!(found[0].1.contains("app-3 2.5.5"), "{found:?}");
}

#[tokio::test]
async fn mmd_rotates_a_members_audit_log_and_prints_the_newest_records() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let bao = Arc::new(Bao::default());
    let keeper = keeper(temporary.path());
    let node = Node::new(temporary.path(), "app-1", &bao)?;
    node.member(&keeper).await?;
    let id = store_id(project()?, ENVIRONMENT);
    let log = node.store.layout.audit_log(&id);
    let record = |index: usize| format!("{{\"type\":\"response\",\"index\":{index}}}\n");
    fs::write(format!("{}.1", log.display()), record(1) + &record(2))?;
    fs::write(&log, record(3) + "not a record\n" + &record(4))?;
    let newest = node.store.audit(3)?;
    assert_eq!(
        newest
            .iter()
            .map(|record| record["index"].as_u64().unwrap_or_default())
            .collect::<Vec<_>>(),
        [2, 3, 4],
        "oldest first, across the rotated file"
    );

    let mut big = fs::OpenOptions::new().append(true).open(&log)?;
    std::io::Write::write_all(&mut big, &vec![b'x'; 16 * 1024 * 1024])?;
    drop(big);
    let systemd = Arc::new(Systemd::default());
    let unsealer = StoreUnsealer::new(
        node.store.layout.clone(),
        Arc::new(connections(&node.home())),
        Arc::clone(&bao) as _,
    )
    .rotating(Arc::clone(&systemd) as _);
    unsealer.round().await?;
    assert!(!log.exists(), "the log was moved aside");
    assert!(
        Path::new(&format!("{}.2", log.display())).exists(),
        "the older file moved up"
    );
    assert_eq!(
        systemd.commands.lock().map_err(|_| "poisoned")?.clone(),
        [format!("systemctl reload mm-openbao-{id}.service")],
        "the member reopens its log"
    );
    unsealer.round().await?;
    assert_eq!(
        systemd.commands.lock().map_err(|_| "poisoned")?.len(),
        1,
        "a small log stays"
    );
    Ok(())
}

/// Initialises a store of one member on `bao` under `root`.
async fn initialised(root: &Path, bao: &Arc<Bao>, keeper: &FileStoreKeeper) -> TestResult<Node> {
    let node = Node::new(root, "app-1", bao)?;
    node.member(keeper).await?;
    node.store
        .init(StoreInitRequest {
            recipients: BTreeMap::from([("app-1".to_owned(), node.recipient()?)]),
            operator: keeper.operator_recipient(project()?)?,
        })
        .await?;
    Ok(node)
}

#[tokio::test]
async fn a_snapshot_put_back_on_a_fresh_store_brings_every_value_and_its_versions() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let keeper = keeper(temporary.path());
    let old_bao = Arc::new(Bao::default());
    let old = initialised(&temporary.path().join("old"), &old_bao, &keeper).await?;
    let reference = SecretReference::new(StorageProviderId::new(PROVIDER)?, "database/password")?;
    let scope = SecretScope::new(ENVIRONMENT);
    let store = |node: &Node, bao: &Arc<Bao>| -> TestResult<OpenBaoSecretStore> {
        Ok(OpenBaoSecretStore::new(
            ProviderInstanceName::new(PROVIDER)?,
            OpenBaoMode::Managed,
            project()?,
            Arc::new(connections(&node.home())),
            Arc::clone(bao) as _,
        ))
    };
    let live = store(&old, &old_bao)?;
    for (revision, value) in [
        ("first-revision", "first-value-of-the-test"),
        ("second-revision", VALUE),
    ] {
        live.write_live(
            &scope,
            &reference,
            (&Zeroizing::new(value.to_owned()), revision),
            1,
        )
        .await?;
    }
    let sealed = old
        .store
        .snapshot(&keeper.operator_recipient(project()?)?, 7)
        .await?;
    assert!(!sealed.contains(VALUE) && !sealed.contains(UNSEAL_KEY));

    let opened = keeper.open(project()?, &sealed)?;
    let snapshot: StoreSnapshot = serde_json::from_slice(&opened)?;
    assert_eq!(
        (snapshot.node.as_str(), snapshot.taken_at_unix_ms),
        ("app-1", 7)
    );
    let plan = StoreRestorePlan::of(&snapshot, members(), sealed.as_bytes());
    assert_eq!(
        plan,
        StoreRestorePlan::of(&snapshot, members(), sealed.as_bytes())
    );
    assert_ne!(
        plan.plan_hash,
        StoreRestorePlan::of(&snapshot, members()[..1].to_vec(), sealed.as_bytes()).plan_hash
    );

    let fresh_bao = Arc::new(Bao::default());
    let fresh = initialised(&temporary.path().join("fresh"), &fresh_bao, &keeper).await?;
    assert_eq!(
        store(&fresh, &fresh_bao)?
            .live(&scope, &reference)
            .await?
            .status,
        SecretStatus::Unset
    );
    let recipient = fresh.recipient()?;
    let restored = fresh
        .store
        .restore(StoreRestoreStep::Snapshot {
            sealed: keeper.seal(&recipient, &opened)?,
        })
        .await?;
    assert!(restored.sealed, "the member takes the snapshot's keys next");
    let keys = keeper.seal(&recipient, &serde_json::to_vec(&snapshot.recovery)?)?;
    let restored = fresh
        .store
        .restore(StoreRestoreStep::Keys { sealed: keys })
        .await?;
    assert!(!restored.sealed);
    let metadata = store(&fresh, &fresh_bao)?.live(&scope, &reference).await?;
    assert_eq!(metadata.revision.as_deref(), Some("second-revision"));
    assert_eq!(
        fresh_bao
            .state()?
            .kv
            .values()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        [2],
        "the version history came back too"
    );
    assert!(
        old.store
            .restore(StoreRestoreStep::Snapshot {
                sealed: keeper.seal(&fresh.recipient()?, &opened)?,
            })
            .await
            .is_err(),
        "a snapshot sealed to another node does not open here"
    );
    Ok(())
}

#[tokio::test]
async fn migration_reads_the_age_store_as_it_is_live_never_what_is_staged() -> TestResult {
    use mm::app::ports::SecretCreator as _;
    let temporary = temp_dirs::tempdir()?;
    let age = AgeSecretStore::new(
        temporary.path().join("identities/age.key"),
        temporary.path().join(format!("secrets/{PROJECT}.age")),
    );
    age.ensure_recipient()?;
    let reference = SecretReference::new(StorageProviderId::new("age")?, "db.password")?;
    let scope = SecretScope::new(ENVIRONMENT);
    assert!(age.live_secret(&scope, &reference)?.is_none());
    let written = age
        .create(
            &scope,
            &reference,
            mm::domain::secret::SecretMaterial::from_zeroizing(Zeroizing::new(VALUE.to_owned()))?,
            1,
            None,
        )
        .await?;
    age.stage_values(
        uuid::Uuid::new_v4(),
        vec![StoreStagedChange {
            slot: StagedSlot::Secret {
                environment: ENVIRONMENT.to_owned(),
                reference: reference.clone(),
            },
            change: StoreChange::Set {
                revision: "staged-revision".to_owned(),
                value: Zeroizing::new("staged-value-of-the-test".to_owned()),
            },
        }],
        "alice",
        2,
    )?;
    let (revision, value) = age
        .live_secret(&scope, &reference)?
        .ok_or("no live value")?;
    assert_eq!(revision, written.revision);
    assert_eq!(value.as_str(), VALUE);
    Ok(())
}

#[test]
fn a_plan_names_the_members_it_upgrades_and_the_releases_are_pinned() -> TestResult {
    let pinned = mm::domain::secret_store::pinned_release("2.5").ok_or("2.5 is not pinned")?;
    assert_eq!(pinned.version, "2.5.5");
    assert!(
        pinned
            .archives
            .iter()
            .all(|archive| archive.sha256.len() == 64)
    );
    assert_eq!(
        pinned.url(&pinned.archives[0]),
        "https://github.com/openbao/openbao/releases/download/v2.5.5/bao_2.5.5_Linux_x86_64.tar.gz"
    );
    assert!(mm::domain::secret_store::pinned_release("2.6").is_none());
    let plan = mm::domain::secret_store::ClusterStorePlan {
        provider: PROVIDER.to_owned(),
        release: "2.5".to_owned(),
        version: "2.5.5".to_owned(),
        members: members(),
        installed: BTreeMap::from([
            ("app-1".to_owned(), "2.4.4".to_owned()),
            ("app-2".to_owned(), "2.5.5".to_owned()),
        ]),
        initialised: true,
        rejoins: Vec::new(),
    };
    assert_eq!(
        plan.changes(),
        [
            "app-1: upgrades OpenBao 2.4.4 to 2.5.5",
            "app-2: runs OpenBao 2.5.5",
            "app-3: installs OpenBao 2.5.5 on 10.90.0.3",
        ]
    );
    Ok(())
}

/// What the facts script prints for a node: its member record, if any, and
/// whether the member's Raft data is on disk.
fn facts_printed(record: Option<&MemberRecord>, data: bool) -> TestResult<StoreFacts> {
    let record = match record {
        Some(record) => serde_json::to_string_pretty(record)?,
        None => "null".to_owned(),
    };
    Ok(serde_json::from_str(&format!(
        "{{\"data\":{data},\"record\":{record}}}\n"
    ))?)
}

#[test]
fn a_member_whose_data_is_gone_rejoins_and_one_that_is_merely_down_does_not() -> TestResult {
    let record = |node: &str, initialized: bool| MemberRecord {
        version: "2.4.4".to_owned(),
        initialized,
        provider: PROVIDER.to_owned(),
        project_id: project().unwrap_or(ProjectId(uuid::Uuid::nil())),
        environment: ENVIRONMENT.to_owned(),
        node: node.to_owned(),
        address: std::net::Ipv4Addr::new(10, 90, 0, 1),
        members: members(),
    };
    let serving = facts_printed(Some(&record("app-1", true)), true)?;
    let down = facts_printed(Some(&record("app-2", true)), true)?;
    let reinstalled = facts_printed(None, false)?;
    let wiped = facts_printed(Some(&record("app-3", true)), false)?;
    assert!(serving.holds_data() && down.holds_data());
    assert!(!reinstalled.holds_data() && !wiped.holds_data());
    assert_eq!(serving.version(), Some("2.4.4"));
    assert_eq!(reinstalled.version(), None);

    let listed = |names: &[&str]| {
        names
            .iter()
            .map(|name| StorePeer {
                name: (*name).to_owned(),
                address: format!("{name}:8201"),
                leader: *name == "app-1",
                voter: true,
            })
            .collect::<Vec<_>>()
    };
    let facts = BTreeMap::from([
        ("app-1".to_owned(), serving.clone()),
        ("app-2".to_owned(), down),
        ("app-3".to_owned(), reinstalled.clone()),
    ]);
    assert_eq!(
        rejoining(&members(), &facts, &listed(&["app-1", "app-2", "app-3"])),
        ["app-3"],
        "a member that holds its data stays in the Raft configuration however long it was down"
    );
    assert!(
        rejoining(&members(), &facts, &listed(&["app-1", "app-2"])).is_empty(),
        "a member the Raft configuration does not list joins as any new one"
    );
    let wiped_only = BTreeMap::from([("app-2".to_owned(), wiped), ("app-1".to_owned(), serving)]);
    assert_eq!(
        rejoining(
            &members(),
            &wiped_only,
            &listed(&["app-1", "app-2", "app-3"])
        ),
        ["app-2"],
        "a member left out of the plan is not touched"
    );

    let plan = mm::domain::secret_store::ClusterStorePlan {
        provider: PROVIDER.to_owned(),
        release: "2.4".to_owned(),
        version: "2.4.4".to_owned(),
        members: members(),
        installed: BTreeMap::from([
            ("app-1".to_owned(), "2.4.4".to_owned()),
            ("app-2".to_owned(), "2.4.4".to_owned()),
        ]),
        initialised: true,
        rejoins: vec!["app-3".to_owned()],
    };
    assert_eq!(
        plan.changes().last().map(String::as_str),
        Some(
            "app-3: rejoins: it holds none of the store's data while the Raft configuration still lists it; setup removes that entry through a serving member, then joins it again"
        )
    );

    let healthy = member_status("app-1", "2.4.4", false, &["app-1", "app-2", "app-3"]);
    assert!(removal_through(&healthy, "app-3"));
    assert!(!removal_through(&healthy, "app-1"), "never through itself");
    let sealed = member_status("app-2", "2.4.4", true, &["app-1", "app-2", "app-3"]);
    assert!(!removal_through(&sealed, "app-3"));
    let leaderless = StoreStatus {
        leader: None,
        ..member_status("app-2", "2.4.4", false, &["app-2", "app-3"])
    };
    assert!(!removal_through(&leaderless, "app-3"));
    let led_by_stale = StoreStatus {
        leader: Some("app-3".to_owned()),
        ..member_status("app-2", "2.4.4", false, &["app-2", "app-3"])
    };
    assert!(!removal_through(&led_by_stale, "app-3"));
    Ok(())
}

#[tokio::test]
async fn mmd_removes_a_stale_peer_through_its_member_and_never_itself_or_the_leader() -> TestResult
{
    let temporary = temp_dirs::tempdir()?;
    let keeper = keeper(temporary.path());
    let bao = Arc::new(Bao::default());
    let node = initialised(temporary.path(), &bao, &keeper).await?;
    bao.state()?.raft = Some(vec![
        ("app-1".to_owned(), false),
        ("app-2".to_owned(), true),
        ("app-3".to_owned(), false),
    ]);

    assert!(
        node.store.remove_peer("app-1").await.is_err(),
        "a member does not remove itself"
    );
    assert!(
        node.store.remove_peer("app-2").await.is_err(),
        "the member that leads is not gone"
    );
    let removed = node.store.remove_peer("app-3").await?;
    assert!(removed.removed && removed.peer == "app-3" && removed.through == "app-1");
    let again = node.store.remove_peer("app-3").await?;
    assert!(!again.removed, "a second removal finds nothing to remove");
    {
        let mut state = bao.state()?;
        assert_eq!(
            state
                .servers()
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["app-1", "app-2"]
        );
        assert_eq!(
            state
                .requests
                .iter()
                .filter(|request| request.as_str() == "POST sys/storage/raft/remove-peer")
                .count(),
            1
        );
    }

    let stranger = Node::new(temporary.path(), "app-3", &bao)?;
    assert!(
        stranger.store.remove_peer("app-2").await.is_err(),
        "a node that runs no member of the store removes nothing"
    );
    bao.seal()?;
    assert!(
        node.store.remove_peer("app-2").await.is_err(),
        "a sealed member removes nothing"
    );
    Ok(())
}

#[tokio::test]
async fn mmd_lists_every_version_the_store_keeps_and_no_value() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let keeper = keeper(temporary.path());
    let bao = Arc::new(Bao::default());
    let node = initialised(temporary.path(), &bao, &keeper).await?;
    let store = OpenBaoSecretStore::new(
        ProviderInstanceName::new(PROVIDER)?,
        OpenBaoMode::Managed,
        project()?,
        Arc::new(connections(&node.home())),
        Arc::clone(&bao) as _,
    );
    let scope = SecretScope::new(ENVIRONMENT);
    for (location, revision, value) in [
        (
            "database/password",
            "first-revision",
            "first-value-of-the-test",
        ),
        ("database/password", "second-revision", VALUE),
        ("api.token", "token-revision", VALUE),
    ] {
        let reference = SecretReference::new(StorageProviderId::new(PROVIDER)?, location)?;
        store
            .write_live(
                &scope,
                &reference,
                (&Zeroizing::new(value.to_owned()), revision),
                1,
            )
            .await?;
    }

    let before = bao.state()?.requests.len();
    let versions = node.store.versions().await?;
    assert!(
        bao.state()?.requests[before..]
            .iter()
            .all(|request| !request.contains("mm-kv/data/")),
        "versions are read from the metadata alone"
    );
    assert_eq!(
        versions
            .iter()
            .map(|secret| (
                secret.location.as_str(),
                secret.current,
                secret
                    .versions
                    .iter()
                    .map(|version| version.version)
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        [
            ("api.token", 1, vec![1]),
            ("database/password", 2, vec![1, 2]),
        ]
    );
    assert!(
        versions
            .iter()
            .flat_map(|secret| &secret.versions)
            .all(|version| !version.created.is_empty() && !version.deleted && !version.destroyed)
    );
    let printed = serde_json::to_string(&versions)?;
    assert!(!printed.contains(VALUE) && !printed.contains("first-value-of-the-test"));
    Ok(())
}

fn node_report(name: &str, store: Option<StoreStatus>) -> mm::domain::remote::NodeReport {
    mm::domain::remote::NodeReport {
        name: name.to_owned(),
        host: "203.0.113.1".to_owned(),
        port: 22,
        repo_address: None,
        mesh_address: None,
        state: if store.is_some() {
            mm::domain::remote::NodeState::Ready
        } else {
            mm::domain::remote::NodeState::Unreachable
        },
        reachable: store.is_some(),
        problem: store
            .is_none()
            .then(|| "the host did not answer on port 22".to_owned()),
        mmd_version: None,
        deployed: None,
        rollout: None,
        engine: None,
        daemon: None,
        secret_store: store,
    }
}

#[test]
fn the_store_report_lists_every_member_as_its_node_reports_it() {
    let names = ["app-1", "app-2", "app-3"].map(str::to_owned);
    let voters = ["app-1", "app-2", "app-3"];
    let whole = ClusterStoreReport::of(
        (PROVIDER, ENVIRONMENT, "cluster"),
        "2.4.4",
        &names,
        &names
            .iter()
            .map(|name| node_report(name, Some(member_status(name, "2.4.4", false, &voters))))
            .collect::<Vec<_>>(),
    );
    assert!(whole.healthy(), "{whole:?}");
    assert_eq!(whole.leader.as_deref(), Some("app-1"));
    assert!(whole.diagnostics.is_empty());
    assert_eq!(
        whole
            .members
            .iter()
            .map(|member| member.status.as_ref().map(|status| status.node.as_str()))
            .collect::<Vec<_>>(),
        [Some("app-1"), Some("app-2"), Some("app-3")]
    );

    let missing = ClusterStoreReport::of(
        (PROVIDER, ENVIRONMENT, "cluster"),
        "2.4.4",
        &names,
        &[
            node_report(
                "app-1",
                Some(member_status("app-1", "2.4.4", false, &voters)),
            ),
            node_report(
                "app-2",
                Some(member_status("app-2", "2.4.4", true, &voters)),
            ),
            node_report("app-3", None),
        ],
    );
    assert!(!missing.healthy());
    assert_eq!(
        missing.members[2].problem.as_deref(),
        Some("the node does not answer: the host did not answer on port 22")
    );
    let codes = missing
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code.as_str(), diagnostic.message.as_str()))
        .collect::<Vec<_>>();
    assert!(
        codes
            .iter()
            .any(|(code, message)| *code == "MM_SECRET_STORE_SEALED" && message.contains("app-2")),
        "{codes:?}"
    );
    assert!(
        codes
            .iter()
            .any(|(code, message)| *code == "MM_SECRET_STORE_DEGRADED"
                && message.contains("app-3: the node does not answer")),
        "{codes:?}"
    );
    let json = serde_json::to_value(&missing).unwrap_or_default();
    assert_eq!(json["members"][0]["status"]["node"], "app-1");
    assert_eq!(json["version"], "2.4.4");
    assert_eq!(
        serde_json::from_value::<mm::domain::mcp::McpReadKind>(serde_json::json!("secret_store"))
            .ok(),
        Some(mm::domain::mcp::McpReadKind::SecretStore),
        "an agent reads the same report"
    );
}

#[test]
fn provider_status_of_a_cluster_store_asks_the_nodes() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("api"))?;
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: {PROJECT}\n  name: store\ntarget: local\nstacks:\n  app:\n    files: [compose.yaml]\nmodules:\n  api:\n    path: api\n    services:\n      api: {{stack: app}}\nproviders:\n  deploy-secrets:\n    driver: openbao\n    mode: managed\n    scope: environment\n    release: \"2.4\"\nsecrets:\n  database.password:\n    storage: {{provider: deploy-secrets, location: database/password}}\ntargets:\n  cluster:\n    nodes:\n{}environments:\n  production:\n    target: cluster\n",
            (1..=3)
                .map(|index| format!("      - {{name: app-{index}, host: 203.0.113.{index}, host-key: {HOST_KEY}, daemon-fingerprint: {}}}\n", index.to_string().repeat(64)))
                .collect::<String>()
        ),
    )?;
    let tools = temp_dirs::tempdir()?;
    let ssh = tools.path().join("ssh");
    fs::write(
        &ssh,
        "#!/bin/sh\necho 'Permission denied (publickey).' >&2\nexit 255\n",
    )?;
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755))?;
    }
    let path = format!(
        "{}:{}",
        tools.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    fs::write(
        root.join("compose.yaml"),
        "services:\n  api:\n    image: docker.io/library/alpine:3.22\n",
    )?;
    let output = temp_dirs::mm()
        .arg("--root")
        .arg(&root)
        .arg("--mm-home")
        .arg(temporary.path().join("home"))
        .args([
            "--output",
            "json",
            "-e",
            "production",
            "secret",
            "provider",
            "status",
            "deploy-secrets",
        ])
        .env("PATH", &path)
        .env("MM_SSH_REUSE", "off")
        .output()?;
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "no report ({error}): {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    assert_eq!(output.status.code(), Some(3), "{report}");
    assert_eq!(report["provider"], PROVIDER);
    assert_eq!(report["environment"], ENVIRONMENT);
    assert_eq!(report["version"], "2.4.4");
    let members = report["members"].as_array().ok_or("no members")?;
    assert_eq!(
        members
            .iter()
            .map(|member| member["node"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["app-1", "app-2", "app-3"]
    );
    assert!(
        members
            .iter()
            .all(|member| member["status"].is_null() && member["problem"].is_string()),
        "{report}"
    );
    assert!(
        report["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|diagnostic| diagnostic["code"] == "MM_SECRET_STORE_DEGRADED"),
        "{report}"
    );
    Ok(())
}

#[tokio::test]
async fn mmd_unseals_no_member_whose_data_is_gone_before_setup_removed_its_entry() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let keeper = keeper(temporary.path());
    let bao = Arc::new(Bao::default());
    let node = initialised(temporary.path(), &bao, &keeper).await?;
    let id = store_id(project()?, ENVIRONMENT);
    let data = node.store.layout.raft_data(&id);
    fs::create_dir_all(data.parent().ok_or("no data directory")?)?;
    fs::write(&data, "raft state of the fake store")?;
    let spec = StoreMemberSpec {
        provider: PROVIDER.to_owned(),
        node: "app-1".to_owned(),
        version: "2.4.4".to_owned(),
        members: members(),
        ca_pem: keeper.authority(scope()?)?,
        certificate_pem: None,
    };
    let unsealer = StoreUnsealer::new(
        node.store.layout.clone(),
        Arc::new(connections(&node.home())),
        Arc::clone(&bao) as _,
    );

    bao.seal()?;
    let down = node.store.member(spec.clone()).await?;
    assert!(
        down.initialized && !down.sealed,
        "a member that was only down comes back with its data: {down:?}"
    );

    fs::remove_file(&data)?;
    bao.seal()?;
    let wiped = node.store.member(spec).await?;
    assert!(
        !wiped.initialized && wiped.sealed && wiped.has_keys,
        "a member whose data is gone waits to join, keys or not: {wiped:?}"
    );
    assert!(
        !node
            .store
            .layout
            .record(&id)?
            .is_some_and(|record| record.initialized)
    );
    assert!(
        unsealer.round().await?.is_empty() && bao.state()?.sealed,
        "mmd leaves it sealed, so it joins only after setup removed its old entry"
    );
    Ok(())
}
