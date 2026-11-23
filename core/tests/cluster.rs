//! A three-node cluster target through the real command line and the real
//! SSH client, with nodes nobody listens on: what the user sees when nodes
//! do not answer, and how `--nodes` is checked.

use std::path::Path;

mod temp_dirs;

const HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w";
const COMMIT: &str = "3f2c9a1e5b7d4c6f8a0b2d4e6f8a1c3e5b7d9f0a";

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The minimal fixture project plus a target whose first two nodes have
/// recorded keys and sit on local ports nothing listens on, and a third node
/// that was never set up.
fn project(root: &Path) -> TestResult {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal");
    std::fs::create_dir_all(root.join(".config"))?;
    std::fs::create_dir_all(root.join("api"))?;
    for file in ["compose.yaml", "api/.env.example"] {
        std::fs::copy(source.join(file), root.join(file))?;
    }
    let mut config = std::fs::read_to_string(source.join(".config/mm.yaml"))?;
    config.push_str("targets:\n  production:\n    nodes:\n");
    for (index, name) in ["app-1", "app-2"].into_iter().enumerate() {
        config.push_str(&format!(
            "      - name: {name}\n        host: 127.0.0.1\n        port: {}\n        host-key: {HOST_KEY}\n        daemon-fingerprint: {}\n",
            index + 1,
            (index + 1).to_string().repeat(64)
        ));
    }
    config.push_str("      - {name: db-1, host: 127.0.0.2}\n");
    std::fs::write(root.join(".config/mm.yaml"), config)?;
    Ok(())
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn mm(root: &Path, home: &Path, arguments: &[&str]) -> Result<Run, Box<dyn std::error::Error>> {
    let output = temp_dirs::mm()
        .arg("--root")
        .arg(root)
        .arg("--mm-home")
        .arg(home)
        .args(["--target", "production"])
        .args(arguments)
        .output()?;
    Ok(Run {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn ssh_is_installed() -> bool {
    std::process::Command::new("ssh")
        .arg("-V")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[test]
fn a_plan_names_every_node_that_does_not_answer_and_how_to_go_on() -> TestResult {
    if !ssh_is_installed() {
        eprintln!("skipped: no ssh client on this machine");
        return Ok(());
    }
    let project_directory = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    project(project_directory.path())?;
    let (root, home) = (project_directory.path(), home.path());

    // Every node is asked; the two that cannot be reached and the one that
    // was never set up are all named, with what to run.
    let plan = mm(root, home, &["deploy", "plan", "--commit", COMMIT])?;
    assert_eq!(plan.code, Some(3), "{}{}", plan.stdout, plan.stderr);
    for expected in [
        "MM_UNREACHABLE",
        "node 'app-1' (127.0.0.1:1) did not answer",
        "node 'app-2' (127.0.0.1:2) did not answer",
        "node 'db-1' (127.0.0.2:22) has no recorded keys yet",
        "Check that the host is up",
    ] {
        assert!(
            plan.stderr.contains(expected),
            "{expected}: {}",
            plan.stderr
        );
    }

    // Leaving nodes out is the way around one that is down: the error for
    // the node that is still asked names the list to pass.
    let subset = mm(
        root,
        home,
        &[
            "deploy",
            "plan",
            "--commit",
            COMMIT,
            "--nodes",
            "app-1,app-2",
        ],
    )?;
    assert_eq!(subset.code, Some(3), "{}", subset.stderr);
    assert!(!subset.stderr.contains("db-1"), "{}", subset.stderr);
    let one = mm(
        root,
        home,
        &["team", "sync", "--commit", COMMIT, "--nodes", "app-2"],
    )?;
    assert!(
        one.stderr.contains("node 'app-2' (127.0.0.1:2)"),
        "{}",
        one.stderr
    );
    assert!(!one.stderr.contains("app-1"), "{}", one.stderr);

    let unknown = mm(
        root,
        home,
        &["deploy", "plan", "--commit", COMMIT, "--nodes", "app-9"],
    )?;
    assert_eq!(unknown.code, Some(2), "{}", unknown.stderr);
    assert!(
        unknown.stderr.contains(
            "'app-9', which is not a node of target 'production'; its nodes are app-1, app-2, db-1"
        ),
        "{}",
        unknown.stderr
    );

    // The plan hash fixes the nodes; apply does not take them again.
    let apply = mm(
        root,
        home,
        &[
            "deploy",
            "apply",
            "--plan",
            &"a".repeat(64),
            "--nodes",
            "app-1",
            "--yes",
        ],
    )?;
    assert!(
        apply.stderr.contains("a plan hash already fixes the nodes"),
        "{}",
        apply.stderr
    );
    Ok(())
}

#[test]
fn target_nodes_and_doctor_report_every_node_with_the_reason() -> TestResult {
    if !ssh_is_installed() {
        eprintln!("skipped: no ssh client on this machine");
        return Ok(());
    }
    let project_directory = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    project(project_directory.path())?;
    let (root, home) = (project_directory.path(), home.path());

    let listed = mm(root, home, &["--output", "json", "target", "nodes"])?;
    assert_eq!(
        listed.code,
        Some(3),
        "a node that is not ready fails the command"
    );
    let json: serde_json::Value = serde_json::from_str(&listed.stdout)?;
    assert_eq!(json["target"], "production");
    let nodes = json["nodes"].as_array().ok_or("nodes is not a list")?;
    assert_eq!(
        nodes
            .iter()
            .map(|node| (
                node["name"].as_str().unwrap_or_default(),
                node["state"].as_str().unwrap_or_default(),
                node["reachable"].as_bool(),
                node["mesh_address"].as_str().unwrap_or_default(),
            ))
            .collect::<Vec<_>>(),
        [
            ("app-1", "unreachable", Some(false), "10.90.0.1"),
            ("app-2", "unreachable", Some(false), "10.90.0.2"),
            ("db-1", "not-set-up", Some(false), "10.90.0.3"),
        ]
    );
    assert_eq!(nodes[1]["port"], 2);
    assert!(
        nodes[0]["problem"]
            .as_str()
            .is_some_and(|problem| problem.contains("did not answer")),
        "{}",
        nodes[0]
    );
    assert!(
        nodes[2]["problem"]
            .as_str()
            .is_some_and(|problem| problem.contains("mm target setup record")),
        "{}",
        nodes[2]
    );

    let human = mm(root, home, &["target", "nodes"])?;
    for expected in [
        "Target production: 3 node(s)",
        "unreachable",
        "not set up",
        "127.0.0.1:1",
    ] {
        assert!(
            human.stdout.contains(expected),
            "{expected}: {}",
            human.stdout
        );
    }
    assert!(human.stderr.contains("--nodes"), "{}", human.stderr);

    let doctor = mm(root, home, &["--output", "json", "doctor"])?;
    for code in ["MM_NODE_UNREACHABLE", "MM_NODE_NOT_SET_UP"] {
        assert!(doctor.stdout.contains(code), "{code}: {}", doctor.stdout);
    }
    assert!(doctor.stdout.contains("node 'app-2' (127.0.0.1:2)"));
    Ok(())
}

#[test]
fn mesh_status_lists_every_node_and_fails_while_one_does_not_answer() -> TestResult {
    if !ssh_is_installed() {
        eprintln!("skipped: no ssh client on this machine");
        return Ok(());
    }
    let project_directory = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    project(project_directory.path())?;
    let (root, home) = (project_directory.path(), home.path());

    let status = mm(
        root,
        home,
        &["--output", "json", "target", "mesh", "status"],
    )?;
    assert_eq!(status.code, Some(3), "{}{}", status.stdout, status.stderr);
    let json: serde_json::Value = serde_json::from_str(&status.stdout)?;
    assert_eq!(json["target"], "production");
    let nodes = json["nodes"].as_array().ok_or("nodes is not a list")?;
    assert_eq!(
        nodes
            .iter()
            .map(|node| (
                node["name"].as_str().unwrap_or_default(),
                node["reachable"].as_bool(),
                node.get("status").is_some(),
            ))
            .collect::<Vec<_>>(),
        [
            ("app-1", Some(false), false),
            ("app-2", Some(false), false),
            ("db-1", Some(false), false),
        ]
    );
    assert!(
        nodes[0]["problem"]
            .as_str()
            .is_some_and(|problem| problem.contains("did not answer")),
        "{}",
        nodes[0]
    );
    assert!(
        nodes[2]["problem"]
            .as_str()
            .is_some_and(|problem| problem.contains("mm target setup record")),
        "{}",
        nodes[2]
    );
    assert_eq!(json["problems"], serde_json::json!([]));

    let human = mm(root, home, &["target", "mesh", "status"])?;
    assert_eq!(human.code, Some(3));
    for expected in [
        "Target production: mesh between 3 node(s)",
        "app-2  no answer:",
    ] {
        assert!(
            human.stdout.contains(expected),
            "{expected}: {}",
            human.stdout
        );
    }

    // Rotating needs the node's name; the plan is printed unless --plan
    // names a reviewed one, and --yes only makes sense with it.
    let rotate = mm(root, home, &["target", "mesh", "rotate"])?;
    assert_eq!(rotate.code, Some(2), "{}", rotate.stderr);
    let rotate = mm(root, home, &["target", "mesh", "rotate", "app-1", "--yes"])?;
    assert_eq!(rotate.code, Some(2), "{}", rotate.stderr);
    assert!(rotate.stderr.contains("--plan"), "{}", rotate.stderr);
    Ok(())
}

#[test]
fn node_flags_are_refused_where_there_are_no_nodes() -> TestResult {
    use mm::app::ports::TargetStore as _;
    let project_directory = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    project(project_directory.path())?;
    mm::adapters::target::FilesystemTargetStore::new(home.path().join("targets.yaml"))
        .insert_target(
            "local",
            mm::domain::target::TargetDefinition::local(mm::domain::target::TargetId(
                uuid::Uuid::from_u128(7),
            )),
        )?;
    let run = |arguments: &[&str]| -> Result<std::process::Output, std::io::Error> {
        temp_dirs::mm()
            .arg("--root")
            .arg(project_directory.path())
            .arg("--mm-home")
            .arg(home.path())
            .args(arguments)
            .output()
    };
    // The fixture's own environment runs on this machine.
    let nodes = run(&["target", "nodes"])?;
    let error = String::from_utf8_lossy(&nodes.stderr).into_owned();
    assert_eq!(nodes.status.code(), Some(2), "{error}");
    assert!(
        error.contains("`mm target nodes` lists the hosts of a remote target"),
        "{error}"
    );
    let mesh = run(&["target", "mesh", "status"])?;
    let error = String::from_utf8_lossy(&mesh.stderr).into_owned();
    assert_eq!(mesh.status.code(), Some(2), "{error}");
    assert!(
        error.contains("a mesh connects the nodes of a remote target"),
        "{error}"
    );
    let plan = run(&["deploy", "plan", "--nodes", "app-1"])?;
    let error = String::from_utf8_lossy(&plan.stderr).into_owned();
    assert_eq!(plan.status.code(), Some(2), "{error}");
    assert!(
        error.contains("--nodes applies to a remote target that lists nodes:"),
        "{error}"
    );
    Ok(())
}

#[test]
fn a_request_carries_the_environments_runtime_whatever_the_command() -> TestResult {
    use mm::domain::runtime::RuntimeKind;

    let project_directory = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    let root = project_directory.path();
    project(root)?;
    let path = root.join(".config/mm.yaml");
    let config =
        std::fs::read_to_string(&path)?.replace("runtime:\n  driver: podman-compose\n", "");
    std::fs::write(&path, config)?;
    let document = mm::adapters::config::ConfigLoader.load_from(root)?;
    let targets =
        mm::adapters::target::FilesystemTargetStore::new(home.path().join("targets.json"));
    let runtime = |cli: Option<RuntimeKind>| {
        mm::app::resolve_request_context(
            &document,
            "local",
            (cli, RuntimeKind::PodmanCompose),
            Some("production"),
            (None, None),
            &targets,
        )
        .map(|context| context.runtime)
    };
    assert_eq!(runtime(None)?, RuntimeKind::DockerCompose);
    assert_eq!(
        runtime(Some(RuntimeKind::PodmanCompose))?,
        RuntimeKind::PodmanCompose,
        "what the user passes still wins"
    );
    Ok(())
}

#[test]
fn node_addresses_parse_hosts_and_ports_and_refuse_the_rest() {
    use mm::domain::target::NodeAddress;
    for (text, host, port) in [
        ("10.0.0.3", "10.0.0.3", None),
        ("10.0.0.3:2222", "10.0.0.3", Some(2222)),
        ("node-2.internal", "node-2.internal", None),
        ("node-2.internal:22", "node-2.internal", Some(22)),
        ("fd00::3", "fd00::3", None),
        ("[fd00::3]", "fd00::3", None),
        ("[fd00::3]:2222", "fd00::3", Some(2222)),
    ] {
        let parsed = text.parse::<NodeAddress>();
        assert_eq!(
            parsed,
            Ok(NodeAddress {
                host: host.to_owned(),
                port
            }),
            "{text}"
        );
    }
    assert_eq!(
        "[fd00::3]:2222"
            .parse::<NodeAddress>()
            .map(|address| address.to_string()),
        Ok("[fd00::3]:2222".to_owned())
    );
    for text in [
        "",
        "-oProxyCommand=x",
        "host name",
        "10.0.0.3:",
        "10.0.0.3:0",
        "10.0.0.3:65536",
        "10.0.0.3:+22",
        "[10.0.0.3]:22",
        "[fd00::3]2222",
        "fd00::zz",
        "user@10.0.0.3",
    ] {
        assert!(text.parse::<NodeAddress>().is_err(), "{text}");
    }
}

#[test]
fn a_device_address_is_private_checked_and_used_by_every_connection() -> TestResult {
    let project_directory = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    project(project_directory.path())?;
    let (root, home) = (project_directory.path(), home.path());
    let config = std::fs::read(root.join(".config/mm.yaml"))?;
    let preference =
        |arguments: &[&str]| mm(root, home, &[&["target", "preference"], arguments].concat());
    let selected = preference(&["set-target", "production", "--environment", "local"])?;
    assert_eq!(selected.code, Some(0), "{}", selected.stderr);

    let unknown = preference(&["set-address", "nowhere", "app-1", "10.0.0.3"])?;
    assert_ne!(unknown.code, Some(0));
    assert!(
        unknown
            .stderr
            .contains("declares no environment 'nowhere'; use one of: local"),
        "{}",
        unknown.stderr
    );
    let node = preference(&["set-address", "local", "app-9", "10.0.0.3"])?;
    assert_ne!(node.code, Some(0));
    assert!(
        node.stderr
            .contains("has no node 'app-9'; its nodes are app-1, app-2, db-1")
            && node.stderr.contains("`mm -e local target nodes`"),
        "{}",
        node.stderr
    );
    let invalid = preference(&["set-address", "local", "app-1", "10.0.0.3:0"])?;
    assert_ne!(invalid.code, Some(0));
    assert!(
        invalid.stderr.contains("no valid port"),
        "{}",
        invalid.stderr
    );

    let set = preference(&["set-address", "local", "app-1", "127.0.0.1:3"])?;
    assert_eq!(set.code, Some(0), "{}", set.stderr);
    let shown = preference(&["show"])?;
    let shown: serde_json::Value = serde_json::from_str(&shown.stdout)?;
    assert_eq!(
        shown["node_addresses"],
        serde_json::json!({"local": {"app-1": {"address": "127.0.0.1:3", "target": "production"}}})
    );
    assert_eq!(std::fs::read(root.join(".config/mm.yaml"))?, config);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(home.join("node-addresses.json"))?
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    if ssh_is_installed() {
        let nodes = mm(root, home, &["--output", "json", "target", "nodes"])?;
        let report: serde_json::Value = serde_json::from_str(&nodes.stdout)?;
        let app_1 = &report["nodes"][0];
        assert_eq!(app_1["port"], 3, "{report}");
        assert_eq!(app_1["repo_address"], "127.0.0.1:1", "{report}");
        assert!(report["nodes"][1].get("repo_address").is_none(), "{report}");
        let plan = mm(root, home, &["deploy", "plan", "--commit", COMMIT])?;
        for expected in [
            "node 'app-1' (127.0.0.1:3, this device's address for it) did not answer",
            "`mm target preference show` lists it",
            "node 'app-2' (127.0.0.1:2) did not answer",
        ] {
            assert!(
                plan.stderr.contains(expected),
                "{expected}: {}",
                plan.stderr
            );
        }
    }

    let unset = preference(&["unset-address", "local", "app-1"])?;
    assert_eq!(unset.code, Some(0), "{}", unset.stderr);
    assert!(
        !unset.stderr.contains("No address is set"),
        "{}",
        unset.stderr
    );
    let again = preference(&["unset-address", "local", "app-1"])?;
    assert_eq!(again.code, Some(0), "{}", again.stderr);
    assert!(
        again
            .stderr
            .contains("No address is set on this device for node 'app-1' of environment 'local'"),
        "{}",
        again.stderr
    );
    let shown = preference(&["show"])?;
    let shown: serde_json::Value = serde_json::from_str(&shown.stdout)?;
    assert_eq!(shown["node_addresses"], serde_json::json!({}));
    Ok(())
}

#[test]
fn a_device_address_for_another_target_or_with_unknown_keys_warns_and_a_bad_one_is_named()
-> TestResult {
    if !ssh_is_installed() {
        eprintln!("skipped: no ssh client on this machine");
        return Ok(());
    }
    let project_directory = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    project(project_directory.path())?;
    let (root, home) = (project_directory.path(), home.path());
    let document = mm::adapters::config::ConfigLoader.load_from(root)?;
    let key = format!("{}/local", document.config.project.id.0);
    let shown = mm(root, home, &["target", "preference", "show"])?;
    assert_eq!(shown.code, Some(0), "{}", shown.stderr);
    let first_port = |run: &Run| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let report: serde_json::Value = serde_json::from_str(&run.stdout)
            .map_err(|error| format!("{error}: {}{}", run.stdout, run.stderr))?;
        Ok(report["nodes"][0]["port"].clone())
    };

    std::fs::write(
        home.join("node-addresses.json"),
        format!(
            r#"{{"{key}": {{"app-1": {{"address": "127.0.0.1:3", "target": "production", "via": "vpn"}}}}}}"#
        ),
    )?;
    let unknown = mm(root, home, &["--output", "json", "target", "nodes"])?;
    assert_eq!(first_port(&unknown)?, 3, "{}", unknown.stderr);
    assert!(
        unknown
            .stderr
            .contains("keys this mm does not know for node 'app-1' of environment 'local': via"),
        "{}",
        unknown.stderr
    );

    std::fs::write(
        home.join("node-addresses.json"),
        format!(r#"{{"{key}": {{"app-1": {{"address": "10.0.0.3:0", "target": "production"}}}}}}"#),
    )?;
    let bad = mm(root, home, &["target", "nodes"])?;
    assert_ne!(bad.code, Some(0));
    assert!(
        bad.stderr.contains("node-addresses.json") && bad.stderr.contains("no valid port"),
        "{}",
        bad.stderr
    );

    std::fs::write(
        home.join("node-addresses.json"),
        format!(r#"{{"{key}": {{"app-1": {{"address": "127.0.0.1:3", "target": "staging"}}}}}}"#),
    )?;
    let other = mm(root, home, &["--output", "json", "target", "nodes"])?;
    assert_eq!(first_port(&other)?, 1, "{}", other.stderr);
    assert!(
        other.stderr.contains(
            "was set for target 'staging', and the environment now uses target 'production'"
        ) && other
            .stderr
            .contains("`mm target preference unset-address local app-1`"),
        "{}",
        other.stderr
    );
    Ok(())
}
