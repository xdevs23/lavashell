// SPDX-License-Identifier: GPL-3.0-only
use super::*;

pub(super) fn insert(id: u64, parent: Option<u64>, index: usize, node: Node) -> Change {
    Change::Insert {
        id: SceneNodeId(id),
        parent: parent.map(SceneNodeId),
        index,
        node,
    }
}
