use super::*;

pub(super) fn validate_environment_ingress(
    document: &ConfigDocument,
    report: &mut ValidationReport,
) {
    let source = document.source("environments");
    for (name, environment) in &document.config.environments {
        let Some(ingress) = &environment.ingress else {
            continue;
        };
        for problem in crate::domain::cluster_ingress::header_problems(&ingress.headers) {
            report.push(
                Diagnostic::error(
                    "MM_INGRESS_RULE_INVALID",
                    format!("environments.{name}.ingress: {problem}"),
                )
                .at_path(source),
            );
        }
        let Some((token, image)) = ingress.tunnel() else {
            continue;
        };
        if !document.config.secrets.contains_key(&token) {
            report.push(
                Diagnostic::error(
                    "MM_TUNNEL_TOKEN_MISSING",
                    format!(
                        "environments.{name}.ingress.cloudflared.token names secret '{token}', which secrets: does not declare; declare it, then store the tunnel's token with `mm -e {name} secret set {token} --stdin`"
                    ),
                )
                .at_path(source),
            );
        }
        if image.is_empty() || image.chars().any(|character| character.is_whitespace()) {
            report.push(
                Diagnostic::error(
                    "MM_TUNNEL_IMAGE_INVALID",
                    format!("environments.{name}.ingress.cloudflared.image '{image}' is not an image reference"),
                )
                .at_path(source),
            );
        }
        if environment.tls.is_some() {
            report.push(
                Diagnostic::error(
                    "MM_ENVIRONMENT_TLS_INVALID",
                    format!(
                        "environments.{name}.tls does not apply with ingress.type cloudflared: Cloudflare ends TLS and the connector reaches the proxy on the node itself; remove tls"
                    ),
                )
                .at_path(source),
            );
        }
    }
}
