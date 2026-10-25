// SPDX-License-Identifier: GPL-3.0-only
use super::{
    AccessibleState, AccessibleText, AccessibleValue, ActionKinds, CollectionInfo, CollectionItem,
    LiveRegion, RangeInfo, Relations, Role,
};
use crate::geometry::Rect;
use crate::ui::NodeId;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AccessibleNode {
    pub id: NodeId,
    pub role: Role,
    pub heading_level: Option<u8>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub value: Option<AccessibleValue>,
    pub bounds: Rect,
    /// The part of `bounds` that is visible after clipping, or `None` for a node with no visible part,
    /// such as a node outside the screen.
    pub visible_bounds: Option<Rect>,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::sequence")
    )]
    pub children: Vec<NodeId>,
    pub state: AccessibleState,
    pub actions: ActionKinds,
    pub relations: Relations,
    pub live: LiveRegion,
    pub range: Option<RangeInfo>,
    pub collection: Option<CollectionInfo>,
    pub item: Option<CollectionItem>,
    pub text: Option<AccessibleText>,
}

/// A snapshot of every accessible node of a view, taken after composition, layout, painting and the
/// focus changes made by modals.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityTree {
    pub revision: u64,
    pub root: NodeId,
    pub focus: Option<NodeId>,
    pub nodes: BTreeMap<NodeId, AccessibleNode>,
}

impl Default for AccessibilityTree {
    fn default() -> Self {
        Self {
            revision: 0,
            root: NodeId(1),
            focus: None,
            nodes: BTreeMap::new(),
        }
    }
}
