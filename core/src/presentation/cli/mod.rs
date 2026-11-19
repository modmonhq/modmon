use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use zeroize::Zeroizing;

use crate::app::WorkspaceSelection;
use crate::app::ports::{Clock, CommandSpec, ProcessRunner};
use crate::composition::{self, RuntimeComposition};
use crate::domain::diagnostic::Severity;
use crate::domain::environment::EnvironmentStatusSnapshot;
use crate::domain::error::MmError;
use crate::domain::secret::SecretMaterial;
use crate::presentation::{http as dashboard, output, tui};

mod access;
mod agent;
mod app;
mod build;
mod config;
mod deploy;
mod dev;
mod dispatch;
mod doctor;
mod env;
mod forward;
mod history;
mod ingress;
mod init;
mod internal;
mod job;
mod key;
mod provider;
pub mod root;
mod secret;
mod service;
mod setup;
mod state;
mod store_recovery;
mod target;
mod target_cluster;
mod target_connections;
mod target_mesh;
mod target_nodes;
mod target_storage;
mod team;
mod terminal;
mod values;
mod workspace;

pub use access::*;
pub use agent::{AgentClientArg, AgentCommand};
pub use build::*;
pub use config::*;
pub use deploy::*;
pub use dev::*;
pub use env::*;
pub use forward::{ForwardArgs, ForwardCommand};
pub use history::{HistoryArgs, HistoryCommand};
pub use internal::*;
pub use job::*;
pub use key::{KeyCommand, KeyReview};
pub use provider::*;
pub use root::Command;
pub use secret::*;
pub use service::*;
pub use store_recovery::RecoveryCommand;
pub use target::*;
pub use target_mesh::TargetMeshCommand;
pub use target_storage::TargetStorageCommand;
pub use team::{TeamCommand, TeamReview};
pub use workspace::{WorkspaceCommand, WorkspaceReview};

use terminal::{
    SetupDecision, confirm, current_directory, prompt_interactive_setup, read_public_value,
    read_secret_material, resolve_cli_runtime, snapshot_exit_code,
};

#[derive(Debug, Clone, Parser)]
#[command(
    name = "mm",
    version,
    about = "Plan and apply reviewed monorepo builds and deployments.",
    long_about = "mm compiles one engine-neutral project contract into reviewed plans, then applies them through a selected runtime. Selection is resolved once from CLI flags, private preferences, environment config, project defaults, then target defaults. Missing or contradictory selection is an error. Plans, logs, and JSON never contain secret values.\n\n`--help` never probes a container engine or opens project state.",
    after_help = "Examples:\n  mm init\n  mm dev setup\n  mm config validate\n  mm service status\n  mm deploy plan\n\nSelection: --runtime and --target override private preferences and project defaults. Use `mm target preference show` to inspect provenance.",
    propagate_version = true
)]
pub struct Cli {
    /// Project root or a path inside the project.
    #[arg(long, global = true, value_name = "PATH")]
    pub root: Option<PathBuf>,

    /// Environment to operate on (overrides MM_ENVIRONMENT and default-environment).
    #[arg(
        short,
        long,
        global = true,
        env = "MM_ENVIRONMENT",
        value_name = "NAME"
    )]
    pub environment: Option<String>,

    /// Ephemeral runtime adapter for this invocation only.
    #[arg(long, global = true, value_enum, value_name = "RUNTIME")]
    pub runtime: Option<RuntimeKindArg>,

    /// Ephemeral target name for this invocation only.
    #[arg(long, global = true, value_name = "NAME")]
    pub target: Option<String>,

    /// Directory containing mm's local state and target catalog.
    #[arg(long, global = true, env = "MM_HOME", value_name = "PATH")]
    pub mm_home: Option<PathBuf>,

    /// Human text or machine-readable JSON. JSON errors stay JSON.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Human)]
    pub output: OutputFormat,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RuntimeKindArg {
    #[value(name = "podman-compose")]
    PodmanCompose,
    #[value(name = "docker-compose")]
    DockerCompose,
}

impl From<RuntimeKindArg> for crate::domain::runtime::RuntimeKind {
    fn from(value: RuntimeKindArg) -> Self {
        match value {
            RuntimeKindArg::PodmanCompose => Self::PodmanCompose,
            RuntimeKindArg::DockerCompose => Self::DockerCompose,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SecretProvider {
    Age,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}

pub use dispatch::run;
impl Cli {
    pub fn recovery_context(&self) -> crate::presentation::output::RecoveryContext {
        let mut arguments = Vec::new();
        for (flag, value) in [
            (
                "--root",
                self.root
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
            ),
            (
                "--mm-home",
                self.mm_home
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
            ),
            ("--environment", self.environment.clone()),
            ("--target", self.target.clone()),
            (
                "--runtime",
                self.runtime.and_then(|runtime| {
                    runtime
                        .to_possible_value()
                        .map(|value| value.get_name().to_owned())
                }),
            ),
        ] {
            if let Some(value) = value {
                arguments.extend([flag.to_owned(), value]);
            }
        }
        crate::presentation::output::RecoveryContext { arguments }
    }
}
