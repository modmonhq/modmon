use std::os::unix::net::UnixListener;
use std::sync::Mutex;

use uuid::Uuid;
use zeroize::Zeroizing;

use super::*;
use crate::domain::target::{
    BootstrapInputFile, BootstrapStep, NamedTarget, TargetBootstrap, TargetDefinition, TargetId,
};
use crate::domain::workspace::ProjectId;

#[derive(Debug)]
struct FakeProcess {
    keyscan: String,
    calls: Mutex<Vec<String>>,
    inventory: Mutex<Option<String>>,
    access_variables: Mutex<Option<serde_json::Value>>,
}

impl FakeProcess {
    fn new(keyscan: String) -> Self {
        Self {
            keyscan,
            calls: Mutex::new(Vec::new()),
            inventory: Mutex::new(None),
            access_variables: Mutex::new(None),
        }
    }

    /// Stands in for the access playbook: reads its variables and writes
    /// the report the real playbook writes.
    fn run_access_playbook(&self, variables_argument: &str) -> Result<(), MmError> {
        let path = variables_argument
            .strip_prefix('@')
            .ok_or_else(|| MmError::State {
                message: "extra variables must name a file".to_owned(),
            })?;
        let variables: serde_json::Value =
            serde_json::from_slice(&fs::read(path).map_err(|source| MmError::Io {
                context: "reading access variables".to_owned(),
                source,
            })?)
            .map_err(|error| MmError::State {
                message: error.to_string(),
            })?;
        let report = variables["mm_access_report"]
            .as_str()
            .ok_or_else(|| MmError::State {
                message: "access variables omit the report path".to_owned(),
            })?
            .to_owned();
        let files = PathBuf::from(variables["mm_access_files"].as_str().unwrap_or_default());
        for name in [
            "mm",
            "mmd",
            "mm-relay-shell",
            "90-mm-relay.conf",
            "mmd.service",
            "mm-mesh.service",
            "mm-network.service",
        ] {
            if !files.join(name).is_file() {
                return Err(MmError::State {
                    message: format!("access file {name} was not staged"),
                });
            }
        }
        fs::write(
            &report,
            serde_json::json!({
                "version": "0.1.0",
                "host-key": HOST_KEY,
                "daemon-fingerprint": "d".repeat(64),
                "deploy-key": DEPLOY_KEY,
            })
            .to_string(),
        )
        .map_err(|source| MmError::Io {
            context: "writing fake access report".to_owned(),
            source,
        })?;
        *self.access_variables.lock().map_err(|_| MmError::State {
            message: "fake access lock was poisoned".to_owned(),
        })? = Some(variables);
        Ok(())
    }
}

const HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";
const DEPLOY_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w mmd-deploy";

#[async_trait]
impl ProcessRunner for FakeProcess {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let program = specification.program().to_owned();
        self.calls
            .lock()
            .map_err(|_| MmError::State {
                message: "fake process call lock was poisoned".to_owned(),
            })?
            .push(program.clone());
        let stdout = if program == "id" {
            "local-user\n".to_owned()
        } else if program == "ssh-keyscan" {
            self.keyscan.clone()
        } else {
            if !specification
                .environment_values()
                .any(|(key, value)| key == "LC_ALL" && value.ends_with(".UTF-8"))
            {
                return Err(MmError::State {
                    message: "ansible-playbook needs a UTF-8 locale".to_owned(),
                });
            }
            let arguments = specification.arguments().collect::<Vec<_>>();
            if let Some(index) = arguments
                .iter()
                .position(|argument| *argument == "--extra-vars")
            {
                let variables = arguments.get(index + 1).ok_or_else(|| MmError::State {
                    message: "Ansible test command omitted extra variables".to_owned(),
                })?;
                self.run_access_playbook(variables)?;
            }
            let inventory_index = arguments
                .iter()
                .position(|argument| *argument == "--inventory")
                .ok_or_else(|| MmError::State {
                    message: "Ansible test command omitted inventory".to_owned(),
                })?;
            let inventory_path =
                arguments
                    .get(inventory_index + 1)
                    .ok_or_else(|| MmError::State {
                        message: "Ansible test command omitted inventory path".to_owned(),
                    })?;
            let inventory = fs::read_to_string(inventory_path).map_err(|source| MmError::Io {
                context: "reading generated test inventory".to_owned(),
                source,
            })?;
            *self.inventory.lock().map_err(|_| MmError::State {
                message: "fake inventory lock was poisoned".to_owned(),
            })? = Some(inventory);
            String::new()
        };
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(stdout),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

pub(super) fn project() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let project = crate::test_support::tempdir()?;
    fs::create_dir_all(project.path().join(".config/ansible"))?;
    fs::write(
        project.path().join(".config/ansible/ansible.cfg"),
        "[defaults]\nhost_key_checking = True\n",
    )?;
    fs::write(
        project.path().join(".config/ansible/bootstrap.yml"),
        "---\n- hosts: mm_targets\n  tasks: []\n",
    )?;
    Ok(project)
}

pub(super) fn plan(
    project: &Path,
    transport: TargetTransport,
    ingress: bool,
) -> Result<TargetBootstrapPlan, Box<dyn std::error::Error>> {
    let mut input_files = Vec::new();
    for relative in [
        ".config/ansible/ansible.cfg",
        ".config/ansible/bootstrap.yml",
    ] {
        let bytes = fs::read(project.join(relative))?;
        input_files.push(BootstrapInputFile {
            path: PathBuf::from(relative),
            digest: blake3::hash(&bytes).to_hex().to_string(),
        });
    }
    let selected_runtime = crate::domain::runtime::RuntimeKind::PodmanCompose;
    let definition = TargetDefinition {
        id: TargetId(Uuid::parse_str("319c527b-6242-7b43-bfd4-bd60e27c1de3")?),
        transport,
        bootstrap: TargetBootstrap::Ansible,
        ingress: ingress.then(|| crate::domain::target::TargetIngress::Traefik {
            image: crate::domain::target::DEFAULT_TRAEFIK_IMAGE.to_owned(),
            acme_email: None,
            acme_ca_server: None,
            acme_ca_certificate: None,
        }),
        default_runtime: None,
        available_runtimes: Vec::new(),
        runtime: None,
        nodes: Vec::new(),
    };
    let target = NamedTarget::new("test".to_owned(), definition);
    let mut required_capabilities = vec![BootstrapCapability::AnsiblePlaybook];
    if matches!(target.definition.transport, TargetTransport::Ssh { .. }) {
        required_capabilities.push(BootstrapCapability::SshTransport);
    }
    required_capabilities.extend([
        BootstrapCapability::Python3,
        BootstrapCapability::Podman,
        BootstrapCapability::ComposeProvider,
    ]);
    if ingress {
        required_capabilities.push(BootstrapCapability::Haproxy);
    }
    let mut plan = TargetBootstrapPlan {
        project_id: ProjectId(Uuid::parse_str("019c527b-6242-7b43-bfd4-bd60e27c1de3")?),
        project_name: "bootstrap-test".to_owned(),
        config_hash: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
        environment: "local".to_owned(),
        target,
        selected_runtime,
        rootless: true,
        input_files,
        required_capabilities,
        steps: vec![BootstrapStep::ProvisionHost, BootstrapStep::EnrollRuntime],
        access: None,
        plan_hash: String::new(),
    };
    plan.plan_hash = plan.canonical_hash().map_err(std::io::Error::other)?;
    Ok(plan)
}

#[tokio::test]
async fn local_bootstrap_uses_only_an_ephemeral_inventory() -> Result<(), Box<dyn std::error::Error>>
{
    let project = project()?;
    let staging = crate::test_support::tempdir()?;
    let process = Arc::new(FakeProcess::new(String::new()));
    let executor = AnsibleTargetBootstrapExecutor::with_executables(
        process.clone(),
        staging.path().join("bootstrap"),
        "ansible-playbook",
        "ssh-keyscan",
        None,
    );
    let plan = plan(project.path(), TargetTransport::Local, true)?;

    let execution = executor.execute(project.path(), &plan).await?;

    assert_eq!(execution.capabilities, plan.required_capabilities);
    assert!(execution.access.is_none());
    assert_eq!(
        *process.calls.lock().map_err(|_| "lock poisoned")?,
        vec!["id".to_owned(), "ansible-playbook".to_owned()]
    );
    let inventory = process.inventory.lock().map_err(|_| "lock poisoned")?;
    let inventory = inventory.as_deref().ok_or("inventory was not captured")?;
    assert!(inventory.contains("ansible_connection: local"));
    assert!(inventory.contains("ansible_user: local-user"));
    assert!(inventory.contains("mm_runtime: podman-compose"));
    assert!(inventory.contains("mm_runtime_rootless: true"));
    assert_eq!(fs::read_dir(staging.path().join("bootstrap"))?.count(), 0);
    Ok(())
}

#[tokio::test]
async fn ssh_bootstrap_accepts_only_the_reviewed_host_key() -> Result<(), Box<dyn std::error::Error>>
{
    let project = project()?;
    let staging = crate::test_support::tempdir()?;
    let auth_socket_path = staging.path().join("agent.sock");
    let _auth_socket = UnixListener::bind(&auth_socket_path)?;
    let key = b"test-host-key";
    let encoded_key = base64::engine::general_purpose::STANDARD.encode(key);
    let fingerprint = format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(Sha256::digest(key))
    );
    let process = Arc::new(FakeProcess::new(format!(
        "host.example.test ssh-ed25519 {encoded_key}\n"
    )));
    let executor = AnsibleTargetBootstrapExecutor::with_executables(
        process.clone(),
        staging.path().join("bootstrap"),
        "ansible-playbook",
        "ssh-keyscan",
        Some(auth_socket_path),
    );
    let transport = TargetTransport::Ssh {
        host: "host.example.test".to_owned(),
        port: 22,
        user: "deploy".to_owned(),
        host_key_fingerprint: fingerprint,
        host_key: None,
        identity_file: None,
        remote_root: None,
        daemon_fingerprint: None,
    };
    let plan = plan(project.path(), transport, false)?;

    executor.execute(project.path(), &plan).await?;

    assert_eq!(
        *process.calls.lock().map_err(|_| "lock poisoned")?,
        vec!["ssh-keyscan".to_owned(), "ansible-playbook".to_owned()]
    );
    let inventory = process
        .inventory
        .lock()
        .map_err(|_| "lock poisoned")?
        .clone()
        .ok_or("inventory was not captured")?;
    assert!(inventory.contains("ansible_host: host.example.test"));
    assert!(inventory.contains("UserKnownHostsFile='"));
    assert!(inventory.contains("/known_hosts'"));
    assert!(!inventory.contains("ansible_ssh_private_key_file"));
    assert!(!inventory.contains("IdentitiesOnly"));

    let mut keyed = plan.clone();
    if let TargetTransport::Ssh { identity_file, .. } = &mut keyed.target.definition.transport {
        *identity_file = Some(PathBuf::from("/home/deploy/.mm/identities/ssh/id_ed25519"));
    }
    let keyed_inventory = super::inventory(&keyed, Some(Path::new("/tmp/known_hosts")), None)?;
    assert!(
        keyed_inventory
            .contains("ansible_ssh_private_key_file: /home/deploy/.mm/identities/ssh/id_ed25519")
    );
    assert!(keyed_inventory.contains("-o IdentitiesOnly=yes"));

    // A target with its own key needs no agent; one without a key still does.
    keyed.target = NamedTarget::new(keyed.target.name.clone(), keyed.target.definition.clone());
    keyed.plan_hash = keyed.canonical_hash().map_err(std::io::Error::other)?;
    let agentless = AnsibleTargetBootstrapExecutor::with_executables(
        Arc::new(FakeProcess::new(format!(
            "host.example.test ssh-ed25519 {encoded_key}\n"
        ))),
        staging.path().join("agentless"),
        "ansible-playbook",
        "ssh-keyscan",
        None,
    );
    agentless.execute(project.path(), &keyed).await?;
    assert!(matches!(
        agentless.execute(project.path(), &plan).await,
        Err(MmError::Target { .. })
    ));

    let mut forged = plan.clone();
    if let TargetTransport::Ssh {
        host_key_fingerprint,
        ..
    } = &mut forged.target.definition.transport
    {
        *host_key_fingerprint = "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU".to_owned();
    }
    forged.target = NamedTarget::new(forged.target.name.clone(), forged.target.definition.clone());
    forged.plan_hash = forged.canonical_hash().map_err(std::io::Error::other)?;
    assert!(matches!(
        executor.execute(project.path(), &forged).await,
        Err(MmError::Security { .. })
    ));
    Ok(())
}

fn access_plan(
    project: &Path,
    binaries: &Path,
) -> Result<TargetBootstrapPlan, Box<dyn std::error::Error>> {
    let mut plan = plan(project, TargetTransport::Local, true)?;
    plan.selected_runtime = crate::domain::runtime::RuntimeKind::DockerCompose;
    plan.rootless = false;
    let requirements = crate::domain::target::bootstrap_requirements(
        &plan.target.definition.transport,
        plan.selected_runtime,
        true,
        true,
    )?;
    plan.required_capabilities = requirements.capabilities;
    plan.steps = requirements.steps;
    plan.access = Some(crate::domain::target::BootstrapAccess {
        origin: "git@github.com:acme/shop.git".to_owned(),
        origin_host_key: Some(HOST_KEY.to_owned()),
        break_glass: vec![crate::domain::host_policy::BreakGlassKey {
            name: "ops".to_owned(),
            key: DEPLOY_KEY.replace("mmd-deploy", "ops@offline"),
        }],
        binaries: binaries.to_path_buf(),
        mm_digest: blake3::hash(&fs::read(binaries.join("mm"))?)
            .to_hex()
            .to_string(),
        mmd_digest: blake3::hash(&fs::read(binaries.join("mmd"))?)
            .to_hex()
            .to_string(),
        upgrade: false,
        downgrade: false,
        notices: Vec::new(),
    });
    plan.plan_hash = plan.canonical_hash().map_err(std::io::Error::other)?;
    Ok(plan)
}

#[tokio::test]
async fn access_step_runs_mms_playbook_and_returns_the_host_facts()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let staging = crate::test_support::tempdir()?;
    let binaries = crate::test_support::tempdir()?;
    fs::write(binaries.path().join("mm"), b"mm binary")?;
    fs::write(binaries.path().join("mmd"), b"mmd binary")?;
    let process = Arc::new(FakeProcess::new(String::new()));
    let executor = AnsibleTargetBootstrapExecutor::with_executables(
        process.clone(),
        staging.path().join("bootstrap"),
        "ansible-playbook",
        "ssh-keyscan",
        None,
    );
    let plan = access_plan(project.path(), binaries.path())?;

    let execution = executor.execute(project.path(), &plan).await?;

    assert_eq!(execution.capabilities, plan.required_capabilities);
    let report = execution.access.ok_or("access report missing")?;
    assert_eq!(report.daemon_fingerprint, "d".repeat(64));
    assert_eq!(report.deploy_key, DEPLOY_KEY);
    assert_eq!(
        *process.calls.lock().map_err(|_| "lock poisoned")?,
        ["id", "ansible-playbook", "ansible-playbook"]
    );
    let inventory = process
        .inventory
        .lock()
        .map_err(|_| "lock poisoned")?
        .clone()
        .ok_or("inventory was not captured")?;
    assert!(inventory.contains("mm_runtime: docker-compose"));
    assert!(inventory.contains("mm_install_haproxy: true"));
    let variables = process
        .access_variables
        .lock()
        .map_err(|_| "lock poisoned")?
        .clone()
        .ok_or("access variables were not captured")?;
    let argv = variables["mm_access_enroll_argv"]
        .as_array()
        .ok_or("enroll argv missing")?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    assert_eq!(&argv[..3], ["/usr/libexec/mmd", "access", "enroll"]);
    assert!(argv.contains(&"--origin-host-key"));
    assert!(argv.contains(&"--environment"));
    assert!(
        argv.iter()
            .any(|argument| argument.starts_with("ops=ssh-ed25519 "))
    );
    assert_eq!(fs::read_dir(staging.path().join("bootstrap"))?.count(), 0);
    Ok(())
}

#[tokio::test]
async fn access_step_refuses_a_binary_that_changed_after_review()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let staging = crate::test_support::tempdir()?;
    let binaries = crate::test_support::tempdir()?;
    fs::write(binaries.path().join("mm"), b"mm binary")?;
    fs::write(binaries.path().join("mmd"), b"mmd binary")?;
    let process = Arc::new(FakeProcess::new(String::new()));
    let executor = AnsibleTargetBootstrapExecutor::with_executables(
        process.clone(),
        staging.path().join("bootstrap"),
        "ansible-playbook",
        "ssh-keyscan",
        None,
    );
    let plan = access_plan(project.path(), binaries.path())?;
    fs::write(binaries.path().join("mmd"), b"replaced")?;

    assert!(matches!(
        executor.execute(project.path(), &plan).await,
        Err(MmError::State { .. })
    ));
    // Nothing ran against the host.
    assert_eq!(*process.calls.lock().map_err(|_| "lock poisoned")?, ["id"]);
    Ok(())
}
