// SPDX-License-Identifier: GPL-3.0-only
use super::{Axis, BTreeMap, Dp, Layout, Node, NodeId, Point, View};
use crate::scene::tree::Clamp;
use crate::scroll::{ScrollState, largest_offset};

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

/// The scroll offset of the scrolling `node` before the clamp to the range that it can scroll. This function reads no
/// size that the layout publishes to the scroll state, so that placing the node does not depend on those sizes.
pub(super) fn unclamped_offset(node: &Node, scrolls: &BTreeMap<NodeId, Dp>) -> Dp {
    offset(node, scrolls, ScrollState::unclamped_offset)
}

fn offset(node: &Node, scrolls: &BTreeMap<NodeId, Dp>, read: fn(&ScrollState) -> Dp) -> Dp {
    (node.scroll_state()).map_or_else(|| scrolls.get(&node.id).copied().unwrap_or_default(), read)
}

impl View {
    pub(super) fn wheel(&mut self, at: Point, delta: Point) {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            self.wheel_along(at, axis, axis.coordinate(delta));
        }
    }

    /// Passes a scroll by `delta` along `axis` at `at` to the innermost node under `at` that handles the scroll with
    /// `on_scroll` or that scrolls along `axis`.
    fn wheel_along(&mut self, at: Point, axis: Axis, delta: Dp) {
        if !delta.is_finite() || delta == Dp::ZERO {
            return;
        }
        let Some(hit) = self.hit_at(at) else {
            return;
        };
        let Some(mut index) = self.nodes.iter().position(|node| node.id == hit.id) else {
            return;
        };
        // The search walks up only from the topmost hit. A scrolling node below an overlay that is not inside that
        // node therefore does not scroll.
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
            if (node.scroll_axis()).is_some_and(|scrolled| self.wheel_scrolls(axis, scrolled)) {
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

    /// Whether a wheel turned along `turned` scrolls a node that scrolls along `scrolled`. With
    /// Shift held, a vertical wheel also scrolls a node that scrolls sideways.
    fn wheel_scrolls(&self, turned: Axis, scrolled: Axis) -> bool {
        turned == scrolled || self.modifiers.shift && turned == Axis::Vertical
    }

    pub(super) fn scroll_offset(&self, index: usize) -> Dp {
        offset(&self.nodes[index], &self.scrolls, ScrollState::offset)
    }

    /// The length of the viewport of the scrolling node at `index` along its axis, in the
    /// coordinates of its content, from the bounds where it was painted last.
    pub(super) fn scroll_viewport(&self, index: usize) -> Option<Dp> {
        let axis = self.nodes[index].scroll_axis()?;
        let geometry = self.accessibility_geometry.get(&self.nodes[index].id)?;
        let scale = axis.factor(geometry.scale);
        (scale > 0.0).then(|| axis.length(geometry.content.size()) / scale)
    }

    /// The largest scroll offset of the node at `index`, from the bounds where it was painted last.
    pub(super) fn scroll_maximum(&self, index: usize) -> Option<Dp> {
        let axis = self.nodes[index].scroll_axis()?;
        let content = axis.length(self.layout[index].content);
        Some(largest_offset(self.scroll_viewport(index)?, content))
    }

    pub(super) fn scroll_node_by(&mut self, index: usize, delta: Dp, immediate: bool) {
        let Some(maximum) = self.scroll_maximum(index).filter(|_| delta.is_finite()) else {
            return;
        };
        let node = &self.nodes[index];
        if let Some(state) = node.scroll_state() {
            if immediate {
                state.jump_to((state.offset() + delta).clamp(Dp::ZERO, maximum));
            } else {
                state.scroll_by(delta);
            }
        } else {
            let offset = self.scrolls.entry(node.id).or_default();
            *offset = (*offset + delta).clamp(Dp::ZERO, maximum);
        }
    }

    /// Scrolls the node at `index` at once by `delta`, measured in the coordinates of the window. This function divides
    /// `delta` by the scale of the transforms around the node, so that the content moves as far as the pointer.
    pub(super) fn scroll_node_by_window(&mut self, index: usize, delta: Dp) {
        let Some(axis) = self.nodes[index].scroll_axis() else {
            return;
        };
        let Some(geometry) = self.accessibility_geometry.get(&self.nodes[index].id) else {
            return;
        };
        let scale = axis.factor(geometry.scale);
        if scale > 0.0 {
            self.scroll_node_by(index, delta / scale, true);
        }
    }
}
