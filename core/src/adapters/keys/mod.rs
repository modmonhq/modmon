//! The SSH keys mm manages on a device: the key files, the vaults that
//! keep their passphrases, and handing a passphrase to OpenSSH.

mod askpass;
mod managed;
mod openssh;
mod vault;

pub use askpass::Askpass;
pub use managed::{ManagedKeys, PrivateAgent};
pub use openssh::OpenSshKeyFiles;
pub use vault::{FileKeyVault, MacKeychainVault, NoKeychain, SecretServiceVault, system_keychain};

/// A new directory under `parent` that only the user can enter, whatever
/// the umask says: a temporary directory is otherwise made for everyone to
/// list.
fn private_directory_in(
    parent: &std::path::Path,
    prefix: &str,
) -> std::io::Result<tempfile::TempDir> {
    std::fs::create_dir_all(parent)?;
    let mut builder = tempfile::Builder::new();
    builder.prefix(prefix);
    #[cfg(unix)]
    builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o700));
    builder.tempdir_in(parent)
}
