// SPDX-License-Identifier: GPL-3.0-only
use super::component::index_of;
use super::navigation::FocusDirection;
use super::{InputEvent, View};
use crate::input_method::{
    FieldPresses, FieldText, SessionId, TextAction, TextCommand, TextHints, TextSession,
};
use crate::keyboard::Key;
use crate::ui::{FieldContent, Lines, NodeId, NodeKind};

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
    pub fn apply_text(&mut self, command: TextCommand) {
        self.apply(command);
        self.finish_input();
    }

    pub(super) fn finish_input(&mut self) {
        self.apply_sent_text();
        self.refresh_text_session();
    }

    fn apply_sent_text(&mut self) {
        while let Some(command) = self.editing.sender.receive() {
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

    #[must_use]
    pub fn text_session(&self) -> Option<TextSession> {
        self.editing.sender.last_session()
    }

    pub(super) fn refresh_text_session(&mut self) {
        let field = self.session_field();
        let id = field.map(|index| self.nodes[index].id);
        if id != self.editing.sessions.field {
            self.editing.sessions.field = id;
            if id.is_some() {
                self.editing.sessions.last = self.editing.sessions.last.next();
            }
            self.editing.sessions.presses = FieldPresses::default();
        }
        let session = field.and_then(|index| self.session_at(index));
        self.editing.sender.set_session(session);
    }

    fn session_at(&self, index: usize) -> Option<TextSession> {
        let node = &self.nodes[index];
        let NodeKind::TextInput { content, hints, .. } = &node.kind else {
            return None;
        };
        let text = match content {
            FieldContent::Protected(_) => FieldText::Protected,
            FieldContent::Plain { value, .. } => {
                let before = (self.editing.edits.get(&node.id))
                    .map_or(value.as_str(), |edit| edit.before_selection(value));
                match self
                    .editing
                    .sender
                    .last_session()
                    .map(|session| session.text)
                {
                    Some(FieldText::BeforeCursor(last)) if *last == *before => {
                        FieldText::BeforeCursor(last)
                    }
                    _ => FieldText::BeforeCursor(before.into()),
                }
            }
        };
        Some(TextSession {
            id: self.editing.sessions.last,
            presses: self.editing.sessions.presses,
            hints: *hints,
            multiline: content.lines() == Lines::Several,
            text,
        })
    }

    pub(super) fn session_field(&self) -> Option<usize> {
        let id = self.routing.selection_target()?;
        let index = index_of(&self.nodes, id)?;
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

    pub(super) fn finish_text_action(&mut self, action: TextAction) {
        match action {
            TextAction::Next => self.cycle_focus(FocusDirection::Forward, |view, hit| {
                (hit.input).is_some_and(|target| {
                    matches!(view.nodes[target.index].kind, NodeKind::TextInput { .. })
                })
            }),
            TextAction::Done => {
                self.routing.focus = None;
                self.redraw = true;
            }
            TextAction::Search | TextAction::Go | TextAction::Send => {}
        }
    }
}
