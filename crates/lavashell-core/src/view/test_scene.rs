// SPDX-License-Identifier: GPL-3.0-only

use crate::frame::FrameTime;
use crate::geometry::Viewport;
use crate::scene::tree::{
    ApplyError, Change, GroupValues, NamespaceId, NodeKey, PaintVisitor, PlacedLeaf, SceneSnapshot,
};
use crate::scene::{Affine, ChangeSink, DrawCommand, SaveStackChange, SceneBuilder, SinkId};
use crate::test_error::{Required, TestError, WithoutFailures};
use crate::view::{TextMeasurer, View};
use std::time::Duration;

pub(crate) fn advance_with(
    scene: &mut Scene,
    view: &mut View,
    viewport: Viewport,
    text: &mut impl TextMeasurer,
) -> Result<(), TestError> {
    view.frame_into(viewport, text, scene).without_failures()
}

pub(crate) fn advance_with_timing(
    scene: &mut Scene,
    view: &mut View,
    viewport: Viewport,
    text: &mut impl TextMeasurer,
    timing: FrameTime,
) -> Result<(), TestError> {
    (view.frame_into_at(viewport, text, timing, scene)).without_failures()
}

pub(crate) fn replay(commands: &[DrawCommand]) -> Result<Vec<(Affine, &DrawCommand)>, TestError> {
    let (mut transform, mut saved, mut replayed) = (Affine::IDENTITY, Vec::new(), Vec::new());
    for command in commands {
        replayed.push((transform, command));
        match command.save_stack_change() {
            SaveStackChange::Push => saved.push(transform),
            SaveStackChange::Pop => transform = saved.pop().required("a saved transform")?,
            SaveStackChange::Unchanged => {}
        }
        if let DrawCommand::Transform(inner) = command {
            transform = transform.then(*inner);
        }
    }
    Ok(replayed)
}

pub(crate) fn save_depths(commands: &[DrawCommand]) -> impl Iterator<Item = i32> {
    commands.iter().scan(0, |depth, command| {
        *depth += match command.save_stack_change() {
            SaveStackChange::Push => 1,
            SaveStackChange::Pop => -1,
            SaveStackChange::Unchanged => 0,
        };
        Some(*depth)
    })
}

#[derive(Default)]
pub(crate) struct Scene {
    builder: SceneBuilder,
    open: Vec<Change>,
    ended: Vec<Change>,
    now: Duration,
}

impl Scene {
    pub fn changes(&self) -> &[Change] {
        &self.ended
    }

    pub fn snapshot(&self) -> SceneSnapshot {
        self.builder.snapshot()
    }

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
