// SPDX-License-Identifier: GPL-3.0-only
use super::{InputEvent, View};
use crate::keyboard::Key;
use crate::text_edit::TextEdit;
use crate::ui::{FieldContent, NodeKind};
use zeroize::Zeroize;

enum ProtectedEdit {
    DeleteBackward,
    DeleteForward,
    Insert,
}

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
            .routing
            .hits
            .iter()
            .find(|hit| Some(hit.id) == self.routing.focus && hit.input.is_some())
            .and_then(|hit| hit.input.map(|target| target.index))
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
        let requested = match input {
            InputEvent::Key {
                key: Key::Backspace,
                ..
            } => ProtectedEdit::DeleteBackward,
            InputEvent::Key {
                key: Key::Delete, ..
            } => ProtectedEdit::DeleteForward,
            InputEvent::Text(_)
            | InputEvent::Key {
                key: Key::Space, ..
            } => ProtectedEdit::Insert,
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
            .editing
            .edits
            .entry(self.nodes[index].id)
            .or_insert_with(|| TextEdit::at_end(&value));
        edit.clamp(&value);
        let selected = edit.selection();
        let mut range = secret.char_at(selected.start)..secret.char_at(selected.end);
        if range.is_empty() {
            match requested {
                ProtectedEdit::DeleteBackward => range.start = range.start.saturating_sub(1),
                ProtectedEdit::DeleteForward => {
                    range.end = (range.end + 1).min(secret.char_at(value.len()));
                }
                ProtectedEdit::Insert => {}
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
