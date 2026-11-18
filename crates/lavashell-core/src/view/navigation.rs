// SPDX-License-Identifier: GPL-3.0-only
use super::laid_out::LaidOutTree;
use super::scroll::ScrollStyle;
use super::{Hit, PaintedHit, View, WindowRegionHit};
use crate::geometry::Axis;
use crate::keyboard::{Key, Shortcut};
use crate::scroll::page_length;
use crate::ui::NodeId;
use crate::units::Dp;
use crate::view::component::{collect_descendants, index_of};
use crate::view::layout::Visibility;
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub(super) enum FocusDirection {
    Forward,
    Backward,
}

impl View {
    pub(super) fn recover_focus(&mut self, previous: &LaidOutTree) {
        let recovered = self.routing.focus.node().and_then(|id| {
            let mut index = index_of(previous.nodes(), id)?;
            loop {
                let id = previous.nodes()[index].id;
                if self.nodes.iter().any(|node| node.id == id) {
                    return Some(id);
                }
                index = previous.parents().node_of(index)?;
            }
        });
        self.routing.focus.move_to(recovered);
    }

    pub(super) fn normalize_focus(&mut self) {
        if self
            .routing
            .focus
            .node()
            .is_none_or(|id| self.routing.hits.iter().any(|hit| hit.id == id))
        {
            return;
        }
        let normalized = self.routing.focus.node().and_then(|id| {
            let mut index = index_of(&self.nodes, id)?;
            loop {
                let id = self.nodes[index].id;
                if self
                    .routing
                    .hits
                    .iter()
                    .any(|hit| hit.focusable && hit.id == id)
                {
                    return Some(id);
                }
                index = self.parents.node_of(index)?;
            }
        });
        self.routing.focus.move_to(normalized);
    }

    pub(super) fn cycle_focus(
        &mut self,
        direction: FocusDirection,
        eligible: impl Fn(&Self, &Hit) -> bool,
    ) {
        let mut ids: Vec<_> = (self.routing.hits.iter())
            .filter(|hit| hit.focusable && eligible(self, hit))
            .map(|hit| hit.id)
            .collect();
        ids.dedup();
        let count = ids.len();
        if count == 0 {
            return;
        }
        let focused =
            (self.routing.focus.node()).and_then(|focus| ids.iter().position(|id| *id == focus));
        let next = match (focused, direction) {
            (None, FocusDirection::Forward) => 0,
            (None, FocusDirection::Backward) => count - 1,
            (Some(index), FocusDirection::Forward) => (index + 1) % count,
            (Some(index), FocusDirection::Backward) => (index + count - 1) % count,
        };
        self.routing.focus.move_to(Some(ids[next]));
        self.redraw = true;
    }

    pub(super) fn disabled_nodes(&self) -> BTreeSet<NodeId> {
        let mut disabled = BTreeSet::new();
        if !self.nodes.is_empty() {
            self.collect_disabled(0, false, &mut disabled);
        }
        disabled
    }

    fn collect_disabled(&self, index: usize, inherited: bool, disabled: &mut BTreeSet<NodeId>) {
        let node = &self.nodes[index];
        let inherited = inherited || self.blocks_input(index);
        if inherited {
            let _always_new_as_each_node_is_visited_once = disabled.insert(node.id);
        }
        for child in &node.children {
            self.collect_disabled(*child, inherited, disabled);
        }
    }

    pub(super) fn input_disabled(&self, mut index: usize) -> bool {
        let mut in_modal = self.routing.modal_root.is_none();
        loop {
            in_modal |= Some(self.nodes[index].id) == self.routing.modal_root;
            if self.blocks_input(index) {
                return true;
            }
            let Some(parent) = self.parents.node_of(index) else {
                return !in_modal;
            };
            index = parent;
        }
    }

    fn blocks_input(&self, index: usize) -> bool {
        !self.nodes[index].enabled
            || self.layout[index].visibility == Visibility::Hidden
            || self.accessibility_semantics(index).state.disabled
    }

    pub(super) fn is_modal(&self, index: usize) -> bool {
        self.nodes[index].modal && self.layout[index].visibility == Visibility::Shown
    }

    pub(super) fn topmost_modal(&self) -> Option<usize> {
        (0..self.nodes.len())
            .rev()
            .find(|&index| self.is_modal(index))
    }

    pub(super) fn modal_nodes(&self) -> Option<BTreeSet<NodeId>> {
        let index = self.topmost_modal()?;
        let mut descendants = BTreeSet::new();
        collect_descendants(&self.nodes, index, &mut descendants);
        Some(
            descendants
                .iter()
                .map(|index| self.nodes[*index].id)
                .collect(),
        )
    }

    pub(super) fn input_enabled_nodes(&self) -> BTreeSet<NodeId> {
        let disabled = self.disabled_nodes();
        let modal = self.modal_nodes();
        (self.nodes.iter())
            .map(|node| node.id)
            .filter(|id| !disabled.contains(id))
            .filter(|id| modal.as_ref().is_none_or(|nodes| nodes.contains(id)))
            .collect()
    }

    pub(super) fn filter_input_hits(&mut self) {
        let enabled = self.input_enabled_nodes();
        self.routing.hits.clear();
        self.routing.window_regions.clear();
        let shown = self.routing.painted_hits.iter().filter(|hit| {
            let area = hit.area();
            enabled.contains(&hit.id())
                && area.clips.iter().all(|clip| {
                    let clip_rect = clip.shape.rect();
                    area.rect.intersection(clip_rect).is_some()
                        || clip.group != area.group
                            && (self.routing.moving.rested(area.group, area.rect))
                                .intersection(self.routing.moving.rested(clip.group, clip_rect))
                                .is_some()
                })
        });
        for hit in shown {
            match hit {
                PaintedHit::Input(hit) => self.routing.hits.push(hit.clone()),
                PaintedHit::WindowRegion { area, region, .. } => {
                    self.routing.window_regions.push(WindowRegionHit {
                        area: area.clone(),
                        region: *region,
                        input_hits_below: self.routing.hits.len(),
                    });
                }
            }
        }
    }

    pub(super) fn scroll_key(&mut self, index: usize, shortcut: Shortcut) -> bool {
        let Some(axis) = self.nodes[index].scroll_axis() else {
            return false;
        };
        if self.input_disabled(index) || shortcut.control || shortcut.alt {
            return false;
        }
        let line = self.layout[index].line_step;
        let page = page_length(self.scroll_viewport(index).unwrap_or(Dp::ZERO), line);
        let offset = self.scroll_offset(index);
        let (delta, style) = match (axis, shortcut.key) {
            (Axis::Vertical, Key::Up) | (Axis::Horizontal, Key::Left) => {
                (-line, ScrollStyle::Animated)
            }
            (Axis::Vertical, Key::Down) | (Axis::Horizontal, Key::Right) => {
                (line, ScrollStyle::Animated)
            }
            (Axis::Vertical, Key::PageUp) => (-page, ScrollStyle::Animated),
            (Axis::Vertical, Key::PageDown) => (page, ScrollStyle::Animated),
            (Axis::Vertical, Key::Space) => (
                if shortcut.shift { -page } else { page },
                ScrollStyle::Animated,
            ),
            (_, Key::Home) => (-offset, ScrollStyle::Jump),
            (_, Key::End) => (
                self.scroll_maximum(index).unwrap_or_default() - offset,
                ScrollStyle::Jump,
            ),
            _ => return false,
        };
        self.scroll_node_by(index, delta, style);
        true
    }
}
