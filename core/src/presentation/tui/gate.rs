use crossterm::event::{KeyCode, KeyEvent};

use crate::domain::gate::{GateMutationPlan, GatePolicy};

#[derive(Debug, Clone)]
pub struct GateModel {
    pub policies: Vec<GatePolicy>,
    pub selected: usize,
    pub screen: GateScreen,
}

#[derive(Debug, Clone)]
pub enum GateScreen {
    List,
    Username(String),
    Review {
        policy: String,
        username: String,
        plan: Box<GateMutationPlan>,
    },
    Applying,
    Complete(String),
    Error(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateAction {
    None,
    PlanUser {
        policy: String,
        username: String,
    },
    ApplyUser {
        policy: String,
        username: String,
        plan_hash: String,
    },
}

impl GateModel {
    #[must_use]
    pub fn new(policies: Vec<GatePolicy>) -> Self {
        Self {
            policies,
            selected: 0,
            screen: GateScreen::List,
        }
    }
}

pub fn update(model: &mut GateModel, key: KeyEvent) -> Option<GateAction> {
    match &mut model.screen {
        GateScreen::List => match key.code {
            KeyCode::Char('a') => {
                if model.policies.is_empty() {
                    model.screen = GateScreen::Error(
                        "Create a gate policy before adding an application user.".to_owned(),
                    );
                } else {
                    model.screen = GateScreen::Username(String::new());
                }
                Some(GateAction::None)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                model.selected = model
                    .selected
                    .saturating_add(1)
                    .min(model.policies.len().saturating_sub(1));
                Some(GateAction::None)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                model.selected = model.selected.saturating_sub(1);
                Some(GateAction::None)
            }
            _ => None,
        },
        GateScreen::Username(username) => match key.code {
            KeyCode::Esc => {
                model.screen = GateScreen::Cancelled;
                Some(GateAction::None)
            }
            KeyCode::Backspace => {
                username.pop();
                Some(GateAction::None)
            }
            KeyCode::Enter if !username.trim().is_empty() => {
                let policy = model.policies.get(model.selected)?.name.as_str().to_owned();
                Some(GateAction::PlanUser {
                    policy,
                    username: username.clone(),
                })
            }
            KeyCode::Char(character) if !character.is_control() && username.len() < 128 => {
                username.push(character);
                Some(GateAction::None)
            }
            _ => Some(GateAction::None),
        },
        GateScreen::Review {
            policy,
            username,
            plan,
        } if key.code == KeyCode::Enter => Some(GateAction::ApplyUser {
            policy: policy.clone(),
            username: username.clone(),
            plan_hash: plan.plan_hash.clone(),
        }),
        GateScreen::Review { .. } if key.code == KeyCode::Esc => {
            model.screen = GateScreen::Cancelled;
            Some(GateAction::None)
        }
        GateScreen::Complete(_) | GateScreen::Error(_) | GateScreen::Cancelled
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) =>
        {
            model.screen = GateScreen::List;
            Some(GateAction::None)
        }
        GateScreen::Review { .. }
        | GateScreen::Applying
        | GateScreen::Complete(_)
        | GateScreen::Error(_)
        | GateScreen::Cancelled => Some(GateAction::None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    use crate::domain::gate::GateMutationAction;

    #[test]
    fn empty_gate_state_fails_closed_without_collecting_a_password() {
        let mut model = GateModel::new(Vec::new());
        let action = update(
            &mut model,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
        );
        assert_eq!(action, Some(GateAction::None));
        assert_screen(&model, GateScreen::Error(String::new()));
        assert!(!format!("{model:?}").contains("password="));
    }

    #[test]
    fn list_and_username_input_are_bounded_and_selection_safe() {
        let mut model = GateModel::new(policies(&["first", "second"]));
        assert_eq!(press(&mut model, KeyCode::F(1)), None);
        for code in [KeyCode::Down, KeyCode::Char('j'), KeyCode::Down] {
            assert_eq!(press(&mut model, code), Some(GateAction::None));
        }
        assert_eq!(model.selected, 1);
        for code in [KeyCode::Up, KeyCode::Char('k'), KeyCode::Up] {
            assert_eq!(press(&mut model, code), Some(GateAction::None));
        }
        assert_eq!(model.selected, 0);

        assert_eq!(
            press(&mut model, KeyCode::Char('a')),
            Some(GateAction::None)
        );
        assert!(matches!(model.screen, GateScreen::Username(ref value) if value.is_empty()));
        assert_eq!(press(&mut model, KeyCode::Enter), Some(GateAction::None));
        assert_eq!(
            press(&mut model, KeyCode::Char('\n')),
            Some(GateAction::None)
        );
        for character in "Alice".chars() {
            assert_eq!(
                press(&mut model, KeyCode::Char(character)),
                Some(GateAction::None)
            );
        }
        assert_eq!(
            press(&mut model, KeyCode::Backspace),
            Some(GateAction::None)
        );
        assert_eq!(
            press(&mut model, KeyCode::Enter),
            Some(GateAction::PlanUser {
                policy: "first".to_owned(),
                username: "Alic".to_owned(),
            })
        );

        model.screen = GateScreen::Username("x".repeat(128));
        assert_eq!(
            press(&mut model, KeyCode::Char('y')),
            Some(GateAction::None)
        );
        assert!(matches!(model.screen, GateScreen::Username(ref value) if value.len() == 128));
        model.selected = model.policies.len();
        assert_eq!(press(&mut model, KeyCode::Enter), None);
        assert_eq!(press(&mut model, KeyCode::Esc), Some(GateAction::None));
        assert_screen(&model, GateScreen::Cancelled);
    }

    #[test]
    fn review_terminal_and_applying_screens_emit_only_allowed_actions() {
        let mut model = GateModel::new(policies(&["private"]));
        let plan = GateMutationPlan {
            schema: 1,
            action: GateMutationAction::AddUser,
            policy: "private".to_owned(),
            project_id: model.policies[0].project_id,
            environment: "development".to_owned(),
            subject: "alice".to_owned(),
            policy_revision: Some(1),
            subject_revision: None,
            ingress_generation: None,
            requires_hidden_input: true,
            effects: vec!["Add gate user alice".to_owned()],
            plan_hash: "a".repeat(64),
        };
        model.screen = GateScreen::Review {
            policy: "private".to_owned(),
            username: "alice".to_owned(),
            plan: Box::new(plan),
        };
        assert_eq!(press(&mut model, KeyCode::F(1)), Some(GateAction::None));
        assert_eq!(
            press(&mut model, KeyCode::Enter),
            Some(GateAction::ApplyUser {
                policy: "private".to_owned(),
                username: "alice".to_owned(),
                plan_hash: "a".repeat(64),
            })
        );
        assert_eq!(press(&mut model, KeyCode::Esc), Some(GateAction::None));
        assert_screen(&model, GateScreen::Cancelled);
        assert_eq!(press(&mut model, KeyCode::F(1)), Some(GateAction::None));
        assert_eq!(press(&mut model, KeyCode::Enter), Some(GateAction::None));
        assert_screen(&model, GateScreen::List);

        for screen in [
            GateScreen::Complete("created".to_owned()),
            GateScreen::Error("failed".to_owned()),
        ] {
            model.screen = screen;
            assert_eq!(press(&mut model, KeyCode::Esc), Some(GateAction::None));
            assert_screen(&model, GateScreen::List);
        }
        model.screen = GateScreen::Applying;
        assert_eq!(press(&mut model, KeyCode::Enter), Some(GateAction::None));
        assert_screen(&model, GateScreen::Applying);
    }

    fn press(model: &mut GateModel, code: KeyCode) -> Option<GateAction> {
        update(model, KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn assert_screen(model: &GateModel, expected: GateScreen) {
        assert_eq!(
            std::mem::discriminant(&model.screen),
            std::mem::discriminant(&expected)
        );
    }

    fn policies(names: &[&str]) -> Vec<GatePolicy> {
        let policies = names
            .iter()
            .filter_map(|name| {
                serde_json::from_value(serde_json::json!({
                    "id": "00000000-0000-0000-0000-000000000001",
                    "name": name,
                    "project_id": "00000000-0000-0000-0000-000000000002",
                    "environment": "development",
                    "mode": "any",
                    "drivers": ["password-cap"],
                    "session_ttl_seconds": 60,
                    "secret_provider": "age",
                    "password_pepper": "age://pepper",
                    "session_keys": {
                        "current_version": 1,
                        "current": "age://session"
                    },
                    "cap_site_key": "site",
                    "cap_site_secret": "age://cap",
                    "revision": 1,
                    "created_at_unix_ms": 0,
                    "updated_at_unix_ms": 0
                }))
                .ok()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            policies.len(),
            names.len(),
            "gate policy fixtures are valid"
        );
        policies
    }
}
