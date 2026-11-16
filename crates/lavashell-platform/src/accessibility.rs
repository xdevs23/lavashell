// SPDX-License-Identifier: GPL-3.0-only
use accesskit::{ActionRequest, TreeId, TreeUpdate};
use lavashell_actor::{Merge, MergeEffect};
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum AccessibilityEvent {
    Activate(SurfaceId),
    Deactivate(SurfaceId),
    Action(SurfaceId, ActionRequest),
}

impl AccessibilityEvent {
    #[must_use]
    pub fn surface(&self) -> SurfaceId {
        match self {
            Self::Activate(surface) | Self::Deactivate(surface) | Self::Action(surface, _) => {
                *surface
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum AccessibilityRequest {
    Publish {
        surface: SurfaceId,
        tree: Box<TreeUpdate>,
    },
    SetWindowFocus {
        surface: SurfaceId,
        focused: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccessibilityRequestKey {
    Tree(SurfaceId),
    WindowFocus(SurfaceId),
}

impl Merge for AccessibilityRequest {
    type Key = AccessibilityRequestKey;

    fn effect(&self) -> MergeEffect<AccessibilityRequestKey> {
        match *self {
            Self::Publish { surface, .. } => {
                MergeEffect::Overwrites(AccessibilityRequestKey::Tree(surface))
            }
            Self::SetWindowFocus { surface, .. } => {
                MergeEffect::Overwrites(AccessibilityRequestKey::WindowFocus(surface))
            }
        }
    }
}

#[must_use]
pub fn tree_difference(tree: &TreeUpdate, published: Option<&TreeUpdate>) -> TreeUpdate {
    let Some(published) = published.filter(|published| published.tree == tree.tree) else {
        return tree.clone();
    };
    let old: BTreeMap<_, _> = published
        .nodes
        .iter()
        .map(|(id, node)| (*id, node))
        .collect();
    TreeUpdate {
        nodes: tree
            .nodes
            .iter()
            .filter(|(id, node)| old.get(id) != Some(&node))
            .cloned()
            .collect(),
        tree: None,
        tree_id: TreeId::ROOT,
        focus: tree.focus,
    }
}
