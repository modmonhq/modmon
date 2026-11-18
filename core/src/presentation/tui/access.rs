//! What the Access view knows about you and the team.

/// What the Access pane shows: where the environment runs, this device's key,
/// what the host enforces for you, and the team in the users file.
#[derive(Debug, Clone, Default)]
pub struct AccessView {
    /// The remote target's name, or `None` for a local environment.
    pub remote_target: Option<String>,
    /// This device's mm key fingerprint, if it has one.
    pub device_key: Option<String>,
    pub identity: Option<crate::domain::remote::RemoteIdentity>,
    /// Why the host could not say who you are.
    pub identity_error: Option<String>,
    pub team: Vec<TeamMemberView>,
    /// Saved forwards for this project, ready to print.
    pub forwards: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamMemberView {
    pub name: String,
    /// The level (and add-ons) this user has in the selected environment.
    pub access: Option<String>,
    pub keys: usize,
    pub machine: bool,
    /// One of this user's keys is this device's key.
    pub you: bool,
}
