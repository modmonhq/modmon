//! A provider under `providers:`, whose keys depend on its `mode:`.

use std::path::PathBuf;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

use super::{
    ProviderConfiguration, ProviderDriverId, ProviderInstanceConfig, ProviderMode, ProviderScope,
};
use crate::domain::secret_store::StoreNodes;

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ProviderFields {
    driver: ProviderDriverId,
    scope: ProviderScope,
    mode: ProviderMode,
    #[serde(default)]
    installation: Option<String>,
    #[serde(default)]
    release: Option<String>,
    #[serde(default)]
    management_port: Option<u16>,
    #[serde(default)]
    nodes: Option<StoreNodes>,
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    storage_mount: Option<String>,
    #[serde(default)]
    automation_auth_mount: Option<String>,
    #[serde(default)]
    trust_bundle: Option<PathBuf>,
    #[serde(default)]
    expected_trust_sha256: Option<String>,
}

fn other_mode<E: Error>(set: &[(&str, bool)], mode: &str) -> Result<(), E> {
    match set.iter().find(|(_, present)| *present) {
        Some((key, _)) => Err(E::custom(format!("`{key}:` applies to `mode: {mode}`"))),
        None => Ok(()),
    }
}

fn required<T, E: Error>(value: Option<T>, field: &'static str) -> Result<T, E> {
    value.ok_or_else(|| E::missing_field(field))
}

impl<'de> Deserialize<'de> for ProviderInstanceConfig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = ProviderFields::deserialize(deserializer)?;
        let configuration = match fields.mode {
            ProviderMode::Managed => {
                other_mode::<D::Error>(
                    &[
                        ("address", fields.address.is_some()),
                        ("namespace", fields.namespace.is_some()),
                        ("storage-mount", fields.storage_mount.is_some()),
                        (
                            "automation-auth-mount",
                            fields.automation_auth_mount.is_some(),
                        ),
                        ("trust-bundle", fields.trust_bundle.is_some()),
                        (
                            "expected-trust-sha256",
                            fields.expected_trust_sha256.is_some(),
                        ),
                    ],
                    "external",
                )?;
                ProviderConfiguration::Managed {
                    installation: fields.installation,
                    release: required(fields.release, "release")?,
                    management_port: fields.management_port,
                    nodes: fields.nodes,
                }
            }
            ProviderMode::External => {
                other_mode::<D::Error>(
                    &[
                        ("installation", fields.installation.is_some()),
                        ("release", fields.release.is_some()),
                        ("management-port", fields.management_port.is_some()),
                        ("nodes", fields.nodes.is_some()),
                    ],
                    "managed",
                )?;
                ProviderConfiguration::External {
                    address: required(fields.address, "address")?,
                    namespace: fields.namespace,
                    storage_mount: required(fields.storage_mount, "storage-mount")?,
                    automation_auth_mount: required(
                        fields.automation_auth_mount,
                        "automation-auth-mount",
                    )?,
                    trust_bundle: required(fields.trust_bundle, "trust-bundle")?,
                    expected_trust_sha256: required(
                        fields.expected_trust_sha256,
                        "expected-trust-sha256",
                    )?,
                }
            }
        };
        Ok(Self {
            driver: fields.driver,
            scope: fields.scope,
            configuration,
        })
    }
}
