//! The `mm target setup` command lines mm prints for the operator to run.

use crate::domain::cluster_setup::ReleaseConsent;

/// A `mm target setup plan` or `apply` command line. Plan and apply take the
/// same flags, so the apply command of a plan repeats every one it was made
/// with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupCommandLine {
    environment: String,
    selection: Vec<String>,
    flags: Vec<String>,
    release: ReleaseConsent,
}

impl SetupCommandLine {
    #[must_use]
    pub fn new(environment: &str) -> Self {
        Self {
            environment: environment.to_owned(),
            ..Self::default()
        }
    }

    /// A selection given before `target setup`, such as `--target NAME`.
    #[must_use]
    pub fn selecting(mut self, flag: &str, value: &str) -> Self {
        self.selection.extend([flag.to_owned(), shell_word(value)]);
        self
    }

    #[must_use]
    pub fn with(mut self, flag: &str, value: &str) -> Self {
        self.flags.extend([flag.to_owned(), shell_word(value)]);
        self
    }

    #[must_use]
    pub fn with_flag(mut self, flag: &str) -> Self {
        self.flags.push(flag.to_owned());
        self
    }

    /// `--upgrade` and `--downgrade` as `release` gives them.
    #[must_use]
    pub fn with_release(mut self, release: ReleaseConsent) -> Self {
        self.release = release;
        self
    }

    #[must_use]
    pub fn plan(&self) -> String {
        self.line("plan", None)
    }

    #[must_use]
    pub fn apply(&self, plan_hash: &str) -> String {
        self.line("apply", Some(plan_hash))
    }

    fn line(&self, step: &str, plan_hash: Option<&str>) -> String {
        let mut words = vec![
            "mm".to_owned(),
            "--environment".to_owned(),
            shell_word(&self.environment),
        ];
        words.extend(self.selection.iter().cloned());
        words.extend(["target", "setup", step].map(str::to_owned));
        if let Some(hash) = plan_hash {
            words.extend(["--plan".to_owned(), shell_word(hash)]);
        }
        words.extend(self.flags.iter().cloned());
        for (set, flag) in [
            (self.release.upgrade, "--upgrade"),
            (self.release.downgrade, "--downgrade"),
        ] {
            if set {
                words.push(flag.to_owned());
            }
        }
        words.join(" ")
    }
}

/// Quotes `value` for a POSIX shell when it is not a plain word.
fn shell_word(value: &str) -> String {
    if !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'/' | b'.' | b'_' | b'-' | b':' | b'@' | b'=' | b'+' | b','
                )
        })
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
