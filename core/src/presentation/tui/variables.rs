//! The Variables view by service: the service list, the rows of the chosen
//! service, and search across every service.

use crossterm::event::KeyCode;

use crate::domain::environment::{ResolutionStatus, ServiceEnvironmentStatus, VariableStatus};
use crate::presentation::tui::model::{EnvironmentView, Model, source_label, status_label};
use crate::presentation::tui::update::Action;

/// Which side of the Variables view takes the selection keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VariablesPane {
    Services,
    #[default]
    Variables,
}

/// One visible Variables row; `first` marks the first row shown for its service.
#[derive(Debug, Clone, Copy)]
pub struct EnvironmentRow<'a> {
    pub service: &'a ServiceEnvironmentStatus,
    pub variable: &'a VariableStatus,
    /// The service's position in [`Model::variable_services`].
    pub group: usize,
    /// The variable's position in its service.
    pub index: usize,
    pub first: bool,
}

impl EnvironmentRow<'_> {
    #[must_use]
    pub const fn is_problem(&self) -> bool {
        matches!(
            self.variable.status,
            ResolutionStatus::Unset
                | ResolutionStatus::ProviderUnavailable
                | ResolutionStatus::AccessDenied
                | ResolutionStatus::Invalid
        )
    }

    /// Whether every whitespace-separated term appears in a searchable field.
    fn matches(&self, terms: &[String]) -> bool {
        let variable = self.variable;
        let mut fields = vec![
            variable.name.to_lowercase(),
            self.service.service.to_lowercase(),
            self.service.stack.to_lowercase(),
            source_label(variable.source).to_owned(),
            status_label(variable.status).to_owned(),
        ];
        fields.extend(variable.global_key.as_deref().map(str::to_lowercase));
        if !variable.sensitive {
            fields.extend(variable.value.as_deref().map(str::to_lowercase));
        }
        terms
            .iter()
            .all(|term| fields.iter().any(|field| field.contains(term.as_str())))
    }
}

impl Model {
    /// Every service that declares variables, in the order the environment lists them.
    #[must_use]
    pub fn variable_services(&self) -> Vec<&ServiceEnvironmentStatus> {
        self.environment
            .iter()
            .flat_map(|environment| &environment.modules)
            .flat_map(|module| &module.services)
            .collect()
    }

    /// Every declared variable, before search and filters.
    #[must_use]
    pub fn all_environment_rows(&self) -> Vec<EnvironmentRow<'_>> {
        self.variable_services()
            .into_iter()
            .enumerate()
            .flat_map(|(group, service)| {
                service
                    .variables
                    .iter()
                    .enumerate()
                    .map(move |(index, variable)| EnvironmentRow {
                        service,
                        variable,
                        group,
                        index,
                        first: index == 0,
                    })
            })
            .collect()
    }

    /// Whether a search or the problem filter looks across every service.
    #[must_use]
    pub fn variable_search_active(&self) -> bool {
        self.env.problems_only || self.env.filter.split_whitespace().next().is_some()
    }

    /// The rows on screen: the chosen service's variables, or while
    /// searching every match, the chosen service's first.
    #[must_use]
    pub fn environment_rows(&self) -> Vec<EnvironmentRow<'_>> {
        let current = self.env.service;
        if !self.variable_search_active() {
            return self
                .all_environment_rows()
                .into_iter()
                .filter(|row| row.group == current)
                .collect();
        }
        let terms = self
            .env
            .filter
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>();
        let mut rows = self
            .all_environment_rows()
            .into_iter()
            .filter(|row| (!self.env.problems_only || row.is_problem()) && row.matches(&terms))
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| row.group != current);
        let mut previous = None;
        for row in &mut rows {
            row.first = previous != Some(row.group);
            previous = Some(row.group);
        }
        rows
    }

    #[must_use]
    pub fn selected_environment_row(&self) -> Option<EnvironmentRow<'_>> {
        self.environment_rows().get(self.scroll).copied()
    }

    /// Changes the search or problem filter; leaving search keeps the selected variable.
    pub(super) fn refilter(&mut self, change: impl FnOnce(&mut EnvironmentView)) {
        let selected = self
            .selected_environment_row()
            .map(|row| (row.group, row.index));
        change(&mut self.env);
        if self.variable_search_active() {
            self.scroll = 0;
        } else if let Some((group, index)) = selected {
            self.env.service = group;
            self.scroll = index;
        }
    }

    /// Shows the variables of one service; `false` when it declares none.
    pub(super) fn show_service_variables(&mut self, stack: &str, service: &str) -> bool {
        let Some(position) = self
            .variable_services()
            .iter()
            .position(|candidate| candidate.stack == stack && candidate.service == service)
        else {
            return false;
        };
        self.env.filter.clear();
        self.env.problems_only = false;
        self.env.searching = false;
        self.env.service = position;
        self.env.pane = VariablesPane::Variables;
        self.scroll = 0;
        true
    }

    pub(super) fn clamp_variable_service(&mut self) {
        self.env.service = self
            .env
            .service
            .min(self.variable_services().len().saturating_sub(1));
    }
}

/// Moves between the service list and the table, and through the list
/// while it has the focus.
pub(super) fn pane_key(model: &mut Model, code: KeyCode) -> Option<Action> {
    match (model.env.pane, code) {
        (VariablesPane::Variables, KeyCode::Left | KeyCode::Char('h')) => {
            model.env.pane = VariablesPane::Services;
        }
        (VariablesPane::Services, KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter) => {
            model.env.pane = VariablesPane::Variables;
        }
        (VariablesPane::Variables, KeyCode::Right | KeyCode::Char('l'))
        | (VariablesPane::Services, KeyCode::Left | KeyCode::Char('h'))
        | (VariablesPane::Services, KeyCode::Char('e' | 'x' | 'c' | 'D')) => {}
        (VariablesPane::Services, KeyCode::Down | KeyCode::Char('j')) => step_service(model, 1),
        (VariablesPane::Services, KeyCode::Up | KeyCode::Char('k')) => step_service(model, -1),
        (VariablesPane::Services, KeyCode::PageDown) => step_service(model, 10),
        (VariablesPane::Services, KeyCode::PageUp) => step_service(model, -10),
        (VariablesPane::Services, KeyCode::Home) => step_service(model, isize::MIN),
        (VariablesPane::Services, KeyCode::End) => step_service(model, isize::MAX),
        _ => return None,
    }
    Some(Action::None)
}

fn step_service(model: &mut Model, delta: isize) {
    let last = model.variable_services().len().saturating_sub(1);
    let service = model.env.service.saturating_add_signed(delta).min(last);
    if service != model.env.service {
        model.env.service = service;
        model.scroll = 0;
    }
}
