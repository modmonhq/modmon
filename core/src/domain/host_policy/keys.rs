//! The OpenSSH public keys and user names a host policy and a users file
//! name.

/// An OpenSSH public key line, checked and fingerprinted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
    pub algorithm: String,
    pub blob: String,
    pub comment: String,
    /// `SHA256:<unpadded base64>`, as `ssh-keygen -l` prints it.
    pub fingerprint: String,
}

impl PublicKey {
    /// `<algorithm> <base64>`, without the comment.
    #[must_use]
    pub fn openssh(&self) -> String {
        format!("{} {}", self.algorithm, self.blob)
    }
}

/// Key types a user may register. ECDSA and RSA are rejected in v1.
pub const ALLOWED_KEY_TYPES: [&str; 2] = ["ssh-ed25519", "sk-ssh-ed25519@openssh.com"];

pub fn parse_public_key(line: &str) -> Result<PublicKey, String> {
    use base64::Engine;
    use sha2::Digest;

    let mut parts = line.split_whitespace();
    let algorithm = parts.next().unwrap_or_default();
    let blob = parts.next().unwrap_or_default();
    let comment = parts.collect::<Vec<_>>().join(" ");
    if !ALLOWED_KEY_TYPES.contains(&algorithm) {
        return Err(format!(
            "key type '{algorithm}' is not allowed; use {}",
            ALLOWED_KEY_TYPES.join(" or ")
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(blob)
        .map_err(|_| "public key is not valid base64".to_owned())?;
    // The blob starts with its own length-prefixed algorithm name.
    let declared = bytes
        .get(..4)
        .map(|length| u32::from_be_bytes([length[0], length[1], length[2], length[3]]) as usize)
        .and_then(|length| bytes.get(4..4 + length))
        .ok_or_else(|| "public key blob is truncated".to_owned())?;
    if declared != algorithm.as_bytes() {
        return Err("public key blob does not match its key type".to_owned());
    }
    if comment.chars().any(char::is_control) {
        return Err("public key comment contains control characters".to_owned());
    }
    let digest = sha2::Sha256::digest(&bytes);
    Ok(PublicKey {
        algorithm: algorithm.to_owned(),
        blob: blob.to_owned(),
        comment,
        fingerprint: format!(
            "SHA256:{}",
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
        ),
    })
}

/// User names in the repo's `users:` key and break-glass names.
#[must_use]
pub fn valid_user_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && name.len() <= 63
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}
