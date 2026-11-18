//! Reviewed changes to the project configuration: a short form, the exact
//! diff, and what happens next. Every change goes through the same plan and
//! apply as `mm config ... add|edit|remove`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Map, Value, json};

use crate::domain::host_policy::AccessLevel;
use crate::domain::onboarding::{
    ConfigMutationAction, ConfigMutationPlan, ConfigMutationRequest, ConfigResourceKind,
    UserProposal,
};
use crate::domain::runtime::RuntimeKind;
use crate::presentation::tui::model::{Model, Notice};
use crate::presentation::tui::panel::split_command;
use crate::presentation::tui::update::Action;

#[path = "editor/fields.rs"]
mod fields;

/// Longest value accepted in one form field.
const MAXIMUM_FIELD_CHARS: usize = 4 * 1024;
/// The choice that leaves a user out of an environment.
const NO_ACCESS: &str = "none";
/// The choice that leaves an environment on the project's runtime.
const PROJECT_RUNTIME: &str = "project default";
const LEVELS: [AccessLevel; 4] = [
    AccessLevel::View,
    AccessLevel::Operate,
    AccessLevel::Deploy,
    AccessLevel::Admin,
];
const RUNTIMES: [RuntimeKind; 2] = [RuntimeKind::PodmanCompose, RuntimeKind::DockerCompose];

/// A runtime's name as the configuration spells it.
fn runtime_name(kind: RuntimeKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|name| name.as_str().map(str::to_owned))
        .unwrap_or_default()
}

const YES: &str = "yes";
const NO: &str = "no";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorKind {
    Environment,
    /// One key's value for one environment, in the configuration.
    Binding,
    Job,
    Schedule,
    /// Someone else's entry in the users file.
    Teammate,
    /// This device's own entry in the users file.
    Join,
}

impl EditorKind {
    pub(super) const fn resource(self) -> ConfigResourceKind {
        match self {
            Self::Environment => ConfigResourceKind::Environment,
            Self::Binding => ConfigResourceKind::Binding,
            Self::Job => ConfigResourceKind::Job,
            Self::Schedule => ConfigResourceKind::Schedule,
            Self::Teammate | Self::Join => ConfigResourceKind::User,
        }
    }

    const fn noun(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::Binding => "value",
            Self::Job => "job",
            Self::Schedule => "schedule",
            Self::Teammate | Self::Join => "teammate",
        }
    }
}

/// What to open the editor on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorOpen {
    pub kind: EditorKind,
    pub action: ConfigMutationAction,
    /// The resource to edit or remove; typed in the form when adding.
    pub name: Option<String>,
    /// The environment a binding belongs to.
    pub parent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    /// One of a fixed set, changed with the arrow keys.
    Choice(Vec<String>),
    /// Comma-separated values.
    List,
    /// A command line, split into arguments like the run prompt.
    Command,
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub id: String,
    pub label: String,
    pub value: String,
    pub kind: FieldKind,
    pub help: String,
    pub required: bool,
}

impl Field {
    fn text(id: &str, label: &str, value: &str, help: &str) -> Self {
        Self {
            id: id.to_owned(),
            label: label.to_owned(),
            value: value.to_owned(),
            kind: FieldKind::Text,
            help: help.to_owned(),
            required: false,
        }
    }

    fn choice(id: &str, label: &str, value: &str, choices: Vec<String>, help: &str) -> Self {
        Self {
            kind: FieldKind::Choice(choices),
            ..Self::text(id, label, value, help)
        }
    }

    fn toggle(id: &str, label: &str, on: bool, help: &str) -> Self {
        Self {
            kind: FieldKind::Toggle,
            ..Self::text(id, label, if on { YES } else { NO }, help)
        }
    }

    fn required(mut self) -> Self {
        self.required = true;
        self
    }

    fn cycle(&mut self, forward: bool) {
        match &self.kind {
            FieldKind::Choice(choices) if !choices.is_empty() => {
                let current = choices
                    .iter()
                    .position(|choice| choice == &self.value)
                    .unwrap_or(0);
                let next = if forward {
                    (current + 1) % choices.len()
                } else {
                    (current + choices.len() - 1) % choices.len()
                };
                self.value.clone_from(&choices[next]);
            }
            FieldKind::Toggle => {
                self.value = if self.value == YES { NO } else { YES }.to_owned();
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub enum EditorStage {
    Form,
    /// The exact change, waiting for approval.
    Review {
        plan: Box<ConfigMutationPlan>,
        scroll: usize,
    },
    /// The change is written; what it takes for it to have an effect.
    Done {
        headline: String,
        steps: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Editor {
    pub kind: EditorKind,
    pub action: ConfigMutationAction,
    /// The resource's name; taken from the form when adding.
    pub name: String,
    pub parent: Option<String>,
    /// One sentence on what this change is and when it takes effect.
    pub note: Option<String>,
    /// The resource as written today; an edit keeps the keys it does not show.
    base: Value,
    pub fields: Vec<Field>,
    pub focus: usize,
    pub stage: EditorStage,
    pub error: Option<String>,
}

/// What the form needs to know about the project to offer sensible choices.
pub struct EditorContext {
    /// Remote environments, which are the ones access applies to.
    pub remote_environments: Vec<String>,
    pub jobs: Vec<String>,
    /// This device's public key, for a request to join.
    pub device_key: Option<String>,
    pub user: Option<String>,
}

impl EditorContext {
    pub fn from_model(model: &Model) -> Self {
        Self {
            remote_environments: model
                .environments
                .entries
                .iter()
                .filter(|entry| entry.remote.is_some())
                .map(|entry| entry.name.clone())
                .collect(),
            jobs: model.jobs.jobs.keys().cloned().collect(),
            device_key: None,
            user: None,
        }
    }
}

impl Editor {
    /// Builds the form for `open` from the resource as it is written today.
    #[must_use]
    pub fn new(open: EditorOpen, base: Option<Value>, context: &EditorContext) -> Self {
        let adding = open.action == ConfigMutationAction::Add;
        let base = base.unwrap_or(Value::Null);
        let mut name = open.name.clone().unwrap_or_default();
        let fields = fields::form(open.kind, adding, &base, context, &mut name);
        let base = match (open.kind, &context.device_key) {
            (EditorKind::Join, Some(key)) => json!({ "keys": [key] }),
            _ => base,
        };
        Self {
            kind: open.kind,
            action: open.action,
            name,
            parent: open.parent,
            note: fields::note(open.kind),
            base,
            fields,
            focus: 0,
            stage: EditorStage::Form,
            error: None,
        }
    }

    #[must_use]
    pub fn title(&self) -> String {
        match (self.kind, self.action) {
            (EditorKind::Join, _) => "Request access".to_owned(),
            (EditorKind::Binding, ConfigMutationAction::Remove) => {
                format!("Remove {} from the configuration", self.name)
            }
            (EditorKind::Binding, _) => format!(
                "Set {} for {}",
                self.name,
                self.parent.as_deref().unwrap_or("every environment")
            ),
            (EditorKind::Teammate, ConfigMutationAction::Add) => "Add a teammate".to_owned(),
            (kind, ConfigMutationAction::Add) => format!(
                "Add {} {}",
                if kind == EditorKind::Environment {
                    "an"
                } else {
                    "a"
                },
                kind.noun()
            ),
            (kind, ConfigMutationAction::Edit) => format!("Edit {} {}", kind.noun(), self.name),
            (kind, ConfigMutationAction::Remove) => {
                format!("Remove {} {}", kind.noun(), self.name)
            }
        }
    }

    fn value(&self, id: &str) -> &str {
        self.fields
            .iter()
            .find(|field| field.id == id)
            .map_or("", |field| field.value.trim())
    }

    /// The definition the form describes, starting from the resource as written.
    fn definition(&self) -> Result<Value, String> {
        let mut object = match &self.base {
            Value::Object(object) if self.kind != EditorKind::Binding => object.clone(),
            _ => Map::new(),
        };
        let mut set = |key: &str, value: Option<Value>| match value {
            Some(value) => {
                object.insert(key.to_owned(), value);
            }
            None => {
                object.remove(key);
            }
        };
        let text = |id: &str| Some(self.value(id)).filter(|value| !value.is_empty());
        match self.kind {
            EditorKind::Environment => {
                set("target", text("target").map(Value::from));
                set(
                    "runtime",
                    Some(self.value("runtime"))
                        .filter(|runtime| *runtime != PROJECT_RUNTIME)
                        .map(|runtime| json!({ "driver": runtime })),
                );
                let branches = self
                    .value("deploy-from")
                    .split(',')
                    .map(str::trim)
                    .filter(|branch| !branch.is_empty())
                    .collect::<Vec<_>>();
                set(
                    "deploy-from",
                    (!branches.is_empty()).then(|| json!(branches)),
                );
            }
            EditorKind::Binding => {
                let value = self.value("value");
                if self.value("kind") == "secret" {
                    if value.is_empty() {
                        return Err("Name the secret this key reads.".to_owned());
                    }
                    set("secret", Some(Value::from(value)));
                } else {
                    set("value", Some(Value::from(value)));
                }
            }
            EditorKind::Job => {
                let command = split_command(self.value("command"))?;
                if command.is_empty() {
                    return Err("Command is required.".to_owned());
                }
                set("module", text("module").map(Value::from));
                set("service", text("service").map(Value::from));
                set("command", Some(json!(command)));
                set("timeout", text("timeout").map(Value::from));
            }
            EditorKind::Schedule => {
                set("job", text("job").map(Value::from));
                set("cron", text("cron").map(Value::from));
                set("timezone", text("timezone").map(Value::from));
                set(
                    "enabled",
                    (self.value("enabled") == NO).then_some(Value::Bool(false)),
                );
            }
            EditorKind::Teammate | EditorKind::Join => {
                // An edit changes levels and keeps keys and add-ons such as exec.
                let mut access = match self.base.get("access") {
                    Some(Value::Object(access)) => access.clone(),
                    _ => Map::new(),
                };
                for (environment, level) in self.levels() {
                    match (level, access.get_mut(&environment)) {
                        (None, _) => {
                            access.remove(&environment);
                        }
                        (Some(level), Some(Value::Object(detailed))) => {
                            detailed.insert("level".to_owned(), Value::from(level.as_str()));
                        }
                        (Some(level), _) => {
                            access.insert(environment, Value::from(level.as_str()));
                        }
                    }
                }
                set("access", Some(Value::Object(access)));
            }
        }
        Ok(Value::Object(object))
    }

    /// The level chosen for each environment the form lists; `None` is no access.
    fn levels(&self) -> Vec<(String, Option<AccessLevel>)> {
        self.fields
            .iter()
            .filter_map(|field| {
                let environment = field.id.strip_prefix("access:")?;
                Some((
                    environment.to_owned(),
                    LEVELS
                        .into_iter()
                        .find(|level| level.as_str() == field.value),
                ))
            })
            .collect()
    }

    /// The reviewed change this editor asks for.
    pub fn request(&self) -> Result<ConfigMutationRequest, String> {
        if let Some(missing) = self
            .fields
            .iter()
            .find(|field| field.required && field.value.trim().is_empty())
        {
            return Err(format!("{} is required.", missing.label));
        }
        let adding = self.action == ConfigMutationAction::Add;
        let name = if adding && self.fields.iter().any(|field| field.id == "name") {
            self.value("name").to_owned()
        } else {
            self.name.clone()
        };
        let user = matches!(self.kind, EditorKind::Teammate | EditorKind::Join);
        if user && adding {
            // The same proposal `mm team add` and `mm team join` build.
            let key = match self.kind {
                EditorKind::Join => self
                    .base
                    .get("keys")
                    .and_then(|keys| keys.get(0))
                    .and_then(Value::as_str)
                    .ok_or("This device has no mm key yet; run `mm key create` first.")?,
                _ => self.value("key"),
            };
            let access = self
                .levels()
                .into_iter()
                .filter_map(|(environment, level)| Some((environment, level?)))
                .collect::<Vec<_>>();
            if access.is_empty() {
                return Err(
                    "Choose a level for at least one environment (← → changes it).".to_owned(),
                );
            }
            return UserProposal {
                name,
                key: key.to_owned(),
                machine: self.value("machine") == YES,
                access,
            }
            .request();
        }
        let definition = if self.action == ConfigMutationAction::Remove {
            None
        } else {
            Some(self.definition()?)
        };
        if user {
            return ConfigMutationRequest::user(self.action, &name, definition);
        }
        Ok(ConfigMutationRequest {
            action: self.action,
            resource: self.kind.resource(),
            name,
            parent: self.parent.clone(),
            definition,
        })
    }

    /// What it takes for an applied change to have an effect, when that is
    /// more than saving the file. `file` is the configuration file written.
    #[must_use]
    pub fn next_steps(
        &self,
        request: &ConfigMutationRequest,
        file: &str,
        remote: Option<&str>,
    ) -> Vec<String> {
        let name = &request.name;
        match (self.kind, self.action) {
            (EditorKind::Join, _) => vec![
                format!("Commit {file} on a branch and open a pull request."),
                "A code owner reviews and merges it.".to_owned(),
                "After the next deploy of an environment, open it with `mm -e ENVIRONMENT`: the Access view shows your level.".to_owned(),
            ],
            (EditorKind::Teammate, ConfigMutationAction::Remove) => vec![
                format!("Commit {file} and open a pull request."),
                format!("{name} keeps access to an environment until it is deployed again, or an admin runs `mm team sync` there."),
            ],
            (EditorKind::Teammate, _) => vec![
                format!("Commit {file} on a branch and open a pull request."),
                "After the merge, deploy each environment, or an admin runs `mm team sync` there.".to_owned(),
                format!("{name} opens an environment with `mm -e ENVIRONMENT` and checks the Access view."),
            ],
            _ => match remote {
                Some(target) => vec![
                    format!("Commit {file} and push it."),
                    format!("Press u to review and deploy that commit to '{target}'."),
                ],
                None => Vec::new(),
            },
        }
    }
}

/// Handles every key while the editor is open.
pub(super) fn key(model: &mut Model, key: KeyEvent) -> Action {
    let Some(editor) = model.editor.as_mut() else {
        return Action::None;
    };
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    match &mut editor.stage {
        EditorStage::Done { .. } => {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q')) {
                model.editor = None;
            }
        }
        EditorStage::Review { scroll, .. } => match key.code {
            KeyCode::Enter | KeyCode::Char('y') => return Action::EditorApply,
            KeyCode::Char('b') if editor.action != ConfigMutationAction::Remove => {
                editor.stage = EditorStage::Form;
                editor.error = None;
            }
            KeyCode::Esc | KeyCode::Char('n' | 'q') => {
                model.editor = None;
                model.notice = Some(Notice::info("Nothing was changed."));
            }
            KeyCode::Down | KeyCode::Char('j') => *scroll += 1,
            KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
            KeyCode::PageDown => *scroll += 10,
            KeyCode::PageUp => *scroll = scroll.saturating_sub(10),
            _ => {}
        },
        EditorStage::Form => {
            let count = editor.fields.len().max(1);
            match key.code {
                KeyCode::Esc => {
                    model.editor = None;
                    return Action::None;
                }
                KeyCode::Enter => return Action::EditorPlan,
                KeyCode::Tab | KeyCode::Down => editor.focus = (editor.focus + 1) % count,
                KeyCode::BackTab | KeyCode::Up => {
                    editor.focus = (editor.focus + count - 1) % count;
                }
                _ => {}
            }
            editor.error = None;
            let Some(field) = editor.fields.get_mut(editor.focus) else {
                return Action::None;
            };
            let typed = matches!(
                field.kind,
                FieldKind::Text | FieldKind::List | FieldKind::Command
            );
            match key.code {
                KeyCode::Left if !typed => field.cycle(false),
                KeyCode::Right | KeyCode::Char(' ') if !typed => field.cycle(true),
                KeyCode::Backspace if typed => {
                    field.value.pop();
                }
                KeyCode::Char('u') if typed && control => field.value.clear(),
                KeyCode::Char(character)
                    if typed
                        && !control
                        && !character.is_control()
                        && field.value.chars().count() < MAXIMUM_FIELD_CHARS =>
                {
                    field.value.push(character);
                }
                _ => {}
            }
        }
    }
    Action::None
}

/// Plans the form's change and shows it for review, or reports why it cannot be planned.
pub(super) fn plan(model: &mut Model, onboarding: &crate::app::OnboardingService) {
    let root = model.snapshot.workspace.root.clone();
    let Some(editor) = model.editor.as_mut() else {
        return;
    };
    let planned = editor.request().and_then(|request| {
        onboarding
            .plan_mutation(&root, request)
            .map_err(|error| crate::presentation::output::error_line(&error))
    });
    match planned {
        Ok(plan) => {
            editor.error = None;
            editor.stage = EditorStage::Review {
                plan: Box::new(plan),
                scroll: 0,
            };
        }
        Err(error) if editor.action == ConfigMutationAction::Remove => {
            model.editor = None;
            model.notice = Some(Notice::error(error));
        }
        Err(error) => editor.error = Some(error),
    }
}

/// Applies exactly the reviewed plan. Returns whether the configuration changed.
pub(super) fn apply(model: &mut Model, onboarding: &crate::app::OnboardingService) -> bool {
    let root = model.snapshot.workspace.root.clone();
    let remote = model.remote().map(str::to_owned);
    let Some(editor) = model.editor.as_mut() else {
        return false;
    };
    let EditorStage::Review { plan, .. } = &editor.stage else {
        return false;
    };
    let request = plan.request.clone();
    let result = match onboarding.apply_mutation(&root, request.clone(), &plan.plan_hash) {
        Ok(result) => result,
        Err(error) => {
            editor.error = Some(crate::presentation::output::error_line(&error));
            return false;
        }
    };
    let file = result
        .config_path
        .strip_prefix(&root)
        .unwrap_or(&result.config_path)
        .display()
        .to_string();
    let headline = format!(
        "{} {} '{}' in {file}.",
        match editor.action {
            ConfigMutationAction::Add => "Added",
            ConfigMutationAction::Edit => "Saved",
            ConfigMutationAction::Remove => "Removed",
        },
        editor.kind.noun(),
        request.name
    );
    let steps = editor.next_steps(&request, &file, remote.as_deref());
    if steps.is_empty() {
        let hint = match editor.kind {
            EditorKind::Binding => " Press u to restart the stack with it.",
            EditorKind::Schedule => " Press x to reconcile schedules.",
            _ => "",
        };
        if editor.kind == EditorKind::Binding {
            model.env.pending_restart = true;
        }
        model.editor = None;
        model.notice = Some(Notice::success(format!("{headline}{hint}")));
    } else {
        editor.error = None;
        editor.stage = EditorStage::Done { headline, steps };
    }
    true
}

#[cfg(test)]
#[path = "editor/tests.rs"]
mod tests;
