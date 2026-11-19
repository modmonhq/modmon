//! `hardening: strict` and the host's OCI runtime policy: what mm adds to
//! each service, what a plan shows, and what is refused.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use mm::adapters::config::{ConfigLoader, validate};
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{ProjectDocumentStore, TargetStore};
use mm::app::{HostDocuments, StackManager, WorkspaceSelection};
use mm::domain::hardening::{
    HardeningKey, HostRuntimeStatus, HostRuntimes, NodeRuntimeReport, RuntimeCheck,
    ServiceSecurity, runtime_diagnostics,
};
use mm::domain::host_policy::HostPolicy;
use mm::domain::stack::StackCompilationReport;
use mm::domain::target::{TargetDefinition, TargetId};
use uuid::Uuid;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w";

/// A project with a development environment and a production one, both on
/// this machine's target. `top` goes to the top of mm.yaml, `services`
/// under the module, `stack` under the stack.
fn write_project(root: &Path, compose: &str, top: &str, services: &str, stack: &str) -> TestResult {
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("app"))?;
    fs::write(root.join("compose.yaml"), compose)?;
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3\n  name: shop\n  development-environment: development\ndefault-environment: development\ntarget: local\nruntime:\n  driver: docker-compose\n{top}stacks:\n  app:\n    files: [compose.yaml]\n{stack}modules:\n  app:\n    path: app\n    services:\n{services}environments:\n  development:\n    bindings: {{}}\n  production:\n    domains:\n      shop: shop.example.test\n    bindings: {{}}\n"
        ),
    )?;
    Ok(())
}

/// Compiles `environment` the way a host whose policy says `host` about
/// runtimes does; `None` is the machine `mm` runs on.
fn compile(
    project: &Path,
    environment: &str,
    host: Option<HostRuntimes>,
) -> Result<StackCompilationReport, Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::parse_str(
            "319c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?)),
    )?;
    let mut documents: Arc<dyn ProjectDocumentStore> = Arc::new(ConfigLoader);
    if let Some(host) = host {
        documents = Arc::new(HostDocuments::new(documents, host));
    }
    let manager = StackManager::new(
        documents,
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        targets,
    );
    let report = manager.compile(&WorkspaceSelection::new(
        project,
        Some(environment.to_owned()),
    ))?;
    // The overlay is read before the home goes away.
    let overlay = fs::read_to_string(&report.stacks[0].generated_overlay_path)?;
    fs::write(project.join("overlay.yaml"), overlay)?;
    Ok(report)
}

fn overlay(project: &Path) -> Result<serde_yaml_ng::Value, Box<dyn std::error::Error>> {
    Ok(serde_yaml_ng::from_str(&fs::read_to_string(
        project.join("overlay.yaml"),
    )?)?)
}

fn security<'a>(
    report: &'a StackCompilationReport,
    service: &str,
) -> Result<&'a ServiceSecurity, String> {
    report.stacks[0]
        .manifest
        .security
        .iter()
        .find(|security| security.service == service)
        .ok_or_else(|| format!("the plan says nothing about {service}"))
}

fn refusal(
    result: Result<StackCompilationReport, Box<dyn std::error::Error>>,
) -> Result<String, String> {
    match result {
        Ok(_) => Err("the plan was not refused".to_owned()),
        Err(error) => Ok(error.to_string()),
    }
}

fn host(advertised: &[&str], exposed: Option<&str>) -> HostRuntimes {
    HostRuntimes {
        advertised: advertised.iter().map(|name| (*name).to_owned()).collect(),
        exposed: exposed.map(str::to_owned),
    }
}

const COMPOSE: &str = "services:
  web:
    image: example/web
  db:
    image: example/db
    cap_add: [CHOWN, cap_setuid, SETGID]
    volumes: [data:/var/lib/data]
  worker:
    image: example/worker
    tmpfs: /tmp:size=64m,exec
    security_opt: [no-new-privileges:true]
  sidecar:
    image: example/sidecar
volumes:
  data: {}
";

const SERVICES: &str = "      web:
        stack: app
      db:
        stack: app
        hardening:
          except:
            read-only: writes its socket under /var/run
      worker:
        stack: app
";

#[test]
fn strict_hardens_every_service_and_the_plan_shows_what_each_runs_with() -> TestResult {
    let project = temp_dirs::tempdir()?;
    write_project(project.path(), COMPOSE, "hardening: strict\n", SERVICES, "")?;
    let report = compile(project.path(), "production", None)?;
    let overlay = overlay(project.path())?;

    // One switch gives the whole set.
    let web = &overlay["services"]["web"];
    assert_eq!(web["read_only"], true);
    assert_eq!(web["cap_drop"][0], "ALL");
    assert_eq!(web["security_opt"][0], "no-new-privileges:true");
    assert_eq!(web["tmpfs"][0], "/tmp");
    assert_eq!(
        security(&report, "web")?.describe(),
        ["web: read-only root, no capabilities, no new privileges, default seccomp, tmpfs /tmp"]
    );

    // An exception leaves that one default out and the plan gives the
    // reason; `cap_add` keeps the capabilities a service needs.
    let db = &overlay["services"]["db"];
    assert!(db.get("read_only").is_none(), "{db:?}");
    assert_eq!(db["cap_drop"][0], "ALL");
    assert_eq!(
        security(&report, "db")?.describe(),
        [
            "db: writable root, capabilities CHOWN, SETGID, SETUID only, no new privileges, default seccomp, tmpfs /tmp",
            "db: not read-only (the root filesystem stays writable): writes its socket under /var/run",
        ]
    );
    assert_eq!(
        security(&report, "db")?.exceptions[0].key,
        HardeningKey::ReadOnly
    );

    // What the Compose file already says is not added a second time.
    let worker = &overlay["services"]["worker"];
    assert!(worker.get("tmpfs").is_none(), "{worker:?}");
    assert!(worker.get("security_opt").is_none(), "{worker:?}");
    assert_eq!(worker["read_only"], true);
    assert_eq!(security(&report, "worker")?.tmpfs, ["/tmp"]);

    // A service of the Compose file that mm.yaml does not list is hardened
    // too.
    assert_eq!(overlay["services"]["sidecar"]["read_only"], true);
    assert_eq!(report.stacks[0].manifest.security.len(), 4);
    Ok(())
}

#[test]
fn the_security_options_are_part_of_what_a_plan_hashes() -> TestResult {
    let project = temp_dirs::tempdir()?;
    write_project(project.path(), COMPOSE, "hardening: strict\n", SERVICES, "")?;
    let first = compile(project.path(), "production", None)?;
    let other = SERVICES.replace("writes its socket under /var/run", "another reason");
    write_project(project.path(), COMPOSE, "hardening: strict\n", &other, "")?;
    let second = compile(project.path(), "production", None)?;
    // The overlay is the same; the reason a reviewer reads is not.
    assert_eq!(
        first.stacks[0].manifest.generated_overlay_digest,
        second.stacks[0].manifest.generated_overlay_digest
    );
    assert_ne!(
        first.stacks[0].manifest.artifact_hash,
        second.stacks[0].manifest.artifact_hash
    );
    Ok(())
}

#[test]
fn mm_dev_is_not_hardened_and_a_project_without_the_key_is_unchanged() -> TestResult {
    let project = temp_dirs::tempdir()?;
    write_project(project.path(), COMPOSE, "hardening: strict\n", SERVICES, "")?;
    // The development environment on this machine: code is mounted and
    // tools write where they like.
    let development = compile(project.path(), "development", None)?;
    assert!(development.stacks[0].manifest.security.is_empty());
    let overlay_text = fs::read_to_string(project.path().join("overlay.yaml"))?;
    for key in ["read_only", "cap_drop", "security_opt", "tmpfs", "runtime"] {
        assert!(!overlay_text.contains(key), "{overlay_text}");
    }

    write_project(
        project.path(),
        COMPOSE,
        "",
        "      web:\n        stack: app\n",
        "",
    )?;
    let plain = compile(project.path(), "production", None)?;
    assert!(plain.stacks[0].manifest.security.is_empty());
    let overlay_text = fs::read_to_string(project.path().join("overlay.yaml"))?;
    assert!(!overlay_text.contains("read_only"), "{overlay_text}");
    Ok(())
}

#[test]
fn an_exception_without_a_reason_is_refused() -> TestResult {
    for reason in ["\"\"", "\"  \"", ""] {
        let project = temp_dirs::tempdir()?;
        let services = format!(
            "      web:\n        stack: app\n        hardening:\n          except:\n            cap-drop: {reason}\n"
        );
        write_project(
            project.path(),
            COMPOSE,
            "hardening: strict\n",
            &services,
            "",
        )?;
        let document = ConfigLoader.load_from(project.path())?;
        let report = validate(&document, "production");
        let found = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "MM_HARDENING_REASON")
            .ok_or("an exception without a reason passed validation")?;
        assert_eq!(
            found.message,
            "modules.app.services.web.hardening.except.cap-drop gives no reason"
        );
        assert!(
            found
                .help
                .as_deref()
                .is_some_and(|help| help.contains("cap-drop: \"")),
            "{:?}",
            found.help
        );
        // And nothing is planned from it.
        assert!(compile(project.path(), "production", None).is_err());
    }
    // A key that is no default of the set says which ones are.
    let project = temp_dirs::tempdir()?;
    let services = "      web:\n        stack: app\n        hardening:\n          except:\n            readonly: typo\n";
    write_project(project.path(), COMPOSE, "hardening: strict\n", services, "")?;
    let error = match ConfigLoader.load_from(project.path()) {
        Ok(_) => return Err("an unknown exception was read".into()),
        Err(error) => error.to_string(),
    };
    assert!(
        error.contains("readonly") && error.contains("read-only"),
        "{error}"
    );
    Ok(())
}

#[test]
fn what_a_compose_file_sets_against_the_defaults_needs_an_exception() -> TestResult {
    let compose =
        "services:\n  web:\n    image: example/web\n    security_opt: [seccomp=unconfined]\n";
    let project = temp_dirs::tempdir()?;
    write_project(
        project.path(),
        compose,
        "hardening: strict\n",
        "      web:\n        stack: app\n",
        "",
    )?;
    let refused = refusal(compile(project.path(), "production", None))?;
    assert!(
        refused.contains("service 'app/web' sets `security_opt: seccomp=unconfined`"),
        "{refused}"
    );
    assert!(
        refused.contains("hardening: {except: {seccomp: \"<reason>\"}}"),
        "{refused}"
    );

    let services = "      web:\n        stack: app\n        hardening:\n          except:\n            seccomp: the profiler needs perf_event_open\n";
    write_project(project.path(), compose, "hardening: strict\n", services, "")?;
    let report = compile(project.path(), "production", None)?;
    assert_eq!(
        security(&report, "web")?.describe(),
        [
            "web: read-only root, no capabilities, no new privileges, seccomp unconfined, tmpfs /tmp",
            "web: not seccomp (the seccomp profile is the Compose file's): the profiler needs perf_event_open",
        ]
    );

    for (compose, named) in [
        (
            "services:\n  web:\n    image: example/web\n    security_opt: [no-new-privileges=false]\n",
            "no-new-privileges:false",
        ),
        (
            "services:\n  web:\n    image: example/web\n    privileged: true\n",
            "privileged: true",
        ),
    ] {
        write_project(
            project.path(),
            compose,
            "hardening: strict\n",
            "      web:\n        stack: app\n",
            "",
        )?;
        let refused = refusal(compile(project.path(), "production", None))?;
        assert!(
            refused.contains(named) && refused.contains("except"),
            "{refused}"
        );
    }
    // With both exceptions the plan says what a privileged container is.
    let services = "      web:\n        stack: app\n        hardening:\n          except:\n            cap-drop: loads a kernel module\n            seccomp: loads a kernel module\n";
    write_project(
        project.path(),
        "services:\n  web:\n    image: example/web\n    privileged: true\n",
        "hardening: strict\n",
        services,
        "",
    )?;
    let report = compile(project.path(), "production", None)?;
    assert_eq!(
        security(&report, "web")?.describe()[0],
        "web: read-only root, privileged (every capability), no new privileges, no seccomp profile, tmpfs /tmp"
    );
    Ok(())
}

const RUNTIME: &str = "services:
  web:
    image: example/web
    runtime: kata
";

#[test]
fn a_runtime_the_host_does_not_offer_is_refused_with_the_ones_it_does() -> TestResult {
    let project = temp_dirs::tempdir()?;
    write_project(
        project.path(),
        RUNTIME,
        "",
        "      web:\n        stack: app\n",
        "",
    )?;
    let refused = refusal(compile(
        project.path(),
        "production",
        Some(host(&["runsc", "crun"], None)),
    ))?;
    assert!(
        refused.contains("service 'app/web' sets `runtime: kata`, which this host does not offer. It offers crun, runsc."),
        "{refused}"
    );
    assert!(
        refused.contains("`runtimes:` in /etc/mm/host.yaml"),
        "{refused}"
    );

    let refused = refusal(compile(project.path(), "production", Some(host(&[], None))))?;
    assert!(
        refused.contains("It offers no runtime besides its engine's default."),
        "{refused}"
    );

    // Offered: the plan shows it. On this machine nothing is checked, and
    // the plan shows it all the same.
    for policy in [Some(host(&["kata"], None)), None] {
        let report = compile(project.path(), "production", policy)?;
        assert_eq!(security(&report, "web")?.describe(), ["web: runtime kata"]);
        assert!(
            overlay(project.path())?["services"]["web"]
                .get("runtime")
                .is_none()
        );
    }
    Ok(())
}

#[test]
fn a_host_puts_what_is_reachable_from_outside_under_its_runtime() -> TestResult {
    let compose = "services:
  web:
    image: example/web
    expose: [8080]
  api:
    image: example/api
    ports: ['8081:8081']
  worker:
    image: example/worker
";
    let services = "      web:\n        stack: app\n      api:\n        stack: app\n      worker:\n        stack: app\n";
    let route = "    ingress:\n      domain: shop\n      service: web\n      port: 8080\n";
    let project = temp_dirs::tempdir()?;
    write_project(project.path(), compose, "", services, route)?;
    let policy = host(&["runsc", "crun"], Some("runsc"));
    let report = compile(project.path(), "production", Some(policy.clone()))?;
    let overlay = overlay(project.path())?;
    // `api` publishes a port and `web` takes ingress traffic; `worker` does
    // neither.
    assert_eq!(overlay["services"]["api"]["runtime"], "runsc");
    assert_eq!(overlay["services"]["web"]["runtime"], "runsc");
    assert!(
        overlay["services"]
            .get("worker")
            .is_none_or(|worker| worker.get("runtime").is_none())
    );
    assert_eq!(
        security(&report, "api")?.describe(),
        [
            "api: runtime runsc (this host runs what publishes a port or takes ingress traffic under it)"
        ]
    );
    assert!(security(&report, "worker").is_err());

    // A service that names the host's runtime itself is left as it is, and
    // one that names another is refused.
    let own = compose.replace(
        "ports: ['8081:8081']",
        "ports: ['8081:8081']\n    runtime: runsc",
    );
    write_project(project.path(), &own, "", services, route)?;
    let report = compile(project.path(), "production", Some(policy.clone()))?;
    assert_eq!(security(&report, "api")?.describe(), ["api: runtime runsc"]);
    let other = compose.replace(
        "ports: ['8081:8081']",
        "ports: ['8081:8081']\n    runtime: crun",
    );
    write_project(project.path(), &other, "", services, route)?;
    let refused = refusal(compile(project.path(), "production", Some(policy)))?;
    assert!(
        refused.contains("service 'app/api' sets `runtime: crun`, but it publishes a port or takes ingress traffic, and this host runs such services under runsc"),
        "{refused}"
    );
    assert!(
        refused.contains("`exposed-runtime` in /etc/mm/host.yaml"),
        "{refused}"
    );
    Ok(())
}

fn policy(extra: &str) -> Result<HostPolicy, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(&format!(
        "schema-version: 1\nbreak-glass:\n  - name: ops\n    key: {KEY}\n{extra}"
    ))
}

#[test]
fn the_host_policy_names_its_runtimes() -> TestResult {
    let host = policy("runtimes: [runsc, crun]\nexposed-runtime: runsc\n")?;
    host.validate()?;
    let runtimes = host.host_runtimes();
    assert_eq!(
        runtimes.advertised,
        BTreeSet::from(["crun".to_owned(), "runsc".to_owned()])
    );
    assert_eq!(runtimes.exposed.as_deref(), Some("runsc"));
    assert_eq!(runtimes.offered(), "crun, runsc");

    // A policy that says nothing offers nothing and asks for nothing.
    let silent = policy("")?;
    silent.validate()?;
    assert_eq!(silent.host_runtimes(), HostRuntimes::default());
    assert!(!serde_yaml_ng::to_string(&silent)?.contains("runtimes"));

    let Err(problem) = policy("exposed-runtime: runsc\n")?.validate() else {
        return Err("a runtime the host does not offer was required".into());
    };
    assert_eq!(
        problem,
        "exposed-runtime 'runsc' is not one of `runtimes`; add it there: `runtimes: [runsc]`"
    );
    let Err(problem) = policy("runtimes: ['run sc']\n")?.validate() else {
        return Err("a name with a space passed".into());
    };
    assert!(
        problem.contains("'run sc' is not a runtime name"),
        "{problem}"
    );
    Ok(())
}

#[test]
fn doctor_names_each_runtime_a_host_offers_and_cannot_run() {
    let check = |name: &str, problem: Option<&str>| RuntimeCheck {
        name: name.to_owned(),
        works: problem.is_none(),
        problem: problem.map(str::to_owned),
    };
    let nodes = [
        NodeRuntimeReport {
            name: "app-1".to_owned(),
            reachable: true,
            problem: None,
            status: Some(HostRuntimeStatus {
                runtimes: vec![
                    check("crun", None),
                    check(
                        "runsc",
                        Some("Docker on this host has no runtime named 'runsc' (it has runc)"),
                    ),
                ],
                exposed_runtime: Some("runsc".to_owned()),
            }),
        },
        // A host that offers none, one that was not reached, and one that
        // was reached and could not say.
        NodeRuntimeReport {
            name: "app-2".to_owned(),
            reachable: true,
            problem: None,
            status: Some(HostRuntimeStatus::default()),
        },
        NodeRuntimeReport {
            name: "app-3".to_owned(),
            reachable: false,
            problem: Some("did not answer".to_owned()),
            status: None,
        },
        NodeRuntimeReport {
            name: "app-4".to_owned(),
            reachable: true,
            problem: Some("the host policy does not parse".to_owned()),
            status: None,
        },
    ];
    let found = runtime_diagnostics(&nodes);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].code, "MM_RUNTIME_NOT_WORKING");
    assert_eq!(
        found[0].message,
        "host 'app-1' offers the runtime 'runsc' in its policy and cannot run a container under it"
    );
    assert_eq!(
        found[0].help.as_deref(),
        Some("Docker on this host has no runtime named 'runsc' (it has runc)")
    );
    assert_eq!(found[1].code, "MM_RUNTIME_NOT_CHECKED");
    assert_eq!(
        found[1].message,
        "host 'app-4' did not say whether the runtimes its policy offers work: the host policy does not parse"
    );
}

/// One stack's plan as a host keeps it, with `services` and the runtime
/// each runs under (`None`: the engine's default, no security entry).
fn stack_plan(
    services: &[(&str, Option<&str>)],
) -> Result<mm::domain::deployment::DeploymentStackPlan, serde_json::Error> {
    let manifest_services = services
        .iter()
        .map(|(name, _)| serde_json::json!({"module": "shop", "service": name}))
        .collect::<Vec<_>>();
    let security = services
        .iter()
        .filter_map(|(name, runtime)| {
            runtime.map(|runtime| serde_json::json!({"service": name, "runtime": runtime, "runtime-from-host": true}))
        })
        .collect::<Vec<_>>();
    serde_json::from_value(serde_json::json!({
        "stack": "app",
        "compose_project_name": "shop-production-app",
        "artifact_hash": "a",
        "project_directory": "/srv/shop",
        "artifact": {
            "root": "/state/app",
            "manifest_path": "/state/app/manifest.json",
            "generated_overlay_path": "/state/app/overlay.yaml",
            "compose_files": [],
            "manifest": {
                "project_id": "00000000-0000-0000-0000-000000000001",
                "project_name": "shop",
                "target": "local",
                "target_id": "00000000-0000-0000-0000-000000000002",
                "environment": "production",
                "stack": "app",
                "compose_project_name": "shop-production-app",
                "user_files": [],
                "generated_overlay_digest": "b",
                "services": manifest_services,
                "routes": [],
                "security": security,
                "artifact_hash": "a"
            }
        }
    }))
}

/// On the VMs a deployment after `exposed-runtime` left the host policy
/// moved web off gVisor and its plan said nothing about it.
#[test]
fn a_plan_says_when_a_service_moves_to_another_runtime() -> TestResult {
    let before = [stack_plan(&[("web", Some("runsc")), ("worker", None)])?];
    let after = [stack_plan(&[
        ("web", None),
        ("worker", None),
        ("api", Some("runsc")),
    ])?];
    let changes = mm::domain::hardening::runtime_changes(&before, &after);
    assert_eq!(
        changes
            .iter()
            .map(mm::domain::hardening::RuntimeChange::line)
            .collect::<Vec<_>>(),
        ["web: the engine's default runtime (was runtime runsc); its containers are created again"],
        "a new service is not a change"
    );
    // And back.
    let back = mm::domain::hardening::runtime_changes(&after, &before);
    assert_eq!(
        back.iter()
            .map(mm::domain::hardening::RuntimeChange::line)
            .collect::<Vec<_>>(),
        ["web: runtime runsc (was the engine's default runtime); its containers are created again"]
    );
    // The same runtimes, or no deployment before: nothing to say.
    assert!(mm::domain::hardening::runtime_changes(&before, &before).is_empty());
    assert!(mm::domain::hardening::runtime_changes(&[], &after).is_empty());
    Ok(())
}
