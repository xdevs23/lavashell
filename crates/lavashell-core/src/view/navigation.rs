// SPDX-License-Identifier: GPL-3.0-only
use super::laid_out::LaidOutTree;
use super::{Axis, BTreeSet, Dp, Hit, Key, NodeId, Shortcut, View, collect_descendants, index_of};
use crate::scroll::page_length;

impl View {
    /// Moves the focus to the nearest ancestor still in the tree when composing again removed the focused node.
    /// `previous` is the tree from before that composition, which contains the ancestors of the removed node.
    pub(super) fn recover_focus(&mut self, previous: &LaidOutTree) {
        self.focus = self.focus.and_then(|id| {
            let mut index = index_of(previous.nodes(), id)?;
            loop {
                let id = previous.nodes()[index].id;
                if self.nodes.iter().any(|node| node.id == id) {
                    return Some(id);
                }
                index = previous.parents().node_of(index)?;
            }
        });
    }

    /// Moves the focus from a node without a hit, such as a clipped or disabled node, to the nearest ancestor that can
    /// receive focus. Without such an ancestor, the view removes the focus, so that a hidden or disabled text field
    /// cannot consume the keys of the shortcuts of the application.
    pub(super) fn normalize_focus(&mut self) {
        if self
            .focus
            .is_none_or(|id| self.hits.iter().any(|hit| hit.id == id))
        {
            return;
        }
        self.focus = self.focus.and_then(|id| {
            let mut index = self.nodes.iter().position(|node| node.id == id)?;
            loop {
                let id = self.nodes[index].id;
                if self.hits.iter().any(|hit| hit.focusable && hit.id == id) {
                    return Some(id);
                }
                index = self.parents.node_of(index)?;
            }
        });
    }

    /// Moves focus to the next focusable hit for which `eligible` is true, or to the previous one,
    /// and wraps around at the end.
    pub(super) fn cycle_focus(&mut self, backward: bool, eligible: impl Fn(&Self, &Hit) -> bool) {
        let mut ids: Vec<_> = (self.hits.iter())
            .filter(|hit| hit.focusable && eligible(self, hit))
            .map(|hit| hit.id)
            .collect();
        ids.dedup();
        let count = ids.len();
        if count == 0 {
            return;
        }
        let next = match self
            .focus
            .and_then(|focus| ids.iter().position(|id| *id == focus))
        {
            None if backward => count - 1,
            None => 0,
            Some(index) if backward => (index + count - 1) % count,
            Some(index) => (index + 1) % count,
        };
        self.focus = Some(ids[next]);
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
        let inherited =
            inherited || !node.enabled || self.accessibility_semantics(index).state.disabled;
        if inherited {
            let _always_new_as_each_node_is_visited_once = disabled.insert(node.id);
        }
        for child in &node.children {
            self.collect_disabled(*child, inherited, disabled);
        }
    }

    pub(super) fn input_disabled(&self, mut index: usize) -> bool {
        let mut in_modal = self.modal_root.is_none();
        loop {
            let node = &self.nodes[index];
            in_modal |= Some(node.id) == self.modal_root;
            if !node.enabled || self.accessibility_semantics(index).state.disabled {
                return true;
            }
            let Some(parent) = self.parents.node_of(index) else {
                return !in_modal;
            };
            index = parent;
        }
    }

    pub(super) fn modal_nodes(&self) -> Option<BTreeSet<NodeId>> {
        let index = self.nodes.iter().rposition(|node| node.modal)?;
        let mut descendants = BTreeSet::new();
        collect_descendants(&self.nodes, index, &mut descendants);
        Some(
            descendants
                .iter()
                .map(|index| self.nodes[*index].id)
                .collect(),
        )
    }

    /// The ids of the nodes that receive input: the enabled nodes, inside the modal node when there is one.
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
        self.hits.clear();
        self.hits.extend(
            self.input_hits
                .iter()
                .filter(|hit| {
                    enabled.contains(&hit.id)
                        && hit.clips.iter().all(|clip| {
                            let clip_rect = clip.shape.rect();
                            // A hit outside a clip from another moving group still counts when the hit and the
                            // clip intersect once both groups come to rest.
                            hit.rect.intersection(clip_rect).is_some()
                                || clip.group != hit.group
                                    && (self.moving.rested(hit.group, hit.rect))
                                        .intersection(self.moving.rested(clip.group, clip_rect))
                                        .is_some()
                        })
                })
                .cloned(),
        );
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
        let (delta, immediate) = match (axis, shortcut.key) {
            (Axis::Vertical, Key::Up) | (Axis::Horizontal, Key::Left) => (-line, false),
            (Axis::Vertical, Key::Down) | (Axis::Horizontal, Key::Right) => (line, false),
            (Axis::Vertical, Key::PageUp) => (-page, false),
            (Axis::Vertical, Key::PageDown) => (page, false),
            (Axis::Vertical, Key::Space) => (if shortcut.shift { -page } else { page }, false),
            (_, Key::Home) => (-offset, true),
            (_, Key::End) => (
                self.scroll_maximum(index).unwrap_or_default() - offset,
                true,
            ),
            _ => return false,
        };
        self.scroll_node_by(index, delta, immediate);
        true
    }
}
