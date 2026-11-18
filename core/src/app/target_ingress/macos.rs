//! launchd artifacts for the local target on macOS with Docker Desktop. Every job
//! is a user agent: wildcard binds on ports 80/443 need no root on macOS.
use crate::domain::service_stack::DASHBOARD_BACKEND_PORT;
use crate::domain::target_ingress::{
    CompiledTargetIngressSupportFile, LAUNCHD_DASHBOARD_LABEL, LAUNCHD_FORWARDER_LABEL,
    LAUNCHD_INGRESS_LABEL, MACOS_AUTOSTART_SUPPORT_FILE, MACOS_COMPOSE_FILE,
    MACOS_FORWARDER_SUPPORT_FILE, MACOS_HAPROXY_FILE, MacosIngressHost,
};

use super::{compile_support_file, haproxy_config};

/// Docker Desktop may start after login; launchd relaunches a failed replay this often.
const AUTOSTART_RETRY_SECONDS: u32 = 15;
const FORWARDER_RESTART_SECONDS: u32 = 5;
const DASHBOARD_RESTART_SECONDS: u32 = 5;
/// launchd starts jobs with a minimal environment and no shell profile.
const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaunchdKeepAlive {
    /// Restart whenever the process exits; `launchctl bootout` is the only stop.
    Always,
    /// Relaunch only after a failed exit, so a one-shot replay retries until it succeeds.
    UntilSuccess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LaunchdJob {
    pub(crate) label: String,
    pub(crate) program_arguments: Vec<String>,
    pub(crate) environment: Vec<(String, String)>,
    pub(crate) keep_alive: LaunchdKeepAlive,
    pub(crate) throttle_seconds: u32,
    pub(crate) log_path: String,
    /// File-creation mask, for jobs that write private state.
    pub(crate) umask: Option<u32>,
}

pub(crate) fn render_support_files(
    host: &MacosIngressHost,
    dynamic_config_volume_name: &str,
    compose_project_name: &str,
    proxy_http_port: u16,
    proxy_https_port: u16,
) -> Vec<CompiledTargetIngressSupportFile> {
    vec![
        compile_support_file(
            "dynamic.yaml",
            format!("docker-volume://{dynamic_config_volume_name}/dynamic.yaml"),
            "0644",
            crate::domain::traefik_dynamic::EMPTY_DYNAMIC_CONFIG.to_owned(),
        ),
        compile_support_file(
            MACOS_HAPROXY_FILE,
            host.ingress_path(MACOS_HAPROXY_FILE),
            "0644",
            haproxy_config(proxy_http_port, proxy_https_port),
        ),
        compile_support_file(
            MACOS_FORWARDER_SUPPORT_FILE,
            host.launch_agent_path(LAUNCHD_FORWARDER_LABEL),
            "0644",
            render_launchd_plist(&forwarder_job(host)),
        ),
        compile_support_file(
            MACOS_AUTOSTART_SUPPORT_FILE,
            host.launch_agent_path(LAUNCHD_INGRESS_LABEL),
            "0644",
            render_launchd_plist(&autostart_job(host, compose_project_name)),
        ),
    ]
}

fn forwarder_job(host: &MacosIngressHost) -> LaunchdJob {
    LaunchdJob {
        label: LAUNCHD_FORWARDER_LABEL.to_owned(),
        program_arguments: vec![
            host.haproxy_executable.clone(),
            "-W".to_owned(),
            "-db".to_owned(),
            "-f".to_owned(),
            host.ingress_path(MACOS_HAPROXY_FILE),
        ],
        environment: vec![("PATH".to_owned(), SYSTEM_PATH.to_owned())],
        keep_alive: LaunchdKeepAlive::Always,
        throttle_seconds: FORWARDER_RESTART_SECONDS,
        log_path: host.log_path("ingress-forwarder.log"),
        umask: None,
    }
}

fn autostart_job(host: &MacosIngressHost, compose_project_name: &str) -> LaunchdJob {
    LaunchdJob {
        label: LAUNCHD_INGRESS_LABEL.to_owned(),
        program_arguments: vec![
            host.docker_executable.clone(),
            "compose".to_owned(),
            "--file".to_owned(),
            host.ingress_path(MACOS_COMPOSE_FILE),
            "--project-directory".to_owned(),
            host.ingress_path("current"),
            "--project-name".to_owned(),
            compose_project_name.to_owned(),
            "up".to_owned(),
            "--detach".to_owned(),
            "--remove-orphans".to_owned(),
        ],
        environment: docker_job_environment(host),
        keep_alive: LaunchdKeepAlive::UntilSuccess,
        throttle_seconds: AUTOSTART_RETRY_SECONDS,
        log_path: host.log_path("ingress.log"),
        umask: None,
    }
}

fn docker_job_environment(host: &MacosIngressHost) -> Vec<(String, String)> {
    let docker_directory = host
        .docker_executable
        .rsplit_once('/')
        .map_or("/usr/local/bin", |(directory, _)| directory);
    vec![
        ("DOCKER_HOST".to_owned(), host.docker_host.clone()),
        ("HOME".to_owned(), host.home.clone()),
        // Docker credential helpers and CLI plugins live beside the Docker CLI.
        (
            "PATH".to_owned(),
            format!("{docker_directory}:{SYSTEM_PATH}"),
        ),
    ]
}

/// The dashboard listens on loopback only: Docker Desktop's `host-gateway`
/// reaches host loopback, so Traefik needs no LAN-facing listener.
pub(crate) fn render_dashboard_plist(
    host: &MacosIngressHost,
    executable: &str,
    mm_home: &str,
    runtime: &str,
) -> String {
    render_launchd_plist(&LaunchdJob {
        label: LAUNCHD_DASHBOARD_LABEL.to_owned(),
        program_arguments: vec![
            executable.to_owned(),
            "--mm-home".to_owned(),
            mm_home.to_owned(),
            "--runtime".to_owned(),
            runtime.to_owned(),
            "dev".to_owned(),
            "dashboard".to_owned(),
            "serve".to_owned(),
            "--listen".to_owned(),
            format!("127.0.0.1:{DASHBOARD_BACKEND_PORT}"),
        ],
        environment: docker_job_environment(host),
        keep_alive: LaunchdKeepAlive::UntilSuccess,
        throttle_seconds: DASHBOARD_RESTART_SECONDS,
        log_path: host.log_path("dashboard.log"),
        umask: Some(0o077),
    })
}

pub(crate) fn render_launchd_plist(job: &LaunchdJob) -> String {
    let mut plist = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
        "<plist version=\"1.0\">\n<dict>\n",
    ));
    push_string(&mut plist, "Label", &job.label);
    plist.push_str("  <key>ProgramArguments</key>\n  <array>\n");
    for argument in &job.program_arguments {
        plist.push_str(&format!("    <string>{}</string>\n", escape(argument)));
    }
    plist.push_str("  </array>\n");
    if !job.environment.is_empty() {
        plist.push_str("  <key>EnvironmentVariables</key>\n  <dict>\n");
        for (name, value) in &job.environment {
            plist.push_str(&format!(
                "    <key>{}</key>\n    <string>{}</string>\n",
                escape(name),
                escape(value)
            ));
        }
        plist.push_str("  </dict>\n");
    }
    plist.push_str("  <key>RunAtLoad</key>\n  <true/>\n  <key>KeepAlive</key>\n");
    match job.keep_alive {
        LaunchdKeepAlive::Always => plist.push_str("  <true/>\n"),
        LaunchdKeepAlive::UntilSuccess => {
            plist.push_str("  <dict>\n    <key>SuccessfulExit</key>\n    <false/>\n  </dict>\n")
        }
    }
    plist.push_str(&format!(
        "  <key>ThrottleInterval</key>\n  <integer>{}</integer>\n",
        job.throttle_seconds
    ));
    if let Some(umask) = job.umask {
        plist.push_str(&format!(
            "  <key>Umask</key>\n  <integer>{umask}</integer>\n"
        ));
    }
    push_string(&mut plist, "StandardOutPath", &job.log_path);
    push_string(&mut plist, "StandardErrorPath", &job.log_path);
    plist.push_str("</dict>\n</plist>\n");
    plist
}

fn push_string(plist: &mut String, key: &str, value: &str) {
    plist.push_str(&format!(
        "  <key>{key}</key>\n  <string>{}</string>\n",
        escape(value)
    ));
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests;
