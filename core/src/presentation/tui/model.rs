use std::collections::BTreeSet;

use crate::domain::ca::LocalCaStatus;
use crate::domain::diagnostic::{Diagnostic, Severity};
use crate::domain::environment::{EnvironmentStatusSnapshot, ResolutionStatus, ValueSource};
use crate::domain::service_stack::ServiceStackStatus;
use crate::domain::workspace::WorkspaceSnapshot;
pub use crate::presentation::tui::access::{AccessView, TeamMemberView};
use crate::presentation::tui::editor::Editor;
use crate::presentation::tui::environments::{EnvironmentEntry, EnvironmentsView};
pub use crate::presentation::tui::job_actions::JobsPane;
use crate::presentation::tui::loading::node_findings;
use crate::presentation::tui::ops::ServiceTarget;
use crate::presentation::tui::panel::{Panel, RunPrompt, ServiceAction, ServiceMenu};
use crate::presentation::tui::service_nodes;
pub use crate::presentation::tui::service_nodes::ServiceRow;
pub use crate::presentation::tui::variables::{EnvironmentRow, VariablesPane};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Environments,
    Services,
    Ingress,
    Variables,
    Mounts,
    Diagnostics,
    Tls,
    Access,
    Gates,
    Providers,
    Jobs,
    Guided,
}

/// Sidebar sections; views are listed and numbered in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabGroup {
    Workspace,
    Health,
    Security,
    Automation,
    Setup,
}

impl TabGroup {
    pub const ALL: [Self; 5] = [
        Self::Workspace,
        Self::Health,
        Self::Security,
        Self::Automation,
        Self::Setup,
    ];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Workspace => "Workspace",
            Self::Health => "Health",
            Self::Security => "Security",
            Self::Automation => "Automation",
            Self::Setup => "Setup",
        }
    }

    pub fn tabs(self) -> impl Iterator<Item = Tab> {
        Tab::ALL.into_iter().filter(move |tab| tab.group() == self)
    }
}

impl Tab {
    pub const ALL: [Self; 13] = [
        Self::Overview,
        Self::Environments,
        Self::Services,
        Self::Ingress,
        Self::Variables,
        Self::Mounts,
        Self::Diagnostics,
        Self::Tls,
        Self::Access,
        Self::Gates,
        Self::Providers,
        Self::Jobs,
        Self::Guided,
    ];

    /// Digit keys select the first ten views in sidebar order.
    pub const DIGITS: [char; 10] = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Environments => "Environments",
            Self::Services => "Services",
            Self::Ingress => "Ingress",
            Self::Variables => "Variables",
            Self::Mounts => "Mounts",
            Self::Diagnostics => "Diagnostics",
            Self::Tls => "TLS",
            Self::Access => "Access",
            Self::Gates => "Gates",
            Self::Providers => "Providers",
            Self::Jobs => "Jobs",
            Self::Guided => "Guided",
        }
    }

    #[must_use]
    pub const fn group(self) -> TabGroup {
        match self {
            Self::Overview
            | Self::Environments
            | Self::Services
            | Self::Ingress
            | Self::Variables
            | Self::Mounts => TabGroup::Workspace,
            Self::Diagnostics | Self::Tls => TabGroup::Health,
            Self::Access | Self::Gates | Self::Providers => TabGroup::Security,
            Self::Jobs => TabGroup::Automation,
            Self::Guided => TabGroup::Setup,
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    #[must_use]
    pub fn digit(self) -> Option<char> {
        Self::DIGITS.get(self.index()).copied()
    }

    #[must_use]
    pub fn from_digit(digit: char) -> Option<Self> {
        Self::DIGITS
            .iter()
            .position(|candidate| *candidate == digit)
            .map(|index| Self::ALL[index])
    }

    #[must_use]
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    #[must_use]
    pub fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// The first view of the next sidebar group, wrapping around.
    #[must_use]
    pub fn next_group(self) -> Self {
        let mut tab = self.next();
        while tab.group() == self.group() {
            tab = tab.next();
        }
        tab
    }

    /// The first view of the previous sidebar group, or of this group when inside it.
    #[must_use]
    pub fn previous_group(self) -> Self {
        let first = |group: TabGroup| group.tabs().next().unwrap_or(Tab::Overview);
        if first(self.group()) != self {
            return first(self.group());
        }
        first(self.previous().group())
    }
}

/// Whether the most recent background status refresh succeeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveState {
    Live,
    Failed(String),
}

/// One background status refresh; `None` fields keep the previous value.
#[derive(Debug, Clone)]
pub struct LiveStatus {
    pub snapshot: WorkspaceSnapshot,
    pub environment: Option<EnvironmentStatusSnapshot>,
    pub tls: Option<LocalCaStatus>,
    pub installation_services: Option<ServiceStackStatus>,
    /// What the environment still needs; read only while something is open.
    pub setup: Option<super::setup::SetupView>,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub snapshot: WorkspaceSnapshot,
    pub environment: Option<EnvironmentStatusSnapshot>,
    pub tls: Option<LocalCaStatus>,
    pub installation_services: ServiceStackStatus,
    pub tab: Tab,
    pub should_quit: bool,
    pub interrupted: bool,
    /// Selected row in the current view; reset when the view changes.
    pub scroll: usize,
    pub help: bool,
    pub tick: u64,
    pub live: LiveState,
    pub jobs_pane: JobsPane,
    pub mounts: Vec<crate::domain::development_mount::DevelopmentMountPlan>,
    pub mount_message: Option<String>,
    pub workflow: crate::domain::onboarding::WorkflowState,
    pub providers: super::provider::ProviderModel,
    pub gates: super::gate::GateModel,
    pub jobs: crate::domain::scheduler::DesiredDefinitions,
    pub schedules: Vec<crate::domain::scheduler::ScheduleRecord>,
    pub job_runs: Vec<crate::domain::scheduler::JobRun>,
    pub scheduler_plan: Option<crate::domain::scheduler::ReconciliationPlan>,
    pub scheduler_message: Option<String>,
    /// The stack operation running in the background, if any.
    pub busy: Option<super::control::StackOperation>,
    /// A modal question; it captures every key until answered.
    pub confirm: Option<Confirm>,
    /// The plan under review, or the one being applied.
    pub review: Option<super::review::Review>,
    /// The outcome of the last action; cleared by the next key press.
    pub notice: Option<Notice>,
    pub env: EnvironmentView,
    /// What a remote environment's hosts hold and have staged.
    pub remote_values: Option<crate::domain::staged::RemoteValues>,
    pub access: AccessView,
    /// Every configured environment and what is known about each.
    pub environments: EnvironmentsView,
    /// Services whose containers are folded away in the Services view.
    pub collapsed: BTreeSet<(String, String)>,
    /// The action menu for the selected service or container.
    pub menu: Option<ServiceMenu>,
    /// The open log or command view; it captures keys until closed.
    pub panel: Option<Panel>,
    pub run_prompt: Option<RunPrompt>,
    /// Commands run this session, oldest first.
    pub run_history: Vec<String>,
    /// Labels of the services and containers being restarted.
    pub restarting: Vec<String>,
    /// The open configuration form or review; it captures every key.
    pub editor: Option<Editor>,
    /// What this environment still needs before it can start.
    pub setup: super::setup::SetupView,
    /// What could not be read when this environment was opened.
    pub launch_warnings: Vec<Diagnostic>,
    /// The environment became readable: everything read at launch is due again.
    pub reload_due: bool,
    /// The parts of this session that are still being read.
    pub loading: super::loading::Loading,
    /// What the nodes of a cluster add to the findings: a node that does
    /// not answer, a node that differs from its last deployment.
    node_findings: Vec<Diagnostic>,
    sessions: u64,
}

/// A yes/no dialog that emits `action` when accepted.
#[derive(Debug, Clone)]
pub struct Confirm {
    pub title: String,
    pub lines: Vec<String>,
    pub accept: &'static str,
    pub destructive: bool,
    pub action: super::update::Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Success,
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
}

impl Notice {
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            kind: NoticeKind::Success,
            text: text.into(),
        }
    }

    pub fn info(text: impl Into<String>) -> Self {
        Self {
            kind: NoticeKind::Info,
            text: text.into(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            kind: NoticeKind::Error,
            text: text.into(),
        }
    }
}

/// Search, filter, and editing state for the Environment view.
#[derive(Debug, Clone, Default)]
pub struct EnvironmentView {
    pub filter: String,
    /// Keys go to the search box instead of shortcuts.
    pub searching: bool,
    pub problems_only: bool,
    /// The service whose variables are shown, by position in the service list.
    pub service: usize,
    pub pane: VariablesPane,
    pub edit: Option<EnvironmentEdit>,
    /// An override or secret changed since the stack was last started.
    pub pending_restart: bool,
}

/// An in-progress override edit of one global key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentEdit {
    pub key: String,
    pub value: String,
    /// How many variables across services read this key.
    pub usages: usize,
}

#[must_use]
pub const fn source_label(source: ValueSource) -> &'static str {
    match source {
        ValueSource::ServiceEnv => "service env",
        ValueSource::SqliteOverride => "local override",
        ValueSource::ConfigValue => "config",
        ValueSource::Release => "release",
        ValueSource::Secret => "secret",
        ValueSource::TemplateDefault => "template default",
        ValueSource::TemplateLiteral => "template literal",
        ValueSource::Missing => "missing",
    }
}

#[must_use]
pub const fn status_label(status: ResolutionStatus) -> &'static str {
    match status {
        ResolutionStatus::Resolved => "resolved",
        ResolutionStatus::Defaulted => "defaulted",
        ResolutionStatus::Unset => "unset",
        ResolutionStatus::NotChecked => "not checked",
        ResolutionStatus::ProviderUnavailable => "unavailable",
        ResolutionStatus::AccessDenied => "denied",
        ResolutionStatus::Invalid => "invalid",
    }
}

impl Model {
    #[must_use]
    pub fn new(input: ModelInput) -> Self {
        let ModelInput {
            snapshot,
            environment,
            tls,
            installation_services,
            mounts,
            backends,
            gates,
            jobs,
            schedules,
            job_runs,
            access,
            environments,
            setup,
        } = input;
        let node_findings = node_findings(&snapshot);
        Self {
            snapshot,
            environment,
            remote_values: None,
            tls,
            installation_services,
            tab: Tab::Overview,
            should_quit: false,
            interrupted: false,
            scroll: 0,
            help: false,
            tick: 0,
            live: LiveState::Live,
            jobs_pane: JobsPane::Jobs,
            mounts,
            mount_message: None,
            workflow: crate::domain::onboarding::WorkflowState::default(),
            providers: super::provider::ProviderModel::new(backends),
            gates: super::gate::GateModel::new(gates),
            jobs,
            schedules,
            job_runs,
            scheduler_plan: None,
            scheduler_message: None,
            busy: None,
            confirm: None,
            review: None,
            notice: None,
            env: EnvironmentView::default(),
            access,
            environments: EnvironmentsView::new(environments),
            collapsed: BTreeSet::new(),
            menu: None,
            panel: None,
            run_prompt: None,
            run_history: Vec::new(),
            restarting: Vec::new(),
            editor: None,
            setup,
            launch_warnings: Vec::new(),
            reload_due: false,
            loading: super::loading::Loading::default(),
            node_findings,
            sessions: 0,
        }
    }

    /// A fresh id for a log or command stream, so late events can be told apart.
    pub fn next_session(&mut self) -> u64 {
        self.sessions += 1;
        self.sessions
    }

    /// The remote target this environment runs on, or `None` when it is local.
    #[must_use]
    pub fn remote(&self) -> Option<&str> {
        self.access.remote_target.as_deref()
    }

    /// Services and, beneath each replicated one, its current containers.
    /// On a cluster the view lists a service's nodes beneath it instead.
    #[must_use]
    pub fn service_rows(&self) -> Vec<ServiceRow<'_>> {
        service_nodes::service_rows(self.services(), &self.collapsed)
    }

    /// The optional units that are off on this machine, listed below the
    /// services.
    #[must_use]
    pub fn optional_rows(&self) -> Vec<&crate::domain::development_units::OptionalUnit> {
        self.snapshot
            .optional
            .iter()
            .filter(|unit| !unit.on)
            .collect()
    }

    /// The optional unit a selected row turns on, or the one a selected
    /// running service belongs to, with whether it is on.
    #[must_use]
    pub fn selected_optional(
        &self,
    ) -> Option<(&crate::domain::development_units::OptionalUnit, bool)> {
        let rows = self.service_rows().len();
        if self.scroll >= rows {
            return self
                .optional_rows()
                .get(self.scroll - rows)
                .map(|unit| (*unit, false));
        }
        let service = self.selected_service_row()?.service();
        let name = format!("{}/{}", service.module.as_deref()?, service.name);
        let mut units = self
            .snapshot
            .optional
            .iter()
            .filter(|unit| unit.on && unit.services.contains(&name));
        let first = units.next()?;
        let unit = units
            .find(|unit| unit.kind == crate::domain::development_units::UnitKind::Service)
            .unwrap_or(first);
        Some((unit, true))
    }

    #[must_use]
    pub fn selected_service_row(&self) -> Option<ServiceRow<'_>> {
        self.service_rows().get(self.scroll).copied()
    }

    #[must_use]
    pub fn selected_service_target(&self) -> Option<ServiceTarget> {
        self.selected_service_row().map(|row| row.target())
    }

    /// A command runs in one container: the selected one, or the service's
    /// first running container when the service has several. On a cluster
    /// it runs where `mm service exec` runs it: on the first node the
    /// service is placed on, unless a node is selected.
    #[must_use]
    pub fn exec_target(&self, mut target: ServiceTarget) -> ServiceTarget {
        if target.container.is_none()
            && target.node.is_none()
            && self.snapshot.nodes.len() < 2
            && let Some(service) = self
                .services()
                .into_iter()
                .find(|service| service.stack == target.stack && service.name == target.service)
            && service.containers.len() > 1
            && let Some(container) = service
                .containers
                .iter()
                .find(|container| !container.history && container.is_running())
        {
            target.container = Some(container.id.clone());
            target.label = container.name.clone();
        }
        target
    }

    /// Why `action` cannot run on `target` in this environment, if it cannot.
    #[must_use]
    pub fn action_unavailable(
        &self,
        action: ServiceAction,
        target: &ServiceTarget,
    ) -> Option<String> {
        let remote = self.remote()?;
        match action {
            ServiceAction::Logs | ServiceAction::Variables => None,
            ServiceAction::Restart => {
                let level = self.access.identity.as_ref()?.access.as_ref()?.level;
                (level < crate::domain::host_policy::AccessLevel::Operate).then(|| {
                    format!(
                        "Restarting needs operate on '{}'; the host grants you {} on '{remote}'. Ask for level: operate in the users file.",
                        self.snapshot.environment,
                        level.as_str()
                    )
                })
            }
            ServiceAction::Run => {
                let access = self.access.identity.as_ref()?.access.as_ref()?.effective();
                match access.exec.services() {
                    None => Some(format!(
                        "The host does not grant you exec on '{}'. Ask for +exec in the users file.",
                        self.snapshot.environment
                    )),
                    Some(services)
                        if !services.is_empty() && !services.contains(&target.service) =>
                    {
                        Some(format!(
                            "The host grants you exec only on {}.",
                            services.into_iter().collect::<Vec<_>>().join(", ")
                        ))
                    }
                    Some(_) => None,
                }
            }
        }
    }

    pub fn apply_live(&mut self, result: Result<LiveStatus, String>) {
        match result {
            Ok(status) => {
                self.show_status(status.snapshot);
                if let Some(environment) = status.environment {
                    self.environment = Some(environment);
                }
                if let Some(tls) = status.tls {
                    self.tls = Some(tls);
                }
                if let Some(services) = status.installation_services {
                    self.installation_services = services;
                }
                if let Some(setup) = status.setup {
                    self.reload_due |=
                        self.setup.unavailable.is_some() && setup.unavailable.is_none();
                    self.setup = setup;
                }
                self.live = LiveState::Live;
                self.clamp_selection();
            }
            Err(message) => self.live = LiveState::Failed(message),
        }
    }

    /// Replaces the status on screen.
    pub fn show_status(&mut self, snapshot: WorkspaceSnapshot) {
        self.node_findings = node_findings(&snapshot);
        self.snapshot = snapshot;
    }

    /// Every diagnostic the TUI reports, in display order.
    #[must_use]
    pub fn diagnostics(&self) -> Vec<&Diagnostic> {
        // The shared development services run on this machine: they are
        // no finding of an environment on other hosts.
        let shared = match self.remote() {
            None => self.installation_services.diagnostics.as_slice(),
            Some(_) => &[],
        };
        let mut diagnostics = self
            .snapshot
            .validation
            .diagnostics
            .iter()
            .chain(self.node_findings.iter())
            .chain(self.snapshot.runtime.diagnostics.iter())
            .chain(shared)
            .chain(self.launch_warnings.iter())
            .collect::<Vec<_>>();
        if let Some(environment) = &self.environment {
            diagnostics.extend(environment.diagnostics.iter());
        }
        diagnostics.sort_by_key(|diagnostic| match diagnostic.severity {
            Severity::Error => 0,
            Severity::Warning => 1,
            Severity::Info => 2,
        });
        diagnostics
    }

    /// Warnings and errors, which are what the header and sidebar count as issues.
    #[must_use]
    pub fn issue_count(&self) -> usize {
        self.diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.severity != Severity::Info)
            .count()
    }

    #[must_use]
    pub fn services(&self) -> Vec<&crate::domain::runtime::ServiceSnapshot> {
        self.snapshot
            .stacks
            .iter()
            .flat_map(|stack| &stack.services)
            .collect()
    }

    /// Number of selectable rows in the current view.
    #[must_use]
    pub fn selectable_rows(&self) -> usize {
        match self.tab {
            Tab::Environments => self.environments.entries.len(),
            Tab::Services => self.service_rows().len() + self.optional_rows().len(),
            Tab::Ingress => self.snapshot.ingress.len(),
            Tab::Variables if self.host_values().is_some() => self.host_value_rows().len(),
            Tab::Variables => self.environment_rows().len(),
            Tab::Mounts => self.mounts.len(),
            Tab::Diagnostics => self.diagnostics().len(),
            Tab::Jobs => match self.jobs_pane {
                JobsPane::Jobs => self.jobs.jobs.len(),
                JobsPane::Schedules => self.schedules.len(),
                JobsPane::Runs => self.job_runs.len(),
            },
            Tab::Guided => self
                .workflow
                .review
                .as_ref()
                .map_or(0, |review| review.proposals.len()),
            Tab::Access => self.access.team.len(),
            Tab::Overview => self.setup.steps.len(),
            Tab::Tls | Tab::Gates | Tab::Providers => 0,
        }
    }

    /// Whether keys currently go to a text field rather than shortcuts.
    #[must_use]
    pub fn typing(&self) -> bool {
        self.run_prompt.is_some()
            || self.editor.is_some()
            || matches!(&self.panel, Some(Panel::Logs(logs)) if logs.filtering)
            || (self.panel.is_none()
                && self.tab == Tab::Variables
                && (self.env.searching || self.env.edit.is_some()))
    }

    pub fn clamp_selection(&mut self) {
        self.clamp_variable_service();
        self.scroll = self.scroll.min(self.selectable_rows().saturating_sub(1));
    }
}

pub(super) struct ModelInput {
    pub(super) snapshot: WorkspaceSnapshot,
    pub(super) environment: Option<EnvironmentStatusSnapshot>,
    pub(super) tls: Option<LocalCaStatus>,
    pub(super) installation_services: ServiceStackStatus,
    pub(super) mounts: Vec<crate::domain::development_mount::DevelopmentMountPlan>,
    pub(super) backends: super::provider::SecretBackendTuiContext,
    pub(super) gates: Vec<crate::domain::gate::GatePolicy>,
    pub(super) jobs: crate::domain::scheduler::DesiredDefinitions,
    pub(super) schedules: Vec<crate::domain::scheduler::ScheduleRecord>,
    pub(super) job_runs: Vec<crate::domain::scheduler::JobRun>,
    pub(super) access: AccessView,
    pub(super) environments: Vec<EnvironmentEntry>,
    pub(super) setup: super::setup::SetupView,
}

#[cfg(test)]
#[path = "model/tests.rs"]
mod tests;
