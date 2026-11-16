// SPDX-License-Identifier: GPL-3.0-only
mod actions;
mod snapshot;
#[cfg(test)]
mod tests;
mod text;

use super::component::index_of;
use super::{HitClip, View};
use crate::accessibility::{AccessibilityTree, Semantics};
use crate::geometry::{Rect, Scale};
use crate::reactive::{Phase, Scope};
use crate::scene::Transform;
use crate::ui::NodeId;
use crate::view::component::collect_descendants;
use snapshot::InheritedSemantics;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct AccessibilityExport {
    pub tree: AccessibilityTree,
    pub geometry: BTreeMap<NodeId, SemanticGeometry>,
    pub text_geometry: bool,
    pub scope: Scope,
    pub metadata: BTreeMap<NodeId, Semantics>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct SemanticGeometry {
    pub bounds: Rect,
    pub visible: Option<Rect>,
    pub content: Rect,
    pub scale: Scale,
}

impl SemanticGeometry {
    pub fn new(bounds: Rect, content: Rect, scale: Scale, clips: &[HitClip]) -> Self {
        Self {
            bounds,
            visible: clipped(Some(bounds), clips),
            content,
            scale,
        }
    }

    pub fn placed(self, by: Transform, clips: &[HitClip]) -> Self {
        Self {
            bounds: by.rect(self.bounds),
            visible: clipped(self.visible.map(|visible| by.rect(visible)), clips),
            content: by.rect(self.content),
            scale: Scale {
                x: self.scale.x * by.scale.x,
                y: self.scale.y * by.scale.y,
            },
        }
    }
}

fn clipped(visible: Option<Rect>, clips: &[HitClip]) -> Option<Rect> {
    (clips.iter()).fold(visible, |visible, clip| {
        visible?.intersection(clip.shape.rect())
    })
}

impl View {
    #[must_use]
    pub fn accessibility(&self) -> &AccessibilityTree {
        &self.accessibility.tree
    }

    #[must_use]
    pub fn needs_accessibility_update(&self) -> bool {
        self.accessibility.scope.is_dirty()
    }

    pub fn update_accessibility(&mut self) {
        self.refresh_accessibility();
    }

    pub(super) fn accessibility_semantics(&self, index: usize) -> &Semantics {
        let node = &self.nodes[index];
        self.accessibility
            .metadata
            .get(&node.id)
            .unwrap_or(&node.semantics)
    }

    pub fn set_accessibility_text_geometry(&mut self, enabled: bool) {
        if self.accessibility.text_geometry != enabled {
            self.accessibility.text_geometry = enabled;
            self.retained.clear();
            self.redraw = true;
            if !enabled {
                let fields = self
                    .nodes
                    .iter()
                    .enumerate()
                    .filter_map(|(index, node)| self.text_selectable(index).then_some(node.id))
                    .collect::<BTreeSet<_>>();
                self.editing.positions.retain(|id, _| fields.contains(id));
            }
        }
    }

    pub(super) fn refresh_stale_accessibility(&mut self) {
        if self.needs_accessibility_update() {
            self.refresh_accessibility();
        }
    }

    pub(super) fn refresh_accessibility(&mut self) {
        self.sample_accessibility_metadata();
        self.filter_input_hits();
        self.build_accessibility_tree();
    }

    pub(super) fn sample_accessibility_metadata(&mut self) {
        if self.nodes.is_empty() {
            return;
        }
        let scope = self.accessibility.scope.clone();
        self.accessibility.metadata = scope.track(Phase::Paint, || {
            self.nodes
                .iter()
                .map(|node| {
                    let mut semantics = node.semantics.clone();
                    if let Some(provider) = &node.semantics_provider {
                        provider(&mut semantics);
                    }
                    (node.id, semantics)
                })
                .collect()
        });
    }

    pub(super) fn build_accessibility_tree(&mut self) {
        if self.nodes.is_empty() {
            return;
        }
        let mut nodes = BTreeMap::new();
        let root = self.nodes[0].id;
        let mut top_level = Vec::new();
        self.accessibility_subtree(0, InheritedSemantics::default(), &mut nodes, &mut top_level);
        debug_assert_eq!(top_level, [root]);
        if let Some(modal) = self.routing.modal_root {
            let mut keep = BTreeSet::from([root]);
            if let Some(index) = index_of(&self.nodes, modal) {
                let mut indices = BTreeSet::new();
                collect_descendants(&self.nodes, index, &mut indices);
                keep.extend(indices.iter().map(|i| self.nodes[*i].id));
            }
            nodes.retain(|id, _| keep.contains(id));
            let child = nodes.contains_key(&modal).then_some(modal);
            if let Some(root) = nodes.get_mut(&root) {
                root.children = child.into_iter().collect();
            }
        }
        let ids = nodes.keys().copied().collect::<BTreeSet<_>>();
        for node in nodes.values_mut() {
            node.children.retain(|id| ids.contains(id));
            let relations = &mut node.relations;
            relations.labelled_by.retain(|id| ids.contains(id));
            relations.described_by.retain(|id| ids.contains(id));
            relations.controls.retain(|id| ids.contains(id));
            relations.error_message.retain(|id| ids.contains(id));
            relations.member_of = relations.member_of.filter(|id| ids.contains(id));
        }
        let focus = self.routing.focus.filter(|id| ids.contains(id));
        if self.accessibility.tree.nodes != nodes || self.accessibility.tree.focus != focus {
            self.accessibility.tree.revision += 1;
            self.accessibility.tree.nodes = nodes;
            self.accessibility.tree.focus = focus;
            self.accessibility.tree.root = root;
        }
    }
}
