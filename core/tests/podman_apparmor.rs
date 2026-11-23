//! The AppArmor profile setup gives rootful Podman's containers, and what
//! `mm doctor` says about the profile a node's containers run under.

use mm::domain::diagnostic::Severity;
use mm::domain::remote::{
    CONTAINER_PROFILES, ContainerAppArmor, EngineFacts, MM_CONTAINER_PROFILE,
    MM_SHARED_PID_PROFILE, NodeReport, NodeState, engine_diagnostics,
};
use mm::domain::runtime::RuntimeKind;
use mm::domain::target::{TargetTransport, bootstrap_requirements};

const PROFILE: &str = include_str!("../../packaging/apparmor/mm-containers");
const SHARED_PID: &str = include_str!("../../packaging/apparmor/mm-containers-shared-pid");
const PLAYBOOK: &str = include_str!("../src/adapters/target/ansible_bootstrap/access.yml");

fn rules(profile: &str) -> Vec<&str> {
    profile
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

#[test]
fn the_profile_is_podmans_with_signal_and_ptrace_read_between_peers() {
    let ours = rules(PROFILE);
    let podmans = [
        "profile mm-containers flags=(attach_disconnected,mediate_deleted) {",
        "network,",
        "capability,",
        "file,",
        "umount,",
        "signal (receive) peer=unconfined,",
        "signal (send,receive) peer=mm-containers,",
        "signal (receive) peer={/usr/bin/,/usr/sbin/,}runc,",
        "signal (receive) peer={/usr/bin/,/usr/sbin/,}crun*,",
        "signal (receive) peer={/usr/bin/,/usr/sbin/,}podman,",
        "deny @{PROC}/* w,",
        "deny @{PROC}/{[^1-9],[^1-9][^0-9],[^1-9s][^0-9y][^0-9s],[^1-9][^0-9][^0-9][^0-9]*}/** w,",
        "deny @{PROC}/sys/[^k]** w,",
        "deny @{PROC}/sys/kernel/{?,??,[^s][^h][^m]**} w,",
        "deny @{PROC}/sysrq-trigger rwklx,",
        "deny @{PROC}/kcore rwklx,",
        "deny mount,",
        "deny /sys/[^f]*/** wklx,",
        "deny /sys/f[^s]*/** wklx,",
        "deny /sys/fs/[^c]*/** wklx,",
        "deny /sys/fs/c[^g]*/** wklx,",
        "deny /sys/fs/cg[^r]*/** wklx,",
        "deny /sys/firmware/** rwklx,",
        "deny /sys/kernel/security/** rwklx,",
        "ptrace (trace,read) peer=mm-containers,",
    ];
    let added = ["signal (send,receive),", "ptrace (read),"];
    let mut expected = podmans.to_vec();
    expected.extend(added);
    expected.push("}");
    assert_eq!(ours, expected);
    assert!(PROFILE.contains("#include <tunables/global>"));
    assert!(PROFILE.contains("#include <abstractions/base>"));
    assert!(PROFILE.contains(&format!("profile {MM_CONTAINER_PROFILE} ")));

    let renamed = |line: &&str| line.replace(MM_CONTAINER_PROFILE, MM_SHARED_PID_PROFILE);
    let mut upstream = podmans.iter().map(renamed).collect::<Vec<_>>();
    upstream.push("}".to_owned());
    assert_eq!(rules(SHARED_PID), upstream);
    assert!(SHARED_PID.contains("#include <tunables/global>"));
}

#[test]
fn setup_loads_the_profiles_before_podman_uses_them_and_only_where_apparmor_runs()
-> Result<(), String> {
    let position = |needle: &str| {
        PLAYBOOK
            .find(needle)
            .ok_or_else(|| format!("the playbook has no {needle:?}"))
    };
    let read = position("argv: [cat, /sys/module/apparmor/parameters/enabled]")?;
    let install = position("dest: \"/etc/apparmor.d/{{ item }}\"")?;
    let load = position(
        "argv: [apparmor_parser, --replace, --write-cache, \"/etc/apparmor.d/{{ item.item }}\"]",
    )?;
    let point = position("apparmor_profile = \"mm-containers\"")?;
    let drop_ins = position("path: /etc/containers/containers.conf.d\n")?;
    assert!(drop_ins < read && read < install && install < load && load < point);
    assert!(PLAYBOOK.contains("validate: apparmor_parser --skip-kernel-load --skip-cache %s"));
    assert!(PLAYBOOK.contains("loop: \"{{ mm_access_apparmor_profiles }}\""));
    assert!(PLAYBOOK.contains(
        "item.changed or (item.item ~ ' (enforce)') not in mm_apparmor_loaded.stdout_lines"
    ));
    assert_eq!(
        PLAYBOOK
            .matches("when: (mm_apparmor.stdout | default('')) == 'Y'")
            .count(),
        3
    );
    let removal = &PLAYBOOK[point..];
    assert!(removal.contains(
        "path: /etc/containers/containers.conf.d/50-mm-apparmor.conf\n        state: absent"
    ));
    assert!(removal.contains("(mm_apparmor.stdout | default('')) != 'Y'"));
    assert!(!PLAYBOOK.contains("apparmor=unconfined") && !PLAYBOOK.contains("apparmor:unconfined"));
    Ok(())
}

#[test]
fn the_plan_lists_the_profiles_only_for_rootful_podman_with_remote_access() -> Result<(), String> {
    let local = TargetTransport::Local;
    let podman = bootstrap_requirements(&local, RuntimeKind::PodmanCompose, false, true)?;
    assert_eq!(podman.apparmor_profiles, CONTAINER_PROFILES);
    assert_eq!(
        CONTAINER_PROFILES,
        [MM_SHARED_PID_PROFILE, MM_CONTAINER_PROFILE]
    );
    for (runtime, access) in [
        (RuntimeKind::PodmanCompose, false),
        (RuntimeKind::DockerCompose, true),
    ] {
        let none = bootstrap_requirements(&local, runtime, false, access)?;
        assert!(none.apparmor_profiles.is_empty(), "{runtime:?} {access}");
    }
    Ok(())
}

#[test]
fn the_profile_podman_uses_is_the_last_one_containers_conf_sets() {
    let shipped = "[containers]\n#apparmor_profile = \"container-default\"\nuserns = \"host\"\n";
    let mm = "# Written by mm\n[containers]\napparmor_profile = \"mm-containers\"\n";
    let engine = "[engine]\napparmor_profile = \"elsewhere\"\n";
    let later = "[containers]\napparmor_profile = 'custom' # an operator's\n";
    assert_eq!(ContainerAppArmor::configured_profile([shipped]), "");
    assert_eq!(
        ContainerAppArmor::configured_profile([shipped, mm, engine]),
        "mm-containers"
    );
    assert_eq!(
        ContainerAppArmor::configured_profile([shipped, mm, later]),
        "custom"
    );

    assert_eq!(
        ContainerAppArmor::configured_profile([
            "[containers] # Podman's\napparmor_profile = \"a#b\" # x\n"
        ]),
        "a#b"
    );

    let profiles = "crun (unconfined)\nmm-containers (enforce)\nmm-containers-old (enforce)\n";
    assert!(ContainerAppArmor::is_loaded(profiles, "mm-containers"));
    assert!(ContainerAppArmor::is_loaded(profiles, "crun"));
    assert!(!ContainerAppArmor::is_loaded(profiles, "mm"));
    assert!(!ContainerAppArmor::is_loaded("", "mm-containers"));
}

#[test]
fn doctor_reads_the_files_podman_reads_in_the_order_it_reads_them() {
    let present = [
        "/usr/share/containers/containers.conf",
        "/etc/containers/containers.conf",
    ];
    let exists = |path: &str| present.contains(&path);
    let listing = |directory: &str| -> Vec<String> {
        match directory {
            "/usr/share/containers/containers.conf.d" => vec![
                "10-vendor.conf".to_owned(),
                "50-mm-apparmor.conf".to_owned(),
            ],
            "/usr/share/containers/containers.rootful.conf.d" => vec!["20-rootful.conf".to_owned()],
            "/etc/containers/containers.conf.d" => {
                vec!["50-mm-apparmor.conf".to_owned(), "60-own.conf".to_owned()]
            }
            "/etc/containers/containers.rootful.conf.d" => vec!["60-own.conf".to_owned()],
            _ => Vec::new(),
        }
    };
    let podman_5 = [
        "/usr/share/containers/containers.conf",
        "/etc/containers/containers.conf",
        "/etc/containers/containers.conf.d/50-mm-apparmor.conf",
        "/etc/containers/containers.conf.d/60-own.conf",
    ];
    for release in [None, Some((5, 0)), Some((5, 7)), Some((5, 8))] {
        assert_eq!(
            ContainerAppArmor::podman_config_files(release, &exists, &listing),
            podman_5,
            "{release:?}"
        );
    }
    assert_eq!(
        ContainerAppArmor::podman_config_files(Some((6, 2)), &exists, &listing),
        [
            "/etc/containers/containers.conf",
            "/usr/share/containers/containers.conf.d/10-vendor.conf",
            "/usr/share/containers/containers.rootful.conf.d/20-rootful.conf",
            "/etc/containers/containers.conf.d/50-mm-apparmor.conf",
            "/etc/containers/containers.rootful.conf.d/60-own.conf",
        ]
    );
}

fn node(apparmor: Option<ContainerAppArmor>) -> NodeReport {
    NodeReport {
        name: "app-1".to_owned(),
        host: "203.0.113.11".to_owned(),
        port: 22,
        mesh_address: None,
        state: NodeState::Ready,
        reachable: true,
        problem: None,
        mmd_version: Some("0.1.0".to_owned()),
        deployed: None,
        rollout: None,
        engine: Some(EngineFacts {
            runtime: RuntimeKind::PodmanCompose,
            version: Some("podman version 5.7.0".to_owned()),
            compose: Some("docker-compose".to_owned()),
            compose_version: Some("Docker Compose version 2.37.1".to_owned()),
            problem: None,
            apparmor,
        }),
        daemon: None,
        secret_store: None,
    }
}

fn profile(name: &str, loaded: bool) -> Option<ContainerAppArmor> {
    Some(ContainerAppArmor {
        profile: name.to_owned(),
        loaded,
        shared_pid_loaded: (name == MM_CONTAINER_PROFILE).then_some(true),
    })
}

#[test]
fn doctor_warns_about_podmans_default_profile_and_a_profile_that_is_not_loaded() {
    for fine in [
        None,
        profile(MM_CONTAINER_PROFILE, true),
        profile("unconfined", false),
        profile("custom", true),
    ] {
        assert!(
            engine_diagnostics(&[node(fine.clone())]).is_empty(),
            "{fine:?}"
        );
    }

    for default in [
        profile("", false),
        profile("containers-default-0.66.0", true),
    ] {
        let found = engine_diagnostics(&[node(default)]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].code, "MM_PODMAN_APPARMOR");
        assert_eq!(found[0].severity, Severity::Warning);
        assert!(
            found[0]
                .message
                .contains("containers on 'app-1' run under Podman's default AppArmor profile")
                && found[0].message.contains("cannot signal each other"),
            "{found:?}"
        );
        assert!(
            found[0]
                .help
                .as_deref()
                .is_some_and(|help| help.contains("mm target setup plan")
                    && help.contains("/etc/apparmor.d/mm-containers")),
            "{found:?}"
        );
    }

    let found = engine_diagnostics(&[node(profile(MM_CONTAINER_PROFILE, false))]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].severity, Severity::Error);
    assert!(found[0].message.contains("has not loaded"), "{found:?}");
    assert!(
        found[0].help.as_deref().is_some_and(
            |help| help.contains("apparmor_parser --replace /etc/apparmor.d/mm-containers")
        ),
        "{found:?}"
    );
}

#[test]
fn doctor_warns_when_the_shared_pid_profile_is_not_loaded() {
    let missing = Some(ContainerAppArmor {
        profile: MM_CONTAINER_PROFILE.to_owned(),
        loaded: true,
        shared_pid_loaded: Some(false),
    });
    let found = engine_diagnostics(&[node(missing)]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code, "MM_PODMAN_APPARMOR");
    assert_eq!(found[0].severity, Severity::Warning);
    assert!(
        found[0]
            .message
            .contains("'app-1' has not loaded the AppArmor profile mm-containers-shared-pid"),
        "{found:?}"
    );
    assert!(
        found[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("mm target setup plan")),
        "{found:?}"
    );
    let earlier = Some(ContainerAppArmor {
        profile: MM_CONTAINER_PROFILE.to_owned(),
        loaded: true,
        shared_pid_loaded: None,
    });
    assert!(engine_diagnostics(&[node(earlier)]).is_empty());
}

#[test]
fn a_node_from_an_earlier_mmd_says_nothing_about_apparmor() -> Result<(), serde_json::Error> {
    let earlier = r#"{"runtime":"podman-compose","version":"podman version 5.4.2"}"#;
    let facts: EngineFacts = serde_json::from_str(earlier)?;
    assert_eq!(facts.apparmor, None);
    assert!(!serde_json::to_string(&facts)?.contains("apparmor"));
    Ok(())
}
