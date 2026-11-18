use std::io::Write;

use crate::domain::ca::LocalCaStatus;
use crate::domain::error::MmError;
use crate::presentation::cli::OutputFormat;

use super::{output_error, terminal_safe, write_json};

pub fn write_local_ca_status(status: &LocalCaStatus, format: OutputFormat) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(status);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    if !status.installed {
        writeln!(writer, "local CA: not installed").map_err(output_error)?;
        return Ok(());
    }
    writeln!(writer, "local CA: installed").map_err(output_error)?;
    if let Some(fingerprint) = &status.fingerprint {
        writeln!(writer, "  fingerprint: {}", terminal_safe(fingerprint)).map_err(output_error)?;
    }
    writeln!(writer, "  generation: {}", status.generation).map_err(output_error)?;
    writeln!(
        writer,
        "  private key: {}",
        if status.key_permissions_valid {
            "private"
        } else {
            "unsafe or missing"
        }
    )
    .map_err(output_error)?;
    if status.issued_hosts.is_empty() {
        writeln!(writer, "  issued hosts: none").map_err(output_error)?;
    } else {
        writeln!(
            writer,
            "  issued hosts: {}",
            terminal_safe(
                &status
                    .issued_hosts
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        )
        .map_err(output_error)?;
    }
    if let Some(trust) = &status.trust {
        if !trust.supported {
            writeln!(
                writer,
                "  trust: unsupported on {}; mm will not claim a false installed state",
                terminal_safe(&trust.platform)
            )
            .map_err(output_error)?;
        } else if trust.installed {
            writeln!(
                writer,
                "  trust: installed in {} scope ({})",
                trust.scope,
                terminal_safe(trust.entry.as_deref().unwrap_or("mm-owned"))
            )
            .map_err(output_error)?;
        } else {
            writeln!(
                writer,
                "  trust: not installed in {} scope; run `mm dev tls repair`",
                trust.scope
            )
            .map_err(output_error)?;
        }
    }
    if status.rotation_pending {
        writeln!(
            writer,
            "  rotation: trust overlap active; reapply local deployments, verify routes, then run `mm dev tls repair`"
        )
        .map_err(output_error)?;
    }
    writeln!(
        writer,
        "  note: mm never silently falls back from HTTPS to HTTP"
    )
    .map_err(output_error)?;
    Ok(())
}

pub fn write_tls_privilege_notice(system: bool, no_trust: bool) -> Result<(), MmError> {
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    if no_trust {
        writeln!(writer, "security: --no-trust keeps the private CA key local but browsers and system clients will reject mm HTTPS")
            .map_err(output_error)
    } else if cfg!(target_os = "linux") || system {
        writeln!(writer, "security: setup will request system trust privilege; only the public root is installed, while its private key remains in private mm storage")
            .map_err(output_error)
    } else {
        writeln!(writer, "security: setup will install only the public root in the current-user trust store; the private key remains in private mm storage")
            .map_err(output_error)
    }
}
