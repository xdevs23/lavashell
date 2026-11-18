// SPDX-License-Identifier: GPL-3.0-only
use super::View;
use super::component::index_of;
use super::input::InputRouting;
use crate::geometry::{Axis, Point};
use crate::scene::tree::Clamp;
use crate::scroll::{ScrollState, largest_offset};
use crate::ui::{Node, NodeId};
use crate::units::Dp;
use crate::view::layout::Layout;
use std::collections::BTreeMap;

pub(super) fn content_clamp(node: &Node, layout: &Layout) -> Option<Clamp> {
    let axis = node.scroll_axis()?;
    let viewport = axis.length(layout.innermost_layer());
    Some(Clamp {
        min: Dp::ZERO,
        max: largest_offset(viewport, axis.length(layout.content)),
        axis,
        scale: -1.0,
    })
}

pub(super) fn unclamped_offset(node: &Node, scrolls: &BTreeMap<NodeId, Dp>) -> Dp {
    offset(node, scrolls, ScrollState::unclamped_offset)
}

fn offset(node: &Node, scrolls: &BTreeMap<NodeId, Dp>, read: fn(&ScrollState) -> Dp) -> Dp {
    (node.scroll_state()).map_or_else(|| scrolls.get(&node.id).copied().unwrap_or_default(), read)
}

#[derive(Clone, Copy)]
pub(super) enum ScrollStyle {
    Jump,
    Animated,
}

impl View {
    pub(super) fn wheel(&mut self, at: Point, delta: Point) {
        for axis in Axis::ALL {
            self.wheel_along(at, axis, axis.coordinate(delta));
        }
    }

    fn wheel_along(&mut self, at: Point, axis: Axis, delta: Dp) {
        if !delta.is_finite() || delta == Dp::ZERO {
            return;
        }
        let Some(hit) = self.routing.hit_at(at) else {
            return;
        };
        let Some(mut index) = index_of(&self.nodes, hit.id) else {
            return;
        };
        loop {
            let node = &self.nodes[index];
            if self.input_disabled(index) {
                return;
            }
            if let Some(action) = node.wheel_handler(axis).cloned() {
                action.borrow_mut()(delta);
                self.redraw = true;
                return;
            }
            if (node.scroll_axis())
                .is_some_and(|scrolled| self.routing.wheel_scrolls(axis, scrolled))
            {
                if let Some(state) = node.scroll_state() {
                    state.scroll_by(delta);
                } else {
                    *self.scrolls.entry(node.id).or_default() += delta;
                }
                self.redraw = true;
                return;
            }
            let Some(parent) = self.parents.node_of(index) else {
                return;
            };
            index = parent;
        }
    }

    pub(super) fn scroll_offset(&self, index: usize) -> Dp {
        offset(&self.nodes[index], &self.scrolls, ScrollState::offset)
    }

    pub(super) fn scroll_viewport(&self, index: usize) -> Option<Dp> {
        let axis = self.nodes[index].scroll_axis()?;
        let geometry = self.accessibility.geometry.get(&self.nodes[index].id)?;
        let scale = axis.factor(geometry.scale);
        (scale > 0.0).then(|| axis.length(geometry.content.size()) / scale)
    }

    pub(super) fn scroll_maximum(&self, index: usize) -> Option<Dp> {
        let axis = self.nodes[index].scroll_axis()?;
        let content = axis.length(self.layout[index].content);
        Some(largest_offset(self.scroll_viewport(index)?, content))
    }

    pub(super) fn scroll_node_by(&mut self, index: usize, delta: Dp, style: ScrollStyle) {
        let Some(maximum) = self.scroll_maximum(index).filter(|_| delta.is_finite()) else {
            return;
        };
        let node = &self.nodes[index];
        if let Some(state) = node.scroll_state() {
            match style {
                ScrollStyle::Jump => {
                    state.jump_to((state.offset() + delta).clamp(Dp::ZERO, maximum));
                }
                ScrollStyle::Animated => state.scroll_by(delta),
            }
        } else {
            let offset = self.scrolls.entry(node.id).or_default();
            *offset = (*offset + delta).clamp(Dp::ZERO, maximum);
        }
    }

    pub(super) fn scroll_node_by_window(&mut self, index: usize, delta: Dp) {
        let Some(axis) = self.nodes[index].scroll_axis() else {
            return;
        };
        let Some(geometry) = self.accessibility.geometry.get(&self.nodes[index].id) else {
            return;
        };
        let scale = axis.factor(geometry.scale);
        if scale > 0.0 {
            self.scroll_node_by(index, delta / scale, ScrollStyle::Jump);
        }
    }
}

impl InputRouting {
    fn wheel_scrolls(&self, turned: Axis, scrolled: Axis) -> bool {
        turned == scrolled || self.modifiers.shift && turned == Axis::Vertical
    }
}
