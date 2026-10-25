// SPDX-License-Identifier: GPL-3.0-only
use super::{InputEvent, Key, NodeKind, TextEdit, View};
use crate::ui::FieldContent;
use zeroize::Zeroize;

impl View {
    pub(super) fn protected_index(&self, index: usize) -> bool {
        matches!(
            self.nodes[index].kind,
            NodeKind::TextInput {
                content: FieldContent::Protected(_),
                ..
            }
        )
    }

    pub(super) fn protected_input(&mut self, input: &mut InputEvent) -> bool {
        let Some(index) = self
            .hits
            .iter()
            .find(|hit| Some(hit.id) == self.focus && hit.input.is_some())
            .and_then(|hit| hit.input)
        else {
            return false;
        };
        let NodeKind::TextInput {
            content: FieldContent::Protected(secret),
            ..
        } = &self.nodes[index].kind
        else {
            return false;
        };
        let deletion = match input {
            InputEvent::Key {
                key: Key::Backspace,
                ..
            } => Some(true),
            InputEvent::Key {
                key: Key::Delete, ..
            } => Some(false),
            InputEvent::Text(_)
            | InputEvent::Key {
                key: Key::Space, ..
            } => None,
            _ => return false,
        };
        if self.accessibility_semantics(index).state.form.read_only {
            if let InputEvent::Text(value) = input {
                value.zeroize();
            }
            return true;
        }
        let secret = secret.clone();
        let value = secret.masked();
        let edit = self
            .edits
            .entry(self.nodes[index].id)
            .or_insert_with(|| TextEdit::at_end(&value));
        edit.clamp(&value);
        let selected = edit.selection();
        let mut range = secret.char_at(selected.start)..secret.char_at(selected.end);
        if range.is_empty()
            && let Some(backward) = deletion
        {
            if backward {
                range.start = range.start.saturating_sub(1);
            } else {
                range.end = (range.end + 1).min(secret.char_at(value.len()));
            }
        }
        let inserted = match input {
            InputEvent::Text(value) => value.as_str(),
            InputEvent::Key {
                key: Key::Space, ..
            } => " ",
            _ => "",
        };
        let count = secret.text.replace(range.clone(), inserted);
        if let InputEvent::Text(value) = input {
            value.zeroize();
        }
        edit.move_to(secret.offset_of(range.start + count), false);
        self.redraw = true;
        true
    }
}
