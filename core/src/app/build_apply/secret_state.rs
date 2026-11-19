use super::*;
use crate::domain::secret::{SecretReference, SecretScope, SecretStatus};

impl BuildService {
    /// Why build secret `id`, which reads secret `name`, has no value here.
    pub(super) async fn missing_build_secret(
        &self,
        (id, name): (&str, &str),
        scope: &SecretScope,
        reference: &SecretReference,
        status: SecretStatus,
    ) -> MmError {
        let environment = scope.environment();
        let source = format!(
            "secret '{name}' from {} '{}'",
            reference.scheme(),
            reference.locator()
        );
        let status_command = format!("`mm -e {environment} secret status {name}`");
        let message = match status {
            SecretStatus::Unset => {
                format!(
                    "build secret '{id}' needs {source}, which is not set; {status_command} shows its state"
                )
            }
            SecretStatus::Invalid => match self
                .ports
                .secret_inspector
                .invalid_reason(scope, reference)
                .await
            {
                Some(reason) => {
                    format!("build secret '{id}' needs {source}, which is invalid: {reason}")
                }
                None => format!(
                    "build secret '{id}' needs {source}, which is invalid; {status_command} shows why"
                ),
            },
            SecretStatus::AccessDenied => {
                format!(
                    "build secret '{id}' needs {source}, and its store denied access; {status_command} shows its state"
                )
            }
            SecretStatus::Set | SecretStatus::NotChecked | SecretStatus::ProviderUnavailable => {
                format!(
                    "build secret '{id}' needs {source}, and its store is unavailable; {status_command} shows its state"
                )
            }
        };
        MmError::Secret { message }
    }
}
