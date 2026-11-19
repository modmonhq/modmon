//! `generate:` of a secret, whose keys depend on its `preset:`.

use serde::de::Error;
use serde::{Deserialize, Deserializer};

use super::{
    SecretEncoding, SecretGeneratorConfig, default_byte_length, default_password_length,
    default_token_length,
};

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Preset {
    Password,
    Token,
    Bytes,
    LaravelAppKey,
    Uuid,
}

#[derive(Deserialize)]
struct GeneratorFields {
    preset: Preset,
    #[serde(default)]
    length: Option<usize>,
    #[serde(default)]
    encoding: Option<SecretEncoding>,
}

impl<'de> Deserialize<'de> for SecretGeneratorConfig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let GeneratorFields {
            preset,
            length,
            encoding,
        } = GeneratorFields::deserialize(deserializer)?;
        if encoding.is_some() && !matches!(preset, Preset::Bytes) {
            return Err(D::Error::custom("`encoding:` applies to `preset: bytes`"));
        }
        match preset {
            Preset::Password => Ok(Self::Password {
                length: length.unwrap_or_else(default_password_length),
            }),
            Preset::Token => Ok(Self::Token {
                length: length.unwrap_or_else(default_token_length),
            }),
            Preset::Bytes => Ok(Self::Bytes {
                length: length.unwrap_or_else(default_byte_length),
                encoding: encoding.unwrap_or_default(),
            }),
            Preset::LaravelAppKey | Preset::Uuid if length.is_some() => Err(D::Error::custom(
                "`length:` applies to `preset: password`, `token` and `bytes`",
            )),
            Preset::LaravelAppKey => Ok(Self::LaravelAppKey),
            Preset::Uuid => Ok(Self::Uuid),
        }
    }
}
