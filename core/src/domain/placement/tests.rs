use super::*;

const NODES: [&str; 3] = ["app-1", "app-2", "db-1"];

fn groups() -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([(
        "web".to_owned(),
        vec!["app-2".to_owned(), "app-1".to_owned()],
    )])
}

fn resolve(yaml: &str) -> Result<Vec<String>, String> {
    let spec: PlacementSpec = serde_yaml_ng::from_str(yaml).map_err(|error| error.to_string())?;
    spec.resolve(&NODES, &groups())
}

#[test]
fn a_node_a_group_all_and_lists_resolve_in_declaration_order() -> Result<(), String> {
    assert_eq!(resolve("db-1")?, ["db-1"]);
    assert_eq!(resolve("web")?, ["app-1", "app-2"]);
    assert_eq!(resolve("all")?, NODES);
    assert_eq!(resolve("[db-1, web]")?, NODES);
    assert_eq!(resolve("[db-1, app-1, db-1]")?, ["app-1", "db-1"]);
    Ok(())
}

#[test]
fn unknown_words_name_the_nodes_and_groups_to_choose_from() {
    let error = resolve("cache-1").err().unwrap_or_default();
    assert!(
        error.contains("'cache-1' is not a node or group"),
        "{error}"
    );
    assert!(error.contains("nodes: app-1, app-2, db-1"), "{error}");
    assert!(error.contains("groups: web"), "{error}");
}

#[test]
fn the_swarm_words_are_ordinary_unknown_names_now() {
    for word in ["managers", "any"] {
        assert!(resolve(word).is_err(), "{word}");
    }
}

#[test]
fn an_empty_list_says_what_omitting_placement_does() {
    let error = resolve("[]").err().unwrap_or_default();
    assert!(error.contains("remove placement"), "{error}");
}

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn modules(
    yaml: &str,
) -> Result<BTreeMap<String, crate::domain::config::ModuleConfig>, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(yaml)
}

const MODULES: &str = "\
shop:
  path: shop
  services:
    web: {stack: app, placement: all}
    worker: {stack: app}
    preview: {stack: app, environments: [staging]}
data:
  path: data
  services:
    db: {stack: data, placement: db-1}
";

fn cluster<'a>(groups: &'a BTreeMap<String, Vec<String>>) -> ClusterNodes<'a> {
    ClusterNodes {
        nodes: &NODES,
        groups,
    }
}

#[test]
fn a_service_runs_on_the_nodes_its_placement_names_and_on_the_first_without_one() -> TestResult {
    let groups = groups();
    let modules = modules(MODULES)?;
    let on = |node: &str| services_on_node(&modules, "production", cluster(&groups), node);

    let first = on("app-1")?;
    assert!(
        first.contains("app", "web"),
        "`all` includes the first node"
    );
    assert!(first.contains("app", "worker"), "no placement: first node");
    assert!(!first.has_service("db"), "db is pinned to db-1");
    assert!(
        !first.has_service("preview"),
        "a service that is off in this environment runs nowhere"
    );

    let second = on("app-2")?;
    assert_eq!(
        second
            .iter()
            .map(|placed| placed.service.as_str())
            .collect::<Vec<_>>(),
        ["web"]
    );
    let third = on("db-1")?;
    assert!(third.contains("app", "web") && third.contains("data", "db"));
    assert!(!third.has_service("worker"));

    let staging = services_on_node(&modules, "staging", cluster(&groups), "app-1")?;
    assert!(staging.has_service("preview"));
    Ok(())
}

#[test]
fn a_group_places_a_service_on_each_of_its_nodes() -> TestResult {
    let groups = groups();
    let modules = modules(
        "shop:\n  path: shop\n  services:\n    web: {stack: app, placement: web}\n    cache: {stack: app, placement: [db-1, app-2]}\n",
    )?;
    let on = |node: &str| services_on_node(&modules, "production", cluster(&groups), node);
    assert!(on("app-1")?.has_service("web") && !on("app-1")?.has_service("cache"));
    assert!(on("app-2")?.has_service("web") && on("app-2")?.has_service("cache"));
    assert!(!on("db-1")?.has_service("web") && on("db-1")?.has_service("cache"));
    Ok(())
}

#[test]
fn a_placement_that_names_no_node_or_group_fails_with_the_service() -> TestResult {
    let groups = groups();
    let modules =
        modules("shop:\n  path: shop\n  services:\n    web: {stack: app, placement: cache-9}\n")?;
    let error = services_on_node(&modules, "production", cluster(&groups), "app-1")
        .err()
        .unwrap_or_default();
    assert!(error.contains("service 'shop.web' placement"), "{error}");
    assert!(
        error.contains("'cache-9' is not a node or group"),
        "{error}"
    );
    Ok(())
}

const CONFIG: &str = "\
schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: shop
target: local
stacks:
  app:
    files: [compose.yaml]
    ingress:
      - {domain: site, service: web}
      - {domain: admin, service: worker}
  data:
    files: [data.yaml]
    ingress: {domain: db}
environments:
  local: {}
  production: {target: production}
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
      - {name: db-1, host: 203.0.113.13}
modules:
  shop:
    path: shop
    services:
      web: {stack: app, placement: all}
      worker: {stack: app}
      preview: {stack: app, environments: [local]}
  data:
    path: data
    services:
      db: {stack: data, placement: db-1}
";

fn view(node: &str) -> Result<crate::domain::config::MmConfig, Box<dyn std::error::Error>> {
    let mut config: crate::domain::config::MmConfig = serde_yaml_ng::from_str(CONFIG)?;
    view_from_node(
        &mut config,
        &NodeScope {
            target: "production".to_owned(),
            node: node.to_owned(),
            environment: "production".to_owned(),
            resolver_files: None,
            mesh_addresses: Default::default(),
        },
    )?;
    Ok(config)
}

#[test]
fn a_node_sees_the_services_placed_elsewhere_as_not_part_of_its_environment() -> TestResult {
    let second = view("app-2")?;
    let shop = &second.modules["shop"].services;
    assert!(shop["web"].is_active("production"));
    assert!(!shop["worker"].is_active("production"));
    assert_eq!(
        shop["worker"].placed_elsewhere.as_deref(),
        Some(&["app-1".to_owned()][..]),
        "the view records where the service runs"
    );
    assert!(
        shop["worker"].is_in_environment("production"),
        "it still belongs to the environment"
    );
    assert_eq!(
        second.modules["data"].services["db"].placed_elsewhere,
        Some(vec!["db-1".to_owned()])
    );
    assert_eq!(
        shop["preview"].placed_elsewhere, None,
        "a service the environment leaves out is not placed anywhere"
    );

    let first = view("app-1")?;
    assert!(first.modules["shop"].services["worker"].is_active("production"));
    Ok(())
}

#[test]
fn a_route_stays_on_the_nodes_that_run_its_service() -> TestResult {
    let routes = |config: &crate::domain::config::MmConfig, stack: &str| {
        config.stacks[stack]
            .ingress
            .routes()
            .into_iter()
            .map(|route| (route.domain.clone(), route.service.clone()))
            .collect::<Vec<_>>()
    };
    let second = view("app-2")?;
    assert_eq!(
        routes(&second, "app"),
        [("site".to_owned(), Some("web".to_owned()))],
        "worker runs on app-1, so its route leaves app-2"
    );
    assert!(routes(&second, "data").is_empty(), "db runs on db-1");

    let third = view("db-1")?;
    assert_eq!(
        routes(&third, "data"),
        [("db".to_owned(), Some("db".to_owned()))],
        "a route without a service names the stack's only one"
    );
    let first = view("app-1")?;
    assert_eq!(routes(&first, "app").len(), 2);
    Ok(())
}

#[test]
fn a_route_without_a_service_in_a_stack_with_several_is_refused_on_every_node() -> TestResult {
    // On a node that runs only one of them the route would otherwise
    // silently mean that one.
    let ambiguous = CONFIG.replace("- {domain: site, service: web}", "- {domain: site}");
    for node in ["app-1", "app-2"] {
        let mut config: crate::domain::config::MmConfig = serde_yaml_ng::from_str(&ambiguous)?;
        let error = view_from_node(
            &mut config,
            &NodeScope {
                target: "production".to_owned(),
                node: node.to_owned(),
                environment: "production".to_owned(),
                resolver_files: None,
                mesh_addresses: Default::default(),
            },
        )
        .err()
        .unwrap_or_default();
        assert!(
            error.contains("route for domain 'site' in stack 'app' names no service"),
            "{node}: {error}"
        );
        assert!(error.contains("(web, worker)"), "{node}: {error}");
        assert!(
            error.contains("set `service:` on the route"),
            "{node}: {error}"
        );
    }
    Ok(())
}

#[test]
fn a_view_needs_a_cluster_target_and_one_of_its_nodes() -> TestResult {
    let scope = |target: &str, node: &str| NodeScope {
        target: target.to_owned(),
        node: node.to_owned(),
        environment: "production".to_owned(),
        resolver_files: None,
        mesh_addresses: Default::default(),
    };
    let mut config: crate::domain::config::MmConfig = serde_yaml_ng::from_str(CONFIG)?;
    let error = view_from_node(&mut config, &scope("staging", "app-1"))
        .err()
        .unwrap_or_default();
    assert!(
        error.contains("targets.staging does not list nodes"),
        "{error}"
    );
    let error = view_from_node(&mut config, &scope("production", "cache-1"))
        .err()
        .unwrap_or_default();
    assert!(
        error.contains("'cache-1' is not one of the target's nodes"),
        "{error}"
    );
    Ok(())
}

#[test]
fn a_node_the_target_does_not_list_is_refused_with_the_nodes_it_has() -> TestResult {
    let groups = groups();
    let error = services_on_node(
        &modules(MODULES)?,
        "production",
        cluster(&groups),
        "cache-1",
    )
    .err()
    .unwrap_or_default();
    assert!(
        error.contains("'cache-1' is not one of the target's nodes"),
        "{error}"
    );
    assert!(error.contains("app-1, app-2, db-1"), "{error}");
    Ok(())
}
