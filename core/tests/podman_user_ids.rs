//! The user ids each service gets on a host that runs rootful Podman: its
//! own 65536 host ids, the same on every node and from one deployment to
//! the next, and the files mm hands the service given to them.
#![cfg(unix)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm::adapters::config::ConfigLoader;
use mm::adapters::secrets::DockerComposeFileSecretDelivery;
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::adapters::user_namespace::FilesystemServiceUserIds;
use mm::app::ports::{ProjectDocumentStore, RuntimeSecretDelivery, ServiceUserIds, TargetStore};
use mm::app::{StackManager, WorkspaceSelection};
use mm::domain::operation::OperationId;
use mm::domain::placement::NodeScope;
use mm::domain::remote::SharedPidAppArmor;
use mm::domain::runtime::RuntimeKind;
use mm::domain::runtime_secret::{
    RuntimeSecretConsumer, RuntimeSecretMaterial, RuntimeSecretPlan, RuntimeSecretPlanItem,
};
use mm::domain::secret::SecretMaterial;
use mm::domain::secret_contract::DeliveryDriverId;
use mm::domain::target::{TargetDefinition, TargetId};
use mm::domain::user_namespace::{
    DEFAULT_COUNT, DEFAULT_FIRST, HostIdRange, RANGE_OWNER, SERVICE_IDS, ServiceIds,
    ServiceIdsChoice, ServiceIdsOwner, allocate,
};
use mm::domain::workspace::ProjectId;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const PROJECT_ID: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de7";
/// The lines setup writes to `/etc/subuid` and `/etc/subgid`.
const SETUP_LINES: &str = "containers:2147483647:1073741825\nmm-services:3221225472:1073676288\n";

fn owner(project: u128, environment: &str, stack: &str, service: &str) -> ServiceIdsOwner {
    ServiceIdsOwner {
        project: ProjectId(uuid::Uuid::from_u128(project)),
        environment: environment.to_owned(),
        stack: stack.to_owned(),
        service: service.to_owned(),
    }
}

fn set<const N: usize>(owners: [&ServiceIdsOwner; N]) -> BTreeSet<ServiceIdsOwner> {
    owners.into_iter().cloned().collect()
}

/// What a host keeps after `chosen`, on top of `kept`.
fn keep(
    kept: &BTreeMap<ServiceIdsOwner, ServiceIds>,
    chosen: &BTreeMap<ServiceIdsOwner, ServiceIdsChoice>,
) -> BTreeMap<ServiceIdsOwner, ServiceIds> {
    let mut next = kept.clone();
    next.extend(
        chosen
            .iter()
            .map(|(owner, choice)| (owner.clone(), choice.ids)),
    );
    next
}

fn in_range(range: HostIdRange, ids: ServiceIds) -> bool {
    let Some(offset) = ids.first.checked_sub(range.first).map(u64::from) else {
        return false;
    };
    offset % u64::from(SERVICE_IDS) == 0
        && offset + u64::from(SERVICE_IDS) <= u64::from(range.count)
}

#[test]
fn a_service_gets_the_same_ids_on_every_node_and_keeps_them() -> TestResult {
    let range = HostIdRange::default_range();
    let web = owner(1, "production", "app", "web");
    let worker = owner(1, "production", "app", "worker");
    let db = owner(1, "production", "app", "db");
    let api = owner(1, "production", "app", "api");
    // Two nodes that run different services; web runs on both.
    let node_1 = allocate(range, &BTreeMap::new(), &set([&web, &worker]))?;
    let node_2 = allocate(range, &BTreeMap::new(), &set([&web, &db]))?;
    assert_eq!(node_1[&web].ids, node_2[&web].ids);
    assert_eq!(node_1[&web].ids, web.first_choice(range));
    assert!(node_1[&web].is_first_choice() && node_1[&web].first_choice_held_by.is_none());
    assert_ne!(node_1[&web].ids, node_1[&worker].ids);
    // The next plan, with what the host keeps and one service more, gives
    // every service the ids it had.
    let kept = keep(&BTreeMap::new(), &node_1);
    let next = allocate(range, &kept, &set([&web, &worker, &api]))?;
    assert_eq!(next[&web].ids, node_1[&web].ids);
    assert_eq!(next[&worker].ids, node_1[&worker].ids);
    // Another environment or stack of the project is another service.
    assert_ne!(
        owner(1, "staging", "app", "web").first_choice(range),
        web.first_choice(range)
    );
    assert_ne!(
        owner(1, "production", "jobs", "web").first_choice(range),
        web.first_choice(range)
    );
    Ok(())
}

#[test]
fn no_two_services_or_projects_on_a_host_share_ids() -> TestResult {
    // A small range, so that first choices collide: 240 services in 512
    // blocks.
    let range = HostIdRange::new(DEFAULT_FIRST, 512 * SERVICE_IDS)?;
    let mut by_project = Vec::new();
    for project in 1..=3 {
        let mut owners = BTreeSet::new();
        for environment in ["staging", "production"] {
            for service in 0..40 {
                owners.insert(owner(project, environment, "app", &format!("s{service}")));
            }
        }
        by_project.push(owners);
    }
    // Deployments arrive one project at a time; each keeps what it got.
    let mut kept = BTreeMap::new();
    for owners in &by_project {
        let chosen = allocate(range, &kept, owners)?;
        kept = keep(&kept, &chosen);
    }
    let blocks = kept.values().map(|ids| ids.first).collect::<BTreeSet<_>>();
    assert_eq!(blocks.len(), 240, "two services share ids");
    assert!(kept.values().all(|ids| in_range(range, *ids)));
    let collided = kept
        .iter()
        .filter(|(owner, ids)| owner.first_choice(range) != **ids)
        .count();
    assert!(
        collided > 0,
        "the range was meant to make first choices collide"
    );
    // All at once gives distinct ids too, and planning again changes none.
    let all = by_project
        .iter()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>();
    let at_once = allocate(range, &BTreeMap::new(), &all)?;
    assert_eq!(
        at_once
            .values()
            .map(|choice| choice.ids.first)
            .collect::<BTreeSet<_>>()
            .len(),
        240
    );
    let again = allocate(range, &kept, &all)?;
    assert!(
        again
            .iter()
            .all(|(owner, choice)| kept[owner] == choice.ids)
    );
    Ok(())
}

#[test]
fn a_service_whose_first_choice_another_has_takes_its_next_and_keeps_it() -> TestResult {
    let range = HostIdRange::default_range();
    let web = owner(1, "production", "app", "web");
    let other = owner(2, "production", "shop", "api");
    let kept = BTreeMap::from([(other.clone(), web.first_choice(range))]);
    let chosen = allocate(range, &kept, &set([&web]))?;
    assert_ne!(chosen[&web].ids, web.first_choice(range));
    assert_eq!(chosen[&web].first_choice, web.first_choice(range));
    assert!(!chosen[&web].is_first_choice());
    assert_eq!(chosen[&web].first_choice_held_by.as_ref(), Some(&other));
    assert!(in_range(range, chosen[&web].ids));
    let kept = keep(&kept, &chosen);
    assert_eq!(
        allocate(range, &kept, &set([&web]))?[&web].ids,
        chosen[&web].ids
    );
    Ok(())
}

#[test]
fn a_full_range_is_refused_with_what_to_change() -> TestResult {
    let range = HostIdRange::new(DEFAULT_FIRST, 2 * SERVICE_IDS)?;
    let owners = (0..3)
        .map(|service| owner(1, "production", "app", &format!("s{service}")))
        .collect::<BTreeSet<_>>();
    let refused = allocate(range, &BTreeMap::new(), &owners)
        .err()
        .ok_or("three services fit in two blocks")?;
    assert_eq!(refused.range, range);

    let host = Host::with_lines(&format!(
        "{RANGE_OWNER}:{DEFAULT_FIRST}:{}\n",
        2 * SERVICE_IDS
    ))?;
    let error = host.ids.reserve(&owners).err().ok_or("no error")?;
    assert_eq!(error.code(), "MM_USER_NAMESPACE");
    let message = error.to_string();
    for part in [
        "mm-services:3221225472:131072",
        "2 services",
        "Widen the range in both files",
        "host-user-namespace: <why>",
    ] {
        assert!(message.contains(part), "{part}: {message}");
    }
    Ok(())
}

#[test]
fn setup_reserves_a_range_clear_of_podmans_own() -> TestResult {
    let range = HostIdRange::from_subordinate_ids(SETUP_LINES, SETUP_LINES)?;
    assert_eq!(range, Some(HostIdRange::default_range()));
    assert_eq!(HostIdRange::default_range().blocks(), 16383);
    assert_eq!(
        u64::from(DEFAULT_FIRST) + u64::from(DEFAULT_COUNT),
        0xFFFF_0000
    );
    // The access playbook writes exactly these lines.
    let playbook = include_str!("../src/adapters/target/ansible_bootstrap/access.yml");
    for line in SETUP_LINES.lines() {
        assert!(playbook.contains(&format!("'{line}'")), "{line}");
    }
    assert_eq!(HostIdRange::from_subordinate_ids("", "")?, None);
    let one_file =
        HostIdRange::from_subordinate_ids(SETUP_LINES, "containers:2147483647:1073741825\n")
            .err()
            .ok_or("a range in one file only")?;
    assert!(
        one_file.contains("both files need the same line"),
        "{one_file}"
    );
    // Podman's `containers` range as commit 15 wrote it covers mm's.
    let old = "containers:2147483647:2147483648\nmm-services:3221225472:1073676288\n";
    let overlap = HostIdRange::from_subordinate_ids(old, old)
        .err()
        .ok_or("an overlapping range")?;
    assert!(
        overlap.contains("overlaps containers:2147483647:2147483648"),
        "{overlap}"
    );
    let tiny = "mm-services:3221225472:1000\n";
    assert!(HostIdRange::from_subordinate_ids(tiny, tiny).is_err());
    Ok(())
}

#[test]
fn a_services_namespace_maps_its_container_ids_to_its_own() {
    let ids = ServiceIds {
        first: DEFAULT_FIRST,
    };
    assert_eq!(
        ids.userns_mode(),
        "auto:uidmapping=0:3221225472:65536,gidmapping=0:3221225472:65536,size=65536"
    );
    assert_eq!(ids.host_id(0), Some(DEFAULT_FIRST));
    assert_eq!(ids.host_id(65_535), Some(ids.last()));
    assert_eq!(ids.host_id(65_536), None);
}

/// A host's subordinate id files and its record of the services' ids.
struct Host {
    _directory: tempfile::TempDir,
    record: PathBuf,
    ids: Arc<FilesystemServiceUserIds>,
}

impl Host {
    fn with_lines(lines: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let directory = temp_dirs::tempdir()?;
        let (subuid, subgid) = (
            directory.path().join("subuid"),
            directory.path().join("subgid"),
        );
        fs::write(&subuid, lines)?;
        fs::write(&subgid, lines)?;
        let record = directory.path().join("service-user-ids.json");
        Ok(Self {
            ids: Arc::new(FilesystemServiceUserIds::at(record.clone(), subuid, subgid)),
            record,
            _directory: directory,
        })
    }
}

#[test]
fn the_host_keeps_each_services_ids_in_a_file_only_root_reads() -> TestResult {
    let web = owner(1, "production", "app", "web");
    let api = owner(1, "production", "app", "api");
    let other = owner(2, "production", "app", "web");

    let unprepared = Host::with_lines("containers:2147483647:2147483648\n")?;
    let error = unprepared
        .ids
        .reserve(&set([&web]))
        .err()
        .ok_or("no range")?;
    let message = error.to_string();
    for part in [
        "have no line for mm-services",
        "Run `mm target setup plan` and apply it",
        "mm-services:3221225472:1073676288",
        "host-user-namespace: <why>",
    ] {
        assert!(message.contains(part), "{part}: {message}");
    }

    let host = Host::with_lines(SETUP_LINES)?;
    let first = host.ids.reserve(&set([&web, &other]))?;
    assert_ne!(first[&web].ids, first[&other].ids, "two projects share ids");
    assert_eq!(
        fs::metadata(&host.record)?.permissions().mode() & 0o777,
        0o600
    );
    // Another process of mmd later: the same ids, and a new service's own.
    let later = FilesystemServiceUserIds::at(
        host.record.clone(),
        host.record.with_file_name("subuid"),
        host.record.with_file_name("subgid"),
    );
    let again = later.reserve(&set([&web, &api]))?;
    assert_eq!(again[&web].ids, first[&web].ids);
    assert!(![first[&web].ids, first[&other].ids].contains(&again[&api].ids));

    fs::write(&host.record, "{not json")?;
    let damaged = later
        .reserve(&set([&web]))
        .err()
        .ok_or("a damaged record")?;
    assert!(
        damaged
            .to_string()
            .contains("service-user-ids.json is damaged"),
        "{damaged}"
    );
    Ok(())
}

const COMPOSE: &str = "services:
  web:
    image: example/web
    volumes: [\"uploads:/data\"]
  api:
    image: example/api
  cache:
    image: example/cache
  legacy:
    image: example/legacy
    userns_mode: keep-id
volumes:
  uploads: {}
";

/// A project on a cluster of two nodes whose stack shares `uploads`.
fn write_project(root: &Path) -> TestResult {
    fs::create_dir_all(root.join(".config"))?;
    fs::write(root.join("compose.yaml"), COMPOSE)?;
    fs::write(root.join("api.conf"), "listen 8080\n")?;
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1
project:
  id: {PROJECT_ID}
  name: user-ids
default-environment: production
target: local
runtime: {{driver: podman-compose}}
stacks:
  app:
    files: [compose.yaml]
    shared-volumes: [uploads]
modules:
  shop:
    path: .
    services:
      web: {{stack: app, placement: all}}
      api:
        stack: app
        placement: all
        config-files:
          /etc/api.conf: {{file: api.conf, mode: 0o600}}
      cache: {{stack: app, placement: all, host-user-namespace: shares its socket with the host}}
      legacy: {{stack: app, placement: all}}
targets:
  production:
    nodes:
      - {{name: app-1, host: 203.0.113.11}}
      - {{name: app-2, host: 203.0.113.12}}
environments:
  production:
    target: production
    bindings: {{}}
"
        ),
    )?;
    Ok(())
}

/// Compiles the project as node `node` of the cluster sees it, with `ids`
/// as the host's record of user ids.
fn compile_on(
    root: &Path,
    home: &Path,
    node: &str,
    ids: Arc<dyn ServiceUserIds>,
) -> Result<mm::domain::stack::StackCompilationReport, mm::domain::error::MmError> {
    compile_with(root, home, node, Some(ids))
}

/// `compile_on`, on a host that runs Docker Engine when `ids` is `None`.
fn compile_with(
    root: &Path,
    home: &Path,
    node: &str,
    ids: Option<Arc<dyn ServiceUserIds>>,
) -> Result<mm::domain::stack::StackCompilationReport, mm::domain::error::MmError> {
    let scope = NodeScope {
        target: "production".to_owned(),
        node: node.to_owned(),
        environment: "production".to_owned(),
        resolver_files: None,
        mesh_addresses: [("app-1", [10, 90, 0, 1]), ("app-2", [10, 90, 0, 2])]
            .into_iter()
            .map(|(name, address)| (name.to_owned(), address.into()))
            .collect(),
    };
    fs::create_dir_all(home).map_err(|source| mm::domain::error::MmError::Io {
        context: "creating the test's home".to_owned(),
        source,
    })?;
    let documents: Arc<dyn ProjectDocumentStore> = Arc::new(mm::app::NodeDocuments::new(
        Arc::new(ConfigLoader),
        scope.clone(),
    ));
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.join("targets.yaml")));
    if targets.resolve_target("local").is_err() {
        targets.insert_target(
            "local",
            TargetDefinition::local(TargetId(uuid::Uuid::from_u128(9))),
        )?;
    }
    let manager = StackManager::new(
        documents,
        Arc::new(FilesystemStackArtifactStore::new(home.join("compose"))),
        targets,
    )
    .with_user_ids(ids.clone());
    let mut selection = WorkspaceSelection::new(root, None);
    selection.target = Some("local".to_owned());
    selection.runtime = Some(if ids.is_some() {
        RuntimeKind::PodmanCompose
    } else {
        RuntimeKind::DockerCompose
    });
    selection.node = Some(scope);
    selection.user_ids = ids;
    manager.compile(&selection)
}

fn overlay_of(
    report: &mm::domain::stack::StackCompilationReport,
) -> Result<serde_yaml_ng::Value, Box<dyn std::error::Error>> {
    Ok(serde_yaml_ng::from_str(&fs::read_to_string(
        &report.stacks[0].generated_overlay_path,
    )?)?)
}

#[test]
fn each_service_runs_in_its_own_fixed_namespace_and_its_files_are_its_own() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    write_project(&root)?;
    let range = HostIdRange::default_range();
    let project = ProjectId(uuid::Uuid::parse_str(PROJECT_ID)?);
    let service = |name: &str| ServiceIdsOwner {
        project,
        environment: "production".to_owned(),
        stack: "app".to_owned(),
        service: name.to_owned(),
    };
    let mut modes = BTreeMap::new();
    for node in ["app-1", "app-2"] {
        let host = Host::with_lines(SETUP_LINES)?;
        let home = temporary.path().join(node);
        let compiled = compile_on(&root, &home, node, host.ids.clone());
        if !rustix::process::geteuid().is_root() {
            // Only root gives a file to another user: mmd runs as root.
            let error = compiled.err().ok_or("a config file was given away")?;
            assert!(
                error
                    .to_string()
                    .contains("cannot give a file to another user"),
                "{error}"
            );
            return Ok(());
        }
        let compiled = compiled?;
        let overlay = overlay_of(&compiled)?;
        let services = &overlay["services"];
        for name in ["web", "api"] {
            assert_eq!(
                services[name]["userns_mode"].as_str(),
                Some(service(name).first_choice(range).userns_mode().as_str()),
                "{name} on {node}"
            );
        }
        assert_eq!(services["cache"]["userns_mode"].as_str(), Some("host"));
        assert!(
            services["legacy"]["userns_mode"].is_null(),
            "a Compose file's own userns_mode is left alone: {services:?}"
        );
        modes.insert(node, services["web"]["userns_mode"].clone());
        // The config file belongs to api's root, and only to it.
        let mount = services["api"]["volumes"][0]
            .as_str()
            .ok_or("api mounts no config file")?;
        let source = mount.split(':').next().ok_or("no source")?;
        let metadata = fs::metadata(source)?;
        let first = service("api").first_choice(range).first;
        assert_eq!((metadata.uid(), metadata.gid()), (first, first));
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }
    assert_eq!(
        modes["app-1"], modes["app-2"],
        "web's ids differ between nodes"
    );
    Ok(())
}

/// A host that confines containers sharing a process namespace as `.1` says.
#[derive(Debug)]
struct ConfinedHost(Arc<FilesystemServiceUserIds>, SharedPidAppArmor);

impl ServiceUserIds for ConfinedHost {
    fn reserve(
        &self,
        wanted: &BTreeSet<ServiceIdsOwner>,
    ) -> Result<BTreeMap<ServiceIdsOwner, ServiceIdsChoice>, mm::domain::error::MmError> {
        self.0.reserve(wanted)
    }

    fn release(
        &self,
        owners: &BTreeSet<ServiceIdsOwner>,
    ) -> Result<(), mm::domain::error::MmError> {
        self.0.release(owners)
    }

    fn shared_pid_apparmor(&self) -> SharedPidAppArmor {
        self.1
    }
}

#[test]
fn a_service_that_shares_another_process_namespace_keeps_podmans_apparmor_rules() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    write_project(&root)?;
    fs::write(
        root.join("compose.yaml"),
        "services:
  web:
    image: example/web
  monitor:
    image: example/monitor
    pid: host
  sidecar:
    image: example/sidecar
    pid: \"service:monitor\"
  peer:
    image: example/peer
    pid: \"service:web\"
  joined:
    image: example/joined
    pid: \"container:elsewhere\"
  inherited:
    extends: joined
    image: example/inherited
  chosen:
    image: example/chosen
    pid: host
    security_opt: [\"apparmor=custom\"]
",
    )?;
    let config = root.join(".config/mm.yaml");
    let text = fs::read_to_string(&config)?;
    fs::write(
        &config,
        text.replace("    shared-volumes: [uploads]\n", "").replace(
            "      api:
        stack: app
        placement: all
        config-files:
          /etc/api.conf: {file: api.conf, mode: 0o600}
      cache: {stack: app, placement: all, host-user-namespace: shares its socket with the host}
      legacy: {stack: app, placement: all}",
            "      monitor: {stack: app, placement: all}
      sidecar: {stack: app, placement: all}
      peer: {stack: app, placement: all}
      joined: {stack: app, placement: all}
      chosen: {stack: app, placement: all}",
        ),
    )?;
    let host = Host::with_lines(SETUP_LINES)?;
    let confined = compile_on(
        &root,
        &temporary.path().join("confined"),
        "app-1",
        Arc::new(ConfinedHost(host.ids.clone(), SharedPidAppArmor::Pin)),
    )?;
    let overlay = overlay_of(&confined)?;
    let options = |service: &str| {
        overlay["services"][service]["security_opt"]
            .as_sequence()
            .map(|options| {
                options
                    .iter()
                    .filter_map(serde_yaml_ng::Value::as_str)
                    .filter(|option| option.starts_with("apparmor"))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let pinned = ["apparmor=mm-containers-shared-pid".to_owned()];
    for service in ["monitor", "sidecar", "joined", "inherited"] {
        assert_eq!(options(service), pinned, "{service}: {overlay:?}");
    }
    for service in ["web", "peer", "chosen"] {
        assert!(options(service).is_empty(), "{service}: {overlay:?}");
    }

    let unconfined = compile_on(
        &root,
        &temporary.path().join("unconfined"),
        "app-1",
        host.ids.clone(),
    )?;
    let overlay = overlay_of(&unconfined)?;
    assert!(
        !serde_yaml_ng::to_string(&overlay)?.contains("mm-containers-shared-pid"),
        "a host without AppArmor: {overlay:?}"
    );

    let missing = compile_on(
        &root,
        &temporary.path().join("missing"),
        "app-1",
        Arc::new(ConfinedHost(host.ids.clone(), SharedPidAppArmor::Missing)),
    )
    .err()
    .ok_or("a host that would give monitor mm-containers")?;
    assert_eq!(missing.code(), "MM_PODMAN_APPARMOR");
    let message = missing.to_string();
    assert!(
        message.contains("service 'inherited'")
            && message.contains("`pid: container:elsewhere`")
            && message.contains("has not loaded mm-containers-shared-pid")
            && message.contains("`mm target setup plan`"),
        "{message}"
    );

    let compose = root.join("compose.yaml");
    let text = fs::read_to_string(&compose)?;
    fs::write(
        &compose,
        text.replace(
            "security_opt: [\"apparmor=custom\"]",
            "security_opt: [\"apparmor:mm-containers\"]",
        ),
    )?;
    let wide = compile_on(
        &root,
        &temporary.path().join("wide"),
        "app-1",
        Arc::new(ConfinedHost(host.ids.clone(), SharedPidAppArmor::Pin)),
    )
    .err()
    .ok_or("a pid-sharing service that chooses mm-containers")?;
    assert_eq!(wide.code(), "MM_PODMAN_APPARMOR");
    let message = wide.to_string();
    assert!(
        message.contains("service 'chosen'")
            && message.contains("apparmor=mm-containers")
            && message.contains("Remove the option"),
        "{message}"
    );
    Ok(())
}

#[test]
fn a_service_with_a_network_of_its_own_may_listen_below_1024_as_any_user_on_podman() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    write_project(&root)?;
    fs::write(
        root.join("compose.yaml"),
        "services:
  web:
    image: example/web
  sidecar:
    image: example/sidecar
    network_mode: \"service:web\"
  tuned:
    image: example/tuned
    sysctls: [net.ipv4.ip_unprivileged_port_start=1024]
  other:
    image: example/other
    sysctls: {net.core.somaxconn: 1024}
",
    )?;
    let config = root.join(".config/mm.yaml");
    let text = fs::read_to_string(&config)?;
    fs::write(
        &config,
        text.replace("    shared-volumes: [uploads]\n", "").replace(
            "      api:
        stack: app
        placement: all
        config-files:
          /etc/api.conf: {file: api.conf, mode: 0o600}
      cache: {stack: app, placement: all, host-user-namespace: shares its socket with the host}
      legacy: {stack: app, placement: all}",
            "      sidecar: {stack: app, placement: all}
      tuned: {stack: app, placement: all}
      other: {stack: app, placement: all, host-user-namespace: keeps the host's ids}",
        ),
    )?;
    let host = Host::with_lines(SETUP_LINES)?;
    let podman = compile_on(
        &root,
        &temporary.path().join("podman"),
        "app-1",
        host.ids.clone(),
    )?;
    let overlay = overlay_of(&podman)?;
    let sysctl = |service: &str| {
        overlay["services"][service]["sysctls"]["net.ipv4.ip_unprivileged_port_start"].clone()
    };
    assert_eq!(sysctl("web"), serde_yaml_ng::Value::from("0"));
    assert_eq!(
        sysctl("other"),
        serde_yaml_ng::Value::from("0"),
        "with the host's ids too, beside its own sysctls"
    );
    assert!(sysctl("sidecar").is_null(), "it shares web's network");
    assert!(sysctl("tuned").is_null(), "its own value wins");
    assert_eq!(podman.stacks[0].manifest.low_ports, ["other", "web"]);

    let docker = compile_with(&root, &temporary.path().join("docker"), "app-1", None)?;
    let overlay = overlay_of(&docker)?;
    assert!(
        overlay["services"]["web"]["sysctls"].is_null(),
        "Docker Engine does it itself: {overlay:?}"
    );
    assert!(docker.stacks[0].manifest.low_ports.is_empty());
    Ok(())
}

#[test]
fn a_service_on_a_shared_volume_without_its_first_choice_is_refused() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    write_project(&root)?;
    // No config file: this test is about ids alone.
    let config = fs::read_to_string(root.join(".config/mm.yaml"))?.replace(
        "        config-files:\n          /etc/api.conf: {file: api.conf, mode: 0o600}\n",
        "",
    );
    fs::write(root.join(".config/mm.yaml"), config)?;
    let range = HostIdRange::default_range();
    let web = ServiceIdsOwner {
        project: ProjectId(uuid::Uuid::parse_str(PROJECT_ID)?),
        environment: "production".to_owned(),
        stack: "app".to_owned(),
        service: "web".to_owned(),
    };
    // On app-2 another project's service got web's first choice earlier.
    let host = Host::with_lines(SETUP_LINES)?;
    let other = owner(77, "staging", "blog", "writer");
    let taken = host.ids.reserve(&BTreeSet::from([other.clone()]))?;
    let record = format!(
        "{{\"schema\":1,\"services\":[{{\"project\":\"{}\",\"environment\":\"staging\",\"stack\":\"blog\",\"service\":\"writer\",\"first\":{}}}]}}",
        other.project.0,
        web.first_choice(range).first
    );
    assert!(taken.contains_key(&other));
    fs::write(&host.record, record)?;
    let error = compile_on(
        &root,
        &temporary.path().join("app-2"),
        "app-2",
        host.ids.clone(),
    )
    .err()
    .ok_or("web was planned with other ids than on every other node")?;
    assert_eq!(error.code(), "MM_USER_NAMESPACE");
    let message = error.to_string();
    // The refused service keeps nothing: a later plan tries its first
    // choice again.
    assert!(!fs::read_to_string(&host.record)?.contains("\"web\""));
    for part in [
        "service 'web' mounts shared volume 'uploads'",
        &format!(
            "host ids {} to {}",
            web.first_choice(range).first,
            web.first_choice(range).last()
        ),
        "service 'writer' of stack 'blog' in environment 'staging' of project",
        "`host-user-namespace: <why>` under modules.shop.services.web",
    ] {
        assert!(message.contains(part), "{part}: {message}");
    }
    // api, on no shared volume, takes the next free ids and nothing is
    // refused for it on a node where web keeps the host's ids.
    let config = fs::read_to_string(root.join(".config/mm.yaml"))?.replace(
        "web: {stack: app, placement: all}",
        "web: {stack: app, placement: all, host-user-namespace: writes what every node reads}",
    );
    fs::write(root.join(".config/mm.yaml"), config)?;
    let compiled = compile_on(
        &root,
        &temporary.path().join("app-2b"),
        "app-2",
        host.ids.clone(),
    )?;
    assert_eq!(
        overlay_of(&compiled)?["services"]["web"]["userns_mode"].as_str(),
        Some("host")
    );
    Ok(())
}

/// A service whose Compose file sets its own `userns_mode` gets its ids
/// from Podman when a container is created, after mm wrote its files: an
/// entry whose owner or mode only holds with ids mm knows is refused.
#[test]
fn a_file_of_a_service_whose_ids_podman_picks_has_to_hold_for_any_owner() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    write_project(&root)?;
    let config = fs::read_to_string(root.join(".config/mm.yaml"))?.replace(
        "        config-files:\n          /etc/api.conf: {file: api.conf, mode: 0o600}\n",
        "",
    );
    let with = |legacy: &str| {
        config.replace(
            "legacy: {stack: app, placement: all}",
            &format!("legacy: {{stack: app, placement: all, {legacy}}}"),
        )
    };
    let compile = |config: String, node: &str| -> Result<_, Box<dyn std::error::Error>> {
        fs::write(root.join(".config/mm.yaml"), config)?;
        let host = Host::with_lines(SETUP_LINES)?;
        Ok(compile_on(
            &root,
            &temporary.path().join(node),
            node,
            host.ids.clone(),
        ))
    };
    let owned = with(
        "config-files: {/etc/legacy.conf: {file: api.conf, uid: \"1000\", gid: \"1000\", mode: 0o400}}",
    );
    let error = compile(owned.clone(), "app-1")?
        .err()
        .ok_or("a 0400 file of a service with its own userns_mode was accepted")?
        .to_string();
    for part in [
        "modules.shop.services.legacy.config-files.\"/etc/legacy.conf\"",
        "sets `userns_mode: keep-id`",
        "Remove `userns_mode` from service 'legacy'",
        "uid \"0\", gid \"0\" and a mode every user can read",
    ] {
        assert!(error.contains(part), "{part}: {error}");
    }
    let readable = with("config-files: {/etc/legacy.conf: {file: api.conf}}");
    compile(readable, "app-2")?
        .map_err(|error| format!("a 0444 root file was refused: {error}"))?;
    let kept = owned.replace(
        "legacy: {stack: app, placement: all, ",
        "legacy: {stack: app, placement: all, host-user-namespace: reads host files, ",
    );
    if let Err(error) = compile(kept, "app-1")? {
        assert!(!error.to_string().contains("userns_mode"), "{error}");
    }
    Ok(())
}

/// A file secret `api.token` that `worker` reads as user 1000 and `api` as
/// its root, on a host where both have ids of their own.
fn owned_plan(
    revision: &str,
    previous: Option<&str>,
) -> Result<RuntimeSecretPlan, Box<dyn std::error::Error>> {
    let consumer = |service: &str, uid: &str, mode: u32| RuntimeSecretConsumer {
        stack: "app".to_owned(),
        service: service.to_owned(),
        target: "api-token".to_owned(),
        uid: uid.to_owned(),
        gid: uid.to_owned(),
        mode,
    };
    let plan = RuntimeSecretPlan::new(
        PROJECT_ID.to_owned(),
        "319c527b-6242-7b43-bfd4-bd60e27c1de3".to_owned(),
        "production".to_owned(),
        RuntimeKind::PodmanCompose,
        vec![RuntimeSecretPlanItem {
            logical_name: "api.token".to_owned(),
            backend: "team-secrets".to_owned(),
            delivery: DeliveryDriverId::new("compose-file-secret")?,
            source_revision: revision.to_owned(),
            previous_source_revision: previous.map(str::to_owned),
            consumers: vec![
                consumer("worker", "1000", 0o400),
                consumer("api", "0", 0o440),
            ],
        }],
    )?;
    Ok(plan.with_first_host_ids(&BTreeMap::from([
        (("app".to_owned(), "worker".to_owned()), WORKER_FIRST),
        (("app".to_owned(), "api".to_owned()), API_FIRST),
    ]))?)
}

const WORKER_FIRST: u32 = DEFAULT_FIRST + 7 * SERVICE_IDS;
const API_FIRST: u32 = DEFAULT_FIRST + 9 * SERVICE_IDS;

fn token(value: &str) -> Result<RuntimeSecretMaterial, String> {
    Ok(RuntimeSecretMaterial {
        logical_name: "api.token".to_owned(),
        material: SecretMaterial::new(value.to_owned())?,
    })
}

/// Each service's mount: its Compose secret key and the file behind it.
fn mounts(path: &Path) -> Result<BTreeMap<String, (String, PathBuf)>, Box<dyn std::error::Error>> {
    let overlay: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(path)?)?;
    let mut mounts = BTreeMap::new();
    for service in ["worker", "api"] {
        let key = overlay["services"][service]["secrets"][0]["source"]
            .as_str()
            .ok_or("no secret")?
            .to_owned();
        let file = overlay["secrets"][key.as_str()]["file"]
            .as_str()
            .ok_or("no file")?;
        mounts.insert(service.to_owned(), (key, PathBuf::from(file)));
    }
    Ok(mounts)
}

#[tokio::test]
async fn a_file_secret_on_podman_is_given_to_each_consumers_user() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let run = home.path().join("run");
    let driver = DockerComposeFileSecretDelivery::for_podman(run.clone())?;
    assert_eq!(driver.capability().runtime, RuntimeKind::PodmanCompose);
    assert!(driver.capability().supports_uid_gid_mode);
    let plan = owned_plan("revision-1", None)?;
    let prepared = driver
        .prepare(OperationId::new(), &plan, vec![token("first-value")?])
        .await;
    if !rustix::process::geteuid().is_root() {
        let error = prepared.err().ok_or("a file was given to another user")?;
        assert!(
            error.to_string().contains("giving a runtime secret file"),
            "{error}"
        );
        return Ok(());
    }
    let prepared = prepared?;
    let first = mounts(&prepared.overlays[0].1)?;
    // The copy carries its owner and mode; Docker Compose bind-mounts a
    // file secret and only warns about `uid`, `gid` and `mode` on it.
    let overlay: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&fs::read_to_string(&prepared.overlays[0].1)?)?;
    for service in ["worker", "api"] {
        let mount = &overlay["services"][service]["secrets"][0];
        assert!(
            mount["uid"].is_null() && mount["gid"].is_null() && mount["mode"].is_null(),
            "{mount:?}"
        );
        assert_eq!(mount["target"].as_str(), Some("api-token"));
    }
    // One copy each, owned by the host ids the consumer's uid and gid are,
    // with its mode, in a directory only root reaches.
    for (service, owner, mode) in [
        ("worker", WORKER_FIRST + 1000, 0o400),
        ("api", API_FIRST, 0o440),
    ] {
        let (_, file) = &first[service];
        let metadata = fs::metadata(file)?;
        assert_eq!(
            (metadata.uid(), metadata.gid()),
            (owner, owner),
            "{service}"
        );
        assert_eq!(metadata.permissions().mode() & 0o777, mode, "{service}");
        assert_eq!(fs::read_to_string(file)?, "first-value");
        let directory = file.parent().ok_or("no directory")?;
        assert_eq!(fs::metadata(directory)?.permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(directory)?.uid(), 0);
    }
    assert_ne!(first["worker"], first["api"]);
    driver.dispose(&prepared)?;

    // A rotation gives both consumers new keys; the rollback mounts the
    // copies of the revision before, which stay until the next success.
    let rotated = owned_plan("revision-2", Some("revision-1"))?;
    let second = driver
        .prepare(OperationId::new(), &rotated, vec![token("second-value")?])
        .await?;
    let now = mounts(&second.overlays[0].1)?;
    let rollback = mounts(&second.rollback_overlays[0].1)?;
    for service in ["worker", "api"] {
        assert_ne!(now[service].0, first[service].0, "{service} keeps its key");
        assert_eq!(rollback[service], first[service], "{service}'s rollback");
    }
    driver.dispose(&second)?;
    let removed = driver
        .reconcile(
            &rotated,
            &BTreeMap::from([(
                "api.token".to_owned(),
                BTreeSet::from(["revision-2".to_owned()]),
            )]),
            &Default::default(),
        )
        .await?;
    assert_eq!(removed.len(), 2, "{removed:?}");
    for service in ["worker", "api"] {
        assert!(!first[service].1.exists() && now[service].1.exists());
    }
    assert!(
        driver
            .remove_revision(&rotated, "api.token", "revision-2")
            .await?
    );
    assert!(!now["worker"].1.exists() && !now["api"].1.exists());
    Ok(())
}

#[test]
fn a_file_secret_names_one_of_the_ids_of_the_services_containers() -> TestResult {
    let mut plan = owned_plan("revision-1", None)?;
    for consumer in &mut plan.items[0].consumers {
        if consumer.service == "worker" {
            consumer.uid = "65536".to_owned();
        }
    }
    let plan = RuntimeSecretPlan::new(
        plan.project_id.clone(),
        plan.target_id.clone(),
        plan.environment.clone(),
        plan.runtime,
        plan.items.clone(),
    )?
    .with_first_host_ids(&BTreeMap::from([
        (("app".to_owned(), "worker".to_owned()), WORKER_FIRST),
        (("app".to_owned(), "api".to_owned()), API_FIRST),
    ]))?;
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let home = temp_dirs::tempdir()?;
    let driver = DockerComposeFileSecretDelivery::for_podman(home.path().join("run"))?;
    let error = runtime
        .block_on(driver.prepare(OperationId::new(), &plan, vec![token("value")?]))
        .err()
        .ok_or("an id outside the namespace was accepted")?;
    assert!(
        error
            .to_string()
            .contains("uid 65536 is none of the ids its containers have, 0 to 65535"),
        "{error}"
    );
    Ok(())
}

/// Where `mmd` deploys an environment on hosts setup prepared, as a
/// workstation sees it: the environment's target, or the one a request
/// names, is a target of the repository.
#[test]
fn an_environment_on_hosts_setup_prepared_is_one_on_a_target_of_the_repository() -> TestResult {
    let config: mm::domain::config::MmConfig = serde_yaml_ng::from_str(&format!(
        "schema-version: 1
project: {{id: {PROJECT_ID}, name: user-ids}}
target: local
stacks: {{}}
modules: {{}}
environments:
  local: {{bindings: {{}}}}
  production: {{target: production, bindings: {{}}}}
targets:
  production:
    nodes:
      - {{name: app-1, host: 203.0.113.11}}
"
    ))?;
    use mm::domain::user_namespace::on_prepared_hosts;
    assert!(!on_prepared_hosts(&config, "local", None));
    assert!(on_prepared_hosts(&config, "production", None));
    assert!(!on_prepared_hosts(&config, "production", Some("local")));
    assert!(on_prepared_hosts(&config, "local", Some("production")));
    Ok(())
}
