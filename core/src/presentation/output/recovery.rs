use crate::domain::readiness::SetupIssue;

/// Invocation scope carried into copyable commands and their JSON argv form.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RecoveryContext {
    pub arguments: Vec<String>,
}

impl RecoveryContext {
    pub fn command(&self, issue: &SetupIssue) -> Vec<String> {
        let action: &[&str] = match issue {
            SetupIssue::LegacyState { .. } => &["doctor", "recover-state"],
            SetupIssue::MissingTarget { name, .. } if name == "local" => &["target", "local"],
            SetupIssue::MissingTarget { .. } => &["target", "list"],
            SetupIssue::MissingRuntime => &[],
            SetupIssue::MissingImages { .. } => &["build", "--publish"],
            SetupIssue::MissingDevelopmentServices { .. } => &["dev", "services", "plan"],
            SetupIssue::LocalTlsRequired => &["dev", "setup"],
        };
        let mut argv = vec!["mm".to_owned()];
        argv.extend(self.arguments.clone());
        argv.extend(action.iter().map(|arg| (*arg).to_owned()));
        argv
    }

    pub fn display_command(&self, issue: &SetupIssue) -> String {
        display_command(&self.command(issue))
    }
}

pub fn display_command(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| {
            if !arg.is_empty()
                && arg
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_./:-=".contains(&byte))
            {
                arg.clone()
            } else if cfg!(windows) {
                format!("'{}'", arg.replace('\'', "''"))
            } else {
                format!("'{}'", arg.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
