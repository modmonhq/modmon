//! What a deployment plan does when a secret store it needs cannot answer.

use super::*;

/// The code of a plan refused because a secret store did not answer.
pub const SECRET_STORE_UNAVAILABLE: &str = "MM_SECRET_STORE_UNAVAILABLE";

/// Whether a store that cannot answer stops the review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StoreOutage {
    /// A deployment: without the store's versions there is nothing to plan
    /// from, and the apply would fail when it reads the values.
    Refuse,
    /// `mm dev` and `mm env review`: the review goes on without a version,
    /// and the environment status reports the store as unavailable. The
    /// development environment must start while its managed store is
    /// stopped.
    Tolerate,
}

impl StoreOutage {
    /// The rule for a deployment of `environment` to `target`: tolerated
    /// only for the project's development environment on this machine,
    /// which is what `mm dev` deploys.
    pub(super) fn for_deployment(
        document: &ConfigDocument,
        environment: &str,
        target: &crate::domain::target::NamedTarget,
    ) -> Self {
        let development = document.config.project.development_environment.as_deref()
            == Some(environment)
            && target.definition.transport == crate::domain::target::TargetTransport::Local;
        if development {
            Self::Tolerate
        } else {
            Self::Refuse
        }
    }

    /// Refuses the review when a store reported itself unavailable for a
    /// secret the environment uses.
    pub(super) fn check(
        self,
        environment: &str,
        metadata: &BTreeMap<String, SecretMetadata>,
    ) -> Result<(), MmError> {
        if self == Self::Tolerate {
            return Ok(());
        }
        metadata
            .iter()
            .find(|(_, metadata)| metadata.status == SecretStatus::ProviderUnavailable)
            .map_or(Ok(()), |(reference, _)| {
                Err(store_unavailable(
                    environment,
                    provider_of(reference),
                    "did not answer: it is unreachable, sealed, or not initialized",
                ))
            })
    }

    /// What a store's refusal to inspect a secret becomes. A refused login
    /// is said as such; any other refusal already names what to do.
    pub(super) fn refusal(self, environment: &str, provider: &str, error: MmError) -> MmError {
        // The OpenBao and Vault adapters say a refused login as a security
        // error ("AppRole authentication was denied").
        let denied = match &error {
            MmError::Secret { message } => message.contains("denied"),
            MmError::Security { message } => message.contains("AppRole authentication was denied"),
            _ => false,
        };
        if self == Self::Refuse && denied {
            store_unavailable(environment, provider, "refused the login (access denied)")
        } else {
            error
        }
    }
}

/// The provider part of a canonical secret reference (`provider/...`).
fn provider_of(reference: &str) -> &str {
    reference.split('/').next().unwrap_or(reference)
}

/// What a refused plan says it did, and what to do once the store answers.
const NOT_PLANNED: &str = "nothing was planned";
const PLAN_AGAIN: &str = "Then plan again.";

/// Names the provider and what failed; never a value or a credential.
fn store_unavailable(environment: &str, provider: &str, problem: &str) -> MmError {
    MmError::Remote {
        code: SECRET_STORE_UNAVAILABLE,
        message: format!(
            "secret store '{provider}' {problem}, so environment '{environment}' has no version \
             of its secrets to plan from; {NOT_PLANNED}. Where mm runs, `mm secret status \
             -e {environment}` shows the store's state. On a remote host, check that the store \
             answers from the host, and run `mmd secrets connect` there again if its login was \
             refused. {PLAN_AGAIN}"
        ),
    }
}

/// The same refusal met by an apply, which plans again before it changes
/// anything: it says that, and that the reviewed plan can be applied once
/// the store answers.
#[must_use]
pub fn refused_at_apply(error: MmError) -> MmError {
    match error {
        MmError::Remote { code, message } if code == SECRET_STORE_UNAVAILABLE => MmError::Remote {
            code,
            message: message
                .replacen(
                    NOT_PLANNED,
                    "the apply planned again before changing anything and was refused, so nothing was changed",
                    1,
                )
                .replacen(
                    PLAN_AGAIN,
                    "Then apply the same plan again while it is valid, or plan again.",
                    1,
                ),
        },
        other => other,
    }
}
