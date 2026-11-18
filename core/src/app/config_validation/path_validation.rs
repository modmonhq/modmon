use std::path::Path;

use crate::domain::diagnostic::{Diagnostic, ValidationReport};
use crate::domain::path_policy::ensure_no_symlink_components;

pub(super) fn validate_no_symlinks(path: &Path, kind: &str, report: &mut ValidationReport) {
    if let Err(error) = ensure_no_symlink_components(path) {
        report.push(
            Diagnostic::error(
                "MM_PATH_SYMLINK",
                format!("{kind} may not contain symlink components: {error}"),
            )
            .at_path(path),
        );
    }
}
