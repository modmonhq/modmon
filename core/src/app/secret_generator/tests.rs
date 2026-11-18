use super::*;

#[test]
fn generators_produce_expected_safe_shapes() -> Result<(), MmError> {
    assert_shape(&SecretGeneratorConfig::Password { length: 64 }, |value| {
        value.len() == 64 && value.bytes().all(|byte| PASSWORD_ALPHABET.contains(&byte))
    })?;
    assert_shape(&SecretGeneratorConfig::Token { length: 65 }, |value| {
        value.len() == 65 && value.bytes().all(|byte| TOKEN_ALPHABET.contains(&byte))
    })?;
    assert_shape(
        &SecretGeneratorConfig::Bytes {
            length: 32,
            encoding: SecretEncoding::Base64,
        },
        |value| value.len() == 44,
    )?;
    assert_shape(
        &SecretGeneratorConfig::Bytes {
            length: 32,
            encoding: SecretEncoding::Base64Url,
        },
        |value| value.len() == 43 && !value.contains('='),
    )?;
    assert_shape(
        &SecretGeneratorConfig::Bytes {
            length: 32,
            encoding: SecretEncoding::Hex,
        },
        |value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
    )?;
    assert_shape(&SecretGeneratorConfig::LaravelAppKey, |value| {
        value.starts_with("base64:") && value.len() == 51
    })?;
    assert_shape(&SecretGeneratorConfig::Uuid, |value| {
        uuid::Uuid::parse_str(value).is_ok()
    })?;
    Ok(())
}

#[test]
fn generator_lengths_are_bounded_and_hex_encoding_is_canonical() {
    for generator in [
        SecretGeneratorConfig::Password { length: 15 },
        SecretGeneratorConfig::Token { length: 4097 },
        SecretGeneratorConfig::Bytes {
            length: 0,
            encoding: SecretEncoding::Hex,
        },
    ] {
        assert!(matches!(
            generate_secret(&generator),
            Err(MmError::Secret { .. })
        ));
    }
    assert_eq!(encode_hex(&[0x00, 0x0f, 0x10, 0xab, 0xff]), "000f10abff");
}

fn assert_shape(
    generator: &SecretGeneratorConfig,
    predicate: impl FnOnce(&str) -> bool,
) -> Result<(), MmError> {
    let material = generate_secret(generator)?;
    assert!(material.expose(predicate));
    Ok(())
}
