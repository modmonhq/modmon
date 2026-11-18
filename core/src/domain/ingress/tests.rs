use super::{CanonicalMiddleware, IngressRouteProvenance};

#[test]
fn provenance_labels_and_middleware_digests_are_complete_and_content_sensitive()
-> Result<(), String> {
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

    let replacement = CanonicalMiddleware::ReplacePathRegex {
        regex: "^/old/(.*)".to_owned(),
        replacement: "/new/$1".to_owned(),
    };
    let forward = CanonicalMiddleware::ForwardAuth {
        address: "http://gate/auth".to_owned(),
        trust_forward_header: true,
        auth_response_headers: vec!["X-User".to_owned()],
        add_auth_cookies_to_response: vec!["session".to_owned()],
    };
    let first = replacement.digest()?;
    assert_eq!(first.len(), 64);
    assert_ne!(first, forward.digest()?);
    Ok(())
}
