//! `environments.<environment>.deploy.mm-version`: which releases of `mm`
//! may plan deployments to an environment, and which `mmd` its hosts must
//! run. Without it any `mm` that speaks the hosts' protocol may deploy.
//!
//! The requirement is written as Cargo writes one: `0.9` is any 0.9
//! release, `=0.9.3` exactly that one, `>=0.9.2, <0.11` a range. `mm` and
//! `mmd` of one release report the same version, so one requirement pins
//! both.

use semver::{Version, VersionReq};

use crate::domain::diagnostic::Diagnostic;

/// An environment's pin, as the configuration states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionPin {
    pub environment: String,
    pub requirement: String,
}

impl VersionPin {
    /// The pin `environments.<environment>.deploy.mm-version` sets, if any.
    #[must_use]
    pub fn of(config: &crate::domain::config::MmConfig, environment: &str) -> Option<Self> {
        config
            .environments
            .get(environment)?
            .deploy
            .as_ref()?
            .mm_version
            .as_ref()
            .map(|requirement| Self {
                environment: environment.to_owned(),
                requirement: requirement.clone(),
            })
    }

    fn key(&self) -> String {
        format!("environments.{}.deploy.mm-version", self.environment)
    }

    /// Whether `version` satisfies the pin. A version that does not parse
    /// does not.
    pub fn allows(&self, version: &str) -> Result<bool, String> {
        let requirement = parse(&self.requirement)?;
        Ok(Version::parse(version).is_ok_and(|version| requirement.matches(&version)))
    }

    /// Refuses this machine's `mm` when it is not a pinned release.
    pub fn check_client(&self, version: &str) -> Result<(), String> {
        if self.allows(version)? {
            return Ok(());
        }
        Err(format!(
            "environment '{}' pins mm {} ({}), and this mm is {version}. Install a release that matches: pass `--version vMAJOR.MINOR.PATCH` (and `--with-daemon` for setup) to the installer, https://modmon.dev/install.sh, or change the pin in a reviewed commit",
            self.environment,
            self.requirement,
            self.key(),
        ))
    }

    /// The problem with a host's `mmd`, when it is not a pinned release.
    /// `host` names it for the reader: `node 'app-2'`, or the target.
    pub fn check_host(&self, host: &str, version: Option<&str>) -> Result<(), String> {
        let version = version.unwrap_or("an unknown version");
        if self.allows(version)? {
            return Ok(());
        }
        Err(format!(
            "{host} runs mmd {version}, and environment '{}' pins {} ({}). Install the pinned release on it: `mm target setup plan` with mm and mmd of that release, then apply the plan",
            self.environment,
            self.requirement,
            self.key(),
        ))
    }
}

/// Reads a requirement, or says what to write instead.
pub fn parse(requirement: &str) -> Result<VersionReq, String> {
    VersionReq::parse(requirement.trim()).map_err(|error| {
        format!(
            "'{requirement}' is not a version requirement ({error}); write a release such as 0.9 (any 0.9 release) or =0.9.3 (exactly that one)"
        )
    })
}

/// Which of two releases is newer, for the doctor's upgrade hints: `None`
/// when either does not parse or both are the same.
#[must_use]
pub fn newer(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let (left, right) = (Version::parse(left).ok()?, Version::parse(right).ok()?);
    (left != right).then(|| left.cmp(&right))
}

/// What `mm doctor` says about releases, given this `mm`'s and each host's
/// (`hosts`: how the reader knows the host, and the `mmd` it runs): this
/// `mm` or a host outside the pin (errors, as plans are refused), and the
/// hosts that run an older or a newer release than this `mm` (warnings that
/// name the upgrade).
#[must_use]
pub fn release_diagnostics(
    pin: Option<&VersionPin>,
    this_release: &str,
    hosts: &[(String, Option<String>)],
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if let Some(pin) = pin {
        if let Err(problem) = pin.check_client(this_release) {
            diagnostics.push(Diagnostic::error("MM_VERSION_PIN", problem));
        }
        for (host, version) in hosts {
            if let Err(problem) = pin.check_host(host, version.as_deref()) {
                diagnostics.push(Diagnostic::error("MM_VERSION_PIN", problem));
            }
        }
    }
    let differing = |order: std::cmp::Ordering| {
        hosts
            .iter()
            .filter_map(|(host, version)| {
                let version = version.as_deref()?;
                (newer(version, this_release)? == order).then(|| format!("{host} (mmd {version})"))
            })
            .collect::<Vec<_>>()
    };
    let older = differing(std::cmp::Ordering::Less);
    if !older.is_empty() {
        diagnostics.push(
            Diagnostic::warning(
                "MM_HOST_RELEASE_OLDER",
                format!(
                    "this mm is {this_release}, and these hosts run an older release: {}",
                    older.join(", ")
                ),
            )
            .with_help("run `mm target setup plan` and apply it: setup installs this release on one node at a time, and running containers keep running"),
        );
    }
    let newer_hosts = differing(std::cmp::Ordering::Greater);
    if !newer_hosts.is_empty() {
        diagnostics.push(
            Diagnostic::warning(
                "MM_HOST_RELEASE_NEWER",
                format!(
                    "this mm is {this_release}, and these hosts run a newer release: {}",
                    newer_hosts.join(", ")
                ),
            )
            .with_help("install the hosts' release on this machine with the installer, https://modmon.dev/install.sh, before you plan or set up again"),
        );
    }
    diagnostics
}
