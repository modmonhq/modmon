use std::collections::BTreeSet;
use std::fs;
use uuid::Uuid;

use super::*;
use crate::app::ports::TargetStore as _;
use crate::domain::access::{
    AccessAction, AccessResourceKind, Authorization, Capability, PrincipalId,
};
use crate::domain::remote::RemoteScope;
use crate::domain::target::{TargetBootstrap, TargetDefinition, TargetId, TargetTransport};

fn context(
    scope: RemoteScope,
    project_root: &std::path::Path,
) -> crate::app::ports::RemoteRequestContext {
    let principal_id = PrincipalId(Uuid::from_u128(31));
    crate::app::ports::RemoteRequestContext {
        request_id: Uuid::from_u128(32),
        operation_nonce: Uuid::from_u128(33),
        principal_id,
        principal: "test-principal".to_owned(),
        key_fingerprint: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned(),
        session_id: Uuid::from_u128(34),
        source: "unit".to_owned(),
        request_plan_hash: None,
        authorization: Authorization {
            principal_id,
            display_name: "test-principal".to_owned(),
            capability: Capability::new(AccessAction::Status, AccessResourceKind::Project),
            policy_revision: 1,
            grant_ids: BTreeSet::new(),
            authorization_hash: "a".repeat(64),
        },
        deadline_unix_ms: 10_000,
        scope,
        project_root: project_root.to_path_buf(),
        release: None,
    }
}

#[test]
fn daemon_selection_requires_exact_project_and_environment_scope()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    fs::create_dir(&project)?;
    let initialized = ConfigLoader.initialize(&project, Some("remote-project"))?;
    let environment = initialized
        .document
        .config
        .default_environment
        .clone()
        .ok_or("default environment")?;
    let scope = RemoteScope {
        project_id: initialized.document.config.project.id,
        environment: environment.clone(),
        target: "reviewed-remote".to_owned(),
        target_fingerprint: "b".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: None,
    };
    let adapter = DaemonUseCaseAdapter::new(home);
    let selection = adapter.selection(&context(scope.clone(), &project))?;
    assert_eq!(selection.start, project);
    assert_eq!(selection.environment.as_deref(), Some(environment.as_str()));
    assert_eq!(selection.runtime, Some(RuntimeKind::DockerCompose));
    assert_eq!(selection.target.as_deref(), Some("local"));

    let mut wrong_project = scope.clone();
    wrong_project.project_id = crate::domain::workspace::ProjectId(Uuid::from_u128(99));
    assert!(
        adapter
            .selection(&context(wrong_project, &project))
            .is_err()
    );
    Ok(())
}

#[test]
fn remote_transport_context_distinguishes_local_and_incomplete_ssh_targets()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let project = temporary.path().join("project");
    let home_path = temporary.path().join("home");
    fs::create_dir(&project)?;
    ConfigLoader.initialize(&project, Some("transport-project"))?;
    let home = MmHome::resolve(Some(&home_path))?;
    let targets = FilesystemTargetStore::new(home.targets_file.clone());
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::from_u128(41))),
    )?;
    let selection = WorkspaceSelection::new(&project, None);
    assert!(
        remote_transport_context(&selection, Some(&home_path), RuntimeKind::DockerCompose,)?
            .is_none()
    );

    targets.insert_target(
        "remote",
        TargetDefinition {
            id: TargetId(Uuid::from_u128(42)),
            transport: TargetTransport::Ssh {
                host: "example.test".to_owned(),
                port: 22,
                user: "relay".to_owned(),
                host_key_fingerprint: "SHA256:kmYcvdi2GkPeWxB6XLjrZB8JHsy2Hm8luHMFp9GMvqk"
                    .to_owned(),
                host_key: None,
                identity_file: None,
                remote_root: None,
                daemon_fingerprint: None,
            },
            bootstrap: TargetBootstrap::Ansible,
            ingress: None,
            default_runtime: Some(RuntimeKind::DockerCompose),
            available_runtimes: vec![RuntimeKind::DockerCompose],
            runtime: None,
            nodes: Vec::new(),
        },
    )?;
    let mut remote = WorkspaceSelection::new(&project, None);
    remote.target = Some("remote".to_owned());
    assert!(
        remote_transport_context(&remote, Some(&home_path), RuntimeKind::DockerCompose,).is_err()
    );
    Ok(())
}

const PRIMARY_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w";
const NODE_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBPnHBZZYETwo334Dl5Vu8LQSzJiO4gxHeq5zQGE+eAL";

/// Replaces the repo's `targets:` with a one-host `prod` target whose daemon
/// has the given fingerprint digit.
fn declare_host(config: &std::path::Path, original: &str, daemon: char) -> std::io::Result<()> {
    fs::write(
        config,
        format!(
            "{original}targets:\n  prod:\n    host: app.example.test\n    host-key: {PRIMARY_KEY}\n    daemon-fingerprint: {}\n",
            daemon.to_string().repeat(64)
        ),
    )
}

/// Replaces the repo's `targets:` with a `cluster` target of recorded nodes
/// that use the given daemon fingerprints, one per node.
fn declare_cluster(
    config: &std::path::Path,
    original: &str,
    nodes: &[char],
) -> std::io::Result<()> {
    let mut text = format!("{original}targets:\n  cluster:\n    nodes:\n");
    for (index, daemon) in nodes.iter().enumerate() {
        text.push_str(&format!(
            "      - name: app-{number}\n        host: app-{number}.example.test\n        host-key: {key}\n        daemon-fingerprint: {daemon}\n",
            number = index + 1,
            key = if index == 0 { PRIMARY_KEY } else { NODE_KEY },
            daemon = daemon.to_string().repeat(64)
        ));
    }
    fs::write(config, text)
}

#[test]
fn a_repo_target_is_pinned_on_first_use_and_a_changed_identity_needs_review()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let project = temporary.path().join("project");
    let home_path = temporary.path().join("home");
    fs::create_dir(&project)?;
    ConfigLoader.initialize(&project, Some("pinned-project"))?;
    let config = project.join(crate::adapters::config::CONFIG_RELATIVE_PATH);
    let original = fs::read_to_string(&config)?;
    declare_host(&config, &original, 'a')?;
    let mut selection = WorkspaceSelection::new(&project, None);
    selection.target = Some("prod".to_owned());

    let (_, scope) =
        remote_transport_context(&selection, Some(&home_path), RuntimeKind::DockerCompose)?
            .ok_or("a repo target is remote")?;
    assert_eq!(scope.target, "prod");
    let review = trust_repo_target(&project, Some(&home_path), "prod", false)?;
    assert!(review.repo.nodes.is_empty());
    assert_eq!(
        review.pinned.as_ref(),
        Some(&review.repo),
        "pinned on first use"
    );

    declare_host(&config, &original, 'b')?;
    let refused =
        remote_transport_context(&selection, Some(&home_path), RuntimeKind::DockerCompose);
    assert!(
        matches!(&refused, Err(MmError::Security { message }) if message.contains("mm target trust prod")),
        "a new daemon identity needs review"
    );
    let review = trust_repo_target(&project, Some(&home_path), "prod", false)?;
    assert_ne!(review.pinned.as_ref(), Some(&review.repo));
    assert!(!review.trusted);
    trust_repo_target(&project, Some(&home_path), "prod", true)?;
    let (_, trusted) =
        remote_transport_context(&selection, Some(&home_path), RuntimeKind::DockerCompose)?
            .ok_or("a repo target is remote")?;
    assert_ne!(
        trusted.target_fingerprint, scope.target_fingerprint,
        "plans made before the identity change are rejected"
    );
    Ok(())
}

#[test]
fn a_repo_target_with_nodes_gives_one_named_connection_per_node()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let project = temporary.path().join("project");
    let home_path = temporary.path().join("home");
    fs::create_dir(&project)?;
    ConfigLoader.initialize(&project, Some("cluster-project"))?;
    let config = project.join(crate::adapters::config::CONFIG_RELATIVE_PATH);
    let original = fs::read_to_string(&config)?;
    declare_cluster(&config, &original, &['a', 'b', 'c'])?;
    let mut selection = WorkspaceSelection::new(&project, None);
    selection.target = Some("cluster".to_owned());

    let environment = remote_environment(&selection, Some(&home_path), RuntimeKind::DockerCompose)?
        .ok_or("a repo target is remote")?;
    assert!(environment.is_cluster());
    assert_eq!(environment.target(), "cluster");
    let nodes = environment.nodes();
    assert_eq!(
        nodes
            .iter()
            .map(|node| (
                node.name.as_deref(),
                node.host.as_str(),
                node.scope.node.as_deref()
            ))
            .collect::<Vec<_>>(),
        [
            (Some("app-1"), "app-1.example.test", Some("app-1")),
            (Some("app-2"), "app-2.example.test", Some("app-2")),
            (Some("app-3"), "app-3.example.test", Some("app-3")),
        ],
        "every request names the node it is for"
    );
    assert!(nodes.iter().all(|node| node.transport.is_ok()));
    assert!(
        nodes
            .iter()
            .all(|node| node.scope.target_fingerprint == nodes[0].scope.target_fingerprint),
        "one target, one fingerprint"
    );
    // Until placement is honoured only the first node runs services.
    assert!(nodes[0].placed.is_some());
    assert!(
        nodes[1..]
            .iter()
            .all(|node| node.placed.as_ref().is_some_and(|placed| placed.is_empty()))
    );

    // Connecting pinned every node's keys under its name.
    let review = trust_repo_target(&project, Some(&home_path), "cluster", false)?;
    assert_eq!(review.repo.host_key, None);
    assert_eq!(
        review
            .repo
            .nodes
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>(),
        ["app-1", "app-2", "app-3"]
    );
    assert_eq!(review.pinned.as_ref(), Some(&review.repo));

    declare_cluster(&config, &original, &['a', 'b', 'c', 'd'])?;
    let refused = remote_environment(&selection, Some(&home_path), RuntimeKind::DockerCompose);
    assert!(
        matches!(&refused, Err(MmError::Security { message }) if message.contains("mm target trust cluster")),
        "a new node's keys need review"
    );
    let review = trust_repo_target(&project, Some(&home_path), "cluster", false)?;
    assert_eq!(review.pinned.as_ref().map(|pin| pin.nodes.len()), Some(3));
    assert_eq!(review.repo.nodes.len(), 4);
    trust_repo_target(&project, Some(&home_path), "cluster", true)?;
    let review = trust_repo_target(&project, Some(&home_path), "cluster", false)?;
    assert_eq!(review.pinned.as_ref(), Some(&review.repo));
    assert_eq!(
        remote_environment(&selection, Some(&home_path), RuntimeKind::DockerCompose)?
            .map(|environment| environment.nodes().len()),
        Some(4)
    );
    Ok(())
}

#[test]
fn a_node_without_recorded_keys_is_listed_but_has_no_connection()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let project = temporary.path().join("project");
    let home_path = temporary.path().join("home");
    fs::create_dir(&project)?;
    ConfigLoader.initialize(&project, Some("partial-project"))?;
    let config = project.join(crate::adapters::config::CONFIG_RELATIVE_PATH);
    let original = fs::read_to_string(&config)?;
    declare_cluster(&config, &original, &['a', 'b'])?;
    let mut text = fs::read_to_string(&config)?;
    text.push_str("      - name: app-3\n        host: app-3.example.test\n");
    fs::write(&config, text)?;
    let mut selection = WorkspaceSelection::new(&project, None);
    selection.target = Some("cluster".to_owned());

    let environment = remote_environment(&selection, Some(&home_path), RuntimeKind::DockerCompose)?
        .ok_or("a repo target is remote")?;
    let unavailable = environment.nodes()[2]
        .transport
        .as_ref()
        .err()
        .ok_or("a node without keys has no connection")?;
    assert_eq!(unavailable.subject, "node 'app-3' (app-3.example.test:22)");
    assert!(unavailable.problem.contains("no recorded keys"));
    assert!(unavailable.help.contains("mm target setup record"));
    Ok(())
}

#[test]
fn a_commit_that_does_not_validate_on_the_host_says_what_is_wrong()
-> Result<(), Box<dyn std::error::Error>> {
    // The fixture without its module directory: what a host checks out when
    // the directory was never committed.
    let checkout = crate::test_support::tempdir()?;
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal");
    fs::create_dir_all(checkout.path().join(".config"))?;
    for file in [".config/mm.yaml", "compose.yaml"] {
        fs::copy(fixture.join(file), checkout.path().join(file))?;
    }
    let selection = WorkspaceSelection::new(checkout.path(), Some("local".to_owned()));
    let message = super::super::daemon_checkout::invalid_at_commit(&selection, 1).to_string();
    assert!(
        message.contains("1 error(s) in the host's checkout"),
        "{message}"
    );
    assert!(message.contains("MM_MODULE_PATH_MISSING"), "{message}");
    assert!(message.contains("not in the commit"), "{message}");
    Ok(())
}
