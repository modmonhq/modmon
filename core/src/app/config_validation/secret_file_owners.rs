use super::*;
use crate::domain::runtime::RuntimeKind;
use crate::domain::runtime_secret::{HostIds, SecretFileOwnerNote};
use crate::domain::target::TargetTransport;

/// On Docker on this machine, where mm runs as an ordinary user, each
/// secret file belongs to that user, whatever ids its entry names.
pub(crate) fn validate_secret_file_owners(
    document: &ConfigDocument,
    context: &crate::app::ResolvedOperationContext,
    report: &mut ValidationReport,
) {
    if context.runtime != RuntimeKind::DockerCompose || context.transport != TargetTransport::Local
    {
        return;
    }
    let Some(user) = HostIds::unprivileged() else {
        return;
    };
    for note in SecretFileOwnerNote::in_config(&document.config, &context.environment, user) {
        report.push(
            Diagnostic::warning("MM_SECRET_FILE_OWNER", note.line())
                .at_path(document.source("modules")),
        );
    }
}
