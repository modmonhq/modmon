//! A container or a job that the engine could not start is said in mm's
//! words: what Docker, Podman, and Compose printed, recorded from Docker
//! 29 with Compose 5 and Podman 5, goes through the recognizer, which names
//! the service or job, the cause, and the next step. A cause it does not
//! know keeps the engine's own output.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mm::adapters::runtime::DockerComposeRuntime;
use mm::adapters::runtime::launch_failure::{Launched, explain};
use mm::app::ports::{CommandOutput, CommandSpec, LocalDeploymentRuntime, ProcessRunner};
use mm::domain::error::MmError;
use mm::domain::runtime::{RuntimeKind, RuntimeTarget};
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId};
use mm::domain::workspace::ProjectId;
use uuid::Uuid;
use zeroize::Zeroizing;

const DOCKER_BIND_DIRECTORY_ON_FILE: &str = " Network shop_default Creating \n Network shop_default Created \n Container shop-web-1 Creating \n Container shop-web-1 Created \n Container shop-web-1 Starting \nError response from daemon: failed to create task for container: failed to create shim task: OCI runtime create failed: runc create failed: unable to start container process: error during container init: error mounting \"/srv/shop/config\" to rootfs at \"/app/config.yaml\": mount src=/srv/shop/config, dst=/app/config.yaml, dstFd=/proc/thread-self/fd/14, flags=MS_BIND|MS_REC: not a directory: Are you trying to mount a directory onto a file (or vice-versa)? Check if the specified host path exists and is the expected type\n";

const DOCKER_JOB_VOLUME_ON_FILE: &str = " Volume shop_masked Creating \n Volume shop_masked Created \n Container mm-job-7 Creating \n Container mm-job-7 Created \nError response from daemon: failed to create task for container: failed to create shim task: OCI runtime create failed: runc create failed: unable to start container process: error during container init: error mounting \"/var/lib/docker/volumes/mm-dev-a57dbada3aade81c/_data\" to rootfs at \"/app/.env\": mount src=/var/lib/docker/volumes/mm-dev-a57dbada3aade81c/_data, dst=/app/.env, dstFd=/proc/thread-self/fd/14, flags=MS_BIND|MS_REC: not a directory: Are you trying to mount a directory onto a file (or vice-versa)? Check if the specified host path exists and is the expected type\nfailed to start containers: 52c941d0d9b4e60878b9e417185ca908f359521b487b4ed851fe57785c02008d\n";

const PODMAN_VOLUME_ON_FILE: &str = " Container shop-web-1 Creating \n Container shop-web-1 Created \n Container shop-web-1 Starting \nError response from daemon: crun: mount `/var/lib/containers/storage/volumes/shop_masked/_data` to `app/.env`: Not a directory: OCI runtime error\n";

const PODMAN_FILE_ON_DIRECTORY: &str = "Error response from daemon: crun: mount `/srv/shop/settings.ini` to `etc/app`: Not a directory: OCI runtime error\nfailed to start containers: 198a4ceb514317cb44a4f09404c7ea90d24c22b9f5542edf12fc259608e47dde\n";

const DOCKER_LOCAL_IMAGE_MISSING: &str = " Network shop_default Created \n Container shop-web-1 Creating \n service:web:1 Error response from daemon: No such image: mm.invalid/shop/web:local-0a1b2c3d4e5f \nError response from daemon: No such image: mm.invalid/shop/web:local-0a1b2c3d4e5f\n";

const PODMAN_IMAGE_MISSING: &str = " Container shop-api-1 Creating \n service:api:1 Error response from daemon: no such image: registry.example.com/shop/api:1.4: image not known \nError response from daemon: no such image: registry.example.com/shop/api:1.4: image not known\n";

const DOCKER_REGISTRY_UNREACHABLE: &str = " Image registry.example.com/shop/web:2 Pulling \n Image registry.example.com/shop/web:2 Error failed to resolve reference \"registry.example.com/shop/web:2\": failed to do request: Head \"https://registry.example.com/v2/shop/web/manifests/2\": dial tcp: lookup registry.example.com: no such host\nError response from daemon: failed to resolve reference \"registry.example.com/shop/web:2\": failed to do request: Head \"https://registry.example.com/v2/shop/web/manifests/2\": dial tcp: lookup registry.example.com: no such host\n";

const PODMAN_REGISTRY_UNREACHABLE: &str = " Image registry.example.com/shop/web:2 Pulling \n Image registry.example.com/shop/web:2 Error lookup registry.example.com: no such host\nError response from daemon: lookup registry.example.com: no such host\n";

const DOCKER_PULL_DENIED: &str = "Error response from daemon: pull access denied for shop/worker, repository does not exist or may require 'docker login': denied: requested access to the resource is denied\n";

const DOCKER_TAG_UNKNOWN: &str = "Error response from daemon: manifest for ghcr.io/example/api:9.9 not found: manifest unknown: manifest unknown\n";

const DOCKER_NETWORK_MISSING: &str = " Container shop-web-1 Creating \n Container shop-web-1 Created \n Container shop-web-1 Starting \nError response from daemon: failed to set up container networking: Could not attach to network edge: rpc error: code = NotFound desc = network edge not found\n";

const COMPOSE_EXTERNAL_NETWORK_MISSING: &str =
    "network edge declared as external, but could not be found\n";

const PODMAN_NETWORK_MISSING: &str =
    "Error: unable to find network with name or ID edge: network not found\n";

const DOCKER_PORT_ALLOCATED: &str = " Container shop-web-1 Creating \n Container shop-web-1 Created \n Container shop-web-1 Starting \nError response from daemon: failed to set up container networking: driver failed programming external connectivity on endpoint shop-web-1 (bd45a72b85729c04774b26b97165e9529289317c4285acadc2af46048225c09e): Bind for 127.0.0.1:8080 failed: port is already allocated\n";

const DOCKER_PORT_HELD_BY_PROGRAM: &str = " Container shop-api-1 Starting \nError response from daemon: failed to set up container networking: driver failed programming external connectivity on endpoint shop-api-1 (94d6d0945e925b3866d9adcc6859dc1d967bdbbb520c14d17bca992e70167cb9): failed to bind host port 127.0.0.1:8443/tcp: address already in use\n";

const PODMAN_ROOTLESS_PORT: &str = "Error response from daemon: rootlessport listen tcp 127.0.0.1:8080: bind: address already in use\n";

const PODMAN_PASTA_PORT: &str = "Error: pasta failed with exit code 1:\nListen failed for HOST TCP port 127.0.0.1/8080: Address already in use\nCouldn't listen on requested ports\n";

const DOCKER_NAME_IN_USE: &str = " Container shop-taken Creating \n service:web:1 Error response from daemon: Conflict. The container name \"/shop-taken\" is already in use by container \"4ffb610a8cb2dee43200130a1adafcf367c5438e4916cf6241fc9adbab126a4d\". You have to remove (or rename) that container to be able to reuse that name. \nError response from daemon: Conflict. The container name \"/shop-taken\" is already in use by container \"4ffb610a8cb2dee43200130a1adafcf367c5438e4916cf6241fc9adbab126a4d\". You have to remove (or rename) that container to be able to reuse that name.\n";

const PODMAN_NAME_IN_USE: &str = " service:web:1 Error response from daemon: container create: creating container storage: the container name \"shop-taken\" is already in use by 8bd4509427ceb05eabd5076b195d1f42d34cee3558d528c2231e8726ad30bf21. You have to remove that container to be able to reuse that name: that name is already in use \n";

const UNKNOWN: &str = " Container shop-web-1 Starting \nError response from daemon: failed to create task for container: something this engine never said before\n";

fn stack() -> Launched<'static> {
    Launched::Services {
        stack: "app",
        project: "shop",
        services: &[],
    }
}

fn job() -> Launched<'static> {
    Launched::Job {
        job: "migrate",
        service: "api",
    }
}

fn said(
    runtime: RuntimeKind,
    launched: &Launched<'_>,
    stderr: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let message = explain(runtime, launched, stderr).ok_or("the recognizer did not know it")?;
    assert!(!message.contains("Error response"), "{message}");
    assert!(!message.contains("runc"), "{message}");
    assert!(!message.contains("crun"), "{message}");
    Ok(message)
}

fn has(message: &str, parts: &[&str]) {
    for part in parts {
        assert!(message.contains(part), "missing {part:?} in: {message}");
    }
}

#[test]
fn a_mount_of_a_directory_onto_a_file_names_the_path_and_what_to_change()
-> Result<(), Box<dyn std::error::Error>> {
    let message = said(
        RuntimeKind::DockerCompose,
        &stack(),
        DOCKER_BIND_DIRECTORY_ON_FILE,
    )?;
    has(
        &message,
        &[
            "service 'web' of stack 'app' could not start",
            "its mount at /app/config.yaml puts a directory onto a file or a file onto a directory",
            "`volumes:`",
            "deploy again",
        ],
    );
    assert!(!message.contains("/srv/shop/config"), "{message}");

    let message = said(
        RuntimeKind::DockerCompose,
        &job(),
        DOCKER_JOB_VOLUME_ON_FILE,
    )?;
    has(
        &message,
        &[
            "job 'migrate' (service 'api') could not start",
            "the volume 'mm-dev-a57dbada3aade81c' is a directory, and /app/.env in the container is a file",
            "run the job again",
        ],
    );

    let message = said(RuntimeKind::PodmanCompose, &stack(), PODMAN_VOLUME_ON_FILE)?;
    has(
        &message,
        &[
            "service 'web' of stack 'app'",
            "the volume 'shop_masked' is a directory, and /app/.env in the container is a file",
        ],
    );
    let message = said(
        RuntimeKind::PodmanCompose,
        &stack(),
        PODMAN_FILE_ON_DIRECTORY,
    )?;
    has(
        &message,
        &[
            "a service of stack 'app' could not start",
            "its mount at /etc/app puts a directory onto a file or a file onto a directory",
        ],
    );
    Ok(())
}

#[test]
fn an_image_that_is_missing_or_cannot_be_pulled_names_the_image_and_why()
-> Result<(), Box<dyn std::error::Error>> {
    let message = said(
        RuntimeKind::DockerCompose,
        &stack(),
        DOCKER_LOCAL_IMAGE_MISSING,
    )?;
    has(
        &message,
        &[
            "service 'web' of stack 'app' could not start",
            "its image mm.invalid/shop/web:local-0a1b2c3d4e5f is not in the engine",
            "`mm dev up`",
            "`mm build plan`",
        ],
    );

    let message = said(RuntimeKind::PodmanCompose, &stack(), PODMAN_IMAGE_MISSING)?;
    has(
        &message,
        &[
            "service 'api' of stack 'app'",
            "its image registry.example.com/shop/api:1.4 is not on this host",
            "`podman pull registry.example.com/shop/api:1.4`",
        ],
    );

    for (runtime, sample) in [
        (RuntimeKind::DockerCompose, DOCKER_REGISTRY_UNREACHABLE),
        (RuntimeKind::PodmanCompose, PODMAN_REGISTRY_UNREACHABLE),
    ] {
        let message = said(runtime, &stack(), sample)?;
        has(
            &message,
            &[
                "its image registry.example.com/shop/web:2 could not be pulled",
                "its registry could not be reached from this host",
            ],
        );
    }

    let message = said(RuntimeKind::DockerCompose, &job(), DOCKER_PULL_DENIED)?;
    has(
        &message,
        &[
            "job 'migrate' (service 'api') could not start",
            "its image shop/worker could not be pulled: its registry refused it",
            "`docker login docker.io`",
        ],
    );
    let message = said(RuntimeKind::DockerCompose, &stack(), DOCKER_TAG_UNKNOWN)?;
    has(
        &message,
        &[
            "its image ghcr.io/example/api:9.9 could not be pulled",
            "no such image or tag",
        ],
    );
    Ok(())
}

#[test]
fn a_missing_network_a_taken_port_and_a_taken_name_are_said_with_the_next_step()
-> Result<(), Box<dyn std::error::Error>> {
    let message = said(RuntimeKind::DockerCompose, &stack(), DOCKER_NETWORK_MISSING)?;
    has(
        &message,
        &[
            "service 'web' of stack 'app' could not start",
            "the network 'edge' does not exist on this host",
            "`docker network create edge`",
        ],
    );
    for sample in [COMPOSE_EXTERNAL_NETWORK_MISSING, PODMAN_NETWORK_MISSING] {
        let message = said(RuntimeKind::PodmanCompose, &stack(), sample)?;
        has(
            &message,
            &[
                "the network 'edge' does not exist",
                "`podman network create edge`",
            ],
        );
    }

    let message = said(RuntimeKind::DockerCompose, &stack(), DOCKER_PORT_ALLOCATED)?;
    has(
        &message,
        &[
            "service 'web' of stack 'app' could not start",
            "port 8080 is already in use on this host",
            "`docker ps`",
            "`ports:`",
        ],
    );
    let message = said(
        RuntimeKind::DockerCompose,
        &stack(),
        DOCKER_PORT_HELD_BY_PROGRAM,
    )?;
    has(&message, &["service 'api' of stack 'app'", "port 8443"]);
    for sample in [PODMAN_ROOTLESS_PORT, PODMAN_PASTA_PORT] {
        let message = said(RuntimeKind::PodmanCompose, &stack(), sample)?;
        has(&message, &["port 8080 is already in use", "`podman ps`"]);
    }

    let message = said(RuntimeKind::DockerCompose, &stack(), DOCKER_NAME_IN_USE)?;
    has(
        &message,
        &[
            "service 'web' of stack 'app' could not start",
            "another container is already named 'shop-taken'",
            "`docker rm --force shop-taken`",
            "`container_name`",
        ],
    );
    let message = said(RuntimeKind::PodmanCompose, &job(), PODMAN_NAME_IN_USE)?;
    has(
        &message,
        &[
            "job 'migrate' (service 'api')",
            "`podman rm --force shop-taken`",
        ],
    );
    assert!(!message.contains("container_name"), "{message}");
    Ok(())
}

#[test]
fn a_cause_the_recognizer_does_not_know_is_left_to_the_engine() {
    assert_eq!(explain(RuntimeKind::DockerCompose, &stack(), UNKNOWN), None);
    assert_eq!(explain(RuntimeKind::PodmanCompose, &job(), ""), None);
}

struct Engine {
    answers: Mutex<VecDeque<CommandOutput>>,
}

#[async_trait::async_trait]
impl ProcessRunner for Engine {
    async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
        self.answers
            .lock()
            .map_err(|_| MmError::State {
                message: "the fake engine's lock was poisoned".to_owned(),
            })?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "the fake engine has no answer left".to_owned(),
            })
    }
}

fn answer(code: i32, stderr: &str) -> CommandOutput {
    CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(String::new()),
        stderr: Zeroizing::new(stderr.to_owned()),
        truncated: false,
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
        environment: "local".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "shop".to_owned(),
        artifact_hash: "fixture-hash".to_owned(),
        compose_files: vec![root.join("compose.yaml")],
        user_compose_file_count: 1,
        interpolation_environment: Default::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

async fn deploy(stderr: &str) -> Result<MmError, Box<dyn std::error::Error>> {
    let engine = Arc::new(Engine {
        answers: Mutex::new(VecDeque::from([answer(0, ""), answer(1, stderr)])),
    });
    let error = DockerComposeRuntime::new(engine)
        .apply_services(&target(), &[], &BTreeMap::new())
        .await
        .err()
        .ok_or("compose up succeeded")?;
    Ok(error)
}

#[tokio::test]
async fn a_deployment_whose_container_cannot_start_fails_in_mm_words()
-> Result<(), Box<dyn std::error::Error>> {
    let error = deploy(DOCKER_BIND_DIRECTORY_ON_FILE).await?;
    assert_eq!(error.code(), "MM_RUNTIME_LAUNCH");
    assert_eq!(error.exit_code(), 3);
    let message = error.agent_message();
    has(
        &message,
        &[
            "service 'web' of stack 'app' could not start",
            "/app/config.yaml",
        ],
    );
    assert_eq!(message, error.to_string());

    let error = deploy(UNKNOWN).await?;
    assert!(
        matches!(error, MmError::ProcessFailed { ref program, .. } if program == "docker"),
        "{error:?}"
    );
    assert!(
        error
            .to_string()
            .contains("something this engine never said")
    );
    assert!(
        !error
            .agent_message()
            .contains("something this engine never said")
    );
    Ok(())
}
