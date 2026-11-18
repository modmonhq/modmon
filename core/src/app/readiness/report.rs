use super::*;

pub(super) struct InspectionReport {
    pub doctor: DoctorReport,
    pub readiness: ReadinessReport,
}

impl InspectionReport {
    pub fn new(runtime: Option<RuntimeKind>) -> Self {
        Self {
            readiness: ReadinessReport::default(),
            doctor: DoctorReport {
                readiness: None,
                project_name: None,
                root: None,
                target: None,
                environment: None,
                target_source: None,
                runtime_kind: None,
                runtime_source: None,
                environment_delivery: None,
                project_selected: false,
                runtime: RuntimeProbe::unavailable(
                    runtime.unwrap_or(RuntimeKind::PodmanCompose),
                    Diagnostic::info(
                        "MM_RUNTIME_NOT_CHECKED",
                        "runtime checks are blocked until selection is resolved",
                    ),
                ),
                validation: ValidationReport::default(),
                ingress: Vec::new(),
                update: None,
            },
        }
    }

    pub fn finish(mut self) -> DoctorReport {
        self.doctor.readiness = Some(self.readiness);
        self.doctor
    }

    pub fn record<T>(&mut self, name: &str, result: Result<T, MmError>) -> Option<T> {
        match result {
            Ok(value) => {
                self.check(name, CheckStatus::Passed, "ready");
                Some(value)
            }
            Err(error) => {
                let mut diagnostic = Diagnostic::error(error.code(), error.to_string());
                let issue = match &error {
                    MmError::Setup { issue } => {
                        diagnostic.code = issue.diagnostic_code().to_owned();
                        diagnostic = diagnostic.with_help(issue.help());
                        Some(issue.clone())
                    }
                    _ => None,
                };
                self.doctor.validation.push(diagnostic);
                self.readiness.checks.push(ReadinessCheck {
                    name: name.to_owned(),
                    status: CheckStatus::Failed,
                    message: error.to_string(),
                    issue,
                    blocked_by: Vec::new(),
                });
                None
            }
        }
    }

    pub fn diagnostics(&mut self, name: &str, diagnostics: Vec<Diagnostic>) {
        let failed = diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error);
        self.check(
            name,
            if failed {
                CheckStatus::Failed
            } else {
                CheckStatus::Passed
            },
            if failed {
                crate::domain::readiness::DIAGNOSTICS_MESSAGE
            } else {
                "ready"
            },
        );
        self.doctor.validation.diagnostics.extend(diagnostics);
    }

    pub fn check(&mut self, name: &str, status: CheckStatus, message: &str) {
        // A probe's successful execution and its health are one check.
        self.readiness.checks.retain(|check| check.name != name);
        self.readiness.checks.push(ReadinessCheck {
            name: name.to_owned(),
            status,
            message: message.to_owned(),
            blocked_by: Vec::new(),
            issue: None,
        });
    }

    pub fn blocked(&mut self, name: &str, dependencies: &[&str]) {
        let dependencies = dependencies
            .iter()
            .filter(|name| !self.passed(name))
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        self.readiness.checks.push(ReadinessCheck {
            name: name.to_owned(),
            status: CheckStatus::Blocked,
            message: format!("waiting for {}", dependencies.join(", ")),
            blocked_by: dependencies,
            issue: None,
        });
    }

    pub fn passed(&self, name: &str) -> bool {
        self.readiness
            .checks
            .iter()
            .any(|check| check.name == name && check.status == CheckStatus::Passed)
    }

    pub fn has_issue(&self, predicate: impl Fn(&SetupIssue) -> bool) -> bool {
        self.readiness
            .checks
            .iter()
            .filter_map(|check| check.issue.as_ref())
            .any(predicate)
    }
}
