//! Port forwarding to a remote service: one local listener per forward, one
//! relayed connection per accepted client, bytes carried by the mm protocol.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A forward a developer saved for quick reuse. Private to this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ForwardShortcut {
    pub environment: String,
    pub service: String,
    pub port: u16,
    /// The local port, pinned so tools can keep using it.
    pub local_port: u16,
    #[serde(default = "default_bind")]
    pub bind: String,
    /// Start with `mm forward up`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto: bool,
}

#[must_use]
pub fn default_bind() -> String {
    "127.0.0.1".to_owned()
}

/// Shortcuts by project ID, then by name.
pub type ForwardShortcuts = BTreeMap<String, BTreeMap<String, ForwardShortcut>>;

/// Bytes one forwarded connection carried in each direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ForwardTotals {
    pub to_service: u64,
    pub from_service: u64,
}

/// `SERVICE` or `SERVICE:PORT`.
pub fn parse_forward_spec(value: &str) -> Result<(String, Option<u16>), String> {
    let (service, port) = match value.split_once(':') {
        Some((service, port)) => (
            service,
            Some(
                port.parse::<u16>()
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or_else(|| format!("'{port}' is not a port number"))?,
            ),
        ),
        None => (value, None),
    };
    if service.is_empty()
        || !service
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("'{service}' is not a service name"));
    }
    Ok((service.to_owned(), port))
}

/// Only loopback unless the developer explicitly asks otherwise.
#[must_use]
pub fn is_loopback_bind(bind: &str) -> bool {
    matches!(bind, "127.0.0.1" | "::1" | "localhost")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_specs_name_a_service_and_optional_port() {
        assert_eq!(
            parse_forward_spec("postgres"),
            Ok(("postgres".to_owned(), None))
        );
        assert_eq!(
            parse_forward_spec("db:5432"),
            Ok(("db".to_owned(), Some(5432)))
        );
        assert!(parse_forward_spec("db:0").is_err());
        assert!(parse_forward_spec("db:http").is_err());
        assert!(parse_forward_spec(":5432").is_err());
        assert!(parse_forward_spec("a/b").is_err());
        assert!(is_loopback_bind("::1") && !is_loopback_bind("0.0.0.0"));
    }
}
