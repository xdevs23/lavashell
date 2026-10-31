// SPDX-License-Identifier: GPL-3.0-only
//! A scene for tests that builds the scene from every frame of a view and records the changes of the last frame.
//! Tests read the scene in paint order, with every animated slot evaluated at one chosen time.

use crate::frame::FrameTime;
use crate::geometry::Viewport;
use crate::scene::tree::{
    ApplyError, Change, GroupValues, NamespaceId, NodeKey, PaintVisitor, PlacedLeaf, SceneSnapshot,
};
use crate::scene::{Affine, ChangeSink, DrawCommand, SceneBuilder, SinkId};
use crate::test_error::{TestError, WithoutFailures};
use crate::view::{TextMeasurer, View};
use std::time::Duration;

/// Paints a frame of `view` into `scene`.
pub(crate) fn advance_with(
    scene: &mut Scene,
    view: &mut View,
    viewport: Viewport,
    text: &mut impl TextMeasurer,
) -> Result<(), TestError> {
    view.frame_into(viewport, text, scene).without_failures()
}

/// Paints a frame of `view` at `timing` into `scene`.
pub(crate) fn advance_with_timing(
    scene: &mut Scene,
    view: &mut View,
    viewport: Viewport,
    text: &mut impl TextMeasurer,
    timing: FrameTime,
) -> Result<(), TestError> {
    (view.frame_into_at(viewport, text, timing, scene)).without_failures()
}

#[derive(Default)]
pub(crate) struct Scene {
    builder: SceneBuilder,
    /// The changes received since the last end of a frame.
    open: Vec<Change>,
    /// The changes of the last frame that ended.
    ended: Vec<Change>,
    now: Duration,
}

impl Scene {
    /// The changes of the last frame that ended.
    pub fn changes(&self) -> &[Change] {
        &self.ended
    }

    /// The scene as it was when the last frame ended.
    pub fn snapshot(&self) -> SceneSnapshot {
        self.builder.snapshot()
    }

    /// Sets the time at which later reads of this scene evaluate the animated slots: `ms` milliseconds after the start
    /// of the frame times.
    pub fn at(&mut self, ms: u64) -> &mut Self {
        self.now = Duration::from_millis(ms);
        self
    }

    pub fn commands(&self) -> Vec<DrawCommand> {
        self.snapshot().at(self.now).commands()
    }

    pub fn leaf_commands(&self) -> Vec<DrawCommand> {
        let snapshot = self.snapshot();
        snapshot.at(self.now).leaf_commands().cloned().collect()
    }

    pub fn groups(&self) -> Vec<GroupValues> {
        let mut groups = Groups(Vec::new());
        let snapshot = self.snapshot();
        snapshot.at(self.now).paint_order().walk(&mut groups);
        groups.0
    }
}

impl ChangeSink for Scene {
    type Error = ApplyError;

    fn id(&self) -> SinkId {
        self.builder.id()
    }

    fn change(&mut self, change: Change) -> Result<(), ApplyError> {
        self.open.push(change.clone());
        self.builder.change(change)
    }

    fn end_frame(&mut self) -> Result<(), ApplyError> {
        self.builder.end_frame()?;
        self.ended = std::mem::take(&mut self.open);
        Ok(())
    }

    fn restart(&mut self) -> Result<(), ApplyError> {
        self.open.clear();
        self.builder.restart()
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
