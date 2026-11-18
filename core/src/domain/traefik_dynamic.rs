use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::target::TargetId;

/// An empty document is valid; standalone empty router/service maps are not.
pub const EMPTY_DYNAMIC_CONFIG: &str = "{}\n";

pub const TRAEFIK_VERIFY_ENTRYPOINT: &str = "mm-verify";
pub const TRAEFIK_HTTPS_REDIRECT_MIDDLEWARE: &str = "mm-http-to-https";
pub const TRAEFIK_VERIFY_MIDDLEWARE: &str = "mm-route-verified";
pub const TRAEFIK_VERIFY_HEADER: &str = "X-MM-Route-Verified";
pub const TRAEFIK_VERIFY_HEADER_VALUE: &str = "1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TraefikDynamicConfigManifest {
    pub target_id: TargetId,
    pub claim_count: usize,
    pub router_count: usize,
    pub service_count: usize,
    pub digest: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RenderedTraefikDynamicConfig {
    pub manifest: TraefikDynamicConfigManifest,
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for RenderedTraefikDynamicConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RenderedTraefikDynamicConfig")
            .field("manifest", &self.manifest)
            .field("byte_count", &self.bytes.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PreparedTraefikDynamicConfig {
    pub root: PathBuf,
    pub path: PathBuf,
    pub manifest: TraefikDynamicConfigManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TraefikDynamicConfigError {
    #[error(
        "ingress claim router '{router_name}' belongs to target {actual_target_id:?}, not {expected_target_id:?}"
    )]
    TargetMismatch {
        expected_target_id: TargetId,
        actual_target_id: TargetId,
        router_name: String,
    },

    #[error("ingress claim router name '{router_name}' is invalid")]
    InvalidRouterName { router_name: String },

    #[error("ingress claim router '{router_name}' has invalid host '{host}'")]
    InvalidHost { router_name: String, host: String },

    #[error("ingress claim router '{router_name}' has invalid path '{path}'")]
    InvalidPath { router_name: String, path: String },

    #[error("ingress claim router '{router_name}' must use a non-zero backend port")]
    InvalidPort { router_name: String },

    #[error("ingress claim router '{router_name}' has invalid backend alias '{backend_alias}'")]
    InvalidBackendAlias {
        router_name: String,
        backend_alias: String,
    },

    #[error(
        "ingress claims '{left_router_name}' ({host}{left_path}) and '{right_router_name}' ({host}{right_path}) overlap"
    )]
    OverlappingClaims {
        host: String,
        left_path: String,
        right_path: String,
        left_router_name: String,
        right_router_name: String,
    },

    #[error(
        "ingress claims '{left_router_name}' and '{right_router_name}' generate the same Traefik router name '{generated_name}'"
    )]
    RouterNameCollision {
        generated_name: String,
        left_router_name: String,
        right_router_name: String,
    },

    #[error("ingress claim router '{router_name}' references unknown middleware '{middleware}'")]
    UnknownMiddleware {
        router_name: String,
        middleware: String,
    },

    #[error("ingress middleware name '{middleware}' is invalid")]
    InvalidMiddlewareName { middleware: String },

    #[error("ingress middleware '{middleware}' has conflicting definitions")]
    ConflictingMiddleware { middleware: String },

    #[error("could not serialize Traefik dynamic route configuration: {message}")]
    Serialization { message: String },

    #[error(
        "HTTPS host '{host}' requires the mm local CA; run `mm dev setup` and issue a certificate"
    )]
    LocalHttpsRequiresCa { host: String },
}

#[cfg(test)]
mod tests;
