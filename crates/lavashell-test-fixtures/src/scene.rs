// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::scene::tree::{ApplyError, Change, SceneTree};
use lavashell_core::scene::{ChangeSink, SceneBuilder, SinkId};

/// Applies `changes` to the root namespace of `tree` one at a time, in order.
///
/// # Errors
/// Fails at the first change that does not fit the tree. The earlier changes are applied.
pub fn show_all(
    tree: &mut SceneTree,
    changes: impl IntoIterator<Item = Change>,
) -> Result<(), ApplyError> {
    (changes.into_iter()).try_for_each(|change| tree.show_change(change))
}

/// A change sink that builds the scene from every frame of a view and records the changes of the last frame.
#[derive(Default)]
pub struct RecordedScene {
    builder: SceneBuilder,
    /// The changes received since the last end of a frame.
    open: Vec<Change>,
    /// The changes of the last frame that ended.
    ended: Vec<Change>,
}

impl RecordedScene {
    /// The changes of the last frame that ended.
    #[must_use]
    pub fn changes(&self) -> &[Change] {
        &self.ended
    }
}

impl ChangeSink for RecordedScene {
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
