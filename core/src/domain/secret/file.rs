use std::path::Path;

/// Where the `file` provider reads a secret: an absolute path, or one under
/// the home directory of the user who runs mm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSecretLocation<'a> {
    Absolute(&'a Path),
    Home(&'a Path),
}

pub fn file_secret_location(location: &str) -> Result<FileSecretLocation<'_>, String> {
    if let Some(rest) = location.strip_prefix("~/") {
        let rest = Path::new(rest);
        if !rest.as_os_str().is_empty() && rest.is_relative() {
            return Ok(FileSecretLocation::Home(rest));
        }
    } else if Path::new(location).is_absolute() {
        return Ok(FileSecretLocation::Absolute(Path::new(location)));
    }
    Err(format!(
        "file locations are absolute or start with `~/`, and '{location}' is neither: a relative path names a file in the checkout, which could be committed with the project. Write an absolute path or one under your home, such as `~/.config/tool/auth.json`"
    ))
}
