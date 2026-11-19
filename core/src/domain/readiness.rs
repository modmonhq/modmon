use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::runtime::{RuntimeKind, RuntimeProbe};

/// Recoverable prerequisites. Kept separate from infrastructure error strings so
/// every presentation can offer the same actions without parsing messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetupIssue {
    #[error("local state at {path} uses an unsupported pre-baseline SQLite schema")]
    LegacyState { path: PathBuf },
    #[error("target '{name}' is not configured in {path}")]
    MissingTarget { name: String, path: PathBuf },
    #[error("no runtime was selected for this project")]
    MissingRuntime,
    #[error("services need reviewed image digests: {}", services.join(", "))]
    MissingImages { services: Vec<String> },
    #[error("shared development-services network '{network}' is missing")]
    MissingDevelopmentServices { network: String },
    #[error("local HTTPS needs an installed and trusted mm certificate authority")]
    LocalTlsRequired,
}

impl SetupIssue {
    pub const fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::LegacyState { .. } => "MM_STATE_LEGACY",
            Self::MissingTarget { .. } => "MM_TARGET_MISSING",
            Self::MissingRuntime => "MM_RUNTIME_REQUIRED",
            Self::MissingImages { .. } => "MM_BUILD_REQUIRED",
            Self::MissingDevelopmentServices { .. } => "MM_SERVICES_REQUIRED",
            Self::LocalTlsRequired => "MM_TLS_SETUP_REQUIRED",
        }
    }

    pub const fn code(&self) -> &'static str {
        match self {
            Self::LegacyState { .. } => "MM_STATE",
            Self::MissingTarget { .. } => "MM_TARGET",
            Self::MissingRuntime => "MM_ENVIRONMENT",
            Self::MissingImages { .. } => "MM_BUILD",
            Self::MissingDevelopmentServices { .. } => "MM_ENVIRONMENT",
            Self::LocalTlsRequired => "MM_ENVIRONMENT",
        }
    }

    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::LegacyState { .. } => 1,
            _ => 2,
        }
    }

    pub fn help(&self) -> &'static str {
        match self {
            Self::LocalTlsRequired => {
                "Run `mm dev setup` to install and trust the local CA. Linux trust may prompt for sudo. Project certificates for {service}.{project-name}.test are generated during deployment; no domains entries are needed for automatic routes."
            }
            Self::MissingDevelopmentServices { .. } => {
                "This project uses mm's shared development mail services. Run `mm dev services plan`, then `mm dev services apply --plan HASH` with the reviewed hash before deploying. To opt out of mail injection, set development-mail.enabled: false in mm.yaml."
            }
            Self::LegacyState { .. } => {
                "Run `mm doctor recover-state` to review recovery. It backs up the database before resetting private preferences, history, and resource ownership records. No containers or project files are deleted."
            }
            Self::MissingTarget { name, .. } if name == "local" => {
                "Run `mm target local` to register this machine. Existing targets are preserved."
            }
            Self::MissingTarget { .. } => {
                "Run `mm target list` to see configured targets, then select one with --target NAME. Use `mm target add --help` to register an SSH target."
            }
            Self::MissingRuntime => {
                "Run `mm` for guided runtime selection, or `mm target preference set-runtime RUNTIME` to save a private preference. Runtimes: docker-compose, podman-compose. Use --runtime RUNTIME for one invocation."
            }
            Self::MissingImages { .. } => {
                "Run `mm build` to review and run the build plan: for an environment on this machine it loads the images into the local engine and pushes nothing, and `mm dev up` does it for you when the images are missing or out of date. A remote environment runs published images: build them with `mm build --publish`. Project configuration can still be checked with `mm config validate`."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StateRecoveryPlan {
    pub database: PathBuf,
    pub plan_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReadinessCheck {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_by: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<SetupIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeCandidate {
    pub runtime: RuntimeKind,
    pub probe: RuntimeProbe,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReadinessReport {
    pub checks: Vec<ReadinessCheck>,
    pub candidates: Vec<RuntimeCandidate>,
}

impl ReadinessReport {
    pub fn is_ready(&self) -> bool {
        self.checks
            .iter()
            .all(|check| check.status == CheckStatus::Passed)
    }
}

/// One thing to fix before an environment can start, in plain language. Every
/// interface shows these the same way, so the wording lives here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupStep {
    /// The readiness check that failed.
    pub check: String,
    pub title: String,
    pub detail: String,
    /// The typed prerequisite, when a guided command fixes it.
    pub issue: Option<SetupIssue>,
    /// False while an earlier step has to be finished first.
    pub ready: bool,
}

/// How many service names a step lists before it summarizes the rest.
const LISTED_SERVICES: usize = 3;

impl SetupIssue {
    /// Whether mm has a guided fix. A target on another host has to be
    /// registered by hand, with its address and keys.
    pub fn is_guided(&self) -> bool {
        !matches!(self, Self::MissingTarget { name, .. } if name != "local")
    }

    /// What to do, as a short imperative.
    pub fn title(&self) -> String {
        match self {
            Self::LegacyState { .. } => "Recover mm's local state".to_owned(),
            Self::MissingTarget { name, .. } if name == "local" => {
                "Register this machine".to_owned()
            }
            Self::MissingTarget { name, .. } => format!("Add the target '{name}'"),
            Self::MissingRuntime => "Choose a container runtime".to_owned(),
            Self::MissingImages { .. } => "Build the images".to_owned(),
            Self::MissingDevelopmentServices { .. } => {
                "Install the shared development services".to_owned()
            }
            Self::LocalTlsRequired => "Set up local HTTPS".to_owned(),
        }
    }

    /// Why the step is needed and what it changes, in one or two sentences.
    pub fn summary(&self) -> String {
        match self {
            Self::LegacyState { .. } => {
                "The state database is from an older mm. Recovery backs it up first and deletes no containers or project files.".to_owned()
            }
            Self::MissingTarget { name, .. } if name == "local" => {
                "mm has no record of this machine as a place to run environments.".to_owned()
            }
            Self::MissingTarget { name, .. } => format!(
                "This environment runs on '{name}', which is not registered on this machine."
            ),
            Self::MissingRuntime => {
                "No runtime is chosen for this environment. Pick one and mm remembers it."
                    .to_owned()
            }
            Self::MissingImages { services } => {
                let listed = services
                    .iter()
                    .take(LISTED_SERVICES)
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ");
                let rest = services.len().saturating_sub(LISTED_SERVICES);
                let names = if rest == 0 {
                    listed
                } else {
                    format!("{listed} and {rest} more")
                };
                let count = if services.len() == 1 {
                    "1 service has".to_owned()
                } else {
                    format!("{} services have", services.len())
                };
                format!(
                    "{count} no built image yet: {names}. Building publishes to the configured registry; mm asks first."
                )
            }
            Self::MissingDevelopmentServices { .. } => {
                "This project sends mail to mm's local mail catcher, which is not installed on this machine.".to_owned()
            }
            Self::LocalTlsRequired => {
                "Install and trust mm's certificate authority so local https addresses open without warnings. Linux may ask for sudo.".to_owned()
            }
        }
    }
}

impl SetupStep {
    /// The order to fix steps in: later ones depend on earlier ones.
    fn rank(&self) -> u8 {
        match (&self.issue, self.check.as_str()) {
            (Some(SetupIssue::LegacyState { .. }), _) | (None, "state") => 0,
            (Some(SetupIssue::MissingTarget { .. }), _)
            | (None, "targets" | "target" | "target selection" | "configured target") => 1,
            (None, "environment" | "configuration") => 2,
            (Some(SetupIssue::MissingRuntime), _) | (None, "preferences" | "selection") => 3,
            (None, "runtime") => 4,
            (Some(SetupIssue::LocalTlsRequired), _) | (None, "local TLS") => 5,
            (Some(SetupIssue::MissingImages { .. }), _) | (None, "images") => 6,
            (Some(SetupIssue::MissingDevelopmentServices { .. }), _)
            | (None, "development services") => 7,
            (None, _) => 8,
        }
    }
}

impl crate::domain::workspace::DoctorReport {
    /// What is left to fix before this environment can start, in fix order.
    /// Checks that only wait on another check are left out: they clear with it.
    pub fn setup_steps(&self) -> Vec<SetupStep> {
        let Some(readiness) = &self.readiness else {
            return Vec::new();
        };
        let mut steps = readiness
            .checks
            .iter()
            .filter(|check| check.status == CheckStatus::Failed)
            .map(|check| match &check.issue {
                Some(issue) => SetupStep {
                    check: check.name.clone(),
                    title: issue.title(),
                    detail: issue.summary(),
                    // A build needs the target and runtime it is built for.
                    ready: !matches!(issue, SetupIssue::MissingImages { .. })
                        || self.project_selected,
                    issue: Some(issue.clone()),
                },
                None => self.unguided_step(check),
            })
            .collect::<Vec<_>>();
        steps.sort_by_key(SetupStep::rank);
        steps
    }

    /// A failed check that has no guided fix: say what failed and where to look.
    fn unguided_step(&self, check: &ReadinessCheck) -> SetupStep {
        let (title, detail) = match check.name.as_str() {
            "runtime" => (
                "Start the container runtime".to_owned(),
                self.runtime
                    .diagnostics
                    .iter()
                    .find(|diagnostic| {
                        diagnostic.severity == crate::domain::diagnostic::Severity::Error
                    })
                    .map_or_else(
                        || "The selected runtime is not answering on this machine.".to_owned(),
                        |diagnostic| diagnostic.message.clone(),
                    ),
            ),
            "configuration" => (
                "Fix the project configuration".to_owned(),
                "The configuration has errors; `mm config validate` lists them.".to_owned(),
            ),
            name => (
                format!("Fix {name}"),
                // Checks made of several findings only point at the list.
                if check.message == DIAGNOSTICS_MESSAGE {
                    "The diagnostics list what was found.".to_owned()
                } else {
                    check.message.clone()
                },
            ),
        };
        SetupStep {
            check: check.name.clone(),
            title,
            detail,
            issue: None,
            ready: true,
        }
    }
}

/// The message of a failed check whose findings are separate diagnostics.
pub const DIAGNOSTICS_MESSAGE: &str = "see diagnostics below";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diagnostic::{Diagnostic, ValidationReport};
    use crate::domain::runtime::RuntimeKind;
    use crate::domain::workspace::DoctorReport;

    fn check(name: &str, status: CheckStatus, issue: Option<SetupIssue>) -> ReadinessCheck {
        ReadinessCheck {
            name: name.to_owned(),
            status,
            message: issue
                .as_ref()
                .map_or_else(|| DIAGNOSTICS_MESSAGE.to_owned(), ToString::to_string),
            blocked_by: Vec::new(),
            issue,
        }
    }

    fn report(checks: Vec<ReadinessCheck>, project_selected: bool) -> DoctorReport {
        DoctorReport {
            readiness: Some(ReadinessReport {
                checks,
                candidates: Vec::new(),
            }),
            project_name: None,
            root: None,
            target: None,
            environment: None,
            target_source: None,
            runtime_kind: None,
            runtime_source: None,
            environment_delivery: None,
            project_selected,
            runtime: RuntimeProbe::unavailable(
                RuntimeKind::DockerCompose,
                Diagnostic::error("TEST", "Cannot connect to the Docker daemon"),
            ),
            validation: ValidationReport::default(),
            ingress: Vec::new(),
            update: None,
        }
    }

    #[test]
    fn steps_are_failed_checks_in_fix_order() {
        let services = ["api/api", "docs/docs", "web/web", "web/queue", "web/cp"]
            .map(str::to_owned)
            .to_vec();
        let steps = report(
            vec![
                check("state", CheckStatus::Passed, None),
                check(
                    "development services",
                    CheckStatus::Failed,
                    Some(SetupIssue::MissingDevelopmentServices {
                        network: "mm-services-dev-1".to_owned(),
                    }),
                ),
                check(
                    "images",
                    CheckStatus::Failed,
                    Some(SetupIssue::MissingImages { services }),
                ),
                check("stacks", CheckStatus::Blocked, None),
                check("runtime", CheckStatus::Failed, None),
            ],
            true,
        )
        .setup_steps();
        assert_eq!(
            steps
                .iter()
                .map(|step| step.title.as_str())
                .collect::<Vec<_>>(),
            [
                "Start the container runtime",
                "Build the images",
                "Install the shared development services",
            ]
        );
        assert_eq!(steps[0].detail, "Cannot connect to the Docker daemon");
        assert!(steps[0].issue.is_none());
        assert!(
            steps[1].detail.starts_with(
                "5 services have no built image yet: api/api, docs/docs, web/web and 2 more."
            ),
            "{}",
            steps[1].detail
        );
        assert!(steps.iter().all(|step| step.ready));
        assert!(!steps[2].detail.contains("mm-services-dev-1"));
    }

    #[test]
    fn a_build_waits_for_the_target_and_runtime() {
        let steps = report(
            vec![
                check(
                    "images",
                    CheckStatus::Failed,
                    Some(SetupIssue::MissingImages {
                        services: vec!["api/api".to_owned()],
                    }),
                ),
                check(
                    "target",
                    CheckStatus::Failed,
                    Some(SetupIssue::MissingTarget {
                        name: "local".to_owned(),
                        path: "/mm/targets.yaml".into(),
                    }),
                ),
                check("local TLS", CheckStatus::Failed, None),
            ],
            false,
        )
        .setup_steps();
        assert_eq!(steps[0].title, "Register this machine");
        assert!(steps[0].ready);
        assert_eq!(steps[1].title, "Fix local TLS");
        assert_eq!(steps[1].detail, "The diagnostics list what was found.");
        assert_eq!(steps[2].title, "Build the images");
        assert!(
            steps[2]
                .detail
                .starts_with("1 service has no built image yet: api/api.")
        );
        assert!(!steps[2].ready);
    }

    #[test]
    fn a_ready_or_host_only_report_has_no_steps() {
        assert!(
            report(vec![check("state", CheckStatus::Passed, None)], true)
                .setup_steps()
                .is_empty()
        );
        let mut host_only = report(Vec::new(), false);
        host_only.readiness = None;
        assert!(host_only.setup_steps().is_empty());
    }
}
