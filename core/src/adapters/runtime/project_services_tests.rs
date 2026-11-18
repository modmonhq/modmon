use std::sync::Mutex;

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// An engine with a fixed set of containers `(id, service, one-off)` in
/// project `shop`, that records every command line it is given.
struct Engine {
    name: &'static str,
    containers: Mutex<Vec<(&'static str, &'static str, bool)>>,
    networks: Vec<&'static str>,
    commands: Mutex<Vec<String>>,
}

impl Engine {
    fn new(name: &'static str, containers: &[(&'static str, &'static str, bool)]) -> Self {
        Self {
            name,
            containers: Mutex::new(containers.to_vec()),
            networks: vec!["net1"],
            commands: Mutex::new(Vec::new()),
        }
    }

    fn commands(&self) -> Vec<String> {
        self.commands
            .lock()
            .map(|commands| commands.clone())
            .unwrap_or_default()
    }

    fn output(stdout: String) -> CommandOutput {
        CommandOutput {
            code: Some(0),
            stdout: zeroize::Zeroizing::new(stdout),
            stderr: zeroize::Zeroizing::new(String::new()),
            truncated: false,
        }
    }
}

#[async_trait]
impl ContainerEngine for Engine {
    fn engine_name(&self) -> &'static str {
        self.name
    }

    async fn engine(
        &self,
        args: Vec<String>,
        _timeout: Duration,
    ) -> Result<CommandOutput, MmError> {
        let line = args.join(" ");
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(line.clone());
        }
        let mut containers = self.containers.lock().map_err(|_| MmError::State {
            message: "test engine lock was poisoned".to_owned(),
        })?;
        let stdout = match args.first().map(String::as_str) {
            Some("ps") => {
                let service = args
                    .iter()
                    .find_map(|arg| arg.strip_prefix("label=com.docker.compose.service="));
                let rows = containers
                    .iter()
                    .filter(|(_, name, _)| service.is_none_or(|service| service == *name))
                    .map(|(id, ..)| {
                        // Docker prints one object per line with `ID`,
                        // Podman one array with `Id`.
                        if self.name == "Docker" {
                            format!("{{\"ID\":\"{id}\"}}")
                        } else {
                            format!("{{\"Id\":\"{id}\"}}")
                        }
                    })
                    .collect::<Vec<_>>();
                if self.name == "Docker" {
                    rows.join("\n")
                } else {
                    format!("[{}]", rows.join(","))
                }
            }
            Some("container") => {
                let id = args.last().cloned().unwrap_or_default();
                let (_, service, one_off) = containers
                    .iter()
                    .find(|(candidate, ..)| *candidate == id)
                    .copied()
                    .unwrap_or_default();
                let one_off = if one_off { "True" } else { "False" };
                format!(
                    "[{{\"Config\":{{\"Labels\":{{\"com.docker.compose.service\":\"{service}\",\"com.docker.compose.oneoff\":\"{one_off}\"}}}}}}]"
                )
            }
            Some("rm") => {
                containers.retain(|(id, ..)| !args.iter().any(|arg| arg == id));
                String::new()
            }
            Some("network") if args.get(1).map(String::as_str) == Some("ls") => self
                .networks
                .iter()
                .map(|id| format!("{{\"ID\":\"{id}\"}}"))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        };
        Ok(Self::output(stdout))
    }
}

const CONTAINERS: [(&str, &str, bool); 4] = [
    ("c1", "web", false),
    ("c2", "worker", false),
    ("c3", "worker", false),
    ("c4", "migrate", true),
];

#[tokio::test]
async fn services_are_read_from_container_labels_on_both_engines() -> TestResult {
    for name in ["Docker", "Podman"] {
        let engine = Engine::new(name, &CONTAINERS);
        let services = project_services(&engine, "shop").await?;
        assert_eq!(
            services.into_iter().collect::<Vec<_>>(),
            ["web", "worker"],
            "{name}: a `compose run` container is a job, not a service"
        );
        assert!(
            engine.commands()[0]
                .starts_with("ps --all --filter label=com.docker.compose.project=shop"),
            "{name}"
        );
    }
    assert!(
        project_services(&Engine::new("Docker", &[]), "shop")
            .await?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn a_service_is_stopped_then_removed_and_the_rest_keeps_running() -> TestResult {
    for name in ["Docker", "Podman"] {
        let engine = Engine::new(name, &CONTAINERS);
        remove_services(&engine, "shop", &["worker".to_owned()], false).await?;
        let commands = engine.commands();
        let stop = commands.iter().position(|command| command == "stop c2 c3");
        let remove = commands
            .iter()
            .position(|command| command == "rm --force c2 c3");
        assert!(stop.is_some() && stop < remove, "{name}: {commands:?}");
        assert!(
            !commands
                .iter()
                .any(|command| command.starts_with("network")),
            "{name}: other services still use the project's networks"
        );
        assert_eq!(
            project_services(&engine, "shop")
                .await?
                .into_iter()
                .collect::<Vec<_>>(),
            ["web"]
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_last_service_of_a_project_takes_its_networks_and_no_volume() -> TestResult {
    let engine = Engine::new("Docker", &[("c1", "web", false)]);
    remove_services(&engine, "shop", &["web".to_owned()], true).await?;
    let commands = engine.commands();
    assert!(
        commands.contains(&"network rm net1".to_owned()),
        "{commands:?}"
    );
    assert!(
        !commands.iter().any(|command| command.contains("volume")),
        "{commands:?}"
    );

    // With a service left, the networks stay even for a whole-project call.
    let engine = Engine::new("Docker", &CONTAINERS);
    remove_services(&engine, "shop", &["worker".to_owned()], true).await?;
    assert!(
        !engine
            .commands()
            .iter()
            .any(|command| command.starts_with("network rm"))
    );

    // Nothing to remove is not an error: the step can run twice.
    let engine = Engine::new("Podman", &[]);
    remove_services(&engine, "shop", &["web".to_owned()], false).await?;
    assert!(
        !engine
            .commands()
            .iter()
            .any(|command| command.starts_with("stop"))
    );
    Ok(())
}
