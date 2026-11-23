use std::time::Duration;

use crate::domain::compose_timing::{ComposeOrder, compose_limit, compose_wait};
use crate::domain::runtime::RuntimeTarget;

/// The process limit for a Compose `up`, `down` or `stop` of `target`'s
/// project under a provider that works in `order`, at least `floor`.
pub(crate) fn project_limit(
    target: &RuntimeTarget,
    order: ComposeOrder,
    floor: Duration,
) -> Duration {
    let models = target
        .compose_files
        .iter()
        .filter_map(|file| {
            let path = target.root.join(file);
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) => {
                    tracing::debug!(path = %path.display(), %error, "unreadable Compose file");
                    return None;
                }
            };
            match serde_yaml_ng::from_str(&text) {
                Ok(model) => Some(model),
                Err(error) => {
                    tracing::debug!(path = %path.display(), %error, "unparsable Compose file");
                    None
                }
            }
        })
        .collect::<Vec<serde_yaml_ng::Value>>();
    let variables = |name: &str| {
        target
            .interpolation_environment
            .values()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.to_owned())
    };
    compose_limit(compose_wait(&models, &variables, order), floor)
}

#[cfg(test)]
#[path = "compose_limit_tests.rs"]
mod tests;
