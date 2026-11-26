//! Which rollout steps a node's `mmd` knows. A step a node does not know
//! would be refused whole, so the client only plans the newer steps when
//! every node of the plan runs a release that has them.

/// The first release whose `mmd` knows the `pull` step.
pub const PULL_STEP_SINCE: (u64, u64, u64) = (0, 1, 3);

/// Whether an `mmd` of release `daemon` knows the steps of release
/// `since`. A daemon of this client's own release `client` knows them too:
/// a build between releases carries the release before.
#[must_use]
pub fn knows_steps_since(daemon: &str, client: &str, since: (u64, u64, u64)) -> bool {
    let parse = |version: &str| semver::Version::parse(version.trim().trim_start_matches('v')).ok();
    let Some(daemon) = parse(daemon) else {
        return false;
    };
    let (major, minor, patch) = since;
    daemon >= semver::Version::new(major, minor, patch) || parse(client) == Some(daemon)
}
