//! The release a plan deploys and a host runs: its name and inputs.

use crate::domain::release_facts::{ReleaseFacts, ReleaseRequest};
use crate::presentation::inline::{Card, Value};

/// `name=value, ...`, or `None` without inputs.
fn inputs(inputs: &std::collections::BTreeMap<String, String>) -> Option<String> {
    (!inputs.is_empty()).then(|| {
        inputs
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(", ")
    })
}

/// The release of a remote plan: every node planned the same one.
pub(super) fn cluster_release(
    plan: &crate::domain::remote::ClusterDeploymentPlan,
) -> Option<&ReleaseFacts> {
    plan.nodes
        .iter()
        .find_map(|section| section.plan.release.as_ref())
}

/// The lines a plan's text says about its release.
pub(super) fn release_lines(release: Option<&ReleaseFacts>) -> Vec<String> {
    let Some(release) = release else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    if let Some(name) = release.release_name() {
        lines.push(format!("  release: {name}"));
    }
    lines.extend(
        release
            .inputs
            .iter()
            .map(|(key, value)| format!("  release input: {key}={value}")),
    );
    lines
}

/// The release's rows of a plan's card.
pub(super) fn release_fields(card: &mut Card, release: Option<&ReleaseFacts>) {
    let Some(release) = release else {
        return;
    };
    if let Some(name) = release.release_name() {
        card.field("release", name);
    }
    if let Some(inputs) = inputs(&release.inputs) {
        card.field("inputs", Value::new(inputs));
    }
}

/// What a host's deployment record says about its release, for `mm deploy
/// status`: `  release v1  inputs BUILD=7`.
pub(super) fn record_release(release: Option<&ReleaseFacts>) -> String {
    let Some(release) = release else {
        return String::new();
    };
    let name = release
        .name
        .as_deref()
        .map(|name| format!("  release {name}"))
        .unwrap_or_default();
    let inputs = inputs(&release.inputs)
        .map(|inputs| format!("  inputs {inputs}"))
        .unwrap_or_default();
    format!("{name}{inputs}")
}

/// What a rollback returns to, when it was given a name or inputs.
pub(super) fn rollback_release(release: &ReleaseRequest) -> String {
    let mut parts = Vec::new();
    if let Some(name) = &release.name {
        parts.push(format!("release {name}"));
    }
    if let Some(inputs) = inputs(&release.inputs) {
        parts.push(format!("inputs {inputs}"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" with {}", parts.join(" and "))
    }
}
