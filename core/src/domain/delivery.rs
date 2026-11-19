//! How a commit's files reach a host.
//!
//! With `fetch` (the default) a host's `mmd` fetches the commit from the
//! origin its host policy pins. With `push` the client sends the commit's
//! tracked files over the session it already has, and the host needs no
//! route to the git server, no origin, and no deploy key.
//!
//! A bundle is a tar archive of exactly the files git tracks at one commit,
//! as the commit stores them. The host checks three things before it keeps
//! one: the bytes hash to what the client stated, the commit object hashes
//! to the commit's name, and the files hash to the tree that commit object
//! names. So a bundle can only ever be the commit it claims to be.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::remote::{is_commit_sha, is_lower_hex_digest};

/// What a setup plan says about a host enrolled for push.
pub const PUSH_SETUP_LINE: &str = "push (mm sends each commit's files; the hosts fetch nothing and get no deploy key, and setup removes a deploy key, pinned origin key, or clone that fetching left on them: also remove that deploy key from the git host)";

/// The largest bundle a host takes: the tracked files of one commit.
pub const MAX_BUNDLE_BYTES: u64 = 512 * 1024 * 1024;
/// The most files a bundle may hold.
pub const MAX_BUNDLE_FILES: u64 = 100_000;
/// The largest commit object a client may send: message, signature and all.
pub const MAX_COMMIT_OBJECT_BYTES: usize = 256 * 1024;
/// How many bundle bytes one frame carries.
pub const BUNDLE_CHUNK_BYTES: usize = 256 * 1024;
/// The most branches a client states for one commit.
const MAX_STATED_BRANCHES: usize = 1_000;

/// How a target's hosts get a commit's files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Delivery {
    /// Each host fetches the commit from the origin in its host policy.
    #[default]
    Fetch,
    /// mm sends the commit's tracked files; the hosts never contact the
    /// git server.
    Push,
}

impl Delivery {
    /// serde's `skip_serializing_if` passes a reference.
    #[must_use]
    pub const fn is_fetch(&self) -> bool {
        matches!(self, Self::Fetch)
    }

    #[must_use]
    pub const fn is_push(self) -> bool {
        matches!(self, Self::Push)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fetch => "fetch",
            Self::Push => "push",
        }
    }
}

/// What a client offers a push-mode host for one commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BundleOffer {
    pub commit: String,
    /// The origin's branches that contain the commit, as the client's
    /// repository knows them. The host holds them against `deploy-from`;
    /// it cannot check them against the git server.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<String>,
    /// The bundle the client can send. `None` when the client's checkout
    /// does not hold the commit (a rollback from a shallow clone): the host
    /// then answers from what it already holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<OfferedBundle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OfferedBundle {
    /// BLAKE3 of the bundle's bytes.
    pub hash: String,
    pub bytes: u64,
    /// The commit object, base64. Its SHA-1 is the commit's name, and it
    /// names the tree the bundle's files must hash to.
    pub commit_object: String,
}

impl BundleOffer {
    pub fn validate(&self) -> Result<(), String> {
        if !is_commit_sha(&self.commit) {
            return Err(format!(
                "'{}' is not a full 40-character commit SHA",
                self.commit
            ));
        }
        if self.branches.len() > MAX_STATED_BRANCHES
            || self.branches.iter().any(|branch| {
                branch.is_empty() || branch.len() > 255 || branch.chars().any(char::is_control)
            })
        {
            return Err("the offer's branch names are not valid".to_owned());
        }
        let Some(bundle) = &self.bundle else {
            return Ok(());
        };
        if !is_lower_hex_digest(&bundle.hash) {
            return Err("the bundle hash is not a BLAKE3 digest".to_owned());
        }
        if bundle.commit_object.len() > MAX_COMMIT_OBJECT_BYTES.div_ceil(3) * 4 {
            return Err(format!(
                "the commit object is larger than {} KiB",
                MAX_COMMIT_OBJECT_BYTES / 1024
            ));
        }
        Ok(())
    }
}

/// What a host holds for a commit after a delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Delivered {
    pub commit: String,
    /// BLAKE3 of the bundle the host verified and unpacked for the commit.
    pub bundle_hash: String,
    pub bytes: u64,
    pub files: u64,
    /// Whether this request sent the bundle. `false` when the host already
    /// held the commit and nothing was uploaded.
    pub received: bool,
}

/// The bundle a cluster plan was made from: the same one on every node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanBundle {
    pub bundle_hash: String,
    pub bytes: u64,
    pub files: u64,
    /// The nodes this plan sent the bundle to; the others already held it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sent_to: Vec<String>,
}

/// Why a tracked path never travels in a bundle, if it does not. A `.env`
/// file is where values that must not leave a machine end up, so one that
/// is committed is refused instead of sent: mm delivers environment values
/// through `bindings` and secrets through `secrets`.
#[must_use]
pub fn withheld(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let dotenv = name == ".env"
        || name.strip_prefix(".env.").is_some_and(|rest| {
            !matches!(
                rest,
                "example" | "sample" | "template" | "dist" | "defaults"
            )
        });
    dotenv.then_some("a .env file")
}

/// The line that tells a user how to get a committed `.env` file out of
/// the way of a push delivery.
#[must_use]
pub fn withheld_help(path: &str, reason: &str) -> String {
    format!(
        "the commit tracks '{path}', {reason}, and a push delivery never sends one. Stop tracking it (`git rm --cached {path}`, then commit), and give services their values with `bindings:` and `secrets:` in mm.yaml"
    )
}

/// The size of a bundle for a person.
#[must_use]
pub fn human_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    if bytes >= MIB {
        format!("{}.{} MiB", bytes / MIB, (bytes % MIB) * 10 / MIB)
    } else if bytes >= KIB {
        format!("{} KiB", bytes / KIB)
    } else {
        format!("{bytes} B")
    }
}
