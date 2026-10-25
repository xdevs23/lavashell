// SPDX-License-Identifier: GPL-3.0-only
use super::pointers::PointerSample;
use super::{Activation, FocusSource, InputEvent, Key, NodeKind, Point, PointerId, Shortcut, View};
use crate::input_method::TextAction;
use crate::ui::Lines;

impl InputEvent {
    fn focus_source(&self) -> Option<FocusSource> {
        match self {
            Self::Key { .. } | Self::Shortcut(_) | Self::Text(_) => Some(FocusSource::Key),
            Self::Press(_) | Self::TouchDown(_) | Self::Context(_) => Some(FocusSource::Pointer),
            Self::Leave
            | Self::CancelPointer
            | Self::TouchCancel(_)
            | Self::WindowFocus(_)
            | Self::KeyRelease(_)
            | Self::Modifiers(_)
            | Self::Move(_)
            | Self::TouchMove(_)
            | Self::Release(_)
            | Self::TouchUp(_)
            | Self::Scroll { .. } => None,
        }
    }

    fn shortcut(&self) -> Option<Shortcut> {
        match self {
            Self::Shortcut(shortcut) => Some(*shortcut),
            Self::Key { key, shift } => Some(Shortcut {
                shift: *shift,
                ..Shortcut::key(*key)
            }),
            Self::Text(text) => {
                let mut characters = text.chars();
                let character = characters.next()?;
                characters
                    .next()
                    .is_none()
                    .then(|| Shortcut::key(Key::Character(character)))
            }
            _ => None,
        }
    }
}

/// The focused node when the node edits or selects text with the keyboard: a text field, or a text that can be
/// selected.
#[derive(Clone, Copy)]
pub(super) struct FocusedText {
    pub index: usize,
    /// Whether Enter inserts a line break in the node and the up and down keys move the caret: true for a field of
    /// several lines and for a text.
    pub takes_line_breaks: bool,
}

impl View {
    pub fn input(&mut self, input: InputEvent) {
        self.handle(input, None);
        self.finish_input();
    }

    /// Passes `input` to the first part of the view that consumes it: an interaction, a move of selected text, the
    /// focused text, the keys and shortcuts, and then the pointer and focus handling. An Enter key in the focused
    /// field runs `action` when one is given, in place of the action of the field.
    pub(super) fn handle(&mut self, mut input: InputEvent, action: Option<TextAction>) {
        self.sanitize_interactions();
        if self.interaction_input(&input) {
            return;
        }
        if let Some(source) = input.focus_source() {
            self.record_focus_source(source);
        }
        if self.key_consumed_by_text_drag(&input) {
            return;
        }
        if self.needs_accessibility_update() {
            self.refresh_accessibility();
        }
        let focused = self.focused_text();
        if let Some(focused) = focused
            && (self.protected_input(&mut input) || self.edit_input(&input, focused))
        {
            return;
        }
        if self.key_input(&input, focused, action) {
            return;
        }
        self.pointer_and_focus_input(&input);
        self.sanitize_interactions();
    }

    pub(super) fn focused_text(&self) -> Option<FocusedText> {
        let index = (self.hits.iter())
            .filter(|hit| Some(hit.id) == self.focus)
            .find_map(|hit| hit.input)?;
        let takes_line_breaks = match &self.nodes[index].kind {
            NodeKind::TextInput { content, .. } => content.lines() == Lines::Several,
            NodeKind::Text(..) => true,
            _ => false,
        };
        Some(FocusedText {
            index,
            takes_line_breaks,
        })
    }

    /// Cancels every move of selected text on a key or on typed text. Returns true only for an Escape that cancelled
    /// a move, because that Escape needs no further handling.
    fn key_consumed_by_text_drag(&mut self, input: &InputEvent) -> bool {
        if !matches!(
            input,
            InputEvent::Key { .. } | InputEvent::Shortcut(_) | InputEvent::Text(_)
        ) {
            return false;
        }
        let escape = input
            .shortcut()
            .is_some_and(|shortcut| shortcut.key == Key::Escape);
        let dragging = self.text_drag_in_progress();
        self.cancel_text_drag();
        dragging && escape
    }

    fn edit_input(&mut self, input: &InputEvent, focused: FocusedText) -> bool {
        match input {
            InputEvent::Text(text) => {
                let text = self.normalize_for_field(focused.index, text);
                self.edit(|edit, value| edit.replace(value, &text));
            }
            InputEvent::Key {
                key: Key::Space, ..
            } => self.edit(|edit, value| edit.replace(value, " ")),
            InputEvent::Key { key, shift }
                if matches!(
                    key,
                    Key::Left | Key::Right | Key::Home | Key::End | Key::Backspace | Key::Delete
                ) || focused.takes_line_breaks && matches!(key, Key::Up | Key::Down) =>
            {
                let id = self.nodes[focused.index].id;
                let geometry = (self.text_positions.get(&id))
                    .filter(|p| self.text_value(focused.index).as_deref() == Some(p.value.as_str()))
                    .map(|p| p.geometry.clone());
                self.edit(|edit, value| match &geometry {
                    Some(geometry) => edit.shaped_key(value, *key, *shift, geometry),
                    None => edit.key(value, *key, *shift),
                });
            }
            InputEvent::Key {
                key: Key::Enter,
                shift: true,
            } if focused.takes_line_breaks => self.edit(|edit, value| edit.replace(value, "\n")),
            InputEvent::Shortcut(Shortcut {
                key: key @ (Key::Home | Key::End),
                control: true,
                shift,
                ..
            }) => {
                let end = *key == Key::End;
                self.edit(|edit, value| edit.move_to(if end { value.len() } else { 0 }, *shift));
            }
            InputEvent::Shortcut(Shortcut {
                key: Key::Character('a'),
                control: true,
                ..
            }) => self.edit(|edit, value| edit.select_all(value)),
            _ => return false,
        }
        true
    }

    /// Passes a key to the focused node, then to the focused text field, then to the shortcuts of the view, and returns
    /// whether one of them consumed the key.
    fn key_input(
        &mut self,
        input: &InputEvent,
        focused: Option<FocusedText>,
        action: Option<TextAction>,
    ) -> bool {
        let shortcut = input.shortcut();
        if let Some(shortcut) = shortcut
            && self.focused_key(shortcut)
        {
            self.redraw = true;
            return true;
        }
        if let Some(focused) = focused
            && matches!(
                input,
                InputEvent::Key {
                    key: Key::Enter,
                    ..
                }
            )
        {
            let field_action = || {
                (!focused.takes_line_breaks)
                    .then(|| self.focused_hints().and_then(|hints| hints.action))
                    .flatten()
            };
            if let Some(action) = action.or_else(field_action) {
                self.finish_text_action(action);
                return true;
            }
            if focused.takes_line_breaks {
                self.edit(|edit, value| edit.replace(value, "\n"));
                return true;
            }
        }
        if let Some(shortcut) = shortcut
            && (focused.is_none() || shortcut.key == Key::Escape)
            && let Some((_, action)) =
                (self.shortcuts.iter().rev()).find(|(bound, _)| *bound == shortcut)
        {
            action.borrow_mut()();
            self.redraw = true;
            return true;
        }
        false
    }

    fn pointer_and_focus_input(&mut self, input: &InputEvent) {
        match *input {
            InputEvent::Modifiers(modifiers) => self.modifiers = modifiers,
            InputEvent::Context(p) => {
                let hit = self.hit_at(p).cloned();
                if let Some(hit) = hit {
                    if hit.focusable {
                        self.focus = Some(hit.id);
                    }
                    if let Some(action) = hit.context {
                        action.borrow_mut()(Activation {
                            position: p,
                            modifiers: self.modifiers,
                        });
                    }
                }
                self.redraw = true;
            }
            InputEvent::Leave => {
                self.cancel_text_drag();
                if let Some(primary) = self.pointers.get_mut(&PointerId::Primary) {
                    primary.over = None;
                }
                self.redraw = true;
            }
            InputEvent::Move(p) => self.move_pointer(PointerSample::Primary(p)),
            InputEvent::TouchMove(sample) => self.move_pointer(PointerSample::Touch(sample)),
            InputEvent::Press(p) => self.press(PointerSample::Primary(p)),
            InputEvent::TouchDown(sample) => self.press(PointerSample::Touch(sample)),
            InputEvent::Release(p) => self.release(PointerSample::Primary(p)),
            InputEvent::TouchUp(sample) => self.release(PointerSample::Touch(sample)),
            InputEvent::Scroll { at, delta } => self.wheel(at, delta),
            InputEvent::Key {
                key: Key::Tab,
                shift,
            } => self.cycle_focus(shift, |_, _| true),
            InputEvent::Key {
                key: key @ (Key::Enter | Key::Space),
                ..
            } => {
                if self.begin_keyboard_activation(key)
                    && let Some(focus) = self.focus
                {
                    self.activate_hit(focus, Point::default());
                }
            }
            _ => {}
        }
    }

    pub(super) fn focused_key(&mut self, shortcut: Shortcut) -> bool {
        let Some(mut index) = self.nodes.iter().position(|n| Some(n.id) == self.focus) else {
            return false;
        };
        loop {
            if self.input_disabled(index) {
                return false;
            }
            if let Some(action) = self.nodes[index].key_action.clone()
                && action.borrow_mut()(shortcut)
            {
                return true;
            }
            if Some(self.nodes[index].id) == self.focus
                && matches!(shortcut.key, Key::Enter | Key::Space)
                && !shortcut.control
                && !shortcut.alt
                && (self.hits.iter()).any(|hit| Some(hit.id) == self.focus && hit.activates())
            {
                return false;
            }
            if self.scroll_key(index, shortcut) {
                return true;
            }
            let Some(parent) = self.parents.node_of(index) else {
                return false;
            };
            index = parent;
        }
    }
}
