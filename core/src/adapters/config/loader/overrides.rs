use serde_yaml_ng::Value;

pub const OVERRIDES_RELATIVE_PATH: &str = ".config/mm.overrides.yaml";

/// Parse a configuration that has no split files, then apply the optional
/// local layer. The fingerprint binds both sources; without an override it
/// retains the historical shared-file hash. Editors hash their proposed output
/// separately.
fn parse_single_file(
    path: &Path,
    source: &str,
    overrides: Option<&[u8]>,
) -> Result<(MmConfig, String, Vec<UnknownConfigKey>), MmError> {
    let (base, ignored) =
        deserialize_tracked::<MmConfig, _>(serde_yaml_ng::Deserializer::from_str(source))
            .map_err(|(error, ignored)| MmError::ConfigParse {
                path: path.to_path_buf(),
                message: with_unknown_keys(error.to_string(), &ignored),
            })?;
    let Some(bytes) = overrides else {
        return Ok((
            base,
            blake3::hash(source.as_bytes()).to_hex().to_string(),
            unknown_keys_in(&ignored, &[], None),
        ));
    };
    let override_path = path.with_file_name("mm.overrides.yaml");
    let (overlay, local, unknown_development) = parse_overlay(&override_path, bytes)?;
    let mut merged: Value =
        serde_yaml_ng::from_str(source).map_err(|error| MmError::ConfigParse {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    merge_configuration(&mut merged, overlay.clone());
    let (mut config, ignored) =
        deserialize_tracked::<MmConfig, _>(merged).map_err(|(error, ignored)| {
            MmError::ConfigParse {
                path: override_path,
                message: format!(
                    "invalid configuration after applying local overrides: {}",
                    with_unknown_keys(error.to_string(), &ignored)
                ),
            }
        })?;
    config.development.enabled = local.enabled;
    let mut unknown_keys = unknown_keys_in(&ignored, &[], Some(&overlay));
    unknown_keys.extend(unknown_development);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"mm-config-overrides-v1\0");
    for input in [source.as_bytes(), bytes] {
        hasher.update(&(input.len() as u64).to_le_bytes());
        hasher.update(input);
    }
    Ok((config, hasher.finalize().to_hex().to_string(), unknown_keys))
}

/// The overlay to merge, its `development:`, which only this file has, and
/// the keys of `development:` this release does not read.
fn parse_overlay(
    override_path: &Path,
    bytes: &[u8],
) -> Result<
    (
        Value,
        crate::domain::development_units::LocalDevelopment,
        Vec<UnknownConfigKey>,
    ),
    MmError,
> {
    let overrides = std::str::from_utf8(bytes).map_err(|_| MmError::ConfigParse {
        path: override_path.to_path_buf(),
        message: "configuration must be valid UTF-8".to_owned(),
    })?;
    let mut overlay: Value =
        serde_yaml_ng::from_str(overrides).map_err(|error| MmError::ConfigParse {
            path: override_path.to_path_buf(),
            message: error.to_string(),
        })?;
    if !overlay.is_mapping() {
        return Err(MmError::ConfigParse {
            path: override_path.to_path_buf(),
            message: "local overrides must be a YAML mapping (use {} for an empty override)"
                .to_owned(),
        });
    }
    if let Some(key) = protected_override(&overlay) {
        return Err(MmError::ConfigParse {
            path: override_path.to_path_buf(),
            message: format!(
                "local overrides cannot set '{key}'; access and deploy rules come only from committed configuration"
            ),
        });
    }
    let (local, ignored) = match overlay
        .as_mapping_mut()
        .and_then(|mapping| mapping.remove(LOCAL_DEVELOPMENT_KEY))
    {
        Some(development) => deserialize_tracked(development).map_err(|(error, _)| {
            MmError::ConfigParse {
                path: override_path.to_path_buf(),
                message: format!("{LOCAL_DEVELOPMENT_KEY}: {error}"),
            }
        })?,
        None => Default::default(),
    };
    Ok((overlay, local, unknown_development_keys(&ignored)))
}

const LOCAL_DEVELOPMENT_KEY: &str = "development";

/// `development:` lists what one machine runs, so only the local overrides
/// may set it.
fn refuse_local_development(path: &Path, keys: &[String]) -> Result<(), MmError> {
    if keys.iter().any(|key| key == LOCAL_DEVELOPMENT_KEY) {
        return Err(MmError::ConfigParse {
            path: path.to_path_buf(),
            message: format!(
                "'{LOCAL_DEVELOPMENT_KEY}:' belongs in {OVERRIDES_RELATIVE_PATH}, which keeps the optional units this machine runs; remove it here and run `mm dev enable UNIT`"
            ),
        });
    }
    Ok(())
}

/// Keys that decide who may reach remote environments and which commits may
/// deploy there. A checkout-local file must never change them.
fn protected_override(overlay: &Value) -> Option<String> {
    for key in ["users", "agents", "targets"] {
        if overlay.get(key).is_some() {
            return Some(key.to_owned());
        }
    }
    let job_key = overlay
        .get("jobs")
        .and_then(Value::as_mapping)
        .and_then(|jobs| {
            jobs.iter().find_map(|(name, job)| {
                ["run", "user", "environments", "environment"]
                    .into_iter()
                    .find(|key| job.get(*key).is_some())
                    .map(|key| format!("jobs.{}.{key}", name.as_str().unwrap_or("?")))
            })
        });
    if job_key.is_some() {
        return job_key;
    }
    overlay
        .get("environments")
        .and_then(Value::as_mapping)
        .and_then(|environments| {
            environments.iter().find_map(|(name, environment)| {
                environment.get("deploy-from").map(|_| {
                    format!(
                        "environments.{}.deploy-from",
                        name.as_str().unwrap_or("?")
                    )
                })
            })
        })
}

fn merge_configuration(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Mapping(base), Value::Mapping(overlay)) => {
            for (key, value) in overlay {
                if let Some(existing) = base.get_mut(&key) {
                    merge_configuration(existing, value);
                } else {
                    base.insert(key, value);
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}
