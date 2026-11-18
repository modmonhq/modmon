//! Sealing and opening secret bundles with age. The sender seals a bundle to
//! the host store's X25519 recipient; only that store's identity opens it.

use zeroize::Zeroizing;

use crate::domain::error::MmError;
use crate::domain::secret::{MAX_SECRET_BUNDLE_BYTES, SecretBundle};

/// Upper bound on a sealed bundle: the plaintext limit plus age's header,
/// per-chunk tags, and ASCII armor (about 4/3 of the binary size).
pub const MAX_SEALED_SECRET_BUNDLE_BYTES: usize =
    MAX_SECRET_BUNDLE_BYTES + MAX_SECRET_BUNDLE_BYTES / 2;

/// Seals `bundle` to an age X25519 `recipient` (`age1…`, as
/// `mmd secrets recipient` prints it). The result is ASCII-armored text, safe
/// to pass as a string on stdin.
pub fn seal_secret_bundle(recipient: &str, bundle: &SecretBundle) -> Result<Vec<u8>, MmError> {
    let recipient = recipient
        .trim()
        .parse::<age::x25519::Recipient>()
        .map_err(|message| MmError::Secret {
            message: format!("invalid age recipient: {message}"),
        })?;
    let plaintext = bundle
        .encode()
        .map_err(|message| MmError::Secret { message })?;
    age::encrypt_and_armor(&recipient, &plaintext)
        .map(String::into_bytes)
        .map_err(|error| MmError::Secret {
            message: format!("could not seal the secret bundle: {error}"),
        })
}

/// Opens a bundle sealed with [`seal_secret_bundle`], armored or binary, and
/// validates its contents. It does not check the project or environment;
/// see [`SecretBundle::ensure_scope`].
pub fn open_secret_bundle(
    identity: &age::x25519::Identity,
    sealed: &[u8],
) -> Result<SecretBundle, MmError> {
    if sealed.len() > MAX_SEALED_SECRET_BUNDLE_BYTES {
        return Err(MmError::Secret {
            message: format!(
                "sealed secret bundle exceeds the {MAX_SEALED_SECRET_BUNDLE_BYTES}-byte limit"
            ),
        });
    }
    let sealed = trim_armor(sealed);
    let plaintext =
        Zeroizing::new(
            age::decrypt(identity, sealed).map_err(|error| MmError::Secret {
                message: format!("could not open the secret bundle: {error}"),
            })?,
        );
    SecretBundle::decode(&plaintext).map_err(|message| MmError::Secret { message })
}

/// Text transports may add surrounding whitespace to armored input; binary
/// input is left untouched.
fn trim_armor(sealed: &[u8]) -> &[u8] {
    let trimmed = sealed.trim_ascii();
    if trimmed.starts_with(b"-----BEGIN AGE ENCRYPTED FILE-----") {
        trimmed
    } else {
        sealed
    }
}

#[cfg(test)]
#[path = "bundle/tests.rs"]
mod tests;

/// Seals a stage's values to one node's store `recipient`, as ASCII armor.
pub fn seal_staged_values(
    recipient: &str,
    payload: &crate::domain::staged::StagedPayload,
) -> Result<String, MmError> {
    let recipient = recipient
        .trim()
        .parse::<age::x25519::Recipient>()
        .map_err(|message| MmError::Secret {
            message: format!("the node's store recipient is not an age recipient: {message}"),
        })?;
    let plaintext = payload
        .encode()
        .map_err(|message| MmError::Secret { message })?;
    age::encrypt_and_armor(&recipient, &plaintext).map_err(|error| MmError::Secret {
        message: format!("the values could not be sealed: {error}"),
    })
}

/// Opens values sealed with [`seal_staged_values`].
pub fn open_staged_values(
    identity: &age::x25519::Identity,
    sealed: &[u8],
) -> Result<crate::domain::staged::StagedPayload, MmError> {
    if sealed.len() > crate::domain::staged::MAX_SEALED_STAGE_BYTES {
        return Err(MmError::Secret {
            message: "the sealed values exceed their size limit".to_owned(),
        });
    }
    let plaintext = Zeroizing::new(age::decrypt(identity, trim_armor(sealed)).map_err(|_| {
        MmError::Secret {
            message: "the sealed values do not open with this node's store; they were sealed to another recipient".to_owned(),
        }
    })?);
    crate::domain::staged::StagedPayload::decode(&plaintext)
        .map_err(|message| MmError::Secret { message })
}

/// Seals `plaintext` to an age X25519 `recipient`, as ASCII armor.
pub fn seal_to_recipient(recipient: &str, plaintext: &[u8]) -> Result<String, MmError> {
    let recipient = recipient
        .trim()
        .parse::<age::x25519::Recipient>()
        .map_err(|message| MmError::Secret {
            message: format!("not an age recipient: {message}"),
        })?;
    age::encrypt_and_armor(&recipient, plaintext).map_err(|error| MmError::Secret {
        message: format!("could not seal to the recipient: {error}"),
    })
}

/// Opens what [`seal_to_recipient`] sealed to `identity`, up to `limit` bytes.
pub fn open_sealed(
    identity: &age::x25519::Identity,
    sealed: &[u8],
    limit: usize,
) -> Result<Zeroizing<Vec<u8>>, MmError> {
    if sealed.len() > limit {
        return Err(MmError::Secret {
            message: format!("the sealed document exceeds its {limit}-byte limit"),
        });
    }
    age::decrypt(identity, trim_armor(sealed))
        .map(Zeroizing::new)
        .map_err(|_| MmError::Secret {
            message: "the sealed document does not open with this store; it was sealed to another recipient".to_owned(),
        })
}
