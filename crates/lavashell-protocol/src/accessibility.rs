// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::accessibility::AccessibleNode;
use lavashell_core::encoding::sequence;
use lavashell_core::ui::NodeId;
use serde::{Deserialize, Serialize};

/// The accessible nodes of a guest that are new or changed since its previous update, and the ids of the nodes
/// that are gone since then.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AccessibilityUpdate {
    pub revision: u64,
    pub root: NodeId,
    pub focus: Option<NodeId>,
    #[serde(deserialize_with = "sequence")]
    pub changed: Vec<AccessibleNode>,
    #[serde(deserialize_with = "sequence")]
    pub removed: Vec<NodeId>,
}
