//! `mm target trust`: the keys a repo target pins on this machine.

use super::*;

pub(super) fn repo_target_pins(home: &MmHome) -> crate::adapters::target::TargetPins {
    crate::adapters::target::TargetPins::new(home.root.join("pinned-targets.json"))
}

pub(super) fn pin_key(document: &crate::domain::config::ConfigDocument, name: &str) -> String {
    format!("{}/{name}", document.config.project.id.0)
}

/// `mm target trust`: the pinned and current facts of a repo target.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TargetTrustReview {
    pub target: String,
    pub pinned: Option<crate::domain::repo_target::TargetPin>,
    pub repo: crate::domain::repo_target::TargetPin,
    pub trusted: bool,
}

pub(crate) fn trust_repo_target(
    start: &Path,
    explicit_home: Option<&Path>,
    name: &str,
    apply: bool,
) -> Result<TargetTrustReview, MmError> {
    let document = ConfigLoader.load_from(start)?;
    let repo = document
        .config
        .targets
        .get(name)
        .ok_or_else(|| MmError::Environment {
            message: format!("the repo declares no target '{name}' under targets:"),
        })?;
    let home = MmHome::resolve(explicit_home)?;
    let pins = repo_target_pins(&home);
    let key = pin_key(&document, name);
    let pinned = pins.pinned(&key);
    let repo = repo
        .pin(name)
        .map_err(|message| MmError::Target { message })?;
    if apply {
        pins.trust(&key, &repo)?;
        // The host's keys changed hands: nothing connected before stays.
        close_kept_connections(&home);
    }
    Ok(TargetTrustReview {
        target: name.to_owned(),
        pinned,
        repo,
        trusted: apply,
    })
}
