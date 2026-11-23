//! Keys a remote host checks against the values it runs: `security_opt`,
//! namespace modes, the names of volumes and networks, and the files of
//! configs and secrets.

mod daemon_host;
mod guardrail_project;
mod temp_dirs;

use daemon_host::TestResult;
use guardrail_project::{ALLOWED, REFUSED, host_policy, project, validate};

fn refused(findings: &[String]) -> Vec<String> {
    let mut lines = findings
        .iter()
        .filter_map(|line| line.strip_prefix(REFUSED))
        .map(|line| line.split(" | ").next().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    lines.sort();
    lines
}

fn sorted(lines: &[&str]) -> Vec<String> {
    let mut lines = lines
        .iter()
        .map(|line| (*line).to_owned())
        .collect::<Vec<_>>();
    lines.sort();
    lines
}

#[test]
fn only_no_new_privileges_and_the_builtin_seccomp_profile_pass() -> TestResult {
    let temporary = project(
        &["web", "worker"],
        &[(
            "compose.yaml",
            "services:\n  web:\n    image: example/web\n    security_opt: [no-new-privileges, 'no-new-privileges:true', no-new-privileges=true, seccomp=builtin]\n  worker:\n    image: example/worker\n    security_opt: [seccomp=./allow-all.json, 'label=type:spc_t', apparmor=other]\n",
        )],
    )?;
    let (_, findings) = validate(temporary.path(), &[])?;
    let only = "; a host runs only no-new-privileges, no-new-privileges:true, seccomp:builtin";
    assert_eq!(
        refused(&findings),
        sorted(&[
            &format!(
                "app/worker uses security-opt (security_opt: seccomp=./allow-all.json in compose.yaml{only})"
            ),
            &format!(
                "app/worker uses security-opt (security_opt: label=type:spc_t in compose.yaml{only})"
            ),
            &format!(
                "app/worker uses security-opt (security_opt: apparmor=other in compose.yaml{only})"
            ),
        ])
    );

    let policy = host_policy(
        temporary.path(),
        "      - service: app/worker\n        features: [security-opt]\n",
    )?;
    let (_, findings) = validate(temporary.path(), &["--host-policy", &policy])?;
    assert!(refused(&findings).is_empty(), "{findings:?}");
    assert_eq!(
        findings
            .iter()
            .filter(|line| line.starts_with(ALLOWED))
            .count(),
        3,
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn capabilities_are_compared_in_upper_case() -> TestResult {
    let temporary = project(
        &["web"],
        &[(
            "compose.yaml",
            "services:\n  web:\n    image: example/web\n    cap_add: [cap_chown, net_bind_service, cap_sys_admin]\n",
        )],
    )?;
    let (_, findings) = validate(temporary.path(), &[])?;
    assert_eq!(
        refused(&findings),
        ["app/web uses cap-add (cap_add: cap_sys_admin)"]
    );
    Ok(())
}

#[test]
fn a_namespace_mode_must_be_one_a_host_runs() -> TestResult {
    let temporary = project(
        &["web", "proxy", "probe", "mapped", "rootless", "slirp"],
        &[(
            "compose.yaml",
            "services:\n  proxy:\n    image: example/proxy\n    network_mode: bridge\n    ipc: shareable\n    pid: private\n    uts: private\n    cgroup: private\n    userns_mode: auto\n  web:\n    image: example/web\n    network_mode: service:proxy\n    ipc: none\n  probe:\n    image: example/probe\n    network_mode: ns:/proc/1/ns/net\n    uts: container:other-db-1\n  mapped:\n    image: example/web\n    userns_mode: 'auto:uidmapping=0:0:65536'\n  rootless:\n    image: example/web\n    userns_mode: 'keep-id:uid=1000'\n    network_mode: pasta\n  slirp:\n    image: example/web\n    userns_mode: nomap\n    network_mode: 'slirp4netns:allow_host_loopback=true'\n",
        )],
    )?;
    let (_, findings) = validate(temporary.path(), &[])?;
    assert_eq!(
        refused(&findings),
        sorted(&[
            "app/probe uses container-join (network_mode: ns:/proc/1/ns/net in compose.yaml is not a mode a host runs; write `bridge`, `none`, `default`, `private`, `service:<service>`, or leave `network_mode` out)",
            "app/probe uses container-join (uts: container:other-db-1 in compose.yaml is not a mode a host runs; write `private`, or leave `uts` out)",
            "app/mapped uses container-join (userns_mode: auto:uidmapping=0:0:65536 in compose.yaml is not a mode a host runs; write `auto`, `private`, or leave `userns_mode` out)",
            "app/rootless uses userns-host (userns_mode: keep-id:uid=1000 in compose.yaml keeps part of the host's namespace)",
            "app/rootless uses network-host (network_mode: pasta in compose.yaml keeps part of the host's namespace)",
            "app/slirp uses userns-host (userns_mode: nomap in compose.yaml keeps part of the host's namespace)",
            "app/slirp uses network-host (network_mode: slirp4netns:allow_host_loopback=true in compose.yaml keeps part of the host's namespace)",
        ])
    );

    let policy = host_policy(
        temporary.path(),
        "      - service: app/rootless\n        features: [userns-host, network-host]\n      - service: app/slirp\n        features: [userns-host, network-host]\n",
    )?;
    let (_, findings) = validate(temporary.path(), &["--host-policy", &policy])?;
    assert_eq!(refused(&findings).len(), 3, "{findings:?}");
    assert_eq!(
        findings
            .iter()
            .filter(|line| line.starts_with(ALLOWED))
            .count(),
        4,
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_named_volume_or_network_is_left_to_the_host_that_plans_it() -> TestResult {
    let temporary = project(
        &["web"],
        &[(
            "compose.yaml",
            "services:\n  web:\n    image: example/web\n    volumes: [data:/data, shared:/shared]\n    networks: [back, other]\nvolumes:\n  data: {}\n  shared: {name: otherproject_data}\nnetworks:\n  back: {}\n  other: {name: otherproject_default}\n",
        )],
    )?;
    let (status, findings) = validate(temporary.path(), &[])?;
    assert_eq!(status, 0, "{findings:?}");
    assert!(refused(&findings).is_empty(), "{findings:?}");
    let warned =
        "MM_REMOTE_GUARDRAIL_NAME a remote host may refuse this in environment 'staging': ";
    let mut warnings = findings
        .iter()
        .filter_map(|line| line.strip_prefix(warned))
        .map(|line| line.split(" | ").next().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    warnings.sort();
    assert_eq!(
        warnings,
        sorted(&[
            "app/volume:shared uses external-volume (name: otherproject_data in compose.yaml)",
            "app/network:other uses external-network (name: otherproject_default in compose.yaml)",
        ])
    );
    assert!(
        findings
            .iter()
            .all(|line| line.contains("Leaving `name:` out makes Compose create a new, empty one")),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_config_or_secret_file_must_stay_in_the_project() -> TestResult {
    let temporary = project(
        &["web"],
        &[(
            "compose.yaml",
            "services:\n  web:\n    image: example/web\n    secrets: [plain, linked, inside_link, escaped]\n    configs: [nested]\nsecrets:\n  plain: {file: ./secrets/plain.txt}\n  linked: {file: ./linked.txt}\n  inside_link: {file: ./elsewhere/hostname}\n  escaped: {file: ./secrets/../../outside.txt}\nconfigs:\n  nested: {file: secrets/./plain.txt}\n",
        )],
    )?;
    let root = temporary.path().join("project");
    std::fs::create_dir_all(root.join("secrets"))?;
    std::fs::write(root.join("secrets/plain.txt"), "value\n")?;
    std::fs::write(temporary.path().join("outside.txt"), "value\n")?;
    std::os::unix::fs::symlink(
        temporary.path().join("outside.txt"),
        root.join("linked.txt"),
    )?;
    std::os::unix::fs::symlink(temporary.path(), root.join("elsewhere"))?;
    let (_, findings) = validate(temporary.path(), &[])?;
    let outside = "is outside the project or behind a symlink; keep the file itself in the project";
    assert_eq!(
        refused(&findings),
        sorted(&[
            &format!(
                "app/secrets:linked uses file-outside-project (secrets.linked.file: ./linked.txt in compose.yaml {outside})"
            ),
            &format!(
                "app/secrets:inside_link uses file-outside-project (secrets.inside_link.file: ./elsewhere/hostname in compose.yaml {outside})"
            ),
            &format!(
                "app/secrets:escaped uses file-outside-project (secrets.escaped.file: ./secrets/../../outside.txt in compose.yaml {outside})"
            ),
        ])
    );
    Ok(())
}

#[test]
fn the_planning_host_runs_a_name_under_the_stacks_compose_project_only() -> TestResult {
    let temporary = project(
        &["web"],
        &[(
            "compose.yaml",
            "services:\n  web:\n    image: example/web\n    volumes: [own:/own, whole:/whole, other:/other]\nvolumes:\n  own: {name: example-mm-0123456789abcdef_own}\n  whole: {name: example-mm-0123456789abcdef}\n  other: {name: example-mm-0123456789abcdefx_data}\n",
        )],
    )?;
    let document =
        mm::adapters::config::ConfigLoader.load_from(&temporary.path().join("project"))?;
    let names = std::collections::BTreeMap::from([(
        "app".to_owned(),
        "example-mm-0123456789abcdef".to_owned(),
    )]);
    let findings = mm::app::named_resource_guardrails(&document, "staging", &names, &[])?;
    assert_eq!(
        findings.iter().map(ToString::to_string).collect::<Vec<_>>(),
        [
            "app/volume:other uses external-volume (name: example-mm-0123456789abcdefx_data in compose.yaml is outside this stack's Compose project `example-mm-0123456789abcdef`; leaving `name:` out makes Compose create a new, empty one, so to keep an existing volume's data a host admin allows external-volume for app/volume:other)"
        ]
    );

    let allowed = mm::domain::host_policy::GuardrailException {
        service: "app/volume:other".to_owned(),
        features: std::collections::BTreeSet::from(["external-volume".to_owned()]),
        paths: std::collections::BTreeSet::new(),
    };
    assert!(
        mm::app::named_resource_guardrails(&document, "staging", &names, &[allowed])?.is_empty()
    );
    assert!(
        mm::app::workload_guardrails(&document, "staging", &[])?.is_empty(),
        "names wait for the plan"
    );
    Ok(())
}
