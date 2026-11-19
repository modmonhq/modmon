//! Which remote protocol a client and a daemon speak, and what each says
//! when they have none in common.

use serde::Deserialize;
use uuid::Uuid;

use super::{ClientHello, MINIMUM_PROTOCOL_VERSION, PROTOCOL_VERSION};
use crate::domain::error::MmError;

/// The code of a refusal that one side has to be upgraded for.
pub const UPGRADE_REQUIRED_CODE: &str = "MM_REMOTE_UPGRADE_REQUIRED";

pub const PROTOCOL_REFUSED: &str = "has no remote protocol in common with this mm";
pub const IDENTITY_REFUSED: &str = "runs an mmd with another identity than the one recorded for it";
pub const HANDSHAKE_MISMATCH: &str =
    "answered the handshake with an unexpected identity, nonce, or version";

const UPGRADE_HOST: &str =
    "Upgrade the host: run `mm target setup plan --upgrade` and apply that plan with --upgrade";

/// Whether `problem` says the host's `mmd` refused the handshake.
#[must_use]
pub fn refused_handshake(problem: &str) -> bool {
    [PROTOCOL_REFUSED, IDENTITY_REFUSED, HANDSHAKE_MISMATCH]
        .iter()
        .any(|refused| problem.starts_with(refused))
}

/// The remote protocols one side speaks, `minimum` to `maximum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolRange {
    pub minimum: u16,
    pub maximum: u16,
}

impl ProtocolRange {
    /// What this build speaks.
    pub const THIS: Self = Self {
        minimum: MINIMUM_PROTOCOL_VERSION,
        maximum: PROTOCOL_VERSION,
    };

    #[must_use]
    pub const fn contains(self, version: u16) -> bool {
        self.minimum <= version && version <= self.maximum
    }
}

impl std::fmt::Display for ProtocolRange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.minimum == self.maximum {
            write!(formatter, "{}", self.maximum)
        } else {
            write!(formatter, "{} to {}", self.minimum, self.maximum)
        }
    }
}

/// The side that has to be upgraded before a client and a daemon have a
/// protocol in common.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behind {
    Client,
    Daemon,
}

/// The newest protocol both a client and a daemon speak.
pub fn negotiate(client: ProtocolRange, daemon: ProtocolRange) -> Result<u16, Behind> {
    let newest = client.maximum.min(daemon.maximum);
    if client.contains(newest) && daemon.contains(newest) {
        Ok(newest)
    } else if client.maximum < daemon.minimum {
        Err(Behind::Client)
    } else {
        Err(Behind::Daemon)
    }
}

/// A daemon's refusal of a client it has no protocol in common with: both
/// releases and protocols, and what the side that is behind runs.
#[must_use]
pub fn protocol_refusal(
    (client_release, client): (&str, ProtocolRange),
    (daemon_release, daemon): (&str, ProtocolRange),
) -> String {
    let this = match client_release {
        "" => "This mm".to_owned(),
        release => format!("This mm {release}"),
    };
    let step = match negotiate(client, daemon) {
        Err(Behind::Client) => format!(
            "Update mm on this machine to {daemon_release} or newer: run `mm update`, or the installer, https://modmon.dev/install.sh"
        ),
        Ok(_) | Err(Behind::Daemon) => UPGRADE_HOST.to_owned(),
    };
    format!(
        "{this} speaks remote protocol {client}, and the host's mmd {daemon_release} speaks {daemon}. {step}"
    )
}

/// A daemon's answer to a request it cannot read.
#[must_use]
pub fn request_not_understood(method: &str, daemon_release: &str, client_release: &str) -> String {
    let method = if !method.is_empty()
        && method.len() <= 64
        && method
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        format!("`{method}`")
    } else {
        "this request".to_owned()
    };
    let from = match client_release {
        "" => "this mm".to_owned(),
        release => format!("mm {release}"),
    };
    format!(
        "mmd {daemon_release} on this host does not understand {method} from {from}. {UPGRADE_HOST}"
    )
}

/// Fails, before anything is sent, a request that needs protocol `needed`
/// on a host whose session `negotiated` an older one. `host` is how
/// messages name it and the release of its `mmd`.
pub fn require_protocol(
    negotiated: u16,
    needed: u16,
    (host, daemon_release): (&str, &str),
    what: &str,
) -> Result<(), MmError> {
    if negotiated >= needed {
        return Ok(());
    }
    Err(MmError::Remote {
        code: UPGRADE_REQUIRED_CODE,
        message: format!(
            "{host} runs mmd {daemon_release}, which does not support {what}; upgrade it with `mm target setup plan --upgrade` and apply that plan with --upgrade"
        ),
    })
}

/// What a peer reads of any frame first: its kind and protocol, which a
/// frame from a newer peer has too.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FrameHead {
    #[serde(rename = "type")]
    pub kind: String,
    pub version: u16,
}

/// A client's hello as a daemon reads it: what a newer client adds is left
/// out, and a client from before 0.1.1, which sends no `minimum`, speaks
/// only `version`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HelloOffer {
    pub version: u16,
    #[serde(default)]
    pub minimum: Option<u16>,
    pub hello: ClientHello,
}

impl HelloOffer {
    #[must_use]
    pub fn protocols(&self) -> ProtocolRange {
        ProtocolRange {
            minimum: self.minimum.unwrap_or(self.version),
            maximum: self.version,
        }
    }

    /// The client's release as it said, or nothing when it said none fit to
    /// print.
    #[must_use]
    pub fn release(&self) -> &str {
        let release = self.hello.client_version.as_str();
        let printable = release.len() <= 64
            && release
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'));
        if printable { release } else { "" }
    }
}

/// What a daemon reads of a request it cannot read whole: enough to answer
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RequestHead {
    pub request_id: Uuid,
    pub method: MethodHead,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MethodHead {
    pub method: String,
}
