use super::*;
use crate::domain::ca::{LocalCaStatus, TrustScope};
use crate::domain::ingress::IngressCertificateReference;

/// Local HTTPS works end to end: CA present, key private, trusted, and nothing pending.
pub(super) fn tls_ready(status: &LocalCaStatus) -> bool {
    status.installed
        && status.key_permissions_valid
        && !status.rotation_pending
        && status.trust.as_ref().is_some_and(|trust| trust.installed)
}

pub(super) fn render_tls(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    if model.remote().is_some() {
        render_remote_https(frame, area, model);
        return;
    }
    let theme = theme();
    let Some(status) = &model.tls else {
        empty_state(
            frame,
            area,
            card("Local HTTPS", true),
            Tone::Bad,
            "TLS status is unavailable",
            "",
        );
        return;
    };
    let check = |passed: bool, label: String, fix: &str| {
        let mut lines = vec![Line::from(badge(
            if passed { Tone::Ok } else { Tone::Bad },
            label,
        ))];
        if !passed && !fix.is_empty() {
            lines.push(Line::styled(format!("  → {fix}"), theme.soft()));
        }
        lines
    };
    let mut lines = check(
        status.installed,
        if status.installed {
            format!("Local CA installed · generation {}", status.generation)
        } else {
            "Local CA missing".to_owned()
        },
        "run mm dev setup",
    );
    if status.installed {
        lines.extend(check(
            status.key_permissions_valid,
            if status.key_permissions_valid {
                "Private key is private".to_owned()
            } else {
                "Private key permissions are unsafe".to_owned()
            },
            "run mm dev setup to repair key permissions",
        ));
        let (trusted, trust_label) = match &status.trust {
            None => (false, "Platform trust unknown".to_owned()),
            Some(trust) if trust.installed => (
                true,
                match trust.scope {
                    TrustScope::CurrentUser => "Trusted by your user certificate store",
                    TrustScope::System => "Trusted by the system certificate store",
                }
                .to_owned(),
            ),
            Some(trust) if trust.mismatch => {
                (false, "Trusted CA fingerprint does not match".to_owned())
            }
            Some(_) => (false, "CA is not trusted by this machine".to_owned()),
        };
        lines.extend(check(trusted, trust_label, "run mm dev setup"));
        lines.extend(check(
            !status.rotation_pending,
            if status.rotation_pending {
                "CA rotation pending".to_owned()
            } else {
                "No rotation pending".to_owned()
            },
            "run mm dev setup to finish rotation",
        ));
        if !status.issued_hosts.is_empty() {
            lines.push(Line::raw(""));
            lines.push(section("Issued hosts"));
            for host in &status.issued_hosts {
                lines.push(Line::from(vec![
                    Span::styled("  ↗ ", theme.accent()),
                    Span::styled(terminal_safe(host), theme.text()),
                ]));
            }
        }
    }
    let meta = if tls_ready(status) {
        badge(Tone::Ok, "ready")
    } else {
        badge(Tone::Warn, "needs repair")
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(with_meta(card("Local HTTPS", true), meta))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_remote_https(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let routes = model
        .snapshot
        .ingress
        .iter()
        .filter(|status| status.tls == Some(true))
        .collect::<Vec<_>>();
    let block = with_meta(
        card("HTTPS", true),
        vec![Span::styled(
            format!(
                "{} of {} routes",
                routes.len(),
                model.snapshot.ingress.len()
            ),
            theme.muted(),
        )],
    );
    if routes.is_empty() {
        empty_state(
            frame,
            area,
            block,
            Tone::Idle,
            "No HTTPS routes",
            "Routes with tls in the ingress section of mm.yaml appear here once deployed.",
        );
        return;
    }
    let mut lines = Vec::new();
    for status in routes {
        let (tone, certificate) = match &status.certificate_reference {
            Some(IngressCertificateReference::LocalCa { .. }) => {
                (Tone::Warn, "mm local CA".to_owned())
            }
            Some(IngressCertificateReference::Acme { .. } | IngressCertificateReference::None)
            | None => (Tone::Ok, "public certificate".to_owned()),
        };
        let (tone, certificate) = match status.warning.as_deref().or(status.reason.as_deref()) {
            Some(problem) if status.url.is_none() => (Tone::Bad, problem.to_owned()),
            Some(problem) => (Tone::Warn, format!("{certificate} · {problem}")),
            None => (tone, certificate),
        };
        let host = status.url.as_deref().unwrap_or(status.service.as_str());
        let mut spans = badge(tone, terminal_safe(host));
        spans.push(Span::styled(
            format!("  {}", terminal_safe(&certificate)),
            theme.soft(),
        ));
        lines.push(Line::from(spans));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}
