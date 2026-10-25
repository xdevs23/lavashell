// SPDX-License-Identifier: GPL-3.0-only
mod actions;
mod snapshot;
#[cfg(test)]
mod tests;
mod text;

use super::{
    Axis, BTreeMap, BTreeSet, Dp, FocusSource, HitClip, NodeId, NodeKind, Phase, Point, Rect,
    Scale, TextEdit, TextGeometry, Transform, View, collect_descendants,
};
use crate::accessibility::{
    AccessibilityAction, AccessibilityTree, AccessibleNode, AccessibleState, AccessibleTextRun,
    AccessibleValue, ActionError, ActionKind, ActionOutcome, ActionRequest, RangeInfo, Role,
    Semantics, SemanticsMode, SemanticsState, TextDirection, TextSelection,
};

#[derive(Clone, Copy, Default)]
pub(super) struct SemanticGeometry {
    pub bounds: Rect,
    /// The part of `bounds` left visible by the clips of the enclosing nodes, or `None` where those clips hide all of
    /// it. This rectangle is not yet cut to the edges of the view.
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
        &self.accessibility
    }

    #[must_use]
    pub fn needs_accessibility_update(&self) -> bool {
        self.accessibility_scope.dirty().any()
    }

    /// Rebuilds the tree returned by [`View::accessibility`] from the last complete layout, without composing and
    /// without changing the drawing commands. Before the first layout, the view still needs a frame from `frame_at`.
    pub fn update_accessibility(&mut self) {
        self.refresh_accessibility();
    }

    pub(super) fn accessibility_semantics(&self, index: usize) -> &Semantics {
        let node = &self.nodes[index];
        self.accessibility_metadata
            .get(&node.id)
            .unwrap_or(&node.semantics)
    }

    /// Turns on or off the text geometry, such as the position of each character, for text that cannot be selected,
    /// and repaints the view. Only an assistive client reads that geometry. Text inputs and selectable text always have
    /// it, because selecting and editing need it.
    pub fn set_accessibility_text_geometry(&mut self, enabled: bool) {
        if self.accessibility_text_geometry != enabled {
            self.accessibility_text_geometry = enabled;
            self.retained.clear();
            self.redraw = true;
            if !enabled {
                let fields = self
                    .nodes
                    .iter()
                    .enumerate()
                    .filter_map(|(index, node)| self.text_selectable(index).then_some(node.id))
                    .collect::<BTreeSet<_>>();
                self.text_positions.retain(|id, _| fields.contains(id));
            }
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
        let scope = self.accessibility_scope.clone();
        self.accessibility_metadata = scope.track(Phase::Paint, || {
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
        self.accessibility_subtree(0, false, false, &mut nodes, &mut top_level);
        // The root node has no semantics by which `accessibility_subtree` could leave it out. The root node is
        // therefore the only node at the top of the tree.
        debug_assert_eq!(top_level, [root]);
        if let Some(modal) = self.modal_root {
            let mut keep = BTreeSet::from([root]);
            if let Some(index) = self.nodes.iter().position(|node| node.id == modal) {
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
        let focus = self.focus.filter(|id| ids.contains(id));
        if self.accessibility.nodes != nodes || self.accessibility.focus != focus {
            self.accessibility.revision += 1;
            self.accessibility.nodes = nodes;
            self.accessibility.focus = focus;
            self.accessibility.root = root;
        }
    }
}
