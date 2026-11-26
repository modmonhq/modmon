//! `mm update` past the binaries: this machine's user services, and the
//! nodes of the project's remote environments.

use super::*;

use crate::adapters::local_services::UserServiceManager;
use crate::app::UpdateFollowUpService;
use crate::app::ports::HostVersions;
use crate::domain::update_steps::{
    EnvironmentHosts, HostsView, NO_PROJECT_HOSTS, NodeVersion, host_is_older, upgrade_commands,
};

pub(crate) fn update_follow_up_service(
    start: PathBuf,
    home: Option<PathBuf>,
    environment: Option<String>,
) -> Result<UpdateFollowUpService, MmError> {
    let process: Arc<dyn crate::app::ports::ProcessRunner> =
        Arc::new(crate::adapters::process::TokioProcessRunner);
    Ok(UpdateFollowUpService::new(
        Arc::new(UserServiceManager::new(process)?),
        Arc::new(ProjectHostVersions {
            start,
            home,
            environment,
        }),
    ))
}

struct ProjectHostVersions {
    start: PathBuf,
    home: Option<PathBuf>,
    environment: Option<String>,
}

#[async_trait::async_trait]
impl HostVersions for ProjectHostVersions {
    async fn hosts(&self, version: &str) -> HostsView {
        let Ok(document) = ConfigLoader.load_from(&self.start) else {
            return HostsView::NoProject {
                note: NO_PROJECT_HOSTS.to_owned(),
            };
        };
        let names = match &self.environment {
            Some(name) => vec![name.clone()],
            None => document.config.environments.keys().cloned().collect(),
        };
        let environments = futures_util::future::join_all(
            names.iter().map(|name| self.environment(name, version)),
        )
        .await
        .into_iter()
        .flatten()
        .collect();
        HostsView::Project { environments }
    }
}

impl ProjectHostVersions {
    async fn environment(&self, name: &str, version: &str) -> Option<EnvironmentHosts> {
        let selection = WorkspaceSelection::new(&self.start, Some(name.to_owned()));
        let home = self.home.as_deref();
        let remote = resolve_runtime(&self.start, home, None, None, Some(name), false, true)
            .and_then(|runtime| remote_environment(&selection, home, runtime));
        let environment = match remote {
            Ok(Some(environment)) => environment,
            Ok(None)
            | Err(MmError::Setup {
                issue: crate::domain::readiness::SetupIssue::MissingTarget { .. },
            }) => return None,
            Err(error) => {
                return Some(EnvironmentHosts {
                    environment: name.to_owned(),
                    target: String::new(),
                    cluster: false,
                    nodes: Vec::new(),
                    problem: Some(format!(
                        "{error}; `mm --environment {name} target nodes` lists its hosts once that is fixed"
                    )),
                    upgrade: Vec::new(),
                    note: None,
                });
            }
        };
        let nodes = environment
            .node_reports()
            .await
            .into_iter()
            .map(|report| NodeVersion {
                older: report
                    .mmd_version
                    .as_deref()
                    .is_some_and(|running| host_is_older(running, version)),
                name: report.name,
                version: report.mmd_version,
                problem: report.problem,
            })
            .collect();
        let mut hosts = EnvironmentHosts {
            environment: name.to_owned(),
            target: environment.target().to_owned(),
            cluster: environment.is_cluster(),
            nodes,
            problem: None,
            upgrade: Vec::new(),
            note: None,
        };
        if hosts.needs_upgrade() {
            let (commands, note) = upgrade_commands(name, hosts.cluster);
            hosts.upgrade = commands;
            hosts.note = Some(note);
        }
        Some(hosts)
    }
}
