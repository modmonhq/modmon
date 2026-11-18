use super::*;

pub(super) async fn run(
    check_updates: bool,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let progress = crate::presentation::inline::Progress::new(invocation.format);
    let mut report = crate::presentation::inline::step(
        &progress,
        ("doctor", "Checking this machine and the project"),
        async {
            Ok::<_, MmError>(
                composition::doctor_service(invocation.home(), &invocation.services)?
                    .inspect(&invocation.selection)
                    .await,
            )
        },
        |_| Some("Checked this machine and the project".to_owned()),
    )
    .await?;
    // What this machine needs for local development (its certificate
    // authority, the shared development services) says nothing about an
    // environment that runs on remote hosts: there it is listed as notes
    // and does not decide the exit code. Only a selection that resolved is
    // asked where it runs: the checks above read the same then, and a
    // doctor that found no target must leave the mm home untouched.
    let remote = report.project_selected
        && matches!(
            composition::remote_environment(
                &invocation.selection,
                invocation.home(),
                invocation.runtime_kind,
            ),
            Ok(Some(_))
        );
    let local = |result| if remote { as_notes(result) } else { result };
    match composition::local_ca_manager(invocation.home(), false, invocation.services.process()) {
        Ok(ca) => {
            append_diagnostics(&mut report, local(ca.diagnostics().await));
            if report.environment.is_some() {
                let diagnostics = dev::local_route_tls_diagnostics(
                    &invocation.start,
                    report.environment.as_deref(),
                    &ca,
                    invocation.services.process().as_ref(),
                )
                .await;
                append_diagnostics(&mut report, local(diagnostics));
            }
        }
        Err(error) => append_diagnostics(&mut report, local(Err(error))),
    }
    let runtime = report.runtime_kind.unwrap_or(invocation.runtime_kind);
    append_diagnostics(
        &mut report,
        local(
            composition::inspect_service_stack(invocation.home(), runtime, &invocation.services)
                .await
                .map(|status| status.diagnostics),
        ),
    );
    if check_updates {
        report.update = Some(
            composition::update_check_service()?
                .check(env!("CARGO_PKG_VERSION"))
                .await,
        );
    }
    progress.finish();
    output::write_doctor(&report, invocation.format, &invocation.recovery_context)?;
    if !report.validation.is_valid() {
        Ok(2)
    } else if report.runtime.inspected_here()
        && (!report.runtime.available
            || report
                .runtime
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error))
    {
        // Only an engine this machine asked decides this: the engine of a
        // remote environment is its hosts', and the checks above asked them.
        Ok(3)
    } else if report
        .update
        .as_ref()
        .is_some_and(|update| update.status == crate::domain::release::UpdateStatus::Unavailable)
    {
        Ok(1)
    } else {
        Ok(0)
    }
}

fn append_diagnostics(
    report: &mut crate::domain::workspace::DoctorReport,
    result: Result<Vec<crate::domain::diagnostic::Diagnostic>, MmError>,
) {
    match result {
        Ok(diagnostics) => report.validation.diagnostics.extend(diagnostics),
        Err(error) => report
            .validation
            .push(crate::domain::diagnostic::Diagnostic::error(
                error.code(),
                error.to_string(),
            )),
    }
}

/// The findings as notes: each keeps its code, its text, and what to run,
/// and none of them is an error or a warning.
fn as_notes(
    result: Result<Vec<crate::domain::diagnostic::Diagnostic>, MmError>,
) -> Result<Vec<crate::domain::diagnostic::Diagnostic>, MmError> {
    let note = |mut diagnostic: crate::domain::diagnostic::Diagnostic| {
        diagnostic.severity = Severity::Info;
        diagnostic.message = format!(
            "{} (local development on this machine; it does not affect this remote environment)",
            diagnostic.message
        );
        diagnostic
    };
    Ok(match result {
        Ok(diagnostics) => diagnostics.into_iter().map(note).collect(),
        Err(error) => vec![note(crate::domain::diagnostic::Diagnostic::error(
            error.code(),
            error.to_string(),
        ))],
    })
}
