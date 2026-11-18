use super::*;
use crate::domain::deployment::{DroppedDependency, LeavingService};

fn placed(service: &str) -> PlacedService {
    PlacedService {
        stack: "app".to_owned(),
        service: service.to_owned(),
    }
}

#[test]
fn a_node_section_lists_what_runs_starts_stops_and_was_left_out() {
    let node = NodeProjectPlan {
        node: "app-1".to_owned(),
        services: vec![placed("api"), placed("web")],
        starting: vec![placed("api")],
        leaving: vec![
            LeavingService {
                stack: "app".to_owned(),
                service: "worker".to_owned(),
                compose_project_name: "shop-production-app".to_owned(),
                moves_to: vec!["app-2".to_owned()],
            },
            LeavingService {
                stack: "app".to_owned(),
                service: "legacy".to_owned(),
                compose_project_name: "shop-production-app".to_owned(),
                moves_to: Vec::new(),
            },
        ],
        dropped_dependencies: vec![DroppedDependency {
            stack: "app".to_owned(),
            service: "web".to_owned(),
            depends_on: "db".to_owned(),
            nodes: vec!["db-1".to_owned()],
        }],
        networks: Vec::new(),
        restart_defaults: Vec::new(),
        shared_volumes: Vec::new(),
        ingress: None,
        scaled: Vec::new(),
        health: Vec::new(),
    };
    assert_eq!(
        node_project_lines(&node),
        [
            "  services: app/api, app/web",
            "  starts here: app/api",
            "  stops here: app/worker, after it runs on app-2 (last step of the apply)",
            "  removed here: app/legacy (no node runs it any more)",
            "  depends_on not kept: app/web depends on db, which runs on db-1. An engine cannot wait for a service on another node, so web starts without waiting for it",
        ]
    );
}

#[test]
fn a_node_that_only_loses_services_says_it_runs_none() {
    let node = NodeProjectPlan {
        node: "app-1".to_owned(),
        services: Vec::new(),
        starting: Vec::new(),
        leaving: Vec::new(),
        dropped_dependencies: Vec::new(),
        networks: Vec::new(),
        restart_defaults: Vec::new(),
        shared_volumes: Vec::new(),
        ingress: None,
        scaled: Vec::new(),
        health: Vec::new(),
    };
    assert_eq!(node_project_lines(&node), ["  services: none"]);
}

#[test]
fn moves_are_listed_in_the_order_they_happen() {
    let moves = [ServiceMove {
        stack: "app".to_owned(),
        service: "worker".to_owned(),
        from: "app-1".to_owned(),
        to: vec!["app-2".to_owned()],
        starts_on: vec!["app-2".to_owned()],
    }];
    assert_eq!(
        move_lines(&moves),
        ["    app/worker: starts on app-2, then stops on app-1"]
    );
}
