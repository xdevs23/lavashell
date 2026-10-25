// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Action, ActivateAction, Activation, BTreeSet, DrawCommand, InputEvent, Key, NodeId, NodeKind,
    Point, PointerId, Rect, Transform, View, paint,
};
use crate::animation::{Animation, AnimationError};
use crate::effects::MaterialCommands;
use crate::shape::{RoundedRect, Shape};
use crate::style::{AnimatedAppearance, FocusState, InteractionState, PointerState, StateStyles};
use std::collections::btree_map::Entry;

/// The animated appearance of a node with state styles, with the styles and the interaction state from which the
/// view resolved the target of the animation.
pub(super) struct StyledAppearance {
    styles: StateStyles,
    state: InteractionState,
    pub animation: Animation<AnimatedAppearance>,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum FocusSource {
    Key,
    Pointer,
    Assistive,
}

impl View {
    /// Cancels the move of selected text that the view drags by itself. A host calls this when a drag and drop of the
    /// platform starts with that text, because the platform then moves the text.
    pub fn cancel_pending_text_drag(&mut self) {
        self.cancel_text_drag();
    }

    /// The interaction state of the first node tagged `tag`: the state from which the view picks the state styles of
    /// that node.
    #[must_use]
    pub fn interaction_state(&self, tag: &str) -> Option<InteractionState> {
        self.nodes
            .iter()
            .position(|node| node.tag.as_deref() == Some(tag))
            .map(|index| self.interaction_state_at(index))
    }

    fn interaction_state_at(&self, index: usize) -> InteractionState {
        let node = &self.nodes[index];
        let id = node.id;
        let disabled = self.input_disabled(index);
        let available = !disabled && self.window_focused;
        let focused = available && self.focus == Some(id);
        InteractionState {
            pointer: PointerState {
                hovered: !disabled && self.is_hovered(id),
                active: !disabled
                    && (self.is_pressed(id)
                        || available
                            && self.keyboard_press.is_some_and(|(pressed, _)| {
                                pressed == id && self.focus == Some(id)
                            })),
                captured: !disabled && self.is_captured(id),
            },
            focus: FocusState {
                focused,
                visible: focused && self.focus_shows_on(index),
            },
            disabled,
            selected: node.selected == Some(true),
        }
    }

    /// The appearance for the interaction state of the node at `index`. A node painted while it leaves the tree has
    /// no interaction state any more. Its appearance therefore continues the animation toward the appearance set last
    /// while the node was in the tree.
    pub(super) fn interaction_visual(
        &mut self,
        index: usize,
        source: paint::Source,
    ) -> Option<AnimatedAppearance> {
        match source {
            paint::Source::Composition => self.live_interaction_visual(index),
            paint::Source::Leaving => (self.interaction_visuals.get(&self.nodes[index].id))
                .map(|styled| styled.animation.get()),
        }
    }

    fn live_interaction_visual(&mut self, index: usize) -> Option<AnimatedAppearance> {
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
            let _visuals_of_a_node_without_styles = self.interaction_visuals.remove(&node.id);
            return None;
        };
        match self.styled_appearance(node.id, state, styles) {
            Ok(appearance) => Some(appearance),
            Err(error) => {
                self.fail_paint(error);
                None
            }
        }
    }

    /// The appearance of the node `id`, animated from the appearance with which the node was first styled toward the
    /// appearance given by `styles` for `state`.
    fn styled_appearance(
        &mut self,
        id: NodeId,
        state: InteractionState,
        styles: StateStyles,
    ) -> Result<AnimatedAppearance, AnimationError> {
        let visual = match self.interaction_visuals.entry(id) {
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

    /// The drawing commands for the material of the state appearance, painted below the decorations of the node at
    /// the bounds to which `stretch` maps the surface `rect`.
    pub(super) fn state_material(
        rect: Rect,
        appearance: Option<&AnimatedAppearance>,
        stretch: Transform,
    ) -> Option<MaterialCommands> {
        let material = appearance?.material.as_ref()?;
        Some(paint::material::commands(rect, material.value(), stretch))
    }

    /// Applies the scale and the opacity of the state appearance to everything that the node paints until `unwind`
    /// runs. Input and layout use the unscaled bounds of the node.
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

    /// The bounds at which the node at `index` paints: the layout bounds of the node, scaled by its state appearance.
    pub(super) fn painted_bounds(&self, index: usize) -> Rect {
        let bounds = self.layout[index].rect;
        match self.interaction_visuals.get(&self.nodes[index].id) {
            Some(styled) => {
                let scale = styled.animation.get().scale.get();
                paint::scale::about(bounds, scale).map_or(bounds, |scaled| scaled.rect(bounds))
            }
            None => bounds,
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

    pub(super) fn sanitize_interactions(&mut self) {
        let enabled = self.input_enabled_nodes();
        let keyboard_invalid = (self.keyboard_press)
            .is_some_and(|(id, _)| self.focus != Some(id) || !enabled.contains(&id));
        let invalid: BTreeSet<_> = (self.pointers.values())
            .flat_map(|pointer| pointer.nodes().chain(pointer.over))
            .filter(|id| !enabled.contains(id))
            .collect();
        if keyboard_invalid {
            self.keyboard_press = None;
        }
        if (self.pointers.values()).any(|pointer| pointer.nodes().any(|id| invalid.contains(&id))) {
            self.end_captures(|id| invalid.contains(&id));
            self.cancel_text_drag();
        }
        for pointer in self.pointers.values_mut() {
            if pointer.over.is_some_and(|id| invalid.contains(&id)) {
                pointer.over = None;
            }
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
                    self.keyboard_press = None;
                    self.cancel_text_drag();
                }
                self.redraw = true;
                true
            }
            InputEvent::KeyRelease(key) => {
                if self
                    .keyboard_press
                    .is_some_and(|(_, pressed)| pressed == *key)
                {
                    self.keyboard_press = None;
                    self.redraw = true;
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn record_focus_source(&mut self, source: FocusSource) {
        if self.focus_source != Some(source) {
            self.focus_source = Some(source);
            self.redraw = true;
        }
    }

    pub(super) fn focus_shows_on(&self, index: usize) -> bool {
        matches!(
            self.focus_source,
            None | Some(FocusSource::Key | FocusSource::Assistive)
        ) || matches!(self.nodes[index].kind, NodeKind::TextInput { .. })
    }

    pub(super) fn activate_hit(&mut self, node: NodeId, at: Point) {
        let activate = self.handler(node, |hit| hit.activate.clone());
        let action = self.handler(node, |hit| hit.action.clone());
        self.run_activation(
            activate.map(|(activate, _)| activate),
            action.map(|(action, _)| action),
            at,
        );
    }

    /// Runs `activate` for an activation at `at`, then `action`.
    pub(super) fn run_activation(
        &mut self,
        activate: Option<ActivateAction>,
        action: Option<Action>,
        at: Point,
    ) {
        if let Some(activate) = activate {
            activate.borrow_mut()(Activation {
                position: at,
                modifiers: self.modifiers,
            });
            self.redraw = true;
        }
        if let Some(action) = action {
            action.borrow_mut()();
            self.redraw = true;
        }
    }

    pub(super) fn begin_keyboard_activation(&mut self, key: Key) -> bool {
        let Some(id) = self.focus.filter(|_| self.window_focused) else {
            return false;
        };
        if self.keyboard_press == Some((id, key)) {
            return false;
        }
        if !self.hits.iter().any(|hit| hit.id == id && hit.activates()) {
            return false;
        }
        self.keyboard_press = Some((id, key));
        self.redraw = true;
        true
    }
}
