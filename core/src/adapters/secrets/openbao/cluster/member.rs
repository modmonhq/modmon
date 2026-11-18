//! A member's files on its node.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PublicKeyData as _};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::adapters::node_files::{read_whole, write_whole};
use crate::domain::error::MmError;
use crate::domain::secret_store::{
    STORE_API_PORT, STORE_CLUSTER_PORT, StoreMember, StoreMemberSpec, store_unit,
};
use crate::domain::workspace::ProjectId;

const MEMBER_RECORD: &str = "member.json";
const CA: &str = "ca.pem";
const CERTIFICATE: &str = "member.pem";
const KEY: &str = "member-key.pem";
const CONFIG: &str = "config.hcl";
/// A certificate closer to its end than this is replaced.
const RENEW_BEFORE_SECONDS: i64 = 30 * 24 * 60 * 60;

/// Where a node keeps its members of managed stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreLayout {
    pub config_root: PathBuf,
    pub data_root: PathBuf,
    pub bin_root: PathBuf,
    pub unit_root: PathBuf,
}

impl StoreLayout {
    #[must_use]
    pub fn host() -> Self {
        Self {
            config_root: PathBuf::from("/etc/mm-openbao"),
            data_root: PathBuf::from("/var/lib/mm-openbao"),
            bin_root: PathBuf::from("/usr/libexec/mm-openbao"),
            unit_root: PathBuf::from("/etc/systemd/system"),
        }
    }

    #[must_use]
    pub fn config(&self, id: &str) -> PathBuf {
        self.config_root.join(id)
    }

    #[must_use]
    pub fn data(&self, id: &str) -> PathBuf {
        self.data_root.join(id)
    }

    /// The file OpenBao keeps the member's Raft state machine in.
    #[must_use]
    pub fn raft_data(&self, id: &str) -> PathBuf {
        self.data(id).join("data").join("vault.db")
    }

    #[must_use]
    pub fn binary(&self, version: &str) -> PathBuf {
        self.bin_root.join(version).join("bao")
    }

    #[must_use]
    pub fn audit_log(&self, id: &str) -> PathBuf {
        self.data(id).join("audit.log")
    }

    #[must_use]
    pub fn ca(&self, id: &str) -> PathBuf {
        self.config(id).join(CA)
    }

    /// Every store this node runs a member of, with its record.
    pub fn members(&self) -> Result<Vec<(String, MemberRecord)>, MmError> {
        let entries = match std::fs::read_dir(&self.config_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("reading {}", self.config_root.display()),
                    source,
                });
            }
        };
        let mut members = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let Ok(id) = entry.file_name().into_string() else {
                continue;
            };
            if let Some(record) = self.record(&id)? {
                members.push((id, record));
            }
        }
        members.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(members)
    }

    pub fn record(&self, id: &str) -> Result<Option<MemberRecord>, MmError> {
        read_whole(&self.config(id).join(MEMBER_RECORD))?
            .map(|bytes| {
                serde_json::from_slice(&bytes).map_err(|error| MmError::State {
                    message: format!("the member record of store {id} is not valid: {error}"),
                })
            })
            .transpose()
    }

    pub fn save_record(&self, id: &str, record: &MemberRecord) -> Result<bool, MmError> {
        let bytes = serde_json::to_vec_pretty(record).map_err(|error| MmError::State {
            message: format!("serializing the member record of store {id}: {error}"),
        })?;
        replace(&self.config(id).join(MEMBER_RECORD), &bytes, 0o640)
    }
}

/// What a node knows about its member, readable by `mm target setup`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MemberRecord {
    pub version: String,
    /// The member joined the store and was unsealed with its data.
    pub initialized: bool,
    pub provider: String,
    pub project_id: ProjectId,
    pub environment: String,
    pub node: String,
    pub address: Ipv4Addr,
    pub members: Vec<StoreMember>,
}

impl MemberRecord {
    #[must_use]
    pub fn unit(&self, id: &str) -> String {
        store_unit(id)
    }
}

/// The text at `path`; `None` when there is no file.
pub(super) fn read_text(path: &Path) -> Result<Option<String>, MmError> {
    read_whole(path)?
        .map(String::from_utf8)
        .transpose()
        .map_err(|_| MmError::State {
            message: format!("{} is not text", path.display()),
        })
}

/// Writes `bytes` unless `path` holds exactly them; true when it changed.
fn replace(path: &Path, bytes: &[u8], mode: u32) -> Result<bool, MmError> {
    if read_whole(path)?.is_some_and(|held| held == bytes) {
        return Ok(false);
    }
    write_whole(path, bytes, mode)?;
    Ok(true)
}

fn tls_error(error: impl std::fmt::Display) -> MmError {
    MmError::Security {
        message: format!("the secret store's TLS identity: {error}"),
    }
}

/// The node's own TLS key, made once.
pub(super) fn member_key(layout: &StoreLayout, id: &str) -> Result<KeyPair, MmError> {
    let path = layout.config(id).join(KEY);
    if let Some(held) = read_whole(&path)? {
        let held = Zeroizing::new(String::from_utf8(held).map_err(tls_error)?);
        return KeyPair::from_pem(&held).map_err(tls_error);
    }
    let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(tls_error)?;
    let pem = Zeroizing::new(key.serialize_pem());
    write_whole(&path, pem.as_bytes(), 0o640)?;
    Ok(key)
}

/// A certificate request for the node's key, naming the node, its mesh
/// address and loopback.
pub(super) fn certificate_request(
    key: &KeyPair,
    node: &str,
    address: Ipv4Addr,
) -> Result<String, MmError> {
    let mut params = CertificateParams::new(vec![
        node.to_owned(),
        "localhost".to_owned(),
        address.to_string(),
        Ipv4Addr::LOCALHOST.to_string(),
    ])
    .map_err(tls_error)?;
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, node);
    params
        .serialize_request(key)
        .and_then(|request| request.pem())
        .map_err(tls_error)
}

/// Whether `certificate` is for `key`, names `address`, comes from the
/// authority `ca`, and has more than a month to run.
pub(super) fn certificate_fits(
    certificate: &str,
    key: &KeyPair,
    ca: &str,
    address: Ipv4Addr,
) -> bool {
    use x509_parser::prelude::FromDer;
    let (Ok(der), Ok(ca_der)) = (
        crate::domain::ca::certificate_der(certificate.as_bytes()),
        crate::domain::ca::certificate_der(ca.as_bytes()),
    ) else {
        return false;
    };
    let (Ok((_, parsed)), Ok((_, authority))) = (
        x509_parser::certificate::X509Certificate::from_der(&der),
        x509_parser::certificate::X509Certificate::from_der(&ca_der),
    ) else {
        return false;
    };
    let names = match parsed.subject_alternative_name() {
        Ok(names) => names,
        Err(_) => return false,
    };
    let names_address = names.is_some_and(|names| {
        names.value.general_names.iter().any(|name| {
            matches!(name, x509_parser::extensions::GeneralName::IPAddress(bytes)
                    if *bytes == address.octets().as_slice())
        })
    });
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    parsed.public_key().raw == key.subject_public_key_info()
        && names_address
        && parsed.issuer().as_raw() == authority.subject().as_raw()
        && parsed.validity().not_after.timestamp() > now + RENEW_BEFORE_SECONDS
}

/// The member's OpenBao configuration.
#[must_use]
pub fn render_config(layout: &StoreLayout, id: &str, record: &MemberRecord) -> String {
    let config = layout.config(id);
    let certificate = config.join(CERTIFICATE);
    let key = config.join(KEY);
    let ca = config.join(CA);
    let joins = record
        .members
        .iter()
        .filter(|member| member.name != record.node)
        .map(|member| {
            format!(
                "  retry_join {{\n    leader_api_addr = \"https://{}:{STORE_API_PORT}\"\n    leader_ca_cert_file = \"{}\"\n  }}\n",
                member.address,
                ca.display()
            )
        })
        .collect::<String>();
    let listener = |address: IpAddr| {
        format!(
            "listener \"tcp\" {{\n  address = \"{address}:{STORE_API_PORT}\"\n  cluster_address = \"{address}:{STORE_CLUSTER_PORT}\"\n  tls_cert_file = \"{}\"\n  tls_key_file = \"{}\"\n  tls_min_version = \"tls13\"\n}}\n",
            certificate.display(),
            key.display()
        )
    };
    format!(
        "ui = false\ndisable_mlock = true\ncluster_name = \"mm-{id}\"\napi_addr = \"https://{address}:{STORE_API_PORT}\"\ncluster_addr = \"https://{address}:{STORE_CLUSTER_PORT}\"\n\nstorage \"raft\" {{\n  path = \"{data}\"\n  node_id = \"{node}\"\n{joins}}}\n\n{loopback}\n{mesh}\naudit \"file\" \"mm\" {{\n  options {{\n    file_path = \"{audit}\"\n  }}\n}}\n",
        address = record.address,
        data = layout.data(id).join("data").display(),
        node = record.node,
        loopback = listener(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        mesh = listener(IpAddr::V4(record.address)),
        audit = layout.audit_log(id).display(),
    )
}

/// The member's systemd unit: the pinned release as the store's own
/// account, with nothing to write but its data.
#[must_use]
pub fn render_unit(layout: &StoreLayout, id: &str, record: &MemberRecord) -> String {
    format!(
        "[Unit]\nDescription=mm secret store of environment {environment} (OpenBao {version})\nDocumentation=https://mm.local/docs/how-to/managed-secret-store\nAfter=network-online.target mm-mesh.service\nWants=network-online.target\nStartLimitIntervalSec=0\n\n[Service]\nUser=mm-openbao\nGroup=mm-openbao\nExecStart={binary} server -config={config}\nExecReload=/bin/kill -HUP $MAINPID\nRestart=on-failure\nRestartSec=5s\nTimeoutStopSec=30s\nLimitNOFILE=65536\nUMask=0077\nNoNewPrivileges=yes\nPrivateTmp=yes\nPrivateDevices=yes\nProtectSystem=strict\nProtectHome=yes\nProtectKernelTunables=yes\nProtectKernelModules=yes\nProtectKernelLogs=yes\nProtectControlGroups=yes\nProtectClock=yes\nProtectHostname=yes\nLockPersonality=yes\nRestrictRealtime=yes\nRestrictSUIDSGID=yes\nRestrictNamespaces=yes\nRemoveIPC=yes\nCapabilityBoundingSet=\nAmbientCapabilities=\nRestrictAddressFamilies=AF_UNIX AF_INET AF_INET6\nSystemCallArchitectures=native\nReadWritePaths={data}\n\n[Install]\nWantedBy=multi-user.target\n",
        environment = record.environment,
        version = record.version,
        binary = layout.binary(&record.version).display(),
        config = layout.config(id).join(CONFIG).display(),
        data = layout.data(id).display(),
    )
}

/// What [`write_member`] changed, and the request to sign when the node
/// holds no fitting certificate.
pub(super) struct WrittenMember {
    pub changed: bool,
    pub unit_changed: bool,
    pub request: Option<String>,
}

/// Brings the member's files to `spec`. With no fitting certificate it
/// writes nothing and returns the request to sign.
pub(super) fn write_member(
    layout: &StoreLayout,
    (id, project, environment): (&str, ProjectId, &str),
    spec: &StoreMemberSpec,
    initialized: bool,
) -> Result<(WrittenMember, MemberRecord), MmError> {
    let address = spec
        .members
        .iter()
        .find(|member| member.name == spec.node)
        .map(|member| member.address)
        .ok_or_else(|| MmError::Target {
            message: format!(
                "node {} is not a member of the store it is given",
                spec.node
            ),
        })?;
    if !layout.binary(&spec.version).is_file() {
        return Err(MmError::Target {
            message: format!(
                "OpenBao {} is not installed at {}; `mm target setup apply` installs it",
                spec.version,
                layout.binary(&spec.version).display()
            ),
        });
    }
    let config = layout.config(id);
    let key = member_key(layout, id)?;
    let held = read_text(&config.join(CERTIFICATE))?;
    let certificate = match (&spec.certificate_pem, held) {
        (Some(given), _) if certificate_fits(given, &key, &spec.ca_pem, address) => given.clone(),
        (Some(_), _) => {
            return Err(MmError::Security {
                message: format!(
                    "the certificate given to node {} is not for its key, its address {address}, or the environment's authority",
                    spec.node
                ),
            });
        }
        (None, Some(held)) if certificate_fits(&held, &key, &spec.ca_pem, address) => held,
        (None, _) => {
            let record = MemberRecord {
                version: spec.version.clone(),
                initialized,
                provider: spec.provider.clone(),
                project_id: project,
                environment: environment.to_owned(),
                node: spec.node.clone(),
                address,
                members: spec.members.clone(),
            };
            return Ok((
                WrittenMember {
                    changed: false,
                    unit_changed: false,
                    request: Some(certificate_request(&key, &spec.node, address)?),
                },
                record,
            ));
        }
    };
    let record = MemberRecord {
        version: spec.version.clone(),
        initialized,
        provider: spec.provider.clone(),
        project_id: project,
        environment: environment.to_owned(),
        node: spec.node.clone(),
        address,
        members: spec.members.clone(),
    };
    let mut changed = replace(&config.join(CA), spec.ca_pem.as_bytes(), 0o640)?;
    changed |= replace(&config.join(CERTIFICATE), certificate.as_bytes(), 0o640)?;
    changed |= replace(
        &config.join(CONFIG),
        render_config(layout, id, &record).as_bytes(),
        0o640,
    )?;
    let unit_changed = replace(
        &layout.unit_root.join(store_unit(id)),
        render_unit(layout, id, &record).as_bytes(),
        0o644,
    )?;
    layout.save_record(id, &record)?;
    Ok((
        WrittenMember {
            changed: changed || unit_changed,
            unit_changed,
            request: None,
        },
        record,
    ))
}
