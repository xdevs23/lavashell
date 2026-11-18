// SPDX-License-Identifier: GPL-3.0-only
use super::component::index_of;
use super::focus::FocusSource;
use super::paint::PaintOutput;
use super::{InputEvent, View, paint};
use crate::animation::{Animation, AnimationError};
use crate::geometry::{Point, Rect};
use crate::keyboard::Key;
use crate::scene::{DrawCommand, MaterialCommands, Transform};
use crate::shape::{RoundedRect, Shape};
use crate::style::{AnimatedAppearance, FocusState, InteractionState, PointerState, StateStyles};
use crate::ui::{Activation, NodeId, NodeKind, PointerId, ViewTag};
use std::collections::BTreeSet;
use std::collections::btree_map::Entry;

pub(super) struct StyledAppearance {
    styles: StateStyles,
    state: InteractionState,
    pub animation: Animation<AnimatedAppearance>,
}

impl View {
    pub fn cancel_pending_text_drag(&mut self) {
        self.cancel_text_drag();
    }

    #[must_use]
    pub fn interaction_state<T: ViewTag>(&self, tag: &T) -> Option<InteractionState> {
        (self.index_tagged(tag)).map(|index| self.interaction_state_at(index))
    }

    fn interaction_state_at(&self, index: usize) -> InteractionState {
        let node = &self.nodes[index];
        let id = node.id;
        let disabled = self.input_disabled(index);
        let available = !disabled && self.window_focused;
        let focused = available && self.routing.focus.node() == Some(id);
        InteractionState {
            pointer: PointerState {
                hovered: !disabled && self.routing.is_hovered(id),
                active: !disabled
                    && (self.routing.is_pressed(id)
                        || focused && self.routing.focus.pressed_key().is_some()),
                captured: !disabled && self.routing.is_captured(id),
            },
            focus: FocusState {
                focused,
                visible: focused && self.focus_shows_on(index),
            },
            disabled,
            selected: node.selected == Some(true),
        }
    }

    pub(super) fn interaction_visual(
        &mut self,
        index: usize,
        inherited: paint::Inherited,
    ) -> Option<AnimatedAppearance> {
        match inherited.source {
            paint::Source::Composition => {
                self.live_interaction_visual(index, inherited.composed_parent)
            }
            paint::Source::Leaving => {
                (self.painting.interaction_visuals.get(&self.nodes[index].id))
                    .map(|styled| styled.animation.get())
            }
        }
    }

    fn live_interaction_visual(
        &mut self,
        index: usize,
        composed_parent: usize,
    ) -> Option<AnimatedAppearance> {
        let node = &self.nodes[index];
        let state = self.interaction_state_at(index);
        if let Some(observer) = &node.interaction_observer
            && observer.peek() != state
        {
            observer.set(state);
        }
        let interactive = node.handles_input()
            || matches!(node.kind, NodeKind::TextInput { .. })
            || self.text_selectable(index);
        let styles = node
            .interaction
            .states
            .clone()
            .filter(|_| interactive || node.selected.is_some());
        let Some(styles) = styles else {
            let _visuals_of_a_node_without_styles =
                self.painting.interaction_visuals.remove(&node.id);
            return None;
        };
        match self.styled_appearance(node.id, state, styles) {
            Ok(appearance) => Some(appearance),
            Err(error) => {
                self.failures.fail_paint(composed_parent, error);
                None
            }
        }
    }

    fn styled_appearance(
        &mut self,
        id: NodeId,
        state: InteractionState,
        styles: StateStyles,
    ) -> Result<AnimatedAppearance, AnimationError> {
        let visual = match self.painting.interaction_visuals.entry(id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let state = InteractionState {
                    selected: false,
                    ..state
                };
                entry.insert(StyledAppearance {
                    animation: self.motion.animation(styles.appearance(state))?,
                    styles: styles.clone(),
                    state,
                })
            }
        };
        if visual.state != state || visual.styles != styles {
            (visual.animation).animate_to(styles.appearance(state), styles.motion)?;
            visual.state = state;
            visual.styles = styles;
        }
        Ok(visual.animation.get())
    }

    pub(super) fn state_material(
        rect: Rect,
        appearance: Option<&AnimatedAppearance>,
        stretch: Transform,
    ) -> Option<MaterialCommands> {
        let material = appearance?.material.as_ref()?;
        Some(paint::material::commands(rect, material.value(), stretch))
    }

    pub(super) fn painted_bounds(&self, index: usize) -> Rect {
        let bounds = self.layout[index].rect;
        match self.painting.interaction_visuals.get(&self.nodes[index].id) {
            Some(styled) => {
                let scale = styled.animation.get().scale.get();
                paint::scale::about(bounds, scale).map_or(bounds, |scaled| scaled.rect(bounds))
            }
            None => bounds,
        }
    }

    pub(super) fn sanitize_interactions(&mut self) {
        let enabled = self.input_enabled_nodes();
        let focus_disabled = (self.routing.focus.node()).is_some_and(|id| !enabled.contains(&id));
        let invalid: BTreeSet<_> = (self.routing.pointers.values())
            .flat_map(|pointer| pointer.nodes().chain(pointer.over()))
            .filter(|id| !enabled.contains(id))
            .collect();
        if focus_disabled {
            self.routing.focus.set_pressed_key(None);
        }
        if (self.routing.pointers.values())
            .any(|pointer| pointer.nodes().any(|id| invalid.contains(&id)))
        {
            self.end_captures(|id| invalid.contains(&id));
            self.cancel_text_drag();
        }
        for hover in (self.routing.pointers.values_mut()).filter_map(|state| state.hover.as_mut()) {
            hover.over = hover.over.filter(|id| !invalid.contains(id));
        }
    }

    pub(super) fn interaction_input(&mut self, input: &InputEvent) -> bool {
        match input {
            InputEvent::CancelPointer => {
                self.cancel_pointer(PointerId::Primary);
                true
            }
            InputEvent::TouchCancel(touch) => {
                self.cancel_pointer(PointerId::Touch(*touch));
                true
            }
            InputEvent::WindowFocus(focused) => {
                if self.window_focused == *focused {
                    return true;
                }
                self.window_focused = *focused;
                if !focused {
                    self.cancel_pointers();
                    self.routing.focus.set_pressed_key(None);
                    self.cancel_text_drag();
                }
                self.redraw = true;
                true
            }
            InputEvent::KeyRelease(key) => {
                if self.routing.focus.pressed_key() == Some(*key) {
                    self.routing.focus.set_pressed_key(None);
                    self.redraw = true;
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn move_focus_with_source(&mut self, node: NodeId, source: FocusSource) {
        self.redraw |= self.routing.focus.move_with_source(node, source);
    }

    pub(super) fn focus_shows_on(&self, index: usize) -> bool {
        matches!(
            self.routing.focus.source(),
            None | Some(FocusSource::Key | FocusSource::Assistive)
        ) || matches!(self.nodes[index].kind, NodeKind::TextInput { .. })
    }

    pub(super) fn activate_node(&mut self, node: NodeId, at: Option<Point>) {
        let activate = index_of(&self.nodes, node)
            .and_then(|index| self.nodes[index].gestures.activate().cloned());
        if let Some(activate) = activate {
            activate.borrow_mut()(Activation {
                position: at,
                modifiers: self.routing.modifiers,
            });
            self.redraw = true;
        }
    }

    pub(super) fn begin_keyboard_activation(&mut self, key: Key) -> bool {
        let Some(id) = self.routing.focus.node().filter(|_| self.window_focused) else {
            return false;
        };
        if self.routing.focus.pressed_key() == Some(key) {
            return false;
        }
        if !self.routing.node_activates(id) {
            return false;
        }
        self.routing.focus.set_pressed_key(Some(key));
        self.redraw = true;
        true
    }
}

impl PaintOutput {
    pub(super) fn open_state(
        &mut self,
        rect: Rect,
        appearance: Option<&AnimatedAppearance>,
        unwind: &mut paint::Unwind,
    ) {
        let Some(appearance) = appearance else {
            return;
        };
        paint::scale::paint(&mut self.canvas, appearance.scale.get(), rect, unwind);
        if appearance.opacity.get() < 1.0 {
            paint::opacity::paint(&mut self.canvas, appearance.opacity, unwind);
        }
    }
    pub(super) fn paint_state_tint(&mut self, rect: Rect, appearance: Option<&AnimatedAppearance>) {
        let Some(tint) = appearance
            .and_then(|appearance| appearance.tint.as_ref())
            .filter(|tint| tint.color.color.alpha() > 0.0)
        else {
            return;
        };
        self.canvas.push(DrawCommand::Fill {
            shape: RoundedRect::new(rect, tint.corners).into(),
            brush: tint.color.color.into(),
        });
    }
    pub(super) fn paint_state_outline(
        &mut self,
        rect: Rect,
        appearance: Option<&AnimatedAppearance>,
    ) {
        let Some(outline) = appearance.and_then(|appearance| appearance.outline.as_ref()) else {
            return;
        };
        let stroke = *outline.stroke.value();
        if stroke.paints() {
            self.canvas.push(DrawCommand::Stroke {
                shape: Shape::rounded(rect, outline.radius),
                stroke,
            });
        }
    }
}
