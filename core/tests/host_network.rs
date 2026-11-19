//! `network_mode: host`: a service on this machine runs there when its
//! `host-network` says why, a remote host when its policy allows
//! `network-host` for it, and `mm config validate` reports a refusal before
//! a deployment does.

mod daemon_host;
mod temp_dirs;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use daemon_host::{ALICE, Host, PROMPT, TestResult};
use mm::adapters::config::ConfigLoader;
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{ProjectDocumentStore, TargetStore};
use mm::app::{HostDocuments, StackManager, WorkspaceSelection};
use mm::domain::hardening::HostRuntimes;
use mm::domain::remote::RemoteMethod;
use mm::domain::stack::{HostNetworkUse, StackCompilationReport};
use mm::domain::target::{TargetDefinition, TargetId};

const VPN: &str = "services:
  web:
    image: example/web
    expose: [\"8080\"]
  vpn:
    image: example/vpn
    network_mode: host
    cap_add: [NET_ADMIN]
";

const REASON: &str = "owns the tunnel interface";

/// A project whose `development` and `production` environments run on this
/// machine. `services` goes under the module, `top` at the top of mm.yaml.
fn write_project(root: &Path, compose: &str, services: &str, top: &str) -> TestResult {
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("app"))?;
    fs::write(root.join("compose.yaml"), compose)?;
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3\n  name: shop\n  development-environment: development\ndefault-environment: development\ntarget: local\nruntime:\n  driver: docker-compose\n{top}stacks:\n  app:\n    files: [compose.yaml]\nmodules:\n  app:\n    path: app\n    services:\n      web:\n        stack: app\n{services}environments:\n  development:\n    bindings: {{}}\n  production:\n    domains:\n      shop: shop.example.test\n    bindings: {{}}\n"
        ),
    )?;
    Ok(())
}

fn vpn(extra: &str) -> String {
    format!("      vpn:\n        stack: app\n{extra}")
}

fn declared() -> String {
    vpn(&format!("        host-network: {REASON}\n"))
}

/// Compiles `environment` on this machine, or as a remote host's `mmd`
/// does when `on_host`.
fn compile(
    project: &Path,
    environment: &str,
    on_host: bool,
) -> Result<(StackCompilationReport, serde_yaml_ng::Value), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(uuid::Uuid::parse_str(
            "319c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?)),
    )?;
    let mut documents: Arc<dyn ProjectDocumentStore> = Arc::new(ConfigLoader);
    if on_host {
        documents = Arc::new(HostDocuments::new(documents, HostRuntimes::default()));
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
    let overlay = serde_yaml_ng::from_str(&fs::read_to_string(
        &report.stacks[0].generated_overlay_path,
    )?)?;
    Ok((report, overlay))
}

fn refusal(project: &Path, environment: &str, on_host: bool) -> Result<String, String> {
    match compile(project, environment, on_host) {
        Ok(_) => Err(format!("{environment} compiled")),
        Err(error) => Ok(error.to_string()),
    }
}

#[test]
fn a_declared_service_runs_on_the_host_network_and_the_plan_says_why() -> TestResult {
    let project = temp_dirs::tempdir()?;
    write_project(project.path(), VPN, &declared(), "")?;
    let (report, overlay) = compile(project.path(), "development", false)?;

    let uses = &report.stacks[0].manifest.host_network;
    assert_eq!(
        uses,
        &[HostNetworkUse {
            service: "vpn".to_owned(),
            reason: Some(REASON.to_owned()),
        }]
    );
    assert_eq!(
        uses[0].line(),
        "vpn uses the host's network: owns the tunnel interface"
    );

    // No route, no network of mm's and no development mail: none of them
    // reaches a container on the host's network. Its capabilities stay.
    let vpn = &overlay["services"]["vpn"];
    assert!(vpn.get("networks").is_none(), "{vpn:?}");
    assert!(vpn.get("environment").is_none(), "{vpn:?}");
    assert!(vpn.get("cap_drop").is_none(), "{vpn:?}");
    assert_eq!(vpn["userns_mode"], "host");
    assert!(
        report.stacks[0]
            .manifest
            .routes
            .iter()
            .all(|route| route.service != "vpn")
    );
    assert!(
        report.stacks[0]
            .manifest
            .routes
            .iter()
            .any(|route| route.service == "web"),
        "web keeps its automatic route"
    );
    Ok(())
}

#[test]
fn an_undeclared_service_is_refused_with_the_file_service_and_key() -> TestResult {
    let project = temp_dirs::tempdir()?;
    write_project(project.path(), VPN, &vpn(""), "")?;
    let message = refusal(project.path(), "development", false)?;
    assert!(message.contains("compose.yaml"), "{message}");
    assert!(message.contains("service 'vpn'"), "{message}");
    assert!(
        message.contains("add `host-network: <why>` under modules.app.services.vpn"),
        "{message}"
    );

    write_project(project.path(), VPN, &vpn("        host-network: ' '\n"), "")?;
    let message = refusal(project.path(), "development", false)?;
    assert!(message.contains("validation"), "{message}");
    Ok(())
}

#[test]
fn a_variable_a_merge_key_a_route_or_a_network_is_refused() -> TestResult {
    let project = temp_dirs::tempdir()?;
    let cases = [
        (
            VPN.replace("network_mode: host", "network_mode: ${MODE}"),
            declared(),
            "",
            "from a variable",
        ),
        (
            format!(
                "x-host: &host\n  network_mode: host\n{}",
                VPN.replace("    network_mode: host\n", "    <<: *host\n")
            ),
            declared(),
            "",
            "without writing `network_mode: host` on the service itself",
        ),
        (
            VPN.to_owned(),
            vpn(&format!(
                "        host-network: {REASON}\n        networks: [backend]\n"
            )),
            "networks:\n  backend: {}\n",
            "`modules.app.services.vpn.networks` in mm.yaml puts it on network 'backend'",
        ),
        (
            VPN.replace("    cap_add", "    ports: [\"5353:5353\"]\n    cap_add"),
            declared(),
            "",
            "both `network_mode: host` and `ports`",
        ),
    ];
    for (compose, services, top, expected) in cases {
        write_project(project.path(), &compose, &services, top)?;
        let message = refusal(project.path(), "development", false)?;
        assert!(message.contains(expected), "{expected}: {message}");
    }

    // A route goes over mm's own network, which the service is not on.
    write_project(project.path(), VPN, &declared(), "")?;
    let mm_yaml = project.path().join(".config/mm.yaml");
    let routed = fs::read_to_string(&mm_yaml)?.replace(
        "    files: [compose.yaml]\n",
        "    files: [compose.yaml]\n    ingress:\n      - domain: shop\n        service: vpn\n        port: 8080\n        environments: [production]\n",
    );
    fs::write(&mm_yaml, routed)?;
    let message = refusal(project.path(), "production", false)?;
    assert!(
        message.contains("an ingress route of stack 'app' selects it"),
        "{message}"
    );
    Ok(())
}

#[test]
fn a_remote_host_decides_without_the_declaration_and_shows_it_when_present() -> TestResult {
    let project = temp_dirs::tempdir()?;
    write_project(project.path(), VPN, &vpn(""), "")?;
    let (report, overlay) = compile(project.path(), "production", true)?;
    assert_eq!(
        report.stacks[0].manifest.host_network,
        [HostNetworkUse {
            service: "vpn".to_owned(),
            reason: None,
        }]
    );
    assert_eq!(overlay["services"]["vpn"]["userns_mode"], "host");

    write_project(project.path(), VPN, &declared(), "")?;
    let (report, _) = compile(project.path(), "production", true)?;
    assert_eq!(
        report.stacks[0].manifest.host_network[0].reason.as_deref(),
        Some(REASON)
    );
    Ok(())
}

#[tokio::test]
async fn a_host_plans_host_networking_only_for_a_service_its_policy_allows() -> TestResult {
    let host = Host::with_alice().await?;
    let commit = host.commit_files(&[(
        "compose.yaml",
        "services:\n  web:\n    image: busybox\n    network_mode: host\n    cap_add: [NET_ADMIN]\n",
    )])?;
    let plan = || RemoteMethod::PlanDeployment {
        commit: commit.clone(),
        receipt: None,
        release: Default::default(),
    };
    let (code, message) = host.ask(ALICE, plan()).await?.refusal(PROMPT).await?;
    assert_eq!(code, "MM_SECURITY", "{message}");
    assert!(
        message.contains("refuses Compose features that reach the host in environment 'production'"),
        "{message}"
    );
    assert!(
        message.contains("app/web uses network-host (network_mode: host)"),
        "{message}"
    );
    assert!(
        message.contains("under `allow` in /etc/mm/host.yaml"),
        "{message}"
    );

    let policy = fs::read_to_string(&host.config.host_policy_path)?;
    fs::write(
        &host.config.host_policy_path,
        format!(
            "{policy}    allow:\n      - service: app/web\n        features: [network-host, cap-add]\n"
        ),
    )?;
    host.plan_at(ALICE, &commit).await?;
    Ok(())
}

#[test]
fn config_validate_reports_the_refusal_for_each_environment() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let project = temporary.path().join("project");
    write_project(&project, VPN, &vpn(""), "")?;
    let validate = || -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let output = temp_dirs::mm()
            .arg("--root")
            .arg(&project)
            .arg("--mm-home")
            .arg(temporary.path().join("home"))
            .args(["--output", "json", "config", "validate"])
            .output()?;
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let report = validate()?;
    let found = report["validation"]["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|diagnostic| diagnostic["code"] == "MM_COMPOSE_SOURCE")
        .map(|diagnostic| {
            diagnostic["message"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{report:#}");
    assert!(
        found[0].contains("add `host-network: <why>` under modules.app.services.vpn"),
        "{}",
        found[0]
    );
    assert!(
        found[0].ends_with("(in environments development, production)"),
        "{}",
        found[0]
    );

    write_project(&project, VPN, &declared(), "")?;
    let report = validate()?;
    assert!(
        report["validation"]["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|diagnostic| diagnostic["code"] != "MM_COMPOSE_SOURCE"),
        "{report:#}"
    );
    Ok(())
}
