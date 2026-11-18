//! The Variables view of a remote environment: what its hosts hold and
//! have staged for each declared secret and variable, and the changes
//! staged from here for the next deployment.

use crossterm::event::KeyCode;

use crate::app::{KeyEdit, ValueEdit};
use crate::domain::staged::{HostValue, RemoteValues, ValueKind};
use crate::presentation::tui::model::{Confirm, EnvironmentEdit, Model, Notice};
use crate::presentation::tui::update::Action;

/// A change asked for in the Variables view. A secret's value is read with
/// hidden input before it becomes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageRequest {
    SetVariable { key: String, value: String },
    Unset(ValueKind, String),
    Discard(ValueKind, String),
}

impl StageRequest {
    #[must_use]
    pub fn edit(self) -> KeyEdit {
        let (kind, key, edit) = match self {
            Self::SetVariable { key, value } => (
                ValueKind::Variable,
                key,
                ValueEdit::Set(zeroize::Zeroizing::new(value)),
            ),
            Self::Unset(kind, key) => (kind, key, ValueEdit::Unset),
            Self::Discard(kind, key) => (kind, key, ValueEdit::Discard),
        };
        KeyEdit { kind, key, edit }
    }
}

/// One key as every node holds it.
#[derive(Debug, Clone)]
pub struct HostRow<'a> {
    pub kind: ValueKind,
    pub key: &'a str,
    pub nodes: Vec<(&'a str, &'a HostValue)>,
}

impl HostRow<'_> {
    /// What the first node holds; the nodes agree unless `disagree` says.
    #[must_use]
    pub fn value(&self) -> Option<&HostValue> {
        self.nodes.first().map(|(_, value)| *value)
    }

    #[must_use]
    pub fn matches(&self, filter: &str) -> bool {
        let filter = filter.to_ascii_lowercase();
        filter.is_empty()
            || self.key.to_ascii_lowercase().contains(&filter)
            || self.kind.as_str().contains(&filter)
            || self.value().is_some_and(|value| {
                value
                    .services
                    .iter()
                    .any(|service| service.to_ascii_lowercase().contains(&filter))
            })
    }
}

impl Model {
    /// What the hosts hold, on a remote environment whose hosts answered.
    #[must_use]
    pub fn host_values(&self) -> Option<&RemoteValues> {
        self.remote().and(self.remote_values.as_ref())
    }

    /// One row per key the hosts hold, after the search.
    #[must_use]
    pub fn host_value_rows(&self) -> Vec<HostRow<'_>> {
        let Some(values) = self.host_values() else {
            return Vec::new();
        };
        let mut rows = Vec::<HostRow<'_>>::new();
        for node in &values.nodes {
            let name = node.node.as_deref().unwrap_or(&values.target);
            for value in &node.values {
                match rows
                    .iter_mut()
                    .find(|row| row.kind == value.kind && row.key == value.key)
                {
                    Some(row) => row.nodes.push((name, value)),
                    None => rows.push(HostRow {
                        kind: value.kind,
                        key: &value.key,
                        nodes: vec![(name, value)],
                    }),
                }
            }
        }
        rows.retain(|row| row.matches(&self.env.filter));
        rows
    }

    fn selected_host_key(&self) -> Option<(ValueKind, String)> {
        self.host_value_rows()
            .get(self.scroll)
            .map(|row| (row.kind, row.key.to_owned()))
    }
}

/// Keys on a row of a remote environment's Variables view.
pub(super) fn key_action(model: &mut Model, code: KeyCode) -> Option<Action> {
    let (kind, key) = model.selected_host_key()?;
    match code {
        KeyCode::Enter | KeyCode::Char('e') => Some(match kind {
            ValueKind::Secret => Action::EditRemoteSecret(key),
            ValueKind::Variable => {
                model.env.edit = Some(EnvironmentEdit {
                    key,
                    value: String::new(),
                    usages: 0,
                });
                Action::None
            }
        }),
        KeyCode::Char('x') => {
            model.confirm = Some(Confirm {
                title: format!("Stage the removal of {key} on every host?"),
                lines: vec![
                    "Running services keep the value until the next deployment.".to_owned(),
                    "X drops a staged change before then.".to_owned(),
                ],
                accept: "stage removal",
                destructive: true,
                action: Action::Stage(StageRequest::Unset(kind, key)),
            });
            Some(Action::None)
        }
        KeyCode::Char('X') => Some(Action::Stage(StageRequest::Discard(kind, key))),
        _ => None,
    }
}

/// What a stage that finished says.
pub(super) fn staged(model: &mut Model, result: Result<String, String>) {
    model.notice = Some(match result {
        Ok(summary) => Notice::success(summary),
        Err(error) => Notice::error(error),
    });
}
