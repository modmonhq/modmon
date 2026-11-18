use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use zeroize::Zeroizing;

use crate::domain::config::{SecretEncoding, SecretGeneratorConfig};
use crate::domain::error::MmError;
use crate::domain::secret::SecretMaterial;

const PASSWORD_ALPHABET: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#%+=_-";
const TOKEN_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";

pub(crate) fn generate_secret(
    generator: &SecretGeneratorConfig,
) -> Result<SecretMaterial, MmError> {
    if let Some(length) = generator.length()
        && !(16..=4096).contains(&length)
    {
        return Err(MmError::Secret {
            message: "generated secret lengths must be between 16 and 4096".to_owned(),
        });
    }
    let value = match generator {
        SecretGeneratorConfig::Password { length } => {
            random_characters(*length, PASSWORD_ALPHABET)?
        }
        SecretGeneratorConfig::Token { length } => random_characters(*length, TOKEN_ALPHABET)?,
        SecretGeneratorConfig::Bytes { length, encoding } => {
            let mut bytes = Zeroizing::new(vec![0_u8; *length]);
            fill_random(&mut bytes)?;
            Zeroizing::new(match encoding {
                SecretEncoding::Base64 => STANDARD.encode(bytes.as_slice()),
                SecretEncoding::Base64Url => URL_SAFE_NO_PAD.encode(bytes.as_slice()),
                SecretEncoding::Hex => encode_hex(&bytes),
            })
        }
        SecretGeneratorConfig::LaravelAppKey => {
            let mut bytes = Zeroizing::new(vec![0_u8; 32]);
            fill_random(&mut bytes)?;
            Zeroizing::new(format!("base64:{}", STANDARD.encode(bytes.as_slice())))
        }
        SecretGeneratorConfig::Uuid => Zeroizing::new(uuid::Uuid::new_v4().to_string()),
    };
    SecretMaterial::from_zeroizing(value).map_err(|message| MmError::Secret { message })
}

fn random_characters(length: usize, alphabet: &[u8]) -> Result<Zeroizing<String>, MmError> {
    let sample_limit = 256 - (256 % alphabet.len());
    let mut output = Zeroizing::new(String::with_capacity(length));
    let mut random = Zeroizing::new([0_u8; 128]);
    while output.len() < length {
        fill_random(random.as_mut_slice())?;
        for &byte in random.iter() {
            if usize::from(byte) < sample_limit {
                output.push(char::from(alphabet[usize::from(byte) % alphabet.len()]));
                if output.len() == length {
                    break;
                }
            }
        }
    }
    Ok(output)
}

fn fill_random(bytes: &mut [u8]) -> Result<(), MmError> {
    getrandom::fill(bytes).map_err(|error| MmError::Secret {
        message: format!("operating system random generation failed: {error}"),
    })
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests;
