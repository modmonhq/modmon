// Building one host's endpoint: where it connects, the key it pins, and
// the identity it offers.

/// How messages name a host: `node 'app-2' (host:22)`, and say so when the
/// address is this device's own.
fn endpoint_subject(label: &str, host: &str, port: u16, device_address: bool) -> String {
    let address = crate::domain::target::host_and_port(host, port);
    if device_address {
        format!("{label} ({address}, this device's address for it)")
    } else {
        format!("{label} ({address})")
    }
}

impl SshEndpoint {
    fn new(
        label: String,
        transport: &TargetTransport,
        dial: Option<(String, u16)>,
        ClientAccess { key, reuse, keys }: ClientAccess<'_>,
    ) -> Result<Self, MmError> {
        let TargetTransport::Ssh {
            host,
            port,
            user,
            host_key,
            identity_file,
            daemon_fingerprint,
            ..
        } = transport
        else {
            return Err(MmError::Target {
                message: "remote transport requires an SSH target".to_owned(),
            });
        };
        let host_key = host_key.clone().ok_or_else(|| MmError::Security {
            message:
                "SSH target has no enrolled host public key; update the target before remote use"
                    .to_owned(),
        })?;
        let host_key_alias = if *port == 22 {
            host.clone()
        } else {
            format!("[{host}]:{port}")
        };
        let device_address = dial.is_some();
        let (host, port) = dial.unwrap_or_else(|| (host.clone(), *port));
        let subject = endpoint_subject(&label, &host, port, device_address);
        let identity = key.source(identity_file.as_ref());
        // Only the user's own key file is ever kept open: not the agent
        // key, and not a key passed in the environment (`reuse.rs`).
        let kept = match (key, &identity) {
            (ClientKey::Default(_), Some(IdentitySource::File(path))) => reuse.connection(
                &reuse::Peer {
                    user,
                    host: &host,
                    port,
                    host_key: &host_key,
                },
                path,
                keys.manages(path),
            ),
            _ => None,
        };
        Ok(Self {
            subject,
            host,
            port,
            host_key_alias,
            device_address,
            user: user.clone(),
            host_key,
            identity,
            agent_key: matches!(key, ClientKey::Only(_)),
            daemon_fingerprint: daemon_fingerprint.clone().ok_or_else(|| MmError::Security {
                message: "SSH target has no enrolled daemon fingerprint; update the target before remote use"
                    .to_owned(),
            })?,
            kept,
            keys: keys.clone(),
        })
    }

    /// Names the key `ssh` offers, and the helper that answers its
    /// passphrase prompt when mm keeps the passphrase. Returns the file a
    /// key from `MM_SSH_KEY` was written to, which lives as long as the
    /// connection.
    fn identify(
        &self,
        command: &mut Command,
        askpass: Option<&Askpass>,
    ) -> Result<Option<tempfile::NamedTempFile>, MmError> {
        let inline_key = match &self.identity {
            Some(IdentitySource::File(path)) => {
                // A rotation in progress: the new key first, so a host
                // that lists it uses it, and the current key for the
                // hosts that do not yet.
                if let Some(next) = self.keys.next_key(path) {
                    command.arg("-i").arg(next);
                }
                command.arg("-i").arg(path);
                None
            }
            Some(IdentitySource::Inline(key)) => {
                let file = private_key_file(key)?;
                command.arg("-i").arg(file.path());
                Some(file)
            }
            None => None,
        };
        if let Some(askpass) = askpass {
            command.envs(askpass.environment());
        }
        Ok(inline_key)
    }
}
