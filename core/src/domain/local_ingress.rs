use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::workspace::ProjectId;

const MAX_SLUG_LENGTH: usize = 63;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ProjectSlug(String);

impl ProjectSlug {
    pub fn normalize(value: &str) -> Result<Self, String> {
        normalize_slug(value, "project").map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectSlug {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Returns a DNS label that remains distinct when projects share a display name.
pub fn project_dashboard_slug(
    name: &str,
    project_id: ProjectId,
) -> Result<String, crate::domain::error::MmError> {
    let readable = ProjectSlug::normalize(name)
        .map_err(|message| crate::domain::error::MmError::Environment { message })?;
    let mut readable = readable.to_string();
    readable.truncate(30);
    Ok(format!("{readable}-{}", project_id.0.simple()))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ServiceSlug(String);

impl ServiceSlug {
    pub fn normalize(value: &str) -> Result<Self, String> {
        normalize_slug(value, "service").map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ServiceSlug {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn normalize_slug(value: &str, kind: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!(
            "{kind} name cannot normalize to an empty local ingress slug"
        ));
    }
    if !value.is_ascii() {
        return Err(format!(
            "{kind} name '{value}' contains non-ASCII characters; local ingress slugs do not apply IDNA or transliteration, so configure an ASCII name"
        ));
    }

    let mut slug = String::with_capacity(value.len());
    let mut separator = false;
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(char::from(byte.to_ascii_lowercase()));
            separator = false;
        } else {
            separator = true;
        }
    }
    if slug.is_empty() {
        return Err(format!(
            "{kind} name '{value}' cannot normalize to a DNS label"
        ));
    }
    if slug.len() > MAX_SLUG_LENGTH {
        return Err(format!(
            "{kind} name '{value}' normalizes to '{slug}', which exceeds the {MAX_SLUG_LENGTH}-byte DNS label limit; choose a shorter name"
        ));
    }
    if slug == "mm" {
        return Err(format!(
            "{kind} name '{value}' normalizes to reserved slug 'mm'; choose another name because mm.*.test is reserved for managed services"
        ));
    }
    Ok(slug)
}

#[must_use]
pub fn is_reserved_mm_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "mm.test"
        || host.ends_with(".mm.test")
        || (host.starts_with("mm.") && host.ends_with(".test"))
}

#[cfg(test)]
mod tests;
