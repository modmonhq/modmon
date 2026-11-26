//! A `start-first` step on a Docker node while a container of the deploy
//! before still drains (`domain::rollout::beside`).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mm::adapters::runtime::DockerComposeRuntime;
use mm::app::ports::{CommandOutput, CommandSpec, LocalDeploymentRuntime, ProcessRunner};
use mm::domain::error::MmError;
use mm::domain::ownership::{ResourceKind, ResourceOwnership};
use mm::domain::runtime::RuntimeTarget;
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId};
use mm::domain::workspace::ProjectId;
use serde_json::{Value, json};
use uuid::Uuid;
use zeroize::Zeroizing;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// One `whoami` container: its id, configuration hash and creation time.
type Container = (&'static str, &'static str, &'static str);

struct Engine {
    files_hash: &'static str,
    containers: Vec<Container>,
    owner: Value,
    commands: Mutex<Vec<Vec<String>>>,
}

impl Engine {
    fn new(files_hash: &'static str, containers: Vec<Container>) -> Result<Arc<Self>, String> {
        let owner = ResourceOwnership::for_runtime_target(
            &target(),
            ResourceKind::Stack,
            "shop-app",
            "app",
        )
        .ok_or("no ownership")?
        .labels();
        Ok(Arc::new(Self {
            files_hash,
            containers,
            owner: json!(owner),
            commands: Mutex::new(Vec::new()),
        }))
    }

    fn inspect(&self, id: &str) -> Option<Value> {
        let (id, hash, created) = self.containers.iter().find(|(own, ..)| *own == id)?;
        let mut labels = self.owner.clone();
        labels["com.docker.compose.project"] = json!("shop-app");
        labels["com.docker.compose.service"] = json!("whoami");
        labels["com.docker.compose.config-hash"] = json!(hash);
        Some(json!({
            "Id": id,
            "Name": format!("/{id}"),
            "Created": created,
            "Image": format!("sha256:{hash}"),
            "State": {"Running": true},
            "Config": {"Labels": labels},
        }))
    }

    fn scaled(&self) -> Result<Vec<String>, String> {
        Ok(self
            .commands
            .lock()
            .map_err(|_| "poisoned")?
            .iter()
            .filter(|args| args.iter().any(|arg| arg == "up"))
            .filter_map(|args| {
                let at = args.iter().position(|arg| arg == "--scale")?;
                args.get(at + 1).cloned()
            })
            .collect())
    }
}

fn answer(stdout: String) -> CommandOutput {
    CommandOutput {
        code: Some(0),
        stdout: Zeroizing::new(stdout),
        stderr: Zeroizing::new(String::new()),
        truncated: false,
    }
}

#[async_trait::async_trait]
impl ProcessRunner for Engine {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let args = specification.redacted_record().arguments;
        self.commands
            .lock()
            .map_err(|_| MmError::State {
                message: "poisoned".to_owned(),
            })?
            .push(args.clone());
        let words = args.iter().map(String::as_str).collect::<Vec<_>>();
        match words.as_slice() {
            ["ps", ..] => Ok(answer(
                self.containers
                    .iter()
                    .map(|(id, ..)| format!("{{\"ID\":\"{id}\"}}\n"))
                    .collect(),
            )),
            ["container", "inspect", "--format", "json", ids @ ..] => Ok(answer(
                Value::Array(ids.iter().filter_map(|id| self.inspect(id)).collect()).to_string(),
            )),
            ["compose", .., "config", "--hash", "*"] => {
                Ok(answer(format!("whoami {}\n", self.files_hash)))
            }
            ["compose", .., "whoami"] if words.contains(&"up") => Ok(answer(String::new())),
            _ => Err(MmError::State {
                message: format!("the fake engine was not asked for {args:?}"),
            }),
        }
    }
}

fn target() -> RuntimeTarget {
    let root = PathBuf::from("/srv/shop");
    RuntimeTarget {
        root: root.clone(),
        project_directory: root.clone(),
        target: Some(NamedTarget::new(
            "local".to_owned(),
            TargetDefinition::local(TargetId(Uuid::from_u128(7))),
        )),
        project_id: ProjectId(Uuid::from_u128(8)),
        environment: "production".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "shop-app".to_owned(),
        artifact_hash: "fixture-hash".to_owned(),
        compose_files: vec![root.join("compose.yaml")],
        user_compose_file_count: 1,
        interpolation_environment: Default::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

const V110_DRAINING: Container = ("v110-old", "v1.10", "2026-11-26T16:41:00.000000000Z");
const V111: Container = ("v111", "v1.11", "2026-11-26T16:42:58.000000000Z");

#[tokio::test]
async fn going_back_to_the_image_that_still_drains_starts_a_new_container_of_it() -> TestResult {
    let engine = Engine::new("v1.10", vec![V110_DRAINING, V111])?;
    let started = DockerComposeRuntime::new(engine.clone())
        .start_beside(&target(), &["whoami".to_owned()], &["v110-old".to_owned()])
        .await?;
    assert_eq!(started, ["whoami"]);
    assert_eq!(engine.scaled()?, ["whoami=3"]);
    Ok(())
}

#[tokio::test]
async fn the_same_files_again_while_the_old_container_drains_start_nothing() -> TestResult {
    let engine = Engine::new("v1.11", vec![V110_DRAINING, V111])?;
    let started = DockerComposeRuntime::new(engine.clone())
        .start_beside(&target(), &["whoami".to_owned()], &["v110-old".to_owned()])
        .await?;
    assert!(started.is_empty(), "{started:?}");
    assert!(engine.scaled()?.is_empty());
    Ok(())
}
