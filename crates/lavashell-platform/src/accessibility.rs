// SPDX-License-Identifier: GPL-3.0-only
use accesskit::{ActionRequest, TreeId, TreeUpdate};
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;

/// What an assistive technology did with a surface.
#[derive(Clone, Debug, PartialEq)]
pub enum AccessibilityEvent {
    /// An assistive technology started reading the surface.
    Activate(SurfaceId),
    /// The last assistive technology stopped reading the surface.
    Deactivate(SurfaceId),
    /// An assistive technology requests an action of the surface.
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

/// What the runtime asks a platform to tell the assistive technologies that read a surface.
#[derive(Debug, PartialEq)]
pub enum AccessibilityRequest {
    /// Publishes the whole accessibility tree of the surface, which replaces the tree of an earlier request. The
    /// platform applies the nodes that changed since the tree it published last, and gives the whole tree to an
    /// assistive technology that starts reading the surface later.
    Publish {
        surface: SurfaceId,
        tree: Box<TreeUpdate>,
    },
    /// Sets whether the window of the surface has the keyboard focus of the host, and replaces the focus set by an
    /// earlier request.
    SetWindowFocus { surface: SurfaceId, focused: bool },
}

/// The update that turns `published` into `tree`: the nodes of `tree` that differ from `published`, or the whole of
/// `tree` when `published` is `None` or carries other tree information.
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
