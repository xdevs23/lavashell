// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::geometry::Color;
use crate::units::DpExt;

pub(in crate::scene) fn square() -> Rect {
    Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp())
}

pub(in crate::scene) fn white_fill(rect: Rect) -> DrawCommand {
    DrawCommand::Fill {
        shape: rect.into(),
        brush: Color::WHITE.into(),
    }
}

pub(in crate::scene) fn fill_content(rect: Rect) -> Result<Content, UnbalancedRestore> {
    Ok(Content::Packet(Packet::new(Arc::from([white_fill(rect)]))?))
}

pub(in crate::scene) fn fill(rect: Rect) -> Result<Node, UnbalancedRestore> {
    Ok(Node::Leaf(fill_content(rect)?))
}

pub(in crate::scene) fn insert(id: u64, parent: Option<u64>, index: usize, node: Node) -> Change {
    Change::Insert {
        id: SceneNodeId(id),
        parent: parent.map(SceneNodeId),
        index,
        node,
    }
}
