//! Who may read the log of every service at once.
//!
//! A log read that names no service is answered only for a principal that
//! may read each service of the environment: the daemon checks them by
//! name, because a request without a service matches a grant that names
//! services.

#![cfg(unix)]

mod daemon_host;
mod temp_dirs;

use std::collections::{BTreeMap, BTreeSet};

use daemon_host::{ALICE, Host, OPS, PROMPT, TestResult};
use mm::domain::access::{
    ACCESS_POLICY_SCHEMA_VERSION, AccessAction, AccessPolicy, AccessRequest, AccessResourceKind,
    BuiltInRole, Capability, Grant, Principal, PrincipalId, PrincipalKey, RoleRef, ScopeMatcher,
};
use mm::domain::observability::{ApprovalContext, LogRequest};
use mm::domain::remote::{RemoteMethod, RemoteResult, ServerFrame};
use mm::domain::workspace::ProjectId;
use uuid::Uuid;

fn every_log() -> RemoteMethod {
    RemoteMethod::Logs(LogRequest::every_service(ApprovalContext {
        principal: "forged".to_owned(),
        source: "forged".to_owned(),
        approved_high_risk: false,
    }))
}

/// Puts the project's configuration where the host deployed it: the
/// daemon reads the environment's services from there.
fn deploy_configuration(host: &Host) -> TestResult {
    let origin = host.temporary.path().join("origin-work");
    let root = host.temporary.path().join("project");
    std::fs::create_dir_all(root.join(".config"))?;
    std::fs::copy(origin.join(".config/mm.yaml"), root.join(".config/mm.yaml"))?;
    std::fs::copy(origin.join("compose.yaml"), root.join("compose.yaml"))?;
    Ok(())
}

#[tokio::test]
async fn a_principal_whose_log_grant_names_no_service_reads_every_service_at_once() -> TestResult {
    let host = Host::with_alice().await?;
    deploy_configuration(&host)?;
    // alice's level lets her read logs, of any service; so does the
    // break-glass key.
    for key in [ALICE, OPS] {
        let mut log = host.ask(key, every_log()).await?;
        assert!(
            matches!(log.result(PROMPT).await?, RemoteResult::StreamStarted),
            "{key}"
        );
        assert!(matches!(
            log.next(PROMPT).await?,
            ServerFrame::End { frames: 0, .. }
        ));
    }
    Ok(())
}

const PROJECT: ProjectId = ProjectId(Uuid::from_u128(4));

/// A policy in which the one principal may read the log of `web` and of
/// no other service. The host compiles its policy from the users file,
/// where log reading comes with the level and is never limited to
/// services, so such a grant is written here by hand.
fn policy_for_web_only() -> AccessPolicy {
    let principal = PrincipalId(Uuid::from_u128(1));
    AccessPolicy {
        schema_version: ACCESS_POLICY_SCHEMA_VERSION,
        revision: 1,
        principals: vec![Principal {
            id: principal,
            display_name: "bob".to_owned(),
            project_id: Some(PROJECT),
            environment: Some("production".to_owned()),
            disabled: false,
            keys: vec![PrincipalKey {
                fingerprint: "SHA256:bob".to_owned(),
                public_key: "ssh-ed25519 bob".to_owned(),
                disabled: false,
                expires_at_unix_ms: None,
            }],
        }],
        roles: BTreeMap::new(),
        grants: vec![Grant {
            id: Uuid::from_u128(10),
            principal_id: principal,
            role: RoleRef::BuiltIn(BuiltInRole::Viewer),
            scope: ScopeMatcher {
                project_id: PROJECT,
                environments: BTreeSet::from(["production".to_owned()]),
                services: BTreeSet::from(["web".to_owned()]),
                jobs: BTreeSet::new(),
                gates: BTreeSet::new(),
            },
        }],
        denials: Vec::new(),
    }
}

fn log_of(service: Option<&str>) -> AccessRequest<'_> {
    AccessRequest {
        principal_id: PrincipalId(Uuid::from_u128(1)),
        capability: Capability::new(AccessAction::Logs, AccessResourceKind::Service),
        project_id: PROJECT,
        environment: "production",
        service,
        job: None,
        gate: None,
    }
}

/// The rule the daemon's check of every service rests on.
#[test]
fn a_grant_that_names_a_service_matches_a_request_that_names_none() {
    let policy = policy_for_web_only();
    assert!(policy.authorize(&log_of(Some("web"))).is_ok());
    assert!(policy.authorize(&log_of(Some("db"))).is_err());
    // This is why a read of every service is not authorized as one
    // request without a service: it would pass.
    assert!(policy.authorize(&log_of(None)).is_ok());
    // Asked service by service, as the daemon does, the read is refused
    // at the first service the grant does not name.
    let refused = ["web", "db"]
        .into_iter()
        .find(|service| policy.authorize(&log_of(Some(service))).is_err());
    assert_eq!(refused, Some("db"));
}
