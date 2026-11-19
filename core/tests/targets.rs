use std::fs;
use std::path::Path;
use std::sync::Arc;

use mm::adapters::config::{
    ANSIBLE_CONFIG_RELATIVE_PATH, ANSIBLE_PLAYBOOK_RELATIVE_PATH, ConfigLoader,
};
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::TargetStore;
use mm::app::{SshTargetInput, TargetManager, WorkspaceSelection};
use mm::domain::target::{
    BootstrapCapability, DEFAULT_TRAEFIK_IMAGE, TargetDefinition, TargetId, TargetIngress,
    TargetTransport,
};
use uuid::Uuid;

mod temp_dirs;

const TARGET_ID: &str = "319c527b-6242-7b43-bfd4-bd60e27c1de3";
const HOST_KEY: &str = "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU";

#[test]
fn persists_a_private_catalog_with_stable_unique_ids() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let path = home.path().join("targets.yaml");
    let store: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(path.clone()));
    let manager = TargetManager::new(Arc::new(ConfigLoader), Arc::clone(&store));
    assert!(format!("{manager:?}").contains("TargetManager"));

    let local = manager.ensure_local()?;
    assert_eq!(local.name, "local");
    assert_eq!(local.definition.transport, TargetTransport::Local);
    assert!(matches!(
        &local.definition.ingress,
        Some(TargetIngress::Traefik {
            image,
            acme_email: None,
            ..
        }) if image == DEFAULT_TRAEFIK_IMAGE
    ));
    assert_eq!(manager.ensure_local()?.definition.id, local.definition.id);
    assert_eq!(manager.list()?.targets.len(), 1);
    let production =
        manager.add_ssh("production", ssh_target_input("one.example.test", 22, "mm"))?;
    assert_eq!(production.definition.ingress, None);
    let updated = manager.update_ssh(
        "production",
        &production.fingerprint,
        ssh_target_input("two.example.test", 2222, "deploy"),
    )?;
    assert_eq!(updated.definition.id, production.definition.id);
    assert!(matches!(
        updated.definition.transport,
        TargetTransport::Ssh { ref host, port: 2222, ref user, .. }
            if host == "two.example.test" && user == "deploy"
    ));
    assert_eq!(manager.show("production")?, updated);
    assert!(
        manager
            .update_ssh(
                "production",
                &production.fingerprint,
                ssh_target_input("stale.example.test", 22, "mm"),
            )
            .is_err()
    );
    assert!(
        manager
            .add_ssh("bad", ssh_target_input("-oProxyCommand=x", 22, "mm"),)
            .is_err()
    );
    assert!(
        store
            .insert_target("duplicate", TargetDefinition::local(local.definition.id),)
            .is_err()
    );
    let configured = manager.configure_traefik(
        "production",
        &updated.fingerprint,
        DEFAULT_TRAEFIK_IMAGE.to_owned(),
        Some("ops@example.test".to_owned()),
    )?;
    assert!(matches!(
        configured.definition.ingress,
        Some(TargetIngress::Traefik { ref acme_email, .. })
            if acme_email.as_deref() == Some("ops@example.test")
    ));
    assert_eq!(
        manager.remove("production", &configured.fingerprint)?.name,
        "production"
    );
    assert!(manager.show("production").is_err());
    manager.add_ssh(
        "production",
        ssh_target_input("two.example.test", 2222, "deploy"),
    )?;
    assert_eq!(manager.list()?.targets.len(), 2);

    let foreign_home = temp_dirs::tempdir()?;
    let foreign_store: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(
        foreign_home.path().join("targets.yaml"),
    ));
    foreign_store.insert_target(
        "local",
        TargetDefinition {
            id: TargetId(Uuid::new_v4()),
            transport: TargetTransport::Ssh {
                host: "remote.example.test".to_owned(),
                port: 22,
                user: "mm".to_owned(),
                host_key_fingerprint: HOST_KEY.to_owned(),
                host_key: None,
                identity_file: None,
                remote_root: None,
                daemon_fingerprint: None,
            },
            bootstrap: mm::domain::target::TargetBootstrap::Ansible,
            ingress: None,
            default_runtime: None,
            available_runtimes: Vec::new(),
            runtime: None,
            nodes: Vec::new(),
        },
    )?;
    let foreign = TargetManager::new(Arc::new(ConfigLoader), foreign_store);
    assert!(foreign.ensure_local().is_err());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
    }

    fs::write(
        &path,
        format!(
            "targets:\n  local:\n    id: {TARGET_ID}\n    transport:\n      kind: local\n    bootstrap:\n      provider: ansible\n    unknown: true\n"
        ),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    assert_eq!(manager.list()?.targets.len(), 1);
    Ok(())
}

fn ssh_target_input(host: &str, port: u16, user: &str) -> SshTargetInput {
    SshTargetInput {
        host: host.to_owned(),
        port,
        user: user.to_owned(),
        host_key_fingerprint: HOST_KEY.to_owned(),
        host_key: None,
        identity_file: None,
        remote_root: None,
        daemon_fingerprint: None,
    }
}

#[test]
fn bootstrap_plans_are_deterministic_and_checkout_root_independent()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = temp_dirs::tempdir()?;
    let first_root = workspace.path().join("first");
    let second_root = workspace.path().join("second");
    fs::create_dir(&first_root)?;
    fs::create_dir(&second_root)?;
    fs::write(first_root.join("compose.yaml"), "services: {}\n")?;
    ConfigLoader.initialize(&first_root, Some("bootstrap-test"))?;
    let config_path = first_root.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        replace_project_id(&config, "019c527b-6242-7b43-bfd4-bd60e27c1de3")?,
    )?;
    copy_project(&first_root, &second_root)?;

    let home = temp_dirs::tempdir()?;
    let store: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    store.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::parse_str(TARGET_ID)?)),
    )?;
    let manager = TargetManager::new(Arc::new(ConfigLoader), store);

    let first = manager.bootstrap_plan(&WorkspaceSelection::new(&first_root, None), None, None)?;
    let second =
        manager.bootstrap_plan(&WorkspaceSelection::new(&second_root, None), None, None)?;
    assert_eq!(first, second);
    assert_eq!(
        first.plan_hash,
        "63380c4e3738049bc27d2c0c9d12c5a1ee83c10382e1ae3c885e37fd70cb6e6f"
    );
    assert_eq!(
        first.selected_runtime,
        mm::domain::runtime::RuntimeKind::PodmanCompose
    );
    assert!(first.rootless);
    assert!(
        first
            .required_capabilities
            .contains(&BootstrapCapability::Podman)
    );
    assert!(
        !first
            .required_capabilities
            .contains(&BootstrapCapability::Docker)
    );
    assert_eq!(first.target.definition.id.0.to_string(), TARGET_ID);
    assert_eq!(first.input_files.len(), 2);
    assert!(first.input_files.iter().all(|file| file.path.is_relative()));
    assert!(
        first
            .required_capabilities
            .contains(&BootstrapCapability::Haproxy)
    );
    let podman_config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        podman_config.replace("driver: podman-compose", "driver: docker-compose"),
    )?;
    let docker = manager.bootstrap_plan(&WorkspaceSelection::new(&first_root, None), None, None)?;
    assert_eq!(
        docker.selected_runtime,
        mm::domain::runtime::RuntimeKind::DockerCompose
    );
    assert!(!docker.rootless);
    assert!(
        docker
            .required_capabilities
            .contains(&BootstrapCapability::Docker)
    );
    assert!(
        !docker
            .required_capabilities
            .contains(&BootstrapCapability::Podman)
    );
    assert_ne!(docker.plan_hash, first.plan_hash);
    fs::write(&config_path, podman_config)?;
    assert!(!home.path().join("state.sqlite3").exists());
    let mut forged = first.clone();
    forged.target.definition.id =
        TargetId(Uuid::parse_str("419c527b-6242-7b43-bfd4-bd60e27c1de3")?);
    assert!(forged.canonical_hash().is_err());
    let mut incomplete = first.clone();
    incomplete.input_files.clear();
    assert!(incomplete.canonical_hash().is_err());

    fs::write(
        second_root.join(".config/ansible/reviewed-note.txt"),
        "additional reviewed input\n",
    )?;
    let expanded =
        manager.bootstrap_plan(&WorkspaceSelection::new(&second_root, None), None, None)?;
    assert_ne!(expanded.plan_hash, first.plan_hash);
    assert_eq!(expanded.input_files.len(), 3);
    fs::remove_file(second_root.join(".config/ansible/reviewed-note.txt"))?;

    fs::write(
        second_root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
        "---\n- hosts: mm_targets\n  tasks: []\n",
    )?;
    let changed =
        manager.bootstrap_plan(&WorkspaceSelection::new(&second_root, None), None, None)?;
    assert_ne!(changed.plan_hash, first.plan_hash);
    fs::write(
        second_root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
        "---\n- import_playbook: extra.yml\n",
    )?;
    assert!(
        manager
            .bootstrap_plan(&WorkspaceSelection::new(&second_root, None), None, None)
            .is_err()
    );
    fs::write(
        second_root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
        "---\n- hosts: mm_targets\n  tasks:\n    - ansible.builtin.debug:\n        msg: blocked\n      when: \"((q))('file', '/outside') | length > 0\"\n",
    )?;
    assert!(
        manager
            .bootstrap_plan(&WorkspaceSelection::new(&second_root, None), None, None)
            .is_err()
    );
    fs::write(
        second_root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
        "---\n- hosts: mm_targets\n  tasks:\n    - ansible.builtin.set_fact:\n        \"#jinja2: variable_start_string:'[[', variable_end_string:']]\\n[[ 'ansible_' ~ 'ssh_executable' ]]\": /tmp/wrapper\n",
    )?;
    assert!(
        manager
            .bootstrap_plan(&WorkspaceSelection::new(&second_root, None), None, None)
            .is_err()
    );
    fs::write(
        second_root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
        "---\n- hosts: mm_targets\n  tasks:\n    - ansible.builtin.debug:\n        msg: blocked\n      when: \"(lookup) ('file', '/outside')\"\n",
    )?;
    assert!(
        manager
            .bootstrap_plan(&WorkspaceSelection::new(&second_root, None), None, None)
            .is_err()
    );
    fs::write(
        second_root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
        "---\n- hosts: mm_targets\n  tasks:\n    - ansible.builtin.set_fact:\n        \"{{ 'ansible_' ~ 'ssh_executable' }}\": /tmp/wrapper\n",
    )?;
    assert!(
        manager
            .bootstrap_plan(&WorkspaceSelection::new(&second_root, None), None, None)
            .is_err()
    );
    fs::write(
        second_root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
        "---\n- hosts: mm_targets\n  tasks: []\n",
    )?;
    fs::create_dir(second_root.join(".config/ansible/group_vars"))?;
    fs::write(
        second_root.join(".config/ansible/group_vars/all"),
        "external: \"{{ lookup   ('file', '/outside') }}\"\n",
    )?;
    assert!(
        manager
            .bootstrap_plan(&WorkspaceSelection::new(&second_root, None), None, None)
            .is_err()
    );
    fs::remove_file(second_root.join(".config/ansible/group_vars/all"))?;
    fs::write(
        second_root.join(".config/ansible/group_vars/all.json"),
        r#"{"ansible_ssh_host":"other.example.test"}"#,
    )?;
    assert!(
        manager
            .bootstrap_plan(&WorkspaceSelection::new(&second_root, None), None, None)
            .is_err()
    );
    fs::remove_dir_all(second_root.join(".config/ansible/group_vars"))?;
    fs::write(
        second_root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
        "---\n- hosts: mm_targets\n  tasks:\n    - ansible.builtin.debug:\n        msg: \"{{ q('file', 'outside.txt') }}\"\n",
    )?;
    assert!(
        manager
            .bootstrap_plan(&WorkspaceSelection::new(&second_root, None), None, None)
            .is_err()
    );
    fs::write(
        second_root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
        "---\n- hosts: mm_targets\n  tasks: []\n",
    )?;
    fs::write(
        second_root.join(ANSIBLE_CONFIG_RELATIVE_PATH),
        "[defaults]\nhost_key_checking = True\ninventory = inventory.yml\n",
    )?;
    assert!(
        manager
            .bootstrap_plan(&WorkspaceSelection::new(&second_root, None), None, None)
            .is_err()
    );
    Ok(())
}

fn replace_project_id(source: &str, replacement: &str) -> Result<String, std::io::Error> {
    let id_line = source
        .lines()
        .find(|line| line.trim_start().starts_with("id: "))
        .ok_or_else(|| std::io::Error::other("initialized project id is missing"))?;
    Ok(source.replace(id_line, &format!("  id: {replacement}")))
}

#[test]
fn initialization_is_create_only_and_publishes_the_ansible_starter()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    fs::write(project.path().join("compose.yaml"), "services: {}\n")?;
    let initialized = ConfigLoader.initialize(project.path(), Some("starter-test"))?;
    assert_eq!(initialized.starter_files.len(), 3);
    assert!(project.path().join(ANSIBLE_CONFIG_RELATIVE_PATH).is_file());
    assert!(
        project
            .path()
            .join(ANSIBLE_PLAYBOOK_RELATIVE_PATH)
            .is_file()
    );
    assert!(
        fs::read_to_string(project.path().join(ANSIBLE_CONFIG_RELATIVE_PATH))?
            .contains("host_key_checking = True")
    );
    let config_before_recovery = fs::read_to_string(project.path().join(".config/mm.yaml"))?;
    fs::write(
        project.path().join(".config/ansible/.mm-initializing"),
        "mm Ansible starter publication is incomplete\n",
    )?;
    ConfigLoader.initialize(project.path(), Some("ignored-during-recovery"))?;
    assert_eq!(
        fs::read_to_string(project.path().join(".config/mm.yaml"))?,
        config_before_recovery
    );
    assert!(
        !project
            .path()
            .join(".config/ansible/.mm-initializing")
            .exists()
    );
    fs::write(
        project.path().join(".config/ansible/.mm-initializing"),
        "mm Ansible starter publication is incomplete\n",
    )?;
    fs::write(project.path().join(".config/mm.yaml"), "not: [valid\n")?;
    assert!(
        ConfigLoader
            .initialize(project.path(), Some("failed-recovery"))
            .is_err()
    );
    assert!(
        project
            .path()
            .join(".config/ansible/.mm-initializing")
            .is_file()
    );

    let collision = temp_dirs::tempdir()?;
    fs::create_dir_all(collision.path().join(".config/ansible"))?;
    fs::write(
        collision.path().join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
        "user-owned\n",
    )?;
    assert!(
        ConfigLoader
            .initialize(collision.path(), Some("collision"))
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(collision.path().join(ANSIBLE_PLAYBOOK_RELATIVE_PATH))?,
        "user-owned\n"
    );
    assert!(!collision.path().join(".config/mm.yaml").exists());
    assert!(!collision.path().join(ANSIBLE_CONFIG_RELATIVE_PATH).exists());

    let interrupted = temp_dirs::tempdir()?;
    fs::create_dir_all(interrupted.path().join(".config/ansible"))?;
    fs::write(interrupted.path().join("compose.yaml"), "services: {}\n")?;
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/ansible/ansible.cfg"),
        interrupted.path().join(ANSIBLE_CONFIG_RELATIVE_PATH),
    )?;
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/ansible/bootstrap.yml"),
        interrupted.path().join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
    )?;
    fs::write(
        interrupted.path().join(".config/ansible/.mm-initializing"),
        "mm Ansible starter publication is incomplete\n",
    )?;
    ConfigLoader.initialize(interrupted.path(), Some("recovered"))?;
    assert!(interrupted.path().join(".config/mm.yaml").is_file());
    assert!(
        !interrupted
            .path()
            .join(".config/ansible/.mm-initializing")
            .exists()
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn initialization_refuses_a_symlinked_config_directory() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let project = temp_dirs::tempdir()?;
    let outside = temp_dirs::tempdir()?;
    symlink(outside.path(), project.path().join(".config"))?;

    assert!(
        ConfigLoader
            .initialize(project.path(), Some("unsafe"))
            .is_err()
    );
    assert!(!outside.path().join("mm.yaml").exists());
    assert!(!outside.path().join("ansible").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn bootstrap_inputs_require_user_controlled_project_directories()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let project = temp_dirs::tempdir()?;
    fs::set_permissions(project.path(), fs::Permissions::from_mode(0o777))?;
    assert!(
        ConfigLoader
            .initialize(project.path(), Some("unsafe-mode"))
            .is_err()
    );
    assert!(!project.path().join(".config/mm.yaml").exists());
    Ok(())
}

fn copy_project(source: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(destination.join(".config/ansible"))?;
    fs::copy(
        source.join("compose.yaml"),
        destination.join("compose.yaml"),
    )?;
    fs::copy(
        source.join(".config/mm.yaml"),
        destination.join(".config/mm.yaml"),
    )?;
    fs::copy(
        source.join(ANSIBLE_CONFIG_RELATIVE_PATH),
        destination.join(ANSIBLE_CONFIG_RELATIVE_PATH),
    )?;
    fs::copy(
        source.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
        destination.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH),
    )?;
    Ok(())
}

#[test]
fn bootstrap_plans_use_the_selected_target_and_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = temp_dirs::tempdir()?;
    let root = workspace.path().join("project");
    fs::create_dir(&root)?;
    fs::write(root.join("compose.yaml"), "services: {}\n")?;
    ConfigLoader.initialize(&root, Some("bootstrap-selection"))?;
    let home = temp_dirs::tempdir()?;
    let store: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    store.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::parse_str(TARGET_ID)?)),
    )?;
    let node_id = Uuid::parse_str("519c527b-6242-7b43-bfd4-bd60e27c1de3")?;
    store.insert_target("node-1", TargetDefinition::local(TargetId(node_id)))?;
    let manager = TargetManager::new(Arc::new(ConfigLoader), store);

    // The project's own driver is the default: rootless Podman, and the
    // host proxy because the target has ingress.
    let mut selection = WorkspaceSelection::new(&root, None);
    let podman = manager.bootstrap_plan(&selection, None, None)?;
    assert_eq!(
        podman.selected_runtime,
        mm::domain::runtime::RuntimeKind::PodmanCompose
    );
    assert!(podman.rootless);
    assert!(podman.target.definition.ingress.is_some());
    for capability in [
        mm::domain::target::BootstrapCapability::Podman,
        mm::domain::target::BootstrapCapability::Haproxy,
    ] {
        assert!(podman.required_capabilities.contains(&capability));
    }
    assert!(
        !podman
            .required_capabilities
            .contains(&mm::domain::target::BootstrapCapability::Docker)
    );
    assert!(podman.apply_command().contains("--runtime podman-compose"));

    selection.target = Some("node-1".to_owned());
    selection.runtime = Some(mm::domain::runtime::RuntimeKind::DockerCompose);
    let plan = manager.bootstrap_plan(&selection, None, None)?;
    assert_eq!(plan.target.name, "node-1");
    assert_eq!(plan.target.definition.id, TargetId(node_id));
    assert_eq!(
        plan.selected_runtime,
        mm::domain::runtime::RuntimeKind::DockerCompose
    );
    assert!(!plan.rootless);
    assert_eq!(
        plan.apply_command(),
        format!(
            "mm --environment {} --target node-1 --runtime docker-compose target setup apply --plan {}",
            plan.environment, plan.plan_hash
        )
    );
    Ok(())
}

// GitHub's published ed25519 host key, and an `ssh-keygen -t ed25519` key.
const GITHUB_HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";
const OPS_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w ops@offline";

#[test]
fn bootstrap_plans_bind_the_remote_access_layer() -> Result<(), Box<dyn std::error::Error>> {
    use mm::app::BootstrapAccessRequest;
    use mm::domain::runtime::RuntimeKind;
    use mm::domain::target::BootstrapStep;

    let workspace = temp_dirs::tempdir()?;
    let root = workspace.path().join("project");
    fs::create_dir(&root)?;
    fs::write(root.join("compose.yaml"), "services: {}\n")?;
    ConfigLoader.initialize(&root, Some("bootstrap-access"))?;
    let home = temp_dirs::tempdir()?;
    let store: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    store.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::parse_str(TARGET_ID)?)),
    )?;
    let manager = TargetManager::new(Arc::new(ConfigLoader), store);
    let binaries = workspace.path().join("release");
    fs::create_dir(&binaries)?;
    fs::write(binaries.join("mm"), linux_binary(b"mm 0.1.0"))?;
    fs::write(binaries.join("mmd"), linux_binary(b"mmd 0.1.0"))?;
    let mut request = BootstrapAccessRequest {
        origin: "git@github.com:acme/shop.git".to_owned(),
        origin_host_key: Some(GITHUB_HOST_KEY.to_owned()),
        break_glass: vec![mm::domain::host_policy::parse_break_glass(&format!(
            "ops={OPS_KEY}"
        ))?],
        binaries: Some(binaries.clone()),
        upgrade: false,
        downgrade: false,
    };
    let mut selection = WorkspaceSelection::new(&root, None);

    // The access layer runs mmd as root, which drives rootful Podman.
    selection.runtime = Some(RuntimeKind::PodmanCompose);
    let podman = manager.bootstrap_plan(&selection, None, Some(&request))?;
    assert!(!podman.rootless);
    assert!(
        podman
            .required_capabilities
            .contains(&BootstrapCapability::Podman)
    );

    selection.runtime = Some(RuntimeKind::DockerCompose);
    let without = manager.bootstrap_plan(&selection, None, None)?;
    let plan = manager.bootstrap_plan(&selection, None, Some(&request))?;
    assert_ne!(plan.plan_hash, without.plan_hash);
    assert_eq!(plan.steps.last(), Some(&BootstrapStep::InstallAccess));
    assert_eq!(
        plan.required_capabilities.last(),
        Some(&BootstrapCapability::RemoteAccess)
    );
    let access = plan.access.as_ref().ok_or("access was not planned")?;
    assert_eq!(
        access.binaries[0].mmd_digest,
        blake3::hash(&linux_binary(b"mmd 0.1.0"))
            .to_hex()
            .to_string()
    );
    let command = plan.apply_command();
    assert!(command.contains(" --origin git@github.com:acme/shop.git "));
    assert!(command.contains(&format!(" --break-glass 'ops={OPS_KEY}' ")));
    assert!(command.ends_with(&format!(" --host-binaries {}", binaries.display())));
    assert_eq!(access.binaries[0].architecture, "x86_64");

    // Binaries for macOS, or for another CPU than each other, are refused.
    fs::write(binaries.join("mm"), [0xcf, 0xfa, 0xed, 0xfe, 7, 0, 0, 1])?;
    let macos = manager
        .bootstrap_plan(&selection, None, Some(&request))
        .err()
        .ok_or("macOS binaries were planned")?
        .to_string();
    assert!(macos.contains("is built for macOS"), "{macos}");
    assert!(
        macos.contains("aarch64-unknown-linux-gnu.tar.gz"),
        "{macos}"
    );
    let mut arm = linux_binary(b"mm 0.1.0");
    arm[18..20].copy_from_slice(&183_u16.to_le_bytes());
    fs::write(binaries.join("mm"), arm)?;
    let mixed = manager
        .bootstrap_plan(&selection, None, Some(&request))
        .err()
        .ok_or("mm and mmd for two CPUs were planned")?
        .to_string();
    assert!(
        mixed.contains("mm for aarch64 and mmd for x86_64"),
        "{mixed}"
    );
    fs::write(binaries.join("mm"), linux_binary(b"mm 0.1.0"))?;

    // A binary that changes after review changes the plan.
    fs::write(binaries.join("mmd"), linux_binary(b"mmd 0.2.0"))?;
    assert_ne!(
        manager
            .bootstrap_plan(&selection, None, Some(&request))?
            .plan_hash,
        plan.plan_hash
    );
    // A link is not a reviewed binary.
    fs::remove_file(binaries.join("mmd"))?;
    std::os::unix::fs::symlink(binaries.join("mm"), binaries.join("mmd"))?;
    assert!(
        manager
            .bootstrap_plan(&selection, None, Some(&request))
            .is_err()
    );
    fs::remove_file(binaries.join("mmd"))?;
    fs::write(binaries.join("mmd"), linux_binary(b"mmd 0.1.0"))?;

    // An SSH origin needs the git server's key so mmd can pin it.
    request.origin_host_key = None;
    assert!(
        manager
            .bootstrap_plan(&selection, None, Some(&request))
            .is_err()
    );
    Ok(())
}

/// `payload` behind the ELF header of an x86-64 Linux executable.
fn linux_binary(payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 0];
    bytes.resize(18, 0);
    bytes.extend_from_slice(&62_u16.to_le_bytes());
    bytes.resize(64, 0);
    bytes.extend_from_slice(payload);
    bytes
}
