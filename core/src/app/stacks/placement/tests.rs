use std::fs;

use serde_yaml_ng::Value;

use super::*;
use crate::adapters::config::ConfigLoader;
use crate::domain::placement::{NodeScope, view_from_node};
use crate::domain::target::{TargetDefinition, TargetId};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const COMPOSE: &str = "\
services:
  web:
    image: example/web
    depends_on: [db, worker]
    networks: [front]
    volumes:
      - webdata:/data
      - ./web/site.conf:/etc/site.conf:ro
  worker:
    image: example/worker
    depends_on:
      db: {condition: service_started}
  db:
    image: example/db
    networks: [back]
    volumes: [dbdata:/var/lib/db]
    secrets: [db-password]
networks:
  front: {}
  back: {external: true, name: backbone}
volumes:
  webdata: {}
  dbdata: {}
secrets:
  db-password: {file: ./secrets/db-password}
";

/// A project on a three-node cluster: `web` on every node, `worker` on the
/// first, `db` pinned to `db-1`.
struct Cluster {
    root: tempfile::TempDir,
    target: NamedTarget,
}

impl Cluster {
    fn new(compose: &str, services: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let root = crate::test_support::tempdir()?;
        fs::create_dir_all(root.path().join(".config"))?;
        for directory in ["shop", "web", "secrets"] {
            fs::create_dir_all(root.path().join(directory))?;
        }
        fs::write(root.path().join("web/site.conf"), "server {}\n")?;
        fs::write(
            root.path().join("secrets/db-password"),
            "not-a-real-password\n",
        )?;
        fs::write(root.path().join("compose.yaml"), compose)?;
        fs::write(
            root.path().join(".config/mm.yaml"),
            format!(
                "schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: shop
default-environment: production
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  shop:
    path: shop
    services:
{services}
targets:
  cluster:
    nodes:
      - {{name: app-1, host: 203.0.113.11}}
      - {{name: app-2, host: 203.0.113.12}}
      - {{name: db-1, host: 203.0.113.13}}
environments:
  production:
    target: cluster
"
            ),
        )?;
        let target = NamedTarget::new(
            "local".to_owned(),
            TargetDefinition::local(TargetId(uuid::Uuid::parse_str(
                "319c527b-6242-7b43-bfd4-bd60e27c1de3",
            )?)),
        );
        Ok(Self { root, target })
    }

    fn shop() -> Result<Self, Box<dyn std::error::Error>> {
        Self::new(
            COMPOSE,
            "      web: {stack: app, placement: all}\n      worker: {stack: app}\n      db: {stack: app, placement: db-1}",
        )
    }

    /// What `node` compiles from the commit: its artifacts and notes.
    fn on(&self, node: &str) -> Result<(Vec<CompiledStackArtifact>, PlacementNotes), MmError> {
        let mut document = ConfigLoader.load_from(self.root.path())?;
        view_from_node(
            &mut document.config,
            &NodeScope {
                target: "cluster".to_owned(),
                node: node.to_owned(),
                environment: "production".to_owned(),
                resolver_files: None,
                mesh_addresses: Default::default(),
            },
        )
        .map_err(|message| MmError::Placement { message })?;
        compile_document_for(DocumentCompilation {
            document: &document,
            environment: "production",
            target: &self.target,
            images: &BTreeMap::new(),
            middlewares: &load_active_middlewares(&document, "production")?,
            networks: &active_network_membership(&document, "production")?,
            config_files: std::path::Path::new("config-files"),
            user_ids: None,
        })
    }
}

fn model(artifact: &CompiledStackArtifact) -> Result<Value, serde_yaml_ng::Error> {
    serde_yaml_ng::from_slice(&artifact.user_files[0].bytes)
}

fn keys(value: &Value, section: &str) -> Vec<String> {
    value
        .get(section)
        .and_then(Value::as_mapping)
        .map(|entries| {
            entries
                .keys()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_node_renders_only_its_services_and_only_the_resources_they_use() -> TestResult {
    let cluster = Cluster::shop()?;
    let (artifacts, notes) = cluster.on("app-2")?;
    assert_eq!(artifacts.len(), 1);
    let compose = model(&artifacts[0])?;
    assert_eq!(keys(&compose, "services"), ["web"]);
    assert_eq!(
        artifacts[0]
            .manifest
            .services
            .iter()
            .map(|service| service.service.as_str())
            .collect::<Vec<_>>(),
        ["web"]
    );
    assert_eq!(
        keys(&compose, "networks"),
        ["front"],
        "`back` is db's alone"
    );
    assert_eq!(keys(&compose, "volumes"), ["webdata"]);
    assert!(
        compose.get("secrets").is_none(),
        "db's secret stays on db-1"
    );
    assert_eq!(
        notes.elsewhere,
        BTreeMap::from([
            (("app".to_owned(), "db".to_owned()), vec!["db-1".to_owned()]),
            (
                ("app".to_owned(), "worker".to_owned()),
                vec!["app-1".to_owned()]
            ),
        ])
    );
    assert!(notes.projects.contains_key("app"));

    let (database, _) = cluster.on("db-1")?;
    let compose = model(&database[0])?;
    assert_eq!(keys(&compose, "services"), ["web", "db"]);
    assert_eq!(keys(&compose, "networks"), ["front", "back"]);
    assert_eq!(keys(&compose, "volumes"), ["webdata", "dbdata"]);
    assert_eq!(keys(&compose, "secrets"), ["db-password"]);
    Ok(())
}

#[test]
fn a_dependency_on_another_node_is_dropped_and_reported() -> TestResult {
    let cluster = Cluster::shop()?;
    let (artifacts, notes) = cluster.on("app-2")?;
    let compose = model(&artifacts[0])?;
    assert!(
        compose["services"]["web"].get("depends_on").is_none(),
        "both dependencies run elsewhere, so the key goes: {compose:?}"
    );
    let dropped = notes
        .dropped_dependencies
        .iter()
        .map(|note| {
            (
                note.service.as_str(),
                note.depends_on.as_str(),
                note.nodes.join(","),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        dropped,
        [
            ("web", "db", "db-1".to_owned()),
            ("web", "worker", "app-1".to_owned())
        ]
    );

    // On the first node worker is local: web keeps that dependency, and
    // worker's own long-form dependency on db is dropped.
    let (first, notes) = cluster.on("app-1")?;
    let compose = model(&first[0])?;
    assert_eq!(
        compose["services"]["web"]["depends_on"],
        serde_yaml_ng::from_str::<Value>("[worker]")?
    );
    assert!(compose["services"]["worker"].get("depends_on").is_none());
    assert_eq!(notes.dropped_dependencies.len(), 2);
    Ok(())
}

#[test]
fn a_relative_bind_mount_resolves_in_each_nodes_own_checkout() -> TestResult {
    let cluster = Cluster::shop()?;
    // Every node checks the commit out for itself, so the file is there on
    // each of them: the mount is rewritten to that node's own copy.
    let checkout = ConfigLoader.load_from(cluster.root.path())?.root;
    let expected = format!(
        "{}:/etc/site.conf:ro",
        checkout.join("web/site.conf").display()
    );
    for node in ["app-1", "app-2", "db-1"] {
        let (artifacts, _) = cluster.on(node)?;
        let compose = model(&artifacts[0])?;
        let volumes = compose["services"]["web"]["volumes"]
            .as_sequence()
            .cloned()
            .unwrap_or_default();
        assert!(
            volumes.contains(&Value::String(expected.clone())),
            "{node}: the mount points into this node's checkout of the commit: {volumes:?}"
        );
    }
    Ok(())
}

#[test]
fn a_host_path_outside_the_project_is_refused_on_every_node() -> TestResult {
    // No node could be missing such a path, because no plan mounts one.
    let cluster = Cluster::new(
        "services:\n  web:\n    image: example/web\n    volumes: ['/srv/shared:/shared']\n",
        "      web: {stack: app, placement: all}",
    )?;
    for node in ["app-1", "app-2"] {
        let error = cluster
            .on(node)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(
            error.contains("out-of-project volume source"),
            "{node}: {error}"
        );
    }
    Ok(())
}

#[test]
fn a_node_with_nothing_placed_renders_no_project() -> TestResult {
    let cluster = Cluster::new(
        "services:\n  web: {image: example/web}\n",
        "      web: {stack: app, placement: app-2}",
    )?;
    let (artifacts, notes) = cluster.on("app-1")?;
    assert!(artifacts.is_empty());
    assert!(
        notes.projects.contains_key("app"),
        "its project is still known"
    );
    assert_eq!(cluster.on("app-2")?.0.len(), 1);
    Ok(())
}

#[test]
fn a_node_that_runs_the_whole_stack_keeps_the_files_as_written() -> TestResult {
    let compose = "services:\n  web: {image: example/web}  # as written\nvolumes:\n  spare: {}\n";
    let cluster = Cluster::new(compose, "      web: {stack: app}")?;
    let (artifacts, notes) = cluster.on("app-1")?;
    assert_eq!(artifacts[0].user_files[0].bytes, compose.as_bytes());
    assert!(notes.elsewhere.is_empty() && notes.dropped_dependencies.is_empty());
    Ok(())
}

#[test]
fn changing_a_placement_changes_what_each_affected_node_hashes() -> TestResult {
    let hash = |services: &str, node: &str| -> Result<Option<String>, Box<dyn std::error::Error>> {
        let cluster = Cluster::new(
            "services:\n  web: {image: example/web}\n  api: {image: example/api}\n",
            services,
        )?;
        Ok(cluster
            .on(node)?
            .0
            .first()
            .map(|artifact| artifact.manifest.artifact_hash.clone()))
    };
    let both_first = "      web: {stack: app}\n      api: {stack: app}";
    let api_moved = "      web: {stack: app}\n      api: {stack: app, placement: app-2}";
    assert_ne!(hash(both_first, "app-1")?, hash(api_moved, "app-1")?);
    assert_ne!(hash(both_first, "app-2")?, hash(api_moved, "app-2")?);
    assert_eq!(hash(both_first, "db-1")?, None);
    assert_eq!(hash(api_moved, "db-1")?, None);
    Ok(())
}

#[test]
fn services_that_share_an_engine_namespace_must_share_a_node() -> TestResult {
    let cluster = Cluster::new(
        "services:\n  web: {image: example/web, network_mode: 'service:proxy'}\n  proxy: {image: example/proxy}\n",
        "      web: {stack: app}\n      proxy: {stack: app, placement: app-2}",
    )?;
    let error = cluster
        .on("app-1")
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        error.contains("uses `network_mode` with service 'proxy'"),
        "{error}"
    );
    assert!(error.contains("runs on app-2"), "{error}");
    assert!(error.contains("the same `placement`"), "{error}");
    Ok(())
}

#[test]
fn a_resource_named_through_a_variable_keeps_its_kind_whole() -> TestResult {
    let cluster = Cluster::new(
        "services:\n  web:\n    image: example/web\n    networks: ['${EDGE_NETWORK}']\n  db: {image: example/db}\nnetworks:\n  one: {}\n  two: {}\nvolumes:\n  unused: {}\n",
        "      web: {stack: app}\n      db: {stack: app, placement: db-1}",
    )?;
    let (artifacts, _) = cluster.on("app-1")?;
    let compose = model(&artifacts[0])?;
    assert_eq!(keys(&compose, "networks"), ["one", "two"]);
    assert!(compose.get("volumes").is_none());
    Ok(())
}

#[test]
fn a_node_gives_its_services_a_restart_policy_where_the_compose_file_has_none() -> TestResult {
    let cluster = Cluster::new(
        "services:\n  web: {image: example/web}\n  worker: {image: example/worker, restart: 'no'}\n  cron: {image: example/cron, restart: always}\n  db: {image: example/db}\n",
        "      web: {stack: app, placement: all}\n      worker: {stack: app, placement: all}\n      cron: {stack: app}\n      db: {stack: app, placement: db-1}",
    )?;
    let restart_of = |artifact: &CompiledStackArtifact, service: &str| {
        serde_yaml_ng::from_slice::<Value>(&artifact.generated_overlay).map(|overlay| {
            overlay["services"][service]["restart"]
                .as_str()
                .map(str::to_owned)
        })
    };
    let (artifacts, notes) = cluster.on("app-2")?;
    // What the Compose file leaves open gets the default; what it sets, in
    // either direction, is kept; what runs elsewhere is not touched.
    assert_eq!(
        restart_of(&artifacts[0], "web")?.as_deref(),
        Some("unless-stopped")
    );
    assert_eq!(restart_of(&artifacts[0], "worker")?, None);
    assert_eq!(restart_of(&artifacts[0], "db")?, None);
    assert_eq!(
        notes.restart_defaults,
        [("app".to_owned(), "web".to_owned())]
    );
    let (_, first) = cluster.on("app-1")?;
    assert_eq!(
        first.restart_defaults,
        [("app".to_owned(), "web".to_owned())]
    );

    // Anywhere but on a cluster node the Compose file is left as it is.
    let document = ConfigLoader.load_from(cluster.root.path())?;
    let (artifacts, notes) = compile_document_for(DocumentCompilation {
        document: &document,
        environment: "production",
        target: &cluster.target,
        images: &BTreeMap::new(),
        middlewares: &load_active_middlewares(&document, "production")?,
        networks: &active_network_membership(&document, "production")?,
        config_files: std::path::Path::new("config-files"),
        user_ids: None,
    })?;
    assert_eq!(restart_of(&artifacts[0], "web")?, None);
    assert!(notes.restart_defaults.is_empty());
    Ok(())
}
