//! The OpenSSH private key file format (`PROTOCOL.key`), as far as mm
//! reads and writes it: the clear header of any key, and a whole ed25519
//! key that is not encrypted.

use base64::Engine as _;
use zeroize::Zeroizing;

/// The armor around an OpenSSH private key, spelled in two parts so this
/// file never looks like one.
const ARMOR_BEGIN: &str = concat!("-----BEGIN OPENSSH ", "PRIVATE KEY-----");
const ARMOR_END: &str = concat!("-----END OPENSSH ", "PRIVATE KEY-----");
const MAGIC: &[u8] = b"openssh-key-v1\0";
const ED25519: &str = "ssh-ed25519";

/// What a private key file says in the clear: its type, its public half,
/// and whether the private half is encrypted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateKeyHeader {
    /// `ssh-ed25519`, `sk-ssh-ed25519@openssh.com`, …
    pub key_type: String,
    /// The public key as it goes over the wire and into `authorized_keys`.
    pub public_blob: Vec<u8>,
    pub encrypted: bool,
    /// The key's comment, which only an ed25519 key that is not encrypted
    /// shows.
    pub comment: Option<String>,
}

impl PrivateKeyHeader {
    /// Reads the header of an OpenSSH private key file. `None` for any
    /// other format, a file with more than one key, or a damaged file.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let body = text.split_once(ARMOR_BEGIN)?.1.split_once(ARMOR_END)?.0;
        let encoded = Zeroizing::new(body.split_whitespace().collect::<String>());
        let bytes = Zeroizing::new(
            base64::engine::general_purpose::STANDARD
                .decode(encoded.as_bytes())
                .ok()?,
        );
        let mut rest = bytes.strip_prefix(MAGIC)?;
        let cipher = take_string(&mut rest)?;
        // The KDF and its options: only the cipher says whether the private
        // half is encrypted.
        take_string(&mut rest)?;
        take_string(&mut rest)?;
        let keys = u32::from_be_bytes(take(&mut rest, 4)?.try_into().ok()?);
        let public = take_string(&mut rest)?;
        let mut fields = public;
        let key_type = std::str::from_utf8(take_string(&mut fields)?).ok()?;
        if keys != 1 || key_type.is_empty() {
            return None;
        }
        let encrypted = cipher != b"none";
        let comment = (!encrypted && key_type == ED25519)
            .then(|| ed25519_comment(take_string(&mut rest)?))
            .flatten();
        Some(Self {
            key_type: key_type.to_owned(),
            public_blob: public.to_vec(),
            encrypted,
            comment,
        })
    }

    /// Whether the file is the handle of a FIDO2 security key.
    #[must_use]
    pub fn is_security_key(&self) -> bool {
        self.key_type.starts_with("sk-")
    }

    /// `<type> <base64>`, the line `authorized_keys` and the users file
    /// take, without a comment.
    #[must_use]
    pub fn public_line(&self) -> String {
        format!(
            "{} {}",
            self.key_type,
            base64::engine::general_purpose::STANDARD.encode(&self.public_blob)
        )
    }
}

/// An ed25519 key pair as an OpenSSH private key file that is not
/// encrypted, and its public line. `check` is the format's check number:
/// any value, random by convention.
#[must_use]
pub fn ed25519_private_key(
    seed: &[u8; 32],
    public: &[u8; 32],
    comment: &str,
    check: u32,
) -> (Zeroizing<String>, String) {
    let public_blob = [field(ED25519.as_bytes()), field(public)].concat();
    let mut private = Zeroizing::new(Vec::with_capacity(192 + comment.len()));
    private.extend_from_slice(&check.to_be_bytes());
    private.extend_from_slice(&check.to_be_bytes());
    private.extend_from_slice(&public_blob);
    // The private half is the seed followed by the public key.
    private.extend_from_slice(&64_u32.to_be_bytes());
    private.extend_from_slice(seed);
    private.extend_from_slice(public);
    private.extend_from_slice(&field(comment.as_bytes()));
    // Padded to the block size of the cipher `none` with 1, 2, 3, …
    for pad in 1_u8..=8 {
        if private.len().is_multiple_of(8) {
            break;
        }
        private.push(pad);
    }
    let mut file = Zeroizing::new(Vec::with_capacity(160 + private.len()));
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(&field(b"none"));
    file.extend_from_slice(&field(b"none"));
    file.extend_from_slice(&field(b""));
    file.extend_from_slice(&1_u32.to_be_bytes());
    file.extend_from_slice(&field(&public_blob));
    file.extend_from_slice(
        &u32::try_from(private.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    file.extend_from_slice(&private);
    let encoded = Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(file.as_slice()));
    let mut text = Zeroizing::new(String::with_capacity(encoded.len() + 128));
    text.push_str(ARMOR_BEGIN);
    text.push('\n');
    let mut line = 0;
    for character in encoded.chars() {
        if line == 70 {
            text.push('\n');
            line = 0;
        }
        text.push(character);
        line += 1;
    }
    text.push('\n');
    text.push_str(ARMOR_END);
    text.push('\n');
    let public_line = format!(
        "{ED25519} {} {comment}",
        base64::engine::general_purpose::STANDARD.encode(&public_blob)
    );
    (text, public_line.trim_end().to_owned())
}

/// The comment in the private section of an ed25519 key that is not
/// encrypted: two check numbers, the key type, the public key, the
/// private key, then the comment.
fn ed25519_comment(mut private: &[u8]) -> Option<String> {
    take(&mut private, 8)?;
    take_string(&mut private)?;
    take_string(&mut private)?;
    take_string(&mut private)?;
    let comment = std::str::from_utf8(take_string(&mut private)?).ok()?;
    (!comment.chars().any(char::is_control)).then(|| comment.to_owned())
}

/// `(major, minor)` from what `ssh -V` prints: `OpenSSH_9.2p1 Debian-2, …`.
#[must_use]
pub fn openssh_version(said: &str) -> Option<(u32, u32)> {
    let version = said.split_once("OpenSSH_")?.1;
    let (major, rest) = version.split_once('.')?;
    let minor = rest
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    Some((major.parse().ok()?, minor.parse().ok()?))
}

fn field(bytes: &[u8]) -> Vec<u8> {
    let mut field = u32::try_from(bytes.len())
        .unwrap_or(u32::MAX)
        .to_be_bytes()
        .to_vec();
    field.extend_from_slice(bytes);
    field
}

fn take<'a>(rest: &mut &'a [u8], count: usize) -> Option<&'a [u8]> {
    let (head, tail) = rest.split_at_checked(count)?;
    *rest = tail;
    Some(head)
}

fn take_string<'a>(rest: &mut &'a [u8]) -> Option<&'a [u8]> {
    let length = u32::from_be_bytes(take(rest, 4)?.try_into().ok()?);
    take(rest, usize::try_from(length).ok()?)
}
