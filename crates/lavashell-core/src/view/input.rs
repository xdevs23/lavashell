// SPDX-License-Identifier: GPL-3.0-only
use super::focus::{FocusSource, KeyboardFocus};
use super::moving::Moving;
use super::navigation::FocusDirection;
use super::pointers::Pointer;
use super::pointers::PointerSample;
use super::{Hit, InputEvent, PaintedHit, View, WindowRegionHit};
use crate::geometry::Point;
use crate::input_method::TextAction;
use crate::keyboard::{Key, Shortcut};
use crate::ui::{
    Action, ComposedShortcut, ContextRequest, InputModifiers, Lines, NodeId, NodeKind, PointerId,
};
use std::collections::BTreeMap;

pub fn press_and_release_at(view: &mut View, at: Point) {
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
}

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

#[derive(Default)]
pub(super) struct InputRouting {
    pub pointers: BTreeMap<PointerId, Pointer>,
    pub modifiers: InputModifiers,
    pub focus: KeyboardFocus,
    pub modal_root: Option<NodeId>,
    pub shortcuts: Vec<(Shortcut, Action)>,
    pub root_shortcuts: Vec<ComposedShortcut>,
    pub hits: Vec<Hit>,
    pub window_regions: Vec<WindowRegionHit>,
    pub painted_hits: Vec<PaintedHit>,
    pub moving: Moving,
}

#[derive(Clone, Copy)]
pub(super) struct FocusedText {
    pub index: usize,
    pub takes_line_breaks: bool,
}

impl View {
    pub fn input(&mut self, input: InputEvent) {
        self.handle(input, None);
        self.finish_input();
    }

    pub(super) fn handle(&mut self, mut input: InputEvent, action: Option<TextAction>) {
        self.sanitize_interactions();
        if self.interaction_input(&input) {
            return;
        }
        if let Some(source) = input.focus_source() {
            self.redraw |= self.routing.focus.record_source(source);
        }
        if self.key_consumed_by_text_drag(&input) {
            return;
        }
        self.refresh_stale_accessibility();
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
        let index = (self.routing.hits.iter())
            .filter(|hit| Some(hit.id) == self.routing.focus.node())
            .find_map(|hit| hit.input)?
            .index;
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
        let dragging = self.editing.text_drag_in_progress();
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
                let geometry = (self.editing.positions.get(&id))
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
                (self.routing.shortcuts.iter().rev()).find(|(bound, _)| *bound == shortcut)
        {
            action.borrow_mut()();
            self.redraw = true;
            return true;
        }
        false
    }

    fn pointer_and_focus_input(&mut self, input: &InputEvent) {
        match *input {
            InputEvent::Modifiers(modifiers) => self.routing.modifiers = modifiers,
            InputEvent::Context(p) => {
                let hit = self.routing.hit_at(p).cloned();
                if let Some(hit) = hit {
                    if hit.focusable {
                        self.routing.focus.move_to(Some(hit.id));
                    }
                    if let Some(action) = hit.gestures.context() {
                        action.borrow_mut()(ContextRequest {
                            position: p,
                            modifiers: self.routing.modifiers,
                        });
                    }
                }
                self.redraw = true;
            }
            InputEvent::Leave => {
                self.cancel_text_drag();
                if let Some(primary) = self.routing.pointers.get_mut(&PointerId::Primary) {
                    primary.hover = None;
                }
                self.redraw = true;
            }
            InputEvent::Move(p) => self.move_pointer(PointerSample::Primary(p)),
            InputEvent::TouchMove(moved) => self.move_pointer(PointerSample::Touch(moved.sample)),
            InputEvent::Press(p) => self.press(PointerSample::Primary(p)),
            InputEvent::TouchDown(sample) => self.press(PointerSample::Touch(sample)),
            InputEvent::Release(p) => self.release(PointerSample::Primary(p)),
            InputEvent::TouchUp(sample) => self.release(PointerSample::Touch(sample)),
            InputEvent::Scroll { at, delta } => self.wheel(at, delta),
            InputEvent::Key {
                key: Key::Tab,
                shift,
            } => {
                let direction = if shift {
                    FocusDirection::Backward
                } else {
                    FocusDirection::Forward
                };
                self.cycle_focus(direction, |_, _| true);
            }
            InputEvent::Key {
                key: key @ (Key::Enter | Key::Space),
                ..
            } => {
                if self.begin_keyboard_activation(key)
                    && let Some(focus) = self.routing.focus.node()
                {
                    self.activate_node(focus, None);
                }
            }
            _ => {}
        }
    }

    pub(super) fn focused_key(&mut self, shortcut: Shortcut) -> bool {
        let Some(mut index) = self
            .nodes
            .iter()
            .position(|n| Some(n.id) == self.routing.focus.node())
        else {
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
            if Some(self.nodes[index].id) == self.routing.focus.node()
                && matches!(shortcut.key, Key::Enter | Key::Space)
                && !shortcut.control
                && !shortcut.alt
                && self.routing.node_activates(self.nodes[index].id)
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
