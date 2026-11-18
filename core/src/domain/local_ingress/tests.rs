use super::{ProjectSlug, ServiceSlug, is_reserved_mm_host, project_dashboard_slug};
use crate::domain::workspace::ProjectId;
use uuid::Uuid;

#[test]
fn slugs_normalize_deterministically_and_reject_every_unsafe_class() -> Result<(), String> {
    let project = ProjectSlug::normalize(" File Factory ")?;
    assert_eq!(project.as_str(), "file-factory");
    assert_eq!(project.to_string(), "file-factory");
    let service = ServiceSlug::normalize("WEB_api")?;
    assert_eq!(service.as_str(), "web-api");
    assert_eq!(service.to_string(), "web-api");
    for value in ["", "---", "mM", "caf\u{00e9}", &"a".repeat(64)] {
        assert!(
            ProjectSlug::normalize(value).is_err(),
            "project slug: {value:?}"
        );
        assert!(
            ServiceSlug::normalize(value).is_err(),
            "service slug: {value:?}"
        );
    }
    Ok(())
}

#[test]
fn managed_hosts_are_case_insensitive_and_boundary_aware() {
    for host in [
        "mm.test",
        "mail.mm.test",
        "mm.project.test",
        "MAIL.MM.TEST.",
    ] {
        assert!(is_reserved_mm_host(host), "host: {host}");
    }
    for host in ["web.project.test", "notmm.test", "mm.example.com"] {
        assert!(!is_reserved_mm_host(host), "host: {host}");
    }
}

#[test]
fn project_dashboard_hosts_include_stable_identity_and_reject_invalid_names()
-> Result<(), crate::domain::error::MmError> {
    let first = project_dashboard_slug("same display name", ProjectId(Uuid::from_u128(1)))?;
    let second = project_dashboard_slug("same display name", ProjectId(Uuid::from_u128(2)))?;
    assert_ne!(first, second);
    assert!(first.len() <= 63);
    assert!(first.ends_with("00000000000000000000000000000001"));
    assert!(project_dashboard_slug("---", ProjectId(Uuid::nil())).is_err());
    Ok(())
}
