// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::scene::tree::{ApplyError, Change};
use lavashell_core::scene::{ChangeSink, SceneBuilder, SinkId};

#[derive(Default)]
pub struct RecordedScene {
    builder: SceneBuilder,
    open: Vec<Change>,
    ended: Vec<Change>,
}

impl RecordedScene {
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
