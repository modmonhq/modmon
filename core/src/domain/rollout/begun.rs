//! Who carries a rollout to its end after its own access change.
//!
//! A rollout's Begin applies the access change of the commit it deploys,
//! first, so that a revocation holds even if the stacks fail. When that
//! change lowers the principal who deploys, the principal still runs the
//! rest of exactly that rollout, and its abort: its steps were authorized
//! as a whole, under the access in force when its Begin claimed the
//! reviewed plan, and they can only carry out that plan or undo it. This
//! holds only while nothing else changed access to the environment since
//! the Begin: a sync, another deployment's change, or a block ends it, and
//! the step in flight stops as for any access change.

use serde::{Deserialize, Serialize};

use crate::domain::access::{Authorization, PrincipalId};

/// What a host keeps of the access a rollout began under: the
/// authorization its Begin was let in with, and a digest of what decides
/// access to the environment right after the Begin applied the plan's
/// access change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BegunUnder {
    pub authorization: Authorization,
    pub environment_access: String,
}

/// A request for a step of a rollout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RolloutRequest<'a> {
    pub plan_hash: &'a str,
    pub principal: PrincipalId,
    /// The step that claims the plan: never let in this way.
    pub begins: bool,
}

impl BegunUnder {
    /// Whether `request` is one of the steps of the rollout of `plan_hash`
    /// that `holder` began, while access to the environment is still what
    /// the Begin left (`environment_access` now).
    #[must_use]
    pub fn lets(
        &self,
        (plan_hash, holder): (&str, PrincipalId),
        request: RolloutRequest<'_>,
        environment_access: &str,
    ) -> bool {
        !request.begins
            && request.plan_hash == plan_hash
            && request.principal == holder
            && self.authorization.principal_id == holder
            && self.environment_access == environment_access
    }
}
