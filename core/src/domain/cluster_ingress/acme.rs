//! The proxy's ACME state (`acme.json`) and the certificates an environment
//! provides. Traefik keeps every certificate of its ACME file in the one
//! store it picks a certificate from, beside the provided ones; for a host
//! both cover it serves one or the other, again after each change of its
//! routes. So a host whose environment provides its certificate must have
//! none in the ACME file, and since the proxy reads that file only when it
//! starts, taking one out means a restart.

use std::collections::BTreeSet;

use super::IngressTable;

/// The HTTPS hosts of the tables that provide their certificate.
#[must_use]
pub fn provided_hosts(tables: &[(String, IngressTable)]) -> BTreeSet<String> {
    tables
        .iter()
        .filter(|(_, table)| table.certificate.is_some())
        .flat_map(|(_, table)| table.routes.iter())
        .filter(|route| route.tls)
        .map(|route| route.host.to_ascii_lowercase())
        .collect()
}

/// What [`forget_hosts`] took out of an ACME file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgottenCertificates {
    /// The file without them.
    pub file: Vec<u8>,
    /// The main domain of each certificate taken out, in the file's order.
    pub domains: Vec<String>,
}

/// The ACME file `stored` without the certificates that name any of
/// `hosts`, or `None` when it holds none. Everything else in the file (the
/// account, other certificates, fields mm does not know) stays as it is.
pub fn forget_hosts(
    stored: &[u8],
    hosts: &BTreeSet<String>,
) -> Result<Option<ForgottenCertificates>, String> {
    if hosts.is_empty() || stored.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let mut file: serde_json::Value = serde_json::from_slice(stored)
        .map_err(|error| format!("the proxy's ACME file is not JSON: {error}"))?;
    let Some(resolvers) = file.as_object_mut() else {
        return Err("the proxy's ACME file is not a JSON object".to_owned());
    };
    let mut domains = Vec::new();
    for resolver in resolvers.values_mut() {
        let Some(certificates) = resolver
            .get_mut("Certificates")
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        certificates.retain(|certificate| {
            let named = names(certificate);
            if named.iter().any(|name| hosts.contains(name)) {
                domains.push(named.first().cloned().unwrap_or_default());
                false
            } else {
                true
            }
        });
    }
    if domains.is_empty() {
        return Ok(None);
    }
    let file = serde_json::to_vec_pretty(&file)
        .map_err(|error| format!("cannot write the proxy's ACME file: {error}"))?;
    Ok(Some(ForgottenCertificates { file, domains }))
}

/// The main domain and the other names of one certificate of the file.
fn names(certificate: &serde_json::Value) -> Vec<String> {
    let domain = certificate.get("domain");
    let main = domain
        .and_then(|domain| domain.get("main"))
        .and_then(serde_json::Value::as_str);
    let sans = domain
        .and_then(|domain| domain.get("sans"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str);
    main.into_iter()
        .chain(sans)
        .map(str::to_ascii_lowercase)
        .collect()
}
