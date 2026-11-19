use std::path::PathBuf;

use clap::Subcommand;

use super::{
    AccessCommand, BuildCommand, ConfigCommand, DeployCommand, DevCommand, EnvCommand,
    GenerateCommand, JobCommand, McpCommand, SecretCommand, ServiceCommand, TargetCommand,
};

/// Canonical public command taxonomy. Product families are the only visible roots.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Initialize a project through a reviewed plan or interactive onboarding.
    #[command(
        after_help = "Examples:\n  mm init --name example\n  mm init --name example --plan HASH --yes\n\nWith an interactive terminal and no arguments, onboarding opens without mutating files before review. Noninteractive use prints a deterministic review. Apply requires the exact plan hash and --yes. Exit codes: 0 success; 1 stale/local state; 2 invalid root/configuration; 3 security failure."
    )]
    Init {
        /// Override the project name inferred from the directory.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
        /// Exact hash printed by the read-only initialization review.
        #[arg(long, value_name = "HASH", requires = "yes")]
        plan: Option<String>,
        /// Apply the reviewed configuration, Ansible starter, and local-override ignore rule.
        #[arg(long, requires = "plan")]
        yes: bool,
    },

    /// Set up and operate the local development environment.
    Dev {
        #[command(subcommand)]
        command: Option<DevCommand>,
    },

    /// Validate and edit desired project configuration through typed reviews.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },

    /// Check configuration health and resolved runtime capabilities.
    #[command(
        after_help = "Examples:\n  mm doctor --output json\n  mm doctor recover-state\n\nInspection is read-only. State recovery applies only with an exact reviewed hash and --yes. Exit codes: 0 healthy; 1 state/I/O failure; 2 invalid configuration; 3 unavailable runtime or capability."
    )]
    Doctor {
        #[command(subcommand)]
        command: Option<super::state::DoctorCommand>,
        /// Fetch the stable release manifest and compare the installed version.
        #[arg(long)]
        check_updates: bool,
    },

    /// Start or stop this workstation's global public HTTP/HTTPS forwarding.
    #[command(
        after_help = "Examples:\n  mm ingress stop\n  mm ingress start\n\nGlobal to this workstation; no project is required. Start ensures the required host helper, TLS, and shared ingress are installed, offering reviewed installation or repair when necessary. Stop only stops public forwarding. Interactive use requests sudo authorization when needed; JSON/noninteractive installation requires --yes and never prompts. DNS is configured by install.sh workstation setup."
    )]
    Ingress {
        #[command(subcommand)]
        command: super::ingress::IngressCommand,
    },

    /// Inspect and operate deployed services.
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },

    /// Run finite jobs and operate their schedules.
    Job {
        #[command(subcommand)]
        command: JobCommand,
    },

    /// Inspect and deliver non-secret environment values.
    Env {
        #[command(subcommand)]
        command: EnvCommand,
    },

    /// Manage secret values, rotation, and provider instances.
    Secret {
        #[command(subcommand)]
        command: SecretCommand,
    },

    /// Manage application access policies, users, routes, and sessions.
    Access {
        #[command(subcommand)]
        command: AccessCommand,
    },

    /// Plan and execute deterministic OCI image builds.
    #[command(
        after_help = "Examples:\n  mm build plan\n  mm build apply --plan HASH --yes\n  mm -e production build --publish plan\n\nWithout --publish, an environment whose target is this machine gets its images loaded into the local container engine, where `mm deploy` and `mm dev up` run them, and nothing is pushed. Remote environments run published images."
    )]
    Build {
        /// Push a multi-platform image index and reviewed tags to the registry.
        #[arg(long, global = true)]
        publish: bool,
        /// Build every module that has a build, not only the ones the
        /// selected environment runs and the bases they build FROM.
        #[arg(long, global = true)]
        all: bool,
        /// Skip the build confirmation prompt.
        #[arg(long, global = true)]
        yes: bool,
        #[command(subcommand)]
        command: Option<BuildCommand>,
    },

    /// Plan and apply the selected environment to its target.
    #[command(
        after_help = "Examples:\n  mm deploy plan\n  mm deploy apply --plan HASH --yes\n  mm deploy plan --nodes app-1,app-2\n  mm deploy status\n  mm deploy rollback"
    )]
    Deploy {
        /// Skip the deployment confirmation prompt.
        #[arg(long, global = true)]
        yes: bool,
        /// On a target that lists nodes, plan only these nodes. The others
        /// keep what they run now, and the plan says so.
        #[arg(long, global = true, value_name = "NODE,...", value_delimiter = ',')]
        nodes: Vec<String>,
        /// The release's name, which templates read as MM.RELEASE.
        /// Defaults to sha- and the commit's first 12 characters.
        #[arg(long, global = true, value_name = "NAME")]
        release: Option<String>,
        /// A value of this release that `release-inputs:` declares, read
        /// as RELEASE.<NAME>. Repeat it for each input.
        #[arg(long = "release-input", global = true, value_name = "NAME=VALUE")]
        release_inputs: Vec<String>,
        #[command(subcommand)]
        command: Option<DeployCommand>,
    },

    /// Show who may reach remote environments, join, and sync access.
    #[command(
        after_help = "Examples:\n  mm key create\n  mm team join --request staging=deploy\n  mm --environment production team whoami\n  mm --environment staging team sync"
    )]
    Team {
        #[command(subcommand)]
        command: super::TeamCommand,
    },

    /// Reach a remote service's port on localhost; save forwards for reuse.
    #[command(
        after_help = "Examples:\n  mm --environment staging forward postgres\n  mm --environment staging forward postgres:5432 --local 15432\n  mm --environment staging forward save db postgres --local 15432 --auto\n  mm forward up"
    )]
    Forward(super::ForwardArgs),

    /// Create, show, protect, replace, or remove this device's SSH keys
    /// for remote environments.
    #[command(
        after_help = "Examples:\n  mm key create\n  mm key show\n  mm key protect\n  mm key rotate\n\nmm keeps each key encrypted under a passphrase it generates and stores in the system keychain, and is the only program that uses the key: never pass the private key file to ssh, scp or git."
    )]
    Key {
        #[command(subcommand)]
        command: super::KeyCommand,
    },

    /// Manage targets, private selection, setup, ingress, and remote access.
    Target {
        #[command(subcommand)]
        command: TargetCommand,
    },

    /// Show and trust what AI agents may do, and connect coding tools.
    #[command(
        after_help = "Examples:\n  mm agent status\n  mm agent trust\n  mm agent setup --client claude-code --yes\n\nAgents work through `mm mcp serve`, limited per environment by the project's `agents:` block. mm refuses to run directly for an agent."
    )]
    Agent {
        #[command(subcommand)]
        command: super::AgentCommand,
    },

    /// List the commands run in this project on this machine, and show what
    /// one printed.
    #[command(
        after_help = "Examples:\n  mm history\n  mm history --outcome failed --since 2d\n  mm -e production history\n  mm history show 20261115T201530123Z-4f2a\n\nRecords stay on this machine, under the mm home. Secret values are never recorded."
    )]
    History(super::HistoryArgs),

    /// List the project folders mm knows on this machine, and forget the
    /// ones that are gone.
    #[command(
        after_help = "Examples:\n  mm workspace list\n  mm workspace forget --stale\n  mm workspace forget --stale --plan HASH --yes\n\nmm registers a project folder the first time it runs there. Forgetting one drops its operation history and `mm env set` values on this machine; no file is deleted."
    )]
    Workspace {
        #[command(subcommand)]
        command: super::WorkspaceCommand,
    },

    /// Update this machine's mm, and the mmd and mm-ingress-helper beside
    /// it, from a verified release.
    #[command(
        disable_version_flag = true,
        after_help = "Examples:\n  mm update --check\n  mm update\n  mm update --yes\n  mm update --version v0.1.2 --force\n\nmm update downloads the release's archive for this platform, checks its SHA-256 and size against the release's manifest.json (SHA256SUMS for a release candidate), runs the new mm, and then replaces every binary at once or none. It replaces mm, and mmd and mm-ingress-helper when they sit beside it. Hosts keep the release they run: upgrade them with `mm target setup plan --upgrade`. Exit codes: 0 updated or nothing to do; 1 the release could not be read or installed, or this installation is refused; 3 the archive failed its checks."
    )]
    Update(super::UpdateArgs),

    /// Serve the Model Context Protocol server that AI agents use.
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },

    /// Generate maintained artifacts from the canonical Clap tree.
    #[command(name = "__generate", hide = true)]
    Generate {
        #[command(subcommand)]
        command: GenerateCommand,
    },

    /// Run the SSH forced-command relay. It exposes framed mm operations only.
    #[command(hide = true)]
    Relay {
        #[command(subcommand)]
        command: super::RelayCommand,
    },

    #[command(name = "__dashboard-schema", hide = true)]
    DashboardSchema,

    #[command(name = "__install-ingress", hide = true)]
    InstallIngress {
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long)]
        target_id: String,
        #[arg(long)]
        artifact_hash: String,
    },

    #[command(name = "__uninstall-ingress", hide = true)]
    UninstallIngress {
        #[arg(long)]
        target_id: String,
        #[arg(long)]
        artifact_hash: String,
    },
}
