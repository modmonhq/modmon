use super::*;

pub const IMPORT_PLAN_SCHEMA: &str = "mm-build-import-plan-v1";

/// `mm build import`: what importing a build receipt records for this
/// checkout and environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BuildImportPlan {
    pub schema: String,
    pub project_id: ProjectId,
    pub receipt_hash: String,
    pub commit: String,
    /// The environment the receipt was built for.
    pub source_environment: String,
    /// The environment the receipt is imported into.
    pub environment: String,
    /// Only seeds build reuse; deployments never select these images.
    pub cache_only: bool,
    /// The build plan recorded as this checkout's published build for the
    /// environment; absent for a cache-only import.
    pub build_plan_hash: Option<String>,
    pub images: Vec<BuildImportImage>,
    pub plan_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BuildImportImage {
    pub module: String,
    pub target: Option<String>,
    pub repository: String,
    pub digest: String,
    /// The execution fingerprint a later build in this environment reuses
    /// the image for. Absent when the module cannot be reused, because it
    /// mounts a build secret that is not credential-only or this checkout
    /// configures another repository for it.
    pub reuse_fingerprint: Option<String>,
}

impl BuildImportPlan {
    pub fn canonical_hash(&self) -> Result<String, String> {
        let mut canonical = self.clone();
        canonical.plan_hash.clear();
        serde_json::to_vec(&canonical)
            .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
            .map_err(|error| format!("serializing build import plan: {error}"))
    }
}
