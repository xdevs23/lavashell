// SPDX-License-Identifier: GPL-3.0-only
use super::{InputEvent, Key, NodeId, NodeKind, View};
use crate::input_method::{
    FieldPresses, FieldText, SessionId, TextAction, TextCommand, TextHints, TextSession,
};
use crate::ui::{FieldContent, Lines};

/// The text session of the input method: the field of the last session, the id of that session, and the number of
/// presses on that field during the session.
#[derive(Default)]
pub(super) struct Sessions {
    field: Option<NodeId>,
    last: SessionId,
    presses: FieldPresses,
}

impl Sessions {
    pub(super) fn press(&mut self, node: NodeId) {
        if self.field == Some(node) {
            self.presses.0 += 1;
        }
    }
}

impl View {
    /// Applies `command` from an input method to the focused text field as keyboard input. A host that shows an input
    /// method outside the view, such as an on-screen keyboard, passes the edits made in that input method here. The
    /// view ignores a command while no editable text field has focus.
    pub fn apply_text(&mut self, command: TextCommand) {
        self.apply(command);
        self.finish_input();
    }

    /// Applies the text commands sent through the text sender of the view, then sets the text session of the focused
    /// field on that sender.
    pub(super) fn finish_input(&mut self) {
        self.apply_sent_text();
        self.refresh_text_session();
    }

    fn apply_sent_text(&mut self) {
        while let Some(command) = self.text_sender.receive() {
            self.apply(command);
        }
    }

    fn apply(&mut self, command: TextCommand) {
        if self.session_field().is_none() {
            return;
        }
        let key = |key| InputEvent::Key { key, shift: false };
        match command {
            TextCommand::Commit(text) => self.handle(InputEvent::Text(text), None),
            TextCommand::DeleteBackward => self.handle(key(Key::Backspace), None),
            TextCommand::Key(code) => self.handle(key(code), None),
            TextCommand::Action(action) => self.handle(key(Key::Enter), Some(action)),
        }
    }

    /// The text session of the focused field as set after the last input or frame, or `None` when no editable text
    /// field has focus.
    #[must_use]
    pub fn text_session(&self) -> Option<TextSession> {
        self.text_sender.last_session()
    }

    /// Starts a new text session when another field gains focus, and sets the session of the focused field on the
    /// text sender. The new session reuses the text before the cursor from the last session when that text did not
    /// change, and copies the text otherwise.
    pub(super) fn refresh_text_session(&mut self) {
        let field = self.session_field();
        let id = field.map(|index| self.nodes[index].id);
        if id != self.sessions.field {
            self.sessions.field = id;
            if id.is_some() {
                self.sessions.last = self.sessions.last.next();
            }
            self.sessions.presses = FieldPresses::default();
        }
        let session = field.and_then(|index| self.session_at(index));
        self.text_sender.set_session(session);
    }

    fn session_at(&self, index: usize) -> Option<TextSession> {
        let node = &self.nodes[index];
        let NodeKind::TextInput { content, hints, .. } = &node.kind else {
            return None;
        };
        let text = match content {
            FieldContent::Protected(_) => FieldText::Protected,
            FieldContent::Plain { value, .. } => {
                let before = (self.edits.get(&node.id))
                    .map_or(value.as_str(), |edit| edit.before_selection(value));
                match self.text_sender.last_session().map(|session| session.text) {
                    Some(FieldText::BeforeCursor(last)) if *last == *before => {
                        FieldText::BeforeCursor(last)
                    }
                    _ => FieldText::BeforeCursor(before.into()),
                }
            }
        };
        Some(TextSession {
            id: self.sessions.last,
            presses: self.sessions.presses,
            hints: *hints,
            multiline: content.lines() == Lines::Several,
            text,
        })
    }

    /// The index of the focused text field when that field can be edited, protected fields included.
    pub(super) fn session_field(&self) -> Option<usize> {
        let id = self.selection_target()?;
        let index = self.nodes.iter().position(|node| node.id == id)?;
        (matches!(self.nodes[index].kind, NodeKind::TextInput { .. })
            && !self.accessibility_semantics(index).state.form.read_only)
            .then_some(index)
    }

    pub(super) fn focused_hints(&self) -> Option<TextHints> {
        match &self.nodes[self.session_field()?].kind {
            NodeKind::TextInput { hints, .. } => Some(*hints),
            _ => None,
        }
    }

    /// Runs the effect of `action` in the focused field after no key handler consumed the Enter key: `Next` moves the
    /// focus to the next text field, and `Done` removes the focus. The other actions have no effect here.
    pub(super) fn finish_text_action(&mut self, action: TextAction) {
        match action {
            TextAction::Next => self.cycle_focus(false, |view, hit| {
                (hit.input).is_some_and(|index| {
                    matches!(view.nodes[index].kind, NodeKind::TextInput { .. })
                })
            }),
            TextAction::Done => {
                self.focus = None;
                self.redraw = true;
            }
            TextAction::Search | TextAction::Go | TextAction::Send => {}
        }
    }
}
