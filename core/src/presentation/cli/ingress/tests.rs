use super::*;
use crate::app::ports::PublicIngressControl;
use crate::domain::host_ingress::PublicIngressStatus;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Control {
    status: PublicIngressStatus,
    mutations: AtomicUsize,
}

#[async_trait::async_trait]
impl PublicIngressControl for Control {
    async fn inspect(&self) -> Result<PublicIngressStatus, MmError> {
        Ok(self.status)
    }
    async fn set_state(&self, state: PublicIngressState, _: bool) -> Result<(), MmError> {
        assert_eq!(state, PublicIngressState::Stopped);
        self.mutations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn absent_stopped_and_failed_forwarders_stop_without_privileged_operations()
-> Result<(), MmError> {
    for (installed, running, failed) in [
        (false, false, false),
        (true, false, false),
        (true, false, true),
        (true, true, false),
    ] {
        let control = Control {
            status: forwarder(installed, running, failed),
            mutations: AtomicUsize::new(0),
        };
        assert_eq!(stop_if_running(&control, false).await?, running);
        assert_eq!(
            control.mutations.load(Ordering::SeqCst),
            usize::from(running)
        );
    }
    Ok(())
}

fn forwarder(installed: bool, running: bool, failed: bool) -> PublicIngressStatus {
    PublicIngressStatus {
        installed,
        running,
        failed,
        disabled: false,
    }
}

#[tokio::test]
async fn stop_keeps_an_enabled_forwarder_stopped_and_skips_one_already_kept_stopped()
-> Result<(), MmError> {
    let kept = PublicIngressStatus {
        disabled: true,
        ..forwarder(true, false, false)
    };
    for (status, mutates) in [
        (forwarder(false, false, false), false),
        (forwarder(true, true, false), true),
        (forwarder(true, false, false), true),
        (forwarder(true, false, true), true),
        (kept, false),
    ] {
        let control = Control {
            status,
            mutations: AtomicUsize::new(0),
        };
        assert_eq!(keep_stopped(&control, false).await?, mutates);
        assert_eq!(
            control.mutations.load(Ordering::SeqCst),
            usize::from(mutates)
        );
    }
    Ok(())
}

#[test]
fn status_reports_a_forwarder_kept_stopped_for_another_proxy() -> Result<(), serde_json::Error> {
    let report = status_report(
        PublicIngressStatus {
            disabled: true,
            ..forwarder(true, false, false)
        },
        Ok(services(true)),
        Ok(tls(true)),
        Some((18080, 18081)),
    );
    assert_eq!(report.state, PublicIngressState::Stopped);
    let json = serde_json::to_value(&report)?;
    assert_eq!(json["forwarder"]["kept_stopped"], true);
    assert_eq!(json["loopback_ports"], serde_json::json!([18080, 18081]));
    Ok(())
}

fn services(healthy: bool) -> crate::domain::service_stack::ServiceStackStatus {
    crate::domain::service_stack::ServiceStackStatus {
        installed: true,
        stack_version: None,
        runtime: None,
        target_id: None,
        route: String::new(),
        internal_smtp: String::new(),
        host_smtp_port: None,
        components: vec![crate::domain::service_stack::ServiceComponentStatus {
            kind: crate::domain::service_stack::ServiceComponentKind::Mailpit,
            version: "1".to_owned(),
            running: healthy,
            healthy,
            image: None,
        }],
        diagnostics: Vec::new(),
    }
}

fn tls(trusted: bool) -> crate::domain::ca::LocalCaStatus {
    crate::domain::ca::LocalCaStatus {
        installed: true,
        fingerprint: None,
        generation: 1,
        issued_hosts: std::collections::BTreeSet::new(),
        key_permissions_valid: true,
        trust_disabled: trusted,
        rotation_pending: false,
        leaves: Vec::new(),
        trust: None,
    }
}

#[test]
fn status_separates_stopped_forwarding_from_unhealthy_services() -> Result<(), serde_json::Error> {
    let ready = status_report(
        forwarder(true, true, false),
        Ok(services(true)),
        Ok(tls(true)),
        None,
    );
    assert!(ready.ready);
    assert_eq!(ready.state, PublicIngressState::Running);

    let stopped = status_report(
        forwarder(true, false, false),
        Ok(services(true)),
        Ok(tls(true)),
        None,
    );
    assert!(!stopped.ready && stopped.services.healthy && stopped.tls.ready);
    assert_eq!(stopped.state, PublicIngressState::Stopped);

    let unhealthy = status_report(
        forwarder(true, true, false),
        Ok(services(false)),
        Ok(tls(true)),
        None,
    );
    assert!(!unhealthy.ready && unhealthy.forwarder.running);
    let json = serde_json::to_value(&unhealthy)?;
    assert_eq!(json["state"], "running");
    assert_eq!(json["services"]["unhealthy_components"][0], "mailpit");

    let unreachable = status_report(
        forwarder(false, true, false),
        Err(MmError::Target {
            message: "no local Compose runtime is available".to_owned(),
        }),
        Ok(tls(false)),
        None,
    );
    assert!(!unreachable.forwarder.running && !unreachable.tls.ready);
    assert!(unreachable.services.errors[0].contains("no local Compose runtime"));
    Ok(())
}
