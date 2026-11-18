use std::collections::BTreeSet;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use mm::adapters::config::ConfigLoader;
use mm::adapters::process::TokioProcessRunner;
use mm::adapters::runtime::{DockerComposeRuntime, PodmanComposeRuntime};
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{LocalDeploymentRuntime, RuntimeDriver, TargetStore};
use mm::app::{StackManager, WorkspaceSelection};
use mm::domain::runtime::RuntimeTarget;
use mm::domain::stack::PreparedStackArtifact;
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId};

mod temp_dirs;

trait TestRuntime: RuntimeDriver + LocalDeploymentRuntime {}
impl<T: RuntimeDriver + LocalDeploymentRuntime> TestRuntime for T {}

#[tokio::test]
#[ignore = "requires MM_TEST_RUNTIME and a live local Docker/Podman engine"]
async fn excluded_service_is_removed_and_can_be_reenabled_on_a_live_runtime()
-> Result<(), Box<dyn std::error::Error>> {
    let process = Arc::new(TokioProcessRunner);
    let runtime: Box<dyn TestRuntime> = match std::env::var("MM_TEST_RUNTIME")?.as_str() {
        "docker-compose" => Box::new(DockerComposeRuntime::new(process)),
        "podman-compose" => Box::new(PodmanComposeRuntime::new(process)),
        _ => return Err("MM_TEST_RUNTIME must name a supported runtime".into()),
    };
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    fs::create_dir(&root)?;
    let initialized = ConfigLoader.initialize(&root, Some("service-filter-live"))?;
    let mut config = initialized.document.config;
    config.runtime.driver = Some(runtime.kind());
    config.ingress.automatic_local = Some(false);
    config.development_mail.enabled = Some(false);
    config
        .stacks
        .get_mut("app")
        .ok_or("missing stack")?
        .project_name = Some("mm-filter-test".to_owned());
    config
        .environments
        .insert("production".to_owned(), Default::default());
    config.modules.insert("app".to_owned(), serde_yaml_ng::from_str(
        "path: .\nservices:\n  app: {stack: app, role: worker}\n  scheduler: {stack: app, role: scheduler}\n",
    )?);
    fs::write(
        root.join(".config/mm.yaml"),
        serde_yaml_ng::to_string(&config)?,
    )?;
    let image = "docker.io/library/busybox@sha256:fd8d9aa63ba2f0982b5304e1ee8d3b90a210bc1ffb5314d980eb6962f1a9715d";
    let network_id = uuid::Uuid::new_v4();
    let subnet = format!(
        "10.254.{}.{}/28",
        network_id.as_bytes()[0],
        network_id.as_bytes()[1] & 0xf0
    );
    fs::write(
        root.join("compose.yaml"),
        format!(
            "services:\n  app:\n    image: {image}\n    command: [sleep, '600']\n    deploy: {{resources: {{limits: {{cpus: '0.1', memory: 32m}}}}}}\n  scheduler:\n    image: {image}\n    command: [sleep, '600']\n    deploy: {{resources: {{limits: {{cpus: '0.1', memory: 32m}}}}}}\nnetworks:\n  default:\n    ipam:\n      config:\n        - subnet: {subnet}\n"
        ),
    )?;
    let targets = Arc::new(FilesystemTargetStore::new(
        temporary.path().join("targets.yaml"),
    ));
    let named = NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(uuid::Uuid::new_v4())),
    );
    targets.insert_target("local", named.definition.clone())?;
    let stacks = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            temporary.path().join("compose"),
        )),
        targets,
    );
    let selection = WorkspaceSelection::new(&root, Some("local".to_owned()));
    let initial = stacks.compile(&selection)?;
    let target = runtime_target(
        &root,
        &named,
        initial.stacks.first().ok_or("missing initial stack")?,
    );
    let result = exercise(runtime.as_ref(), &stacks, &selection, &named, &target).await;
    let cleanup = runtime.remove(&target).await;
    result?;
    cleanup?;
    Ok(())
}

async fn exercise(
    runtime: &dyn TestRuntime,
    stacks: &StackManager,
    selection: &WorkspaceSelection,
    named: &NamedTarget,
    initial: &RuntimeTarget,
) -> Result<(), Box<dyn std::error::Error>> {
    runtime.apply(initial).await?;
    wait_for_services(runtime, initial, &["app", "scheduler"]).await?;
    let overrides = selection.start.join(".config/mm.overrides.yaml");
    fs::write(
        &overrides,
        "modules:\n  app:\n    services:\n      scheduler:\n        environments: [production]\n",
    )?;
    let filtered = stacks.compile(selection)?;
    if filtered.excluded_services.len() != 1 || filtered.excluded_services[0].service != "scheduler"
    {
        return Err("compiled report did not identify the excluded scheduler".into());
    }
    let target = runtime_target(
        &selection.start,
        named,
        filtered.stacks.first().ok_or("missing filtered stack")?,
    );
    if runtime.render(&target).await?.service_names != ["app"] {
        return Err("runtime rendering includes the excluded scheduler".into());
    }
    runtime.apply(&target).await?;
    wait_for_services(runtime, &target, &["app"]).await?;
    fs::write(
        &overrides,
        "modules:\n  app:\n    services:\n      scheduler:\n        environments: []\n",
    )?;
    let restored = stacks.compile(selection)?;
    let target = runtime_target(
        &selection.start,
        named,
        restored.stacks.first().ok_or("missing restored stack")?,
    );
    runtime.apply(&target).await?;
    wait_for_services(runtime, &target, &["app", "scheduler"]).await?;
    eprintln!(
        "{:?}: excluded scheduler removed and reenabled successfully",
        runtime.kind()
    );
    Ok(())
}

async fn wait_for_services(
    runtime: &dyn TestRuntime,
    target: &RuntimeTarget,
    expected: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    for _ in 0..60 {
        let observed = runtime.observe(target).await?;
        let names = observed
            .services
            .iter()
            .filter(|service| service.running_count > 0)
            .map(|service| service.identity.service.as_str())
            .collect::<BTreeSet<_>>();
        if names == expected {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(format!("services did not converge to {expected:?}").into())
}

fn runtime_target(
    root: &std::path::Path,
    named: &NamedTarget,
    artifact: &PreparedStackArtifact,
) -> RuntimeTarget {
    RuntimeTarget {
        root: root.to_path_buf(),
        project_directory: root.to_path_buf(),
        target: Some(named.clone()),
        project_id: artifact.manifest.project_id,
        environment: artifact.manifest.environment.clone(),
        stack_name: artifact.manifest.stack.clone(),
        project_name: artifact.manifest.compose_project_name.clone(),
        artifact_hash: artifact.manifest.artifact_hash.clone(),
        compose_files: artifact.compose_files.clone(),
        user_compose_file_count: artifact.manifest.user_files.len(),
        interpolation_environment: Default::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}
