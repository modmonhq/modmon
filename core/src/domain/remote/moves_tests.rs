use super::*;
use crate::domain::deployment::{LeavingService, NodeProjectPlan};
use crate::domain::placement::PlacedService;

fn placed(service: &str) -> PlacedService {
    PlacedService {
        stack: "app".to_owned(),
        service: service.to_owned(),
    }
}

fn leaving(service: &str, moves_to: &[&str]) -> LeavingService {
    LeavingService {
        stack: "app".to_owned(),
        service: service.to_owned(),
        compose_project_name: "shop-production-app".to_owned(),
        moves_to: moves_to.iter().map(|node| (*node).to_owned()).collect(),
    }
}

fn project(
    node: &str,
    services: &[&str],
    starting: &[&str],
    leaving: Vec<LeavingService>,
) -> NodeProjectPlan {
    NodeProjectPlan {
        node: node.to_owned(),
        services: services.iter().map(|service| placed(service)).collect(),
        starting: starting.iter().map(|service| placed(service)).collect(),
        leaving,
        dropped_dependencies: Vec::new(),
        networks: Vec::new(),
        restart_defaults: Vec::new(),
        shared_volumes: Vec::new(),
        ingress: None,
        scaled: Vec::new(),
        health: Vec::new(),
    }
}

fn moves(projects: &[NodeProjectPlan]) -> Vec<ServiceMove> {
    let sections = projects
        .iter()
        .map(|project| (project.node.as_str(), project))
        .collect::<Vec<_>>();
    moves_between(&sections)
}

#[test]
fn a_service_that_leaves_one_node_for_another_is_a_move_in_the_order_it_happens() {
    let moves = moves(&[
        project("app-1", &[], &[], vec![leaving("web", &["app-2"])]),
        project("app-2", &["web"], &["web"], Vec::new()),
    ]);
    assert_eq!(
        moves,
        [ServiceMove {
            stack: "app".to_owned(),
            service: "web".to_owned(),
            from: "app-1".to_owned(),
            to: vec!["app-2".to_owned()],
            starts_on: vec!["app-2".to_owned()],
        }]
    );
    assert_eq!(
        moves[0].describe(),
        "app/web: starts on app-2, then stops on app-1"
    );
}

#[test]
fn a_service_that_shrinks_to_nodes_that_already_run_it_says_so() {
    let moves = moves(&[
        project("app-1", &[], &[], vec![leaving("web", &["app-2", "app-3"])]),
        project("app-2", &["web"], &[], Vec::new()),
        project("app-3", &["web"], &[], Vec::new()),
    ]);
    assert!(moves[0].starts_on.is_empty());
    assert_eq!(
        moves[0].describe(),
        "app/web: already runs on app-2, app-3; stops on app-1 after those nodes have applied"
    );
}

#[test]
fn a_service_that_is_gone_from_the_environment_is_no_move() {
    assert!(moves(&[project("app-1", &[], &[], vec![leaving("old", &[])])]).is_empty());
    assert!(service_moves(&[]).is_empty());
}

#[test]
fn a_plan_that_leaves_out_the_node_a_service_moves_to_is_refused_with_the_fix() {
    let moves = moves(&[project("app-1", &[], &[], vec![leaving("web", &["app-2"])])]);
    let error = check_moves_are_planned(&moves, &["app-2".to_owned()])
        .err()
        .unwrap_or_default();
    assert!(
        error.contains("service 'web' of stack 'app' moves from app-1 to app-2"),
        "{error}"
    );
    assert!(error.contains("without starting there"), "{error}");
    assert!(error.contains("Add app-2 to --nodes"), "{error}");
    assert!(check_moves_are_planned(&moves, &["app-3".to_owned()]).is_ok());
    assert!(check_moves_are_planned(&moves, &[]).is_ok());
}
