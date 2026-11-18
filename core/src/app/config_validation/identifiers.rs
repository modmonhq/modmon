pub(super) fn is_middleware_resource_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && name
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(crate) fn is_global_key(key: &str) -> bool {
    crate::domain::release_facts::is_image_key(key)
        || (!key.is_empty()
            && key.split('.').all(|segment| {
                !segment.is_empty()
                    && segment.chars().all(|character| {
                        character.is_ascii_uppercase()
                            || character.is_ascii_digit()
                            || character == '_'
                    })
            }))
}

pub(crate) fn is_variable_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

pub(crate) fn is_secret_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric())
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
}

pub(super) fn is_oci_platform(value: &str) -> bool {
    let mut parts = value.split('/');
    let valid_part = |part: &str| {
        !part.is_empty()
            && part.len() <= 32
            && part.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'_' | b'-' | b'.')
            })
    };
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(os), Some(architecture), None, None)
            if valid_part(os) && valid_part(architecture)
    ) || matches!(
        (value.split('/').next(), value.split('/').nth(1), value.split('/').nth(2), value.split('/').nth(3)),
        (Some(os), Some(architecture), Some(variant), None)
            if valid_part(os) && valid_part(architecture) && valid_part(variant)
    )
}

pub(super) fn is_build_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub(super) fn looks_like_secret_name(value: &str) -> bool {
    let value = value.to_ascii_uppercase();
    value
        .split('_')
        .any(|part| matches!(part, "PASSWORD" | "PASSPHRASE" | "SECRET" | "TOKEN"))
        || value.ends_with("API_KEY")
        || value.ends_with("PRIVATE_KEY")
}

pub(super) fn looks_sensitive_filename(name: &str) -> bool {
    name == ".env"
        || name.starts_with(".env.")
        || matches!(
            name,
            "id_rsa" | "id_ed25519" | "id_ecdsa" | "id_dsa" | "age-identity.txt"
        )
}
