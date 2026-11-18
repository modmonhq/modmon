use super::{BuildVariantRequest, ImageBuilder};
use crate::domain::build::BuiltImage;
use crate::domain::error::MmError;
use crate::domain::operation::OperationId;

struct MinimalBuilder;

#[async_trait::async_trait]
impl ImageBuilder for MinimalBuilder {
    async fn build_variant(
        &self,
        _request: BuildVariantRequest<'_>,
    ) -> Result<BuiltImage, MmError> {
        Err(MmError::Build {
            message: "not used by default behavior test".to_owned(),
        })
    }

    async fn retag_variant(
        &self,
        _source_repository: &str,
        _digest: &str,
        _destination_repository: &str,
        _tags: &[String],
    ) -> Result<String, MmError> {
        Err(MmError::Build {
            message: "not used by default behavior test".to_owned(),
        })
    }
}

#[test]
fn default_operation_guard_and_cleanup_are_explicit_no_effects() -> Result<(), MmError> {
    let builder = MinimalBuilder;
    let operation = OperationId::new();
    builder.begin_operation(operation)?.preserve();
    builder.cleanup_operation(operation)
}
