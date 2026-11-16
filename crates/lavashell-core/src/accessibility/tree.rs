// SPDX-License-Identifier: GPL-3.0-only
use super::{
    AccessibleState, AccessibleText, AccessibleValue, ActionKinds, CollectionInfo, CollectionItem,
    HeadingLevel, LiveRegion, RangeInfo, Relations, Role,
};
use crate::geometry::Rect;
use crate::ui::NodeId;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AccessibleNode {
    pub id: NodeId,
    pub role: Role,
    pub heading_level: Option<HeadingLevel>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub value: Option<AccessibleValue>,
    pub bounds: Rect,
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
            root: NodeId::ROOT,
            focus: None,
            nodes: BTreeMap::new(),
        }
    }
}
