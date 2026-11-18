use super::*;

pub fn unit_input_fingerprint(
    unit: &ImageBuildUnitPlan,
    platforms: &[String],
) -> Result<String, String> {
    build_node_key(unit, platforms, &unit.upstream_node_ids)?.digest()
}

pub fn build_node_key<'a>(
    unit: &'a ImageBuildUnitPlan,
    platforms: &'a [String],
    upstream_node_ids: &'a [String],
) -> Result<BuildNodeKey<'a>, String> {
    if unit.dependencies.len() != upstream_node_ids.len() {
        return Err(format!(
            "module '{}' has incomplete upstream node identity",
            unit.module
        ));
    }
    let args = unit
        .args
        .iter()
        .filter(|(name, _)| unit.version_embedded || *name != "VERSION")
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    Ok(BuildNodeKey {
        context_digest: &unit.context_hash,
        dockerfile_digest: if unit.dockerfile_hash.is_empty() {
            &unit.context_hash
        } else {
            &unit.dockerfile_hash
        },
        args,
        secret_ids: unit.output_secret_ids().map(String::as_str).collect(),
        extra_input_digests: unit
            .extra_inputs
            .iter()
            .map(|trigger| trigger.digest.as_str())
            .collect(),
        version_embedded: unit.version_embedded,
        reuse_policy: unit.reuse_policy,
        platforms,
        dependencies: unit
            .dependencies
            .iter()
            .zip(upstream_node_ids)
            .map(|(dependency, upstream_node_id)| BuildNodeDependencyKey {
                upstream_node_id,
                stage: dependency.stage.as_deref(),
                reference: &dependency.reference,
            })
            .collect(),
    })
}

impl BuildNodeKey<'_> {
    pub fn digest(&self) -> Result<String, String> {
        serde_json::to_vec(self)
            .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
            .map_err(|error| format!("serializing image build fingerprint: {error}"))
    }
}

#[derive(Serialize)]
pub(super) struct CanonicalExecutionFingerprint<'a> {
    input_fingerprint: &'a str,
    environment: &'a str,
    variant: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    variant_args: Option<&'a BTreeMap<String, String>>,
    upstream_digests: &'a BTreeMap<String, String>,
    secret_revisions: &'a BTreeMap<String, String>,
}

pub fn unit_execution_fingerprint(
    input_fingerprint: &str,
    environment: &str,
    variant: Option<&ImageBuildVariantPlan>,
    upstream_digests: &BTreeMap<String, String>,
    secret_revisions: &BTreeMap<String, String>,
) -> Result<String, String> {
    if input_fingerprint.len() != 64
        || !input_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("image build input fingerprint is not canonical lowercase hex".to_owned());
    }
    let canonical = CanonicalExecutionFingerprint {
        input_fingerprint,
        environment,
        variant: variant.and_then(|variant| variant.target.as_deref()),
        stage: variant.and_then(|variant| variant.stage.as_deref()),
        variant_args: variant
            .map(|variant| &variant.args)
            .filter(|args| !args.is_empty()),
        upstream_digests,
        secret_revisions,
    };
    serde_json::to_vec(&canonical)
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .map_err(|error| format!("serializing image build execution fingerprint: {error}"))
}

pub fn is_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

pub(super) fn is_oci_tag(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_'))
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

pub(super) fn is_oci_platform(value: &str) -> bool {
    let parts = value.split('/').collect::<Vec<_>>();
    matches!(parts.len(), 2 | 3)
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 32
                && part.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_' | b'-')
                })
        })
}

pub(super) fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub(super) fn is_variable_name(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_uppercase() || byte == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

pub(super) fn is_relative_build_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

pub fn is_image_repository(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.contains('@')
        && !value.contains("://")
        && !value.starts_with('/')
        && !value.ends_with('/')
        && {
            let parts = value.split('/').collect::<Vec<_>>();
            parts.iter().enumerate().all(|(index, part)| {
                if index == 0
                    && parts.len() > 1
                    && (part.contains('.') || part.contains(':') || *part == "localhost")
                {
                    is_registry(part)
                } else {
                    is_repository_component(part)
                }
            })
        }
}

pub(super) fn is_registry(value: &str) -> bool {
    let (host, port) = value
        .split_once(':')
        .map_or((value, None), |(host, port)| (host, Some(port)));
    if host.is_empty()
        || host.split('.').any(|label| {
            label.is_empty()
                || !label
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                || !label
                    .bytes()
                    .last()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
    {
        return false;
    }
    port.is_none_or(|port| {
        !port.is_empty()
            && port.bytes().all(|byte| byte.is_ascii_digit())
            && port.parse::<u16>().is_ok_and(|port| port > 0)
    })
}

pub(super) fn is_repository_component(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}
