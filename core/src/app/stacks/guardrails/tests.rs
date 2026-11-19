use std::collections::BTreeSet;

use super::*;

fn compose(yaml: &str) -> Result<Mapping, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(yaml)
}

fn check_compose(
    stack: &str,
    compose: &Mapping,
    project_root: &Path,
    allowed: &[GuardrailException],
) -> Vec<GuardrailFinding> {
    let mut findings = compose_findings(stack, compose, project_root);
    findings.retain(|finding| !allows(allowed, finding));
    findings
}

fn features(findings: &[GuardrailFinding]) -> BTreeSet<String> {
    findings
        .iter()
        .map(|finding| finding.feature.clone())
        .collect()
}

#[test]
fn ordinary_services_pass() -> Result<(), Box<dyn std::error::Error>> {
    let source = compose(
        "services:\n  api:\n    image: busybox\n    cap_add: [NET_BIND_SERVICE]\n    sysctls: {net.core.somaxconn: 1024}\n    security_opt: [no-new-privileges:true]\n    volumes: [data:/data]\nvolumes:\n  data: {}\n",
    )?;
    assert!(check_compose("app", &source, Path::new("/srv/project"), &[]).is_empty());
    Ok(())
}

#[test]
fn host_escapes_are_refused_unless_the_host_allows_them() -> Result<(), Box<dyn std::error::Error>>
{
    let source = compose(
        "services:\n  api:\n    image: busybox\n    privileged: true\n    cap_add: [SYS_ADMIN]\n    devices: [/dev/kmsg]\n    pid: host\n    network_mode: host\n    security_opt: [\"seccomp=unconfined\", \"apparmor:unconfined\"]\n    sysctls: [kernel.shmmax=1]\n    cgroup_parent: /\n    build: .\nvolumes:\n  root:\n    driver_opts: {type: none, o: bind, device: /}\n  shared: {external: true}\nnetworks:\n  host-net: {external: true}\nsecrets:\n  shadow: {file: /etc/shadow}\n  local: {file: ./secret.txt}\n",
    )?;
    let findings = check_compose("app", &source, Path::new("/srv/project"), &[]);
    assert_eq!(
        features(&findings),
        BTreeSet::from(
            [
                "privileged",
                "cap-add",
                "devices",
                "pid-host",
                "network-host",
                "security-opt",
                "sysctls",
                "cgroup-parent",
                "build",
                "volume-bind",
                "external-volume",
                "external-network",
                "file-outside-project",
            ]
            .map(str::to_owned)
        )
    );
    assert_eq!(
        findings
            .iter()
            .filter(|finding| finding.feature == "security-opt")
            .count(),
        2
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.subject == "app/secrets:shadow")
    );
    assert!(
        !findings
            .iter()
            .any(|finding| finding.subject == "app/secrets:local")
    );

    let allowed = check_compose(
        "app",
        &source,
        Path::new("/srv/project"),
        &[GuardrailException {
            service: "app/api".to_owned(),
            features: BTreeSet::from(["pid-host".to_owned(), "privileged".to_owned()]),
        }],
    );
    assert!(!features(&allowed).contains("pid-host"));
    assert!(!features(&allowed).contains("privileged"));
    assert!(
        features(&allowed).contains("cap-add"),
        "exceptions are per feature"
    );
    Ok(())
}
