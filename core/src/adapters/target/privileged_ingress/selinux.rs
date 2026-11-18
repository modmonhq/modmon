#[cfg(unix)]
fn configure_selinux(target_id: TargetId) -> Result<(), MmError> {
    if !Path::new("/sys/fs/selinux/enforce").exists() {
        return Ok(());
    }
    let semodule = selinux_tool("semodule")?;
    let semanage = selinux_tool("semanage")?;
    const POLICY_PATH: &str = "/etc/mm/ingress/mm_ingress.cil";
    atomic_replace(Path::new(POLICY_PATH), include_bytes!("selinux.cil"), 0o644)?;
    run_checked(&semodule, &["-i", POLICY_PATH])?;
    let output = Command::new(&semanage)
        .args(["port", "-l", "-C"])
        .env_clear()
        .output()
        .map_err(|source| MmError::ProcessSpawn {
            program: semanage.clone(),
            source,
        })?;
    if !output.status.success() {
        return Err(MmError::ProcessFailed {
            program: semanage,
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let mappings = String::from_utf8_lossy(&output.stdout);
    let (http, https, _) = crate::domain::target_ingress::ingress_loopback_ports(target_id);
    for port in [http, https] {
        if !has_mm_port_label(&mappings, port) {
            run_checked(
                &semanage,
                &[
                    "port",
                    "-a",
                    "-t",
                    "mm_ingress_port_t",
                    "-p",
                    "tcp",
                    &port.to_string(),
                ],
            )?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn selinux_tool(name: &str) -> Result<String, MmError> {
    ["/usr/sbin", "/usr/bin"].into_iter().map(|directory| format!("{directory}/{name}"))
        .find(|path| Path::new(path).is_file())
        .ok_or_else(|| MmError::Security { message: format!("SELinux is enabled but {name} is missing; install policycoreutils-python-utils, then retry ingress setup") })
}

#[cfg(unix)]
fn has_mm_port_label(mappings: &str, port: u16) -> bool {
    mappings.lines().any(|line| {
        let mut fields = line.split_whitespace();
        if fields.next() != Some("mm_ingress_port_t") || fields.next() != Some("tcp") {
            return false;
        }
        fields
            .collect::<Vec<_>>()
            .join(" ")
            .split(',')
            .any(|value| {
                let value = value.trim();
                if let Some((start, end)) = value.split_once('-') {
                    start
                        .trim()
                        .parse::<u16>()
                        .ok()
                        .zip(end.trim().parse::<u16>().ok())
                        .is_some_and(|(start, end)| (start..=end).contains(&port))
                } else {
                    value.parse::<u16>().ok() == Some(port)
                }
            })
    })
}
