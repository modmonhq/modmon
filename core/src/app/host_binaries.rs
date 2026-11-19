//! Which CPU the `mm` and `mmd` that setup installs are built for.

use std::io::Read;
use std::path::Path;

use crate::domain::error::MmError;
use crate::domain::target::linux_architecture;

const ELF_HEADER_BYTES: u64 = 64;

/// The CPU both `mm` and `mmd` in `directory` are built for: `x86_64` or
/// `aarch64`. Anything else, or two different CPUs, is refused with the
/// release archives to take them from.
pub(crate) fn host_binaries_architecture(directory: &Path) -> Result<String, MmError> {
    let mut found = Vec::new();
    for name in ["mm", "mmd"] {
        let path = directory.join(name);
        let mut header = Vec::new();
        std::fs::File::open(&path)
            .and_then(|file| file.take(ELF_HEADER_BYTES).read_to_end(&mut header))
            .map_err(|source| MmError::Io {
                context: format!("reading host binary {}", path.display()),
                source,
            })?;
        let architecture = linux_architecture(&header).map_err(|why| MmError::Target {
            message: format!(
                "{} {why}; setup installs it on Linux hosts. {RELEASE_ARCHIVES}",
                path.display()
            ),
        })?;
        found.push(architecture);
    }
    match found.as_slice() {
        [mm, mmd] if mm == mmd => Ok((*mm).to_owned()),
        [mm, mmd] => Err(MmError::Target {
            message: format!(
                "{} holds mm for {mm} and mmd for {mmd}; setup installs them together. {RELEASE_ARCHIVES}",
                directory.display()
            ),
        }),
        _ => Err(MmError::Target {
            message: format!("{} holds no mm and mmd", directory.display()),
        }),
    }
}

const RELEASE_ARCHIVES: &str = "Pass --host-binaries with a directory holding mm and mmd from the Linux release archive for the hosts' CPU: mm-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz or mm-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz";
