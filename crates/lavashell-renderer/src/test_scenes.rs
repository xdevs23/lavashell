// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::{Painter, StreamFrames};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::{
    Change, Content, Node, Packet, SceneNodeId, SceneTree, SceneUpdate,
};
use skia_safe::Canvas;
use std::time::Duration;

pub(crate) fn insert(id: u64, parent: Option<u64>, index: usize, node: Node) -> Change {
    Change::Insert {
        id: SceneNodeId(id),
        parent: parent.map(SceneNodeId),
        index,
        node,
    }
}

/// A scene tree with one top-level leaf for each packet in `packets`, in order, with the node IDs 1, 2 and so on.
pub(crate) fn leaves(packets: impl IntoIterator<Item = Packet>) -> Result<SceneTree, TestError> {
    let changes = (packets.into_iter().enumerate())
        .map(|(index, packet)| {
            insert(
                index as u64 + 1,
                None,
                index,
                Node::Leaf(Content::Packet(packet)),
            )
        })
        .collect();
    let mut tree = SceneTree::default();
    tree.show(SceneUpdate { changes })?;
    Ok(tree)
}

pub(crate) fn paint(painter: &mut Painter, canvas: &Canvas, tree: &SceneTree, density: Density) {
    painter.paint_scene(canvas, tree.at(Duration::ZERO), density);
}

impl Painter {
    pub(crate) fn paint_as_leaf(
        &mut self,
        canvas: &Canvas,
        commands: &[DrawCommand],
        density: Density,
    ) -> Result<(), TestError> {
        self.paint_streams_as_leaf(canvas, commands, density, &StreamFrames::default())
    }

    pub(crate) fn paint_streams_as_leaf(
        &mut self,
        canvas: &Canvas,
        commands: &[DrawCommand],
        density: Density,
        streams: &StreamFrames,
    ) -> Result<(), TestError> {
        let tree = SceneTree::with_leaf(commands)?;
        self.paint_scene_with_streams(canvas, tree.at(Duration::ZERO), density, streams);
        Ok(())
    }
}

pub(crate) fn paint_damage(
    painter: &mut Painter,
    canvas: &Canvas,
    tree: &SceneTree,
    damage: Option<Rect>,
    clear: skia_safe::Color,
) -> Result<(), TestError> {
    let frame = crate::SurfaceFrame {
        tree: tree.at(Duration::ZERO),
        density: Density::ONE,
        damage,
        clear,
        scene: None,
        shape: None,
    };
    painter.paint_surface(canvas, frame)?;
    Ok(())
}

pub(crate) fn leaf_commands(update: SceneUpdate) -> Result<Vec<DrawCommand>, TestError> {
    let mut tree = SceneTree::default();
    tree.show(update)?;
    Ok(commands(&tree))
}

pub(crate) fn commands(tree: &SceneTree) -> Vec<DrawCommand> {
    tree.at(Duration::ZERO).leaf_commands().cloned().collect()
}
