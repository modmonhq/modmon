//! Text entry for the Environment view: the search box and the override editor.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::presentation::tui::model::Model;
use crate::presentation::tui::update::Action;

/// Longest override value accepted from the editor.
const MAXIMUM_VALUE_CHARS: usize = 8 * 1024;

/// Handles a key while a text field is focused; `None` lets shortcuts handle it.
pub(super) fn typing_key(model: &mut Model, key: KeyEvent) -> Option<Action> {
    if model.env.edit.is_some() {
        return Some(edit_key(model, key));
    }
    if model.env.searching {
        return Some(search_key(model, key));
    }
    None
}

fn edit_key(model: &mut Model, key: KeyEvent) -> Action {
    let Some(edit) = model.env.edit.as_mut() else {
        return Action::None;
    };
    match key.code {
        KeyCode::Esc => model.env.edit = None,
        KeyCode::Enter => {
            if let Some(edit) = model.env.edit.take() {
                return Action::SetOverride {
                    key: edit.key,
                    value: edit.value,
                };
            }
        }
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::ALT) => {
            delete_word(&mut edit.value);
        }
        KeyCode::Backspace => {
            edit.value.pop();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => edit.value.clear(),
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_word(&mut edit.value);
        }
        KeyCode::Char(character)
            if !character.is_control()
                && !key.modifiers.contains(KeyModifiers::CONTROL)
                && edit.value.chars().count() < MAXIMUM_VALUE_CHARS =>
        {
            edit.value.push(character);
        }
        _ => {}
    }
    Action::None
}

fn search_key(model: &mut Model, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => model.refilter(|env| {
            env.searching = false;
            env.filter.clear();
        }),
        KeyCode::Enter => model.env.searching = false,
        KeyCode::Down => move_selection(model, 1),
        KeyCode::Up => move_selection(model, -1),
        KeyCode::Backspace => model.refilter(|env| {
            env.filter.pop();
        }),
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            model.refilter(|env| env.filter.clear());
        }
        KeyCode::Char(character)
            if !character.is_control() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            model.refilter(|env| env.filter.push(character));
        }
        _ => {}
    }
    Action::None
}

fn move_selection(model: &mut Model, delta: isize) {
    model.scroll = model.scroll.saturating_add_signed(delta);
    model.clamp_selection();
}

fn delete_word(value: &mut String) {
    let trimmed = value.trim_end().len();
    value.truncate(trimmed);
    let start = value
        .char_indices()
        .rev()
        .find(|(_, character)| character.is_whitespace())
        .map_or(0, |(index, character)| index + character.len_utf8());
    value.truncate(start);
}
