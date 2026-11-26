use mm::domain::mcp::{McpSelectedScope, McpServiceRestartPlan};
use mm::domain::runtime::RuntimeKind;
use mm::domain::workspace::WorkspaceSnapshot;

fn scope() -> McpSelectedScope {
    McpSelectedScope {
        environment: "uat".to_owned(),
        target: "uat".to_owned(),
        runtime: RuntimeKind::PodmanCompose,
    }
}

fn container(name: &str, id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id, "name": name, "service": "api", "image": "registry.test/api:1",
        "state": "running", "status": "Up", "health": "healthy", "labels": {},
    })
}

fn snapshot(api_containers: &[serde_json::Value]) -> Result<WorkspaceSnapshot, serde_json::Error> {
    let service = |name: &str, containers: &[serde_json::Value], nodes: serde_json::Value| {
        serde_json::json!({
            "stack": "app", "name": name, "module": "shop", "state": "running",
            "desired_count": containers.len(), "running_count": containers.len(),
            "healthy_count": containers.len(), "containers": containers, "nodes": nodes,
        })
    };
    let node = |name: &str, desired: usize| {
        serde_json::json!({
            "node": name, "state": "running", "desired_count": desired,
            "running_count": desired, "healthy_count": desired,
        })
    };
    serde_json::from_value(serde_json::json!({
        "workspace": {
            "id": "00000000-0000-0000-0000-000000000001",
            "project_id": "00000000-0000-0000-0000-000000000002",
            "root": "/srv/shop", "config_path": "/srv/shop/.config/mm.yaml",
        },
        "project_name": "shop", "target": "uat", "environment": "uat",
        "config_hash": "c".repeat(64),
        "runtime": {
            "kind": "podman-compose", "available": true, "engine_version": null,
            "compose_provider": null, "rootless": null, "socket_path": null,
            "capabilities": [], "diagnostics": [],
        },
        "validation": {"diagnostics": []},
        "stacks": [{
            "name": "app", "compose_project_name": "shop-uat-app", "artifact_hash": "a",
            "rendered_config_hash": "r",
            "services": [
                service(
                    "api",
                    api_containers,
                    serde_json::json!([node("one", 1), node("two", 1), node("three", 0)]),
                ),
                service("worker", &[], serde_json::json!([])),
            ],
        }],
        "recent_operations": [],
    }))
}

#[test]
fn a_restart_plan_names_the_service_its_containers_and_nodes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut replaced = container("shop-uat-app-api-1", "id-0");
    replaced["history"] = true.into();
    let containers = [
        container("shop-uat-app-api-2", "id-2"),
        container("shop-uat-app-api-1", "id-1"),
        replaced,
    ];
    let plan = McpServiceRestartPlan::of(&scope(), true, &snapshot(&containers)?, "api")?;
    assert_eq!(plan.service, "api");
    assert_eq!(plan.stack, "app");
    assert_eq!(plan.module.as_deref(), Some("shop"));
    assert_eq!(plan.target, "uat");
    assert!(plan.remote);
    assert_eq!(
        plan.containers,
        ["shop-uat-app-api-1", "shop-uat-app-api-2"]
    );
    assert_eq!(plan.nodes, ["one", "two"]);
    assert_eq!(plan.plan_hash.len(), 64);

    let again = McpServiceRestartPlan::of(&scope(), true, &snapshot(&containers)?, "api")?;
    assert_eq!(again.plan_hash, plan.plan_hash);
    let recreated = [
        container("shop-uat-app-api-2", "id-2"),
        container("shop-uat-app-api-1", "id-3"),
    ];
    let changed = McpServiceRestartPlan::of(&scope(), true, &snapshot(&recreated)?, "api")?;
    assert_ne!(changed.plan_hash, plan.plan_hash);
    Ok(())
}

#[test]
fn an_unknown_or_stopped_service_is_refused_with_what_to_name()
-> Result<(), Box<dyn std::error::Error>> {
    let snapshot = snapshot(&[container("shop-uat-app-api-1", "id-1")])?;
    let unknown = McpServiceRestartPlan::of(&scope(), false, &snapshot, "web")
        .err()
        .ok_or("an unknown service was planned")?;
    assert_eq!(unknown.code(), "MM_ENVIRONMENT");
    let message = unknown.agent_message();
    assert!(
        message.contains("service 'web' is not in environment 'uat'; name one of: api, worker"),
        "{message}"
    );
    let stopped = McpServiceRestartPlan::of(&scope(), false, &snapshot, "worker")
        .err()
        .ok_or("a service without containers was planned")?;
    let message = stopped.agent_message();
    assert!(
        message.contains("runs no container") && message.contains("mm_deploy_plan"),
        "{message}"
    );
    Ok(())
}
