//! Narrow, one-shot privilege boundary. This is not a daemon or a setuid program.
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mm::domain::target::TargetId;

#[derive(Parser)]
#[command(
    name = "mm-ingress-helper",
    version,
    about = "Apply or remove mm's fixed, reviewed public ingress integration"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(name = "__install-ingress")]
    Install {
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long)]
        target_id: uuid::Uuid,
        #[arg(long)]
        artifact_hash: String,
    },
    #[command(name = "__uninstall-ingress")]
    Uninstall {
        #[arg(long)]
        target_id: uuid::Uuid,
        #[arg(long)]
        artifact_hash: String,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Install {
            artifact_root,
            target_id,
            artifact_hash,
        } => mm::composition::install_ingress(&artifact_root, TargetId(target_id), &artifact_hash),
        Command::Uninstall {
            target_id,
            artifact_hash,
        } => mm::composition::uninstall_ingress(TargetId(target_id), &artifact_hash),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Err(write_error) = mm::presentation::output::write_error(
                &error,
                mm::presentation::cli::OutputFormat::Human,
            ) {
                eprintln!("could not write helper error: {write_error}");
            }
            ExitCode::from(error.exit_code())
        }
    }
}
