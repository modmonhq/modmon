//! SSH host, user, key, image, and email validation for target definitions.

use std::net::IpAddr;

pub(super) fn validate_ssh_host_key_fingerprint(value: &str) -> Result<(), String> {
    let digest = value
        .strip_prefix("SHA256:")
        .ok_or_else(|| "SSH host key fingerprint must start with 'SHA256:'".to_owned())?;
    if digest.len() != 43
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
    {
        return Err("SSH host key fingerprint must be an OpenSSH SHA256 fingerprint".to_owned());
    }
    Ok(())
}

pub(super) fn validate_ssh_host_key(value: &str, expected_fingerprint: &str) -> Result<(), String> {
    use base64::Engine as _;
    use sha2::Digest as _;

    if value.len() > 16 * 1024 || value.contains(['\r', '\n', '\0']) {
        return Err("SSH host public key is invalid".to_owned());
    }
    let mut fields = value.split_ascii_whitespace();
    let key_type = fields.next().unwrap_or_default();
    let encoded = fields.next().unwrap_or_default();
    if fields.next().is_some()
        || !matches!(key_type, "ssh-ed25519" | "ecdsa-sha2-nistp256" | "ssh-rsa")
    {
        return Err("SSH host public key must contain one supported key type and key".to_owned());
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "SSH host public key is not valid base64".to_owned())?;
    if decoded.is_empty() || decoded.len() > 16 * 1024 {
        return Err("SSH host public key has an invalid size".to_owned());
    }
    validate_ssh_key_blob(key_type, &decoded)?;
    let digest = sha2::Sha256::digest(decoded);
    let actual = format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
    );
    if actual != expected_fingerprint {
        return Err("SSH host public key does not match its reviewed fingerprint".to_owned());
    }
    Ok(())
}

pub(super) fn validate_ssh_key_blob(key_type: &str, decoded: &[u8]) -> Result<(), String> {
    let mut remaining = decoded;
    let algorithm = take_ssh_wire_string(&mut remaining)?;
    if algorithm != key_type.as_bytes() {
        return Err("SSH host public key type does not match its key blob".to_owned());
    }
    let valid = match key_type {
        "ssh-ed25519" => {
            let key = take_ssh_wire_string(&mut remaining)?;
            key.len() == 32 && remaining.is_empty()
        }
        "ecdsa-sha2-nistp256" => {
            let curve = take_ssh_wire_string(&mut remaining)?;
            let point = take_ssh_wire_string(&mut remaining)?;
            curve == b"nistp256"
                && point.len() == 65
                && point.first() == Some(&4)
                && remaining.is_empty()
        }
        "ssh-rsa" => {
            let exponent = take_ssh_wire_string(&mut remaining)?;
            let modulus = take_ssh_wire_string(&mut remaining)?;
            !exponent.is_empty()
                && exponent.len() <= 8
                && modulus.len() >= 128
                && remaining.is_empty()
        }
        _ => false,
    };
    if !valid {
        return Err("SSH host public key blob is malformed or unsupported".to_owned());
    }
    Ok(())
}

pub(super) fn take_ssh_wire_string<'a>(remaining: &mut &'a [u8]) -> Result<&'a [u8], String> {
    let length_bytes: [u8; 4] = remaining
        .get(..4)
        .ok_or_else(|| "SSH host public key blob is truncated".to_owned())?
        .try_into()
        .map_err(|_| "SSH host public key blob is truncated".to_owned())?;
    let length = usize::try_from(u32::from_be_bytes(length_bytes))
        .map_err(|_| "SSH host public key blob has an unsupported length".to_owned())?;
    let end = 4_usize
        .checked_add(length)
        .ok_or_else(|| "SSH host public key blob has an unsupported length".to_owned())?;
    let value = remaining
        .get(4..end)
        .ok_or_else(|| "SSH host public key blob is truncated".to_owned())?;
    *remaining = &remaining[end..];
    Ok(value)
}

pub(super) fn validate_digest_pinned_image(image: &str) -> Result<(), String> {
    let Some((repository, digest)) = image.rsplit_once("@sha256:") else {
        return Err("Traefik image must be pinned by a sha256 digest".to_owned());
    };
    if repository.is_empty()
        || image.len() > 512
        || !image.is_ascii()
        || repository.starts_with('-')
        || repository.bytes().any(|value| {
            value.is_ascii_control()
                || value.is_ascii_whitespace()
                || !matches!(
                    value,
                    b'a'..=b'z'
                        | b'A'..=b'Z'
                        | b'0'..=b'9'
                        | b'.'
                        | b'/'
                        | b':'
                        | b'_'
                        | b'-'
                )
        })
        || digest.len() != 64
        || !digest
            .bytes()
            .all(|value| value.is_ascii_digit() || matches!(value, b'a'..=b'f'))
    {
        return Err("Traefik image must be a safe repository reference pinned by a lowercase 64-character sha256 digest".to_owned());
    }
    Ok(())
}

pub(super) fn validate_acme_email(email: &str) -> Result<(), String> {
    let Some((local, domain)) = email.split_once('@') else {
        return Err("ACME email must contain one '@' separator".to_owned());
    };
    if email.len() > 254
        || !email.is_ascii()
        || local.is_empty()
        || local.len() > 64
        || local.bytes().any(|value| {
            !(value.is_ascii_alphanumeric() || matches!(value, b'.' | b'_' | b'+' | b'-'))
        })
        || domain.contains('@')
    {
        return Err("ACME email must be a safe ASCII address".to_owned());
    }
    validate_ssh_host(domain).map_err(|_| "ACME email has an invalid domain".to_owned())
}

pub(super) fn validate_ssh_host(host: &str) -> Result<(), String> {
    if host.is_empty()
        || host.len() > 253
        || host.trim() != host
        || !host.is_ascii()
        || host.starts_with('-')
        || host.bytes().any(|value| value.is_ascii_control())
    {
        return Err("SSH host must be a safe ASCII hostname or IP address".to_owned());
    }
    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    if host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || value == b'-')
    }) {
        Ok(())
    } else {
        Err("SSH host must be a safe ASCII hostname or IP address".to_owned())
    }
}

pub(super) fn validate_ssh_user(user: &str) -> Result<(), String> {
    let valid = !user.is_empty()
        && user.len() <= 64
        && user
            .bytes()
            .next()
            .is_some_and(|value| value.is_ascii_alphanumeric() || value == b'_')
        && user
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'_' | b'-' | b'.'));
    if valid {
        Ok(())
    } else {
        Err("SSH user must be a safe ASCII account name of at most 64 characters".to_owned())
    }
}

pub(super) fn validate_acme_ca_server(server: &str) -> Result<(), String> {
    let valid = server.len() <= 2048
        && server
            .strip_prefix("https://")
            .is_some_and(|rest| !rest.is_empty() && !rest.starts_with('/'))
        && !server
            .chars()
            .any(|character| character.is_whitespace() || character.is_control());
    if valid {
        Ok(())
    } else {
        Err("ACME CA server must be an https:// directory URL".to_owned())
    }
}

pub(super) fn validate_acme_ca_certificate(certificate: &str) -> Result<(), String> {
    let trimmed = certificate.trim();
    if trimmed.len() <= 64 * 1024
        && trimmed.starts_with("-----BEGIN CERTIFICATE-----")
        && trimmed.ends_with("-----END CERTIFICATE-----")
        && !trimmed.contains("PRIVATE KEY")
    {
        Ok(())
    } else {
        Err(
            "ACME CA certificate must be PEM certificates (no private keys), at most 64 KiB"
                .to_owned(),
        )
    }
}
