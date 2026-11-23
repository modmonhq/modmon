use std::time::Duration;

use crate::domain::compose_timing::{compose_limit, compose_wait};
use crate::domain::runtime::RuntimeTarget;

/// The process limit for a Compose `up`, `down` or `stop` of `target`'s
/// project, at least `floor`.
pub(crate) fn project_limit(target: &RuntimeTarget, floor: Duration) -> Duration {
    let models = target
        .compose_files
        .iter()
        .filter_map(|file| {
            let path = target.root.join(file);
            let text = std::fs::read_to_string(&path)
                .map_err(|error| tracing::debug!(path = %path.display(), %error, "unreadable Compose file"))
                .ok()?;
            serde_yaml_ng::from_str(&text)
                .map_err(|error| tracing::debug!(path = %path.display(), %error, "unparsable Compose file"))
                .ok()
        })
        .collect::<Vec<serde_yaml_ng::Value>>();
    compose_limit(compose_wait(&models), floor)
}

#[cfg(test)]
#[path = "compose_limit_tests.rs"]
mod tests;
