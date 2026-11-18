use super::*;

pub fn automatic_tags(
    source: &BuildSource,
    strategy: BuildTagStrategy,
) -> Result<Vec<String>, String> {
    if !is_git_object_id(&source.commit) {
        return Err("build source commit must be a full hexadecimal object id".to_owned());
    }
    let immutable = format!("sha-{}", source.commit);
    if source.dirty {
        return Ok(vec![format!("{immutable}-dirty")]);
    }

    let versions_at_head = parse_versions(&source.tags_at_head);
    if let Some(version) = versions_at_head
        .iter()
        .map(|(_, version)| version)
        .max()
        .cloned()
    {
        if versions_at_head
            .iter()
            .any(|(_, candidate)| candidate != &version)
        {
            return Err("HEAD has multiple different semantic version tags".to_owned());
        }

        let preferred = versions_at_head
            .iter()
            .map(|(tag, _)| tag)
            .filter(|tag| tag.starts_with('v'))
            .min()
            .cloned()
            .unwrap_or_else(|| version.to_string());
        let mut tags = Vec::new();
        push_unique(&mut tags, oci_semver_tag(&preferred)?);
        push_unique(&mut tags, oci_semver_tag(&version.to_string())?);

        if strategy == BuildTagStrategy::Auto && version.pre.is_empty() {
            let repository_versions = parse_versions(&source.repository_tags);
            let stable_versions = repository_versions
                .iter()
                .map(|(_, candidate)| candidate)
                .filter(|candidate| candidate.pre.is_empty());
            let minor_is_current = stable_versions.clone().all(|candidate| {
                candidate.major != version.major
                    || candidate.minor != version.minor
                    || candidate <= &version
            });
            let release_is_latest = stable_versions
                .clone()
                .all(|candidate| candidate <= &version);
            if minor_is_current {
                push_unique(&mut tags, format!("{}.{}", version.major, version.minor));
            }
            if release_is_latest {
                push_unique(&mut tags, "latest".to_owned());
            }
        }
        push_unique(&mut tags, immutable);
        return Ok(tags);
    }

    let mut tags = Vec::new();
    if strategy == BuildTagStrategy::Auto {
        if let Some(branch) = source.branch.as_deref()
            && let Some((major, minor)) = release_branch(branch)
        {
            tags.push(format!("{major}.{minor}-current"));
        } else if source.default_branch {
            tags.push("edge".to_owned());
        }
    }
    tags.push(immutable);
    Ok(tags)
}

pub fn suffixed_tags(tags: &[String], suffix: &str) -> Result<Vec<String>, String> {
    tags.iter()
        .map(|tag| {
            let candidate = if suffix.is_empty() {
                tag.clone()
            } else {
                format!("{tag}-{suffix}")
            };
            if is_oci_tag(&candidate) {
                Ok(candidate)
            } else {
                Err(format!("'{candidate}' is not a valid OCI tag"))
            }
        })
        .collect()
}

pub fn build_version(source: &BuildSource) -> Result<String, String> {
    if !is_git_object_id(&source.commit) {
        return Err("build source commit must be a full hexadecimal object id".to_owned());
    }
    let versions = parse_versions(&source.tags_at_head);
    if let Some(version) = versions.iter().map(|(_, version)| version).max().cloned() {
        if versions.iter().any(|(_, candidate)| candidate != &version) {
            return Err("HEAD has multiple different semantic version tags".to_owned());
        }
        return Ok(versions
            .iter()
            .map(|(tag, _)| tag)
            .filter(|tag| tag.starts_with('v'))
            .min()
            .cloned()
            .unwrap_or_else(|| version.to_string()));
    }
    if let Some((major, minor)) = source.branch.as_deref().and_then(release_branch) {
        return Ok(format!("{major}.{minor}.x"));
    }
    let short = &source.commit[..8];
    Ok(if source.dirty {
        format!("{short}-dirty")
    } else {
        short.to_owned()
    })
}

pub(super) fn parse_versions(tags: &[String]) -> Vec<(String, Version)> {
    tags.iter()
        .filter_map(|tag| {
            let candidate = tag.strip_prefix('v').unwrap_or(tag);
            match Version::parse(candidate) {
                Ok(version) => Some((tag.clone(), version)),
                Err(_) => None,
            }
        })
        .collect()
}

pub(super) fn release_branch(branch: &str) -> Option<(u64, u64)> {
    let prefix = branch.strip_suffix(".x")?;
    let (major, minor) = prefix.split_once('.')?;
    if minor.contains('.')
        || major.starts_with('0') && major.len() > 1
        || minor.starts_with('0') && minor.len() > 1
    {
        return None;
    }
    match (major.parse(), minor.parse()) {
        (Ok(major), Ok(minor)) => Some((major, minor)),
        (Err(_), _) | (_, Err(_)) => None,
    }
}

pub(super) fn oci_semver_tag(tag: &str) -> Result<String, String> {
    let tag = tag.replace('+', "-");
    if is_oci_tag(&tag) {
        Ok(tag)
    } else {
        Err(format!("semantic version tag '{tag}' is not OCI-tag safe"))
    }
}

pub(super) fn push_unique(tags: &mut Vec<String>, tag: String) {
    if !tags.contains(&tag) {
        tags.push(tag);
    }
}

pub(super) fn is_git_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub(super) fn is_blake3_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests;
