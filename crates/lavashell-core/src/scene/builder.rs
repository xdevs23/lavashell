// SPDX-License-Identifier: GPL-3.0-only
use super::tree::{ApplyError, Change, SceneSnapshot, SceneTree};
use crate::unique_id::UniqueId;

pub trait ChangeSink {
    type Error: std::error::Error + 'static;

    fn id(&self) -> SinkId;

    fn change(&mut self, change: Change) -> Result<(), Self::Error>;

    fn end_frame(&mut self) -> Result<(), Self::Error>;

    fn restart(&mut self) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SinkId(UniqueId);

impl Default for SinkId {
    fn default() -> Self {
        Self(UniqueId::next())
    }
}

pub struct SceneBuilder {
    id: SinkId,
    tree: SceneTree,
    published: SceneSnapshot,
}

impl Default for SceneBuilder {
    fn default() -> Self {
        let tree = SceneTree::default();
        Self {
            id: SinkId::default(),
            published: tree.snapshot(),
            tree,
        }
    }
}

impl SceneBuilder {
    #[must_use]
    pub fn snapshot(&self) -> SceneSnapshot {
        self.published.clone()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.published.is_empty()
    }

    #[must_use]
    pub fn size(&self) -> usize {
        self.tree.len()
    }
}

impl ChangeSink for SceneBuilder {
    type Error = ApplyError;

    fn id(&self) -> SinkId {
        self.id
    }

    fn change(&mut self, change: Change) -> Result<(), ApplyError> {
        self.tree.show_change(change)
    }

    fn end_frame(&mut self) -> Result<(), ApplyError> {
        self.published = self.tree.snapshot();
        Ok(())
    }

    fn restart(&mut self) -> Result<(), ApplyError> {
        self.tree = SceneTree::default();
        Ok(())
    }
}
