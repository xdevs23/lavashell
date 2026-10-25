// SPDX-License-Identifier: GPL-3.0-only
//! A scene tree for tests that applies every update returned by a view. Tests read the scene in paint order, with
//! every animated slot evaluated at one chosen time.

use crate::frame::FrameTime;
use crate::geometry::Viewport;
use crate::scene::tree::{
    ApplyError, GroupValues, NamespaceId, NodeKey, PaintVisitor, PlacedLeaf, SceneTree, SceneUpdate,
};
use crate::scene::{Affine, DrawCommand};
use crate::test_error::TestError;
use crate::view::{TextMeasurer, View};
use std::time::Duration;

/// Paints a frame of `view` and shows its update in `scene`.
pub(crate) fn advance_with(
    scene: &mut Scene,
    view: &mut View,
    viewport: Viewport,
    text: &mut impl TextMeasurer,
) -> Result<(), TestError> {
    scene.show(view.frame(viewport, text)?)?;
    Ok(())
}

/// Paints a frame of `view` at `timing` and shows its update in `scene`.
pub(crate) fn advance_with_timing(
    scene: &mut Scene,
    view: &mut View,
    viewport: Viewport,
    text: &mut impl TextMeasurer,
    timing: FrameTime,
) -> Result<(), TestError> {
    scene.show(view.frame_at(viewport, text, timing)?)?;
    Ok(())
}

#[derive(Default)]
pub(crate) struct Scene {
    tree: SceneTree,
    now: Duration,
}

impl Scene {
    pub fn show(&mut self, update: SceneUpdate) -> Result<(), ApplyError> {
        self.tree.show(update)
    }

    /// Sets the time at which later reads of this scene evaluate the animated slots: `ms` milliseconds after the start
    /// of the frame times.
    pub fn at(&mut self, ms: u64) -> &mut Self {
        self.now = Duration::from_millis(ms);
        self
    }

    pub fn commands(&self) -> Vec<DrawCommand> {
        self.tree.at(self.now).commands()
    }

    pub fn leaf_commands(&self) -> Vec<DrawCommand> {
        self.tree.at(self.now).leaf_commands().cloned().collect()
    }

    pub fn groups(&self) -> Vec<GroupValues> {
        let mut groups = Groups(Vec::new());
        self.tree.at(self.now).paint_order().walk(&mut groups);
        groups.0
    }
}

struct Groups(Vec<GroupValues>);

impl PaintVisitor<'_> for Groups {
    type Saved = ();

    fn group(&mut self, _: NodeKey, values: GroupValues, _: Affine) {
        self.0.push(values);
    }

    fn graft(&mut self, _: NodeKey, _: NamespaceId) {}

    fn leaf(&mut self, _: PlacedLeaf<'_>) {}

    fn exit(&mut self, (): ()) {}
}
