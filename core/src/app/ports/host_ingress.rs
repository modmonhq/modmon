use crate::domain::error::MmError;
use crate::domain::host_ingress::PublicIngressState;

#[async_trait::async_trait]
pub trait PublicIngressControl: Send + Sync {
    async fn inspect(&self) -> Result<crate::domain::host_ingress::PublicIngressStatus, MmError>;
    async fn set_state(&self, state: PublicIngressState, allow_prompt: bool)
    -> Result<(), MmError>;
}
