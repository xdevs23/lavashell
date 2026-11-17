// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::{Error, Painter, StreamFrames};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::{
    Change, Content, Node, Packet, SceneNodeId, SceneTree, SceneUpdate,
};
use lavashell_core::surface::SurfaceId;
use lavashell_test_fixtures::WithoutFailures;
use skia_safe::Canvas;
use std::time::Duration;

pub(crate) const TEST_SURFACE: SurfaceId = SurfaceId::numbered(0);
pub(crate) const BACKGROUND_STRIPE_COLORS: [Color; 2] =
    [Color::rgb(0x0011_2255), Color::rgb(0x00fe_ddaa)];

pub(crate) fn insert(id: u64, parent: Option<u64>, index: usize, node: Node) -> Change {
    Change::Insert {
        id: SceneNodeId(id),
        parent: parent.map(SceneNodeId),
        index,
        node,
    }
}

pub(crate) fn leaves(
    packets: impl IntoIterator<Item = Packet>,
) -> Result<SceneSnapshot, TestError> {
    let changes = (1_u64..)
        .zip(packets.into_iter().enumerate())
        .map(|(id, (index, packet))| insert(id, None, index, Node::Leaf(Content::Packet(packet))))
        .collect();
    let mut tree = SceneTree::default();
    tree.show(SceneUpdate { changes })?;
    Ok(tree.snapshot())
}

pub(crate) fn paint(
    painter: &mut Painter,
    canvas: &Canvas,
    scene: &SceneSnapshot,
    density: Density,
) -> Result<(), Error> {
    painter.paint_scene(canvas, TEST_SURFACE, scene.at(Duration::ZERO), density)
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
        let tree = tree.at(Duration::ZERO);
        self.paint_scene_with_streams(canvas, TEST_SURFACE, tree, density, streams)?;
        Ok(())
    }
}

pub(crate) fn paint_damage(
    painter: &mut Painter,
    surface: SurfaceId,
    canvas: &Canvas,
    scene: &SceneSnapshot,
    dp_to_output: Affine,
    damage: Option<Rect>,
    clear: skia_safe::Color,
) -> Result<(), TestError> {
    let frame = crate::SurfaceFrame {
        surface,
        tree: scene.at(Duration::ZERO),
        dp_to_output,
        damage,
        clear,
        scene: None,
        shape: None,
        streams: &StreamFrames::default(),
    };
    painter.paint_surface(canvas, frame)?;
    Ok(())
}

pub(crate) fn commands(scene: &SceneSnapshot) -> Vec<DrawCommand> {
    scene.at(Duration::ZERO).leaf_commands().cloned().collect()
}

pub(crate) fn framed_commands(
    view: &mut View,
    viewport: Viewport,
    painter: &mut Painter,
) -> Result<Vec<DrawCommand>, TestError> {
    let mut builder = SceneBuilder::default();
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
    Ok(commands(&builder.snapshot()))
}
