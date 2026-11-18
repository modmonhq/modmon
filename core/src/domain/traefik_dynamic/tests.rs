use super::{
    RenderedTraefikDynamicConfig, TraefikDynamicConfigError, TraefikDynamicConfigManifest,
};
use crate::domain::target::TargetId;
use uuid::Uuid;

#[test]
fn rendered_debug_reports_metadata_and_size_without_rendered_content() {
    let rendered = RenderedTraefikDynamicConfig {
        manifest: TraefikDynamicConfigManifest {
            target_id: TargetId(Uuid::from_u128(1)),
            claim_count: 1,
            router_count: 2,
            service_count: 1,
            digest: "a".repeat(64),
        },
        bytes: b"private route content".to_vec(),
    };
    let debug = format!("{rendered:?}");
    assert!(debug.contains("byte_count: 21"));
    assert!(!debug.contains("private route content"));
}

#[test]
fn dynamic_configuration_errors_keep_actionable_typed_context() {
    let target = TargetId(Uuid::from_u128(1));
    let errors = [
        TraefikDynamicConfigError::TargetMismatch {
            expected_target_id: target,
            actual_target_id: TargetId(Uuid::from_u128(2)),
            router_name: "web".to_owned(),
        },
        TraefikDynamicConfigError::InvalidRouterName {
            router_name: "bad/name".to_owned(),
        },
        TraefikDynamicConfigError::InvalidHost {
            router_name: "web".to_owned(),
            host: "bad host".to_owned(),
        },
        TraefikDynamicConfigError::InvalidPath {
            router_name: "web".to_owned(),
            path: "relative".to_owned(),
        },
        TraefikDynamicConfigError::InvalidPort {
            router_name: "web".to_owned(),
        },
        TraefikDynamicConfigError::InvalidBackendAlias {
            router_name: "web".to_owned(),
            backend_alias: "bad alias".to_owned(),
        },
        TraefikDynamicConfigError::OverlappingClaims {
            host: "app.test".to_owned(),
            left_path: "/".to_owned(),
            right_path: "/api".to_owned(),
            left_router_name: "left".to_owned(),
            right_router_name: "right".to_owned(),
        },
        TraefikDynamicConfigError::RouterNameCollision {
            generated_name: "same".to_owned(),
            left_router_name: "left".to_owned(),
            right_router_name: "right".to_owned(),
        },
        TraefikDynamicConfigError::UnknownMiddleware {
            router_name: "web".to_owned(),
            middleware: "unknown".to_owned(),
        },
        TraefikDynamicConfigError::InvalidMiddlewareName {
            middleware: "bad/name".to_owned(),
        },
        TraefikDynamicConfigError::ConflictingMiddleware {
            middleware: "auth".to_owned(),
        },
        TraefikDynamicConfigError::Serialization {
            message: "invalid".to_owned(),
        },
        TraefikDynamicConfigError::LocalHttpsRequiresCa {
            host: "app.test".to_owned(),
        },
    ];
    for error in errors {
        assert!(!error.to_string().is_empty());
    }
}
