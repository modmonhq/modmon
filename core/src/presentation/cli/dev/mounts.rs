use super::*;

#[derive(Debug, Clone, Subcommand)]
pub enum DevMountCommand {
    /// Produce a deterministic mount proposal with discovery evidence.
    Plan(DevMountOptions),
    /// Persist the exact reviewed proposal in the project config.
    Configure {
        #[command(flatten)]
        options: DevMountOptions,
        /// Hash printed by `mm dev mounts plan` with identical options.
        #[arg(long, value_name = "HASH")]
        plan: String,
    },
    /// Remove a configured mount after reviewing its current plan.
    Disable {
        #[arg(value_name = "MODULE")]
        module: String,
        /// Hash printed by `mm dev mounts plan MODULE`.
        #[arg(long, value_name = "HASH")]
        plan: String,
    },
}

#[derive(Debug, Clone, Args)]
pub struct DevMountOptions {
    #[arg(value_name = "MODULE")]
    module: String,
    /// Override the module-relative portable source proposed by discovery.
    #[arg(long, value_name = "PATH")]
    source: Option<String>,
    /// Override the absolute container target proposed by discovery.
    #[arg(long, value_name = "PATH")]
    mount_target: Option<String>,
    /// Select a service explicitly; repeat for multiple services.
    #[arg(long, value_name = "SERVICE")]
    service: Vec<String>,
    /// Use a framework preservation preset explicitly.
    #[arg(long, value_enum)]
    preset: Option<DevelopmentMountPresetArg>,
    /// Mount source read-only instead of read-write.
    #[arg(long)]
    read_only: bool,
    /// Explicitly approve one otherwise-sensitive module-relative path.
    #[arg(long, value_name = "PATH")]
    safe_exception: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum DevelopmentMountPresetArg {
    Laravel,
    Node,
    Dotnet,
    Rust,
}

impl From<DevelopmentMountPresetArg> for crate::domain::development_mount::DevelopmentMountPreset {
    fn from(value: DevelopmentMountPresetArg) -> Self {
        match value {
            DevelopmentMountPresetArg::Laravel => Self::Laravel,
            DevelopmentMountPresetArg::Node => Self::Node,
            DevelopmentMountPresetArg::Dotnet => Self::Dotnet,
            DevelopmentMountPresetArg::Rust => Self::Rust,
        }
    }
}

impl DevMountOptions {
    pub(super) fn request(&self) -> crate::app::DevelopmentMountPlanRequest {
        crate::app::DevelopmentMountPlanRequest {
            module: self.module.clone(),
            source: self.source.clone(),
            target: self.mount_target.clone(),
            services: self.service.clone(),
            mode: if self.read_only {
                crate::domain::development_mount::DevelopmentMountMode::ReadOnly
            } else {
                crate::domain::development_mount::DevelopmentMountMode::ReadWrite
            },
            preset: self.preset.map(Into::into),
            safe_exceptions: self.safe_exception.clone(),
        }
    }
}

#[cfg(test)]
mod mount_tests {
    use super::*;

    #[test]
    fn container_mount_target_is_independent_of_runtime_target() -> Result<(), clap::Error> {
        for action in ["plan", "configure"] {
            let mut args = vec![
                "mm",
                "--target",
                "local",
                "dev",
                "mounts",
                action,
                "app",
                "--mount-target",
                "/workspace",
            ];
            if action == "configure" {
                args.extend(["--plan", "reviewed-hash"]);
            }
            let cli = Cli::try_parse_from(args)?;
            assert_eq!(cli.target.as_deref(), Some("local"));
            match cli.command {
                Some(Command::Dev {
                    command: Some(DevCommand::Mounts { command }),
                }) => {
                    let options = match command {
                        DevMountCommand::Plan(options)
                        | DevMountCommand::Configure { options, .. } => options,
                        DevMountCommand::Disable { .. } => unreachable!(),
                    };
                    assert_eq!(options.request().target.as_deref(), Some("/workspace"));
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }
}
