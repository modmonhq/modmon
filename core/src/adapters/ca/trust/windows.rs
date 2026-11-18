use std::path::Path;

use base64::Engine;

use crate::app::ports::CommandSpec;
use crate::domain::ca::{CertificateFingerprint, TrustScope};
use crate::domain::error::MmError;

use super::{BackendStatus, PlatformTrustStore, write_private};

const POWERSHELL: &str = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
const SCRIPT: &str = r#"param([ValidateSet('status','install','remove','secure-keys','check-keys')]$Action,[string]$Fingerprint,[string]$Certificate,[ValidateSet('CurrentUser','LocalMachine')]$Scope)
$ErrorActionPreference='Stop'
$caRoot=Join-Path (Split-Path $PSScriptRoot -Parent) 'ca'
$keys=@(Get-ChildItem -LiteralPath $caRoot -Filter '*.key' -File -Recurse)
if ($Action -eq 'secure-keys') {
  $sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User
  foreach ($key in $keys) {
    $acl=[System.Security.AccessControl.FileSecurity]::new()
    $acl.SetOwner($sid)
    $acl.SetAccessRuleProtection($true,$false)
    $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','Allow'))
    Set-Acl -LiteralPath $key.FullName -AclObject $acl
  }
}
if ($Action -eq 'secure-keys' -or $Action -eq 'check-keys') {
  $sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  if ($keys.Count -eq 0) { throw 'no private keys found' }
  foreach ($key in $keys) {
    $acl=Get-Acl -LiteralPath $key.FullName
    if ($acl.Owner -ne $sid -and $acl.Owner -ne ([System.Security.Principal.SecurityIdentifier]$sid).Translate([System.Security.Principal.NTAccount]).Value) { throw 'private key owner mismatch' }
    foreach ($rule in $acl.Access) { if ($rule.AccessControlType -eq 'Allow' -and $rule.IdentityReference.Value -ne $sid -and $rule.IdentityReference.Value -ne ([System.Security.Principal.SecurityIdentifier]$sid).Translate([System.Security.Principal.NTAccount]).Value) { throw 'private key grants another principal access' } }
  }
  'KEYS-PRIVATE'
  exit 0
}
$store=[System.Security.Cryptography.X509Certificates.X509Store]::new('Root',$Scope)
$flags=if ($Action -eq 'status') { [System.Security.Cryptography.X509Certificates.OpenFlags]::ReadOnly } else { [System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite }
$store.Open($flags)
try {
  $wanted=$Fingerprint.Replace('sha256:','').ToUpperInvariant()
  $matches=@($store.Certificates | Where-Object { $_.GetCertHashString([System.Security.Cryptography.HashAlgorithmName]::SHA256) -eq $wanted })
  $sameName=@($store.Certificates | Where-Object { $_.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName,$false) -eq 'mm local CA' })
  if ($matches.Count -eq 0 -and $sameName.Count -gt 0) { 'MM-MISMATCH'; if ($Action -eq 'install') { throw 'foreign same-name root exists' } }
  if ($Action -eq 'install' -and $matches.Count -eq 0) { $cert=[System.Security.Cryptography.X509Certificates.X509Certificate2]::new($Certificate); $store.Add($cert) }
  if ($Action -eq 'remove') { foreach ($cert in $matches) { $store.Remove($cert) } }
  foreach ($cert in @($store.Certificates | Where-Object { $_.GetCertHashString([System.Security.Cryptography.HashAlgorithmName]::SHA256) -eq $wanted })) { [Convert]::ToBase64String($cert.RawData) }
} finally { $store.Close() }
"#;

pub(super) async fn secure_private_keys(store: &PlatformTrustStore) -> Result<(), MmError> {
    let output = run_script(store, "secure-keys", "sha256:", None).await?;
    if output.success() && output.stdout.lines().any(|line| line == "KEYS-PRIVATE") {
        Ok(())
    } else {
        Err(MmError::Security {
            message: "Windows private-key ACL hardening or verification failed".to_owned(),
        })
    }
}

pub(super) async fn private_keys_valid(store: &PlatformTrustStore) -> Result<bool, MmError> {
    let output = run_script(store, "check-keys", "sha256:", None).await?;
    Ok(output.success() && output.stdout.lines().any(|line| line == "KEYS-PRIVATE"))
}

pub(super) async fn status(
    store: &PlatformTrustStore,
    fingerprint: &str,
) -> Result<BackendStatus, MmError> {
    if !Path::new(POWERSHELL).is_file() {
        return Ok(BackendStatus::unsupported());
    }
    let output = run_script(store, "status", fingerprint, None).await?;
    if !output.success() {
        return Err(MmError::Target {
            message: format!("Windows trust-store query failed (exit {:?})", output.code),
        });
    }
    let expected = crate::domain::ca::CertificateFingerprint::parse(fingerprint)
        .map_err(|message| MmError::Security { message })?;
    let installed = output.stdout.lines().any(|line| {
        base64::engine::general_purpose::STANDARD
            .decode(line.trim())
            .is_ok_and(|der| CertificateFingerprint::from_der(&der) == expected)
    });
    let mismatch = output.stdout.lines().any(|line| line == "MM-MISMATCH");
    Ok(BackendStatus {
        supported: true,
        installed,
        mismatch,
        needs_privilege: store.scope == TrustScope::System,
        entry: format!("{}\\Root", scope_name(store.scope)),
    })
}

pub(super) async fn install(
    store: &PlatformTrustStore,
    certificate: &Path,
    fingerprint: &str,
) -> Result<BackendStatus, MmError> {
    let output = run_script(store, "install", fingerprint, Some(certificate)).await?;
    if !output.success() {
        return Err(MmError::Target {
            message: format!(
                "Windows {} trust installation failed (exit {:?}); LocalMachine scope requires an elevated mm process",
                scope_name(store.scope),
                output.code
            ),
        });
    }
    status(store, fingerprint).await
}

pub(super) async fn uninstall(
    store: &PlatformTrustStore,
    fingerprint: &str,
) -> Result<(), MmError> {
    let output = run_script(store, "remove", fingerprint, None).await?;
    if !output.success() {
        return Err(MmError::Target {
            message: format!(
                "Windows exact trust removal failed (exit {:?})",
                output.code
            ),
        });
    }
    Ok(())
}

async fn run_script(
    store: &PlatformTrustStore,
    action: &str,
    fingerprint: &str,
    certificate: Option<&Path>,
) -> Result<crate::app::ports::CommandOutput, MmError> {
    store.prepare_root()?;
    let script = store.root.join("trust-store.ps1");
    write_private(&script, SCRIPT.as_bytes())?;
    let mut args = vec![
        "-NoLogo".to_owned(),
        "-NoProfile".to_owned(),
        "-NonInteractive".to_owned(),
        "-ExecutionPolicy".to_owned(),
        "Bypass".to_owned(),
        "-File".to_owned(),
        script.display().to_string(),
        "-Action".to_owned(),
        action.to_owned(),
        "-Fingerprint".to_owned(),
        fingerprint.to_owned(),
        "-Scope".to_owned(),
        scope_name(store.scope).to_owned(),
    ];
    if let Some(certificate) = certificate {
        args.extend(["-Certificate".to_owned(), certificate.display().to_string()]);
    }
    store.run(CommandSpec::new(POWERSHELL).args(args)).await
}

const fn scope_name(scope: TrustScope) -> &'static str {
    match scope {
        TrustScope::CurrentUser => "CurrentUser",
        TrustScope::System => "LocalMachine",
    }
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
