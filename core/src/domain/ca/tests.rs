use super::{
    CertificateFingerprint, STANDARD, TrustScope, bundle_contains_fingerprint, certificate_der,
    certificate_fingerprint, is_local_ca_host, local_ca_compatibility_warning, pem_certificates,
    unique_dns_names, validate_hostname,
};
use base64::Engine;
use sha2::{Digest, Sha256};

const KNOWN_DER: &[u8] = &[0x30, 0x00];
const KNOWN_SHA256: &str =
    "sha256:e4f60d0aa6d7f3d3b6a6494b1c861b99f649c6f9ec51abaf201b20f297327c95";

#[test]
fn known_der_vector_matches_sha256() -> Result<(), String> {
    let fingerprint = certificate_fingerprint(KNOWN_DER)?;
    assert_eq!(fingerprint.to_string(), KNOWN_SHA256);
    assert_eq!(CertificateFingerprint::parse(KNOWN_SHA256)?, fingerprint);
    Ok(())
}

#[test]
fn fingerprint_hashes_der_not_pem() -> Result<(), String> {
    let der = KNOWN_DER;
    let pem = format!(
        "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
        STANDARD.encode(der)
    );
    let from_pem = certificate_fingerprint(pem.as_bytes())?;
    let from_der = certificate_fingerprint(der)?;
    assert_eq!(from_pem, from_der);
    assert_eq!(certificate_der(pem.as_bytes())?, der);
    assert_eq!(
        certificate_fingerprint(pem.as_bytes())?.to_string(),
        KNOWN_SHA256
    );
    let pem_hash = Sha256::digest(pem.as_bytes());
    assert_ne!(
        format!(
            "sha256:{}",
            pem_hash
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ),
        KNOWN_SHA256
    );
    Ok(())
}

#[test]
fn bundle_matching_uses_each_canonical_der_certificate() -> Result<(), String> {
    let pem = format!(
        "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
        STANDARD.encode(KNOWN_DER)
    );
    assert!(bundle_contains_fingerprint(pem.as_bytes(), KNOWN_SHA256)?);
    assert_eq!(
        bundle_contains_fingerprint(
            pem.as_bytes(),
            "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        ),
        Ok(false)
    );
    Ok(())
}

#[test]
fn hostname_validation_rejects_unsafe_labels() -> Result<(), String> {
    assert!(validate_hostname("App.Test").is_ok_and(|host| host == "app.test"));
    assert!(validate_hostname("-app.test").is_err());
    assert!(validate_hostname("app-.test").is_err());
    assert!(validate_hostname(".app.test").is_err());
    assert!(validate_hostname("app..test").is_err());
    assert!(validate_hostname(&format!("{}.test", "a".repeat(64))).is_err());
    assert!(is_local_ca_host("App.Test"));
    assert!(!is_local_ca_host("www.example.com"));
    assert!(local_ca_compatibility_warning("app.local").is_some());
    assert!(local_ca_compatibility_warning("app.test").is_none());
    assert_eq!(
        unique_dns_names(["App.Test", "app.test", "api.test"])?,
        vec!["app.test".to_owned(), "api.test".to_owned()]
    );
    Ok(())
}

#[test]
fn fingerprints_and_pem_boundaries_reject_malformed_inputs() -> Result<(), String> {
    assert!(CertificateFingerprint::parse("md5:00").is_err());
    assert!(CertificateFingerprint::parse("sha256:00").is_err());
    assert!(CertificateFingerprint::parse(&format!("sha256:{}g", "0".repeat(63))).is_err());

    assert_eq!(certificate_der(&[0xff, 0x00])?, vec![0xff, 0x00]);
    assert_eq!(certificate_der(b"not a certificate")?, b"not a certificate");
    assert!(certificate_der(b"-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----").is_err());
    assert!(
        certificate_fingerprint(b"-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----").is_err()
    );
    assert!(certificate_der(b"-----BEGIN CERTIFICATE-----\n!\n-----END CERTIFICATE-----").is_err());

    let valid = format!(
        "ignored\n-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
        STANDARD.encode(KNOWN_DER)
    );
    assert_eq!(
        pem_certificates(valid.as_bytes())?,
        vec![KNOWN_DER.to_vec()]
    );
    assert!(pem_certificates(&[0xff]).is_err());
    assert!(pem_certificates(b"-----BEGIN CERTIFICATE-----\n-----BEGIN CERTIFICATE-----").is_err());
    assert!(pem_certificates(b"-----BEGIN CERTIFICATE-----\n!").is_err());
    assert!(
        pem_certificates(b"-----BEGIN CERTIFICATE-----\n!\n-----END CERTIFICATE-----").is_err()
    );
    Ok(())
}

#[test]
fn hostname_classification_and_trust_scope_labels_are_exact() {
    for host in [
        "localhost",
        "app.localhost",
        "app.test",
        "app.invalid",
        "app.local",
    ] {
        assert!(is_local_ca_host(host), "{host}");
    }
    for host in ["", "-bad.test", "example.com"] {
        assert!(!is_local_ca_host(host), "{host}");
    }
    assert!(local_ca_compatibility_warning("localhost").is_some());
    assert!(local_ca_compatibility_warning("bad..local").is_none());
    assert_eq!(TrustScope::CurrentUser.to_string(), "current_user");
    assert_eq!(TrustScope::System.to_string(), "system");
}
