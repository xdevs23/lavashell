// SPDX-License-Identifier: GPL-3.0-only
use super::tree::{ApplyError, Change, SceneSnapshot, SceneTree};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Receives the changes sent by a view for each frame composed by the view, in order. Each frame ends with `end_frame`.
/// After any error, the view restarts the sink. After `restart`, the next changes build the whole scene from an empty
/// scene.
pub trait ChangeSink {
    type Error: std::error::Error + 'static;

    /// The id that tells this sink apart from every other sink. A view sends the whole scene to a sink whose id
    /// differs from the id of the sink that received the last frame.
    fn id(&self) -> SinkId;

    /// # Errors
    ///
    /// Fails when the sink cannot apply `change`.
    fn change(&mut self, change: Change) -> Result<(), Self::Error>;

    /// Ends the frame made of the changes received since the last end.
    ///
    /// # Errors
    ///
    /// Fails when the sink cannot end the frame.
    fn end_frame(&mut self) -> Result<(), Self::Error>;

    /// Drops the scene built from the changes received so far.
    ///
    /// # Errors
    ///
    /// Fails when the sink cannot drop the scene.
    fn restart(&mut self) -> Result<(), Self::Error>;
}

/// Tells one change sink apart from every other change sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SinkId(u64);

impl Default for SinkId {
    /// An id that differs from every id made before in the process.
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// Applies the changes sent by a view to a scene tree and returns a snapshot taken when the last frame ended. A change
/// that does not fit the tree fails. The view then restarts the builder and sends the whole scene again.
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
    /// The scene as it was when the last frame ended. The snapshot changes only when a frame ends.
    #[must_use]
    pub fn snapshot(&self) -> SceneSnapshot {
        self.published.clone()
    }

    /// Whether the snapshot contains no node, as before the end of the first frame.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.published.is_empty()
    }

    /// The earliest time, at or after `now`, at which an animated group of the snapshot changes.
    #[must_use]
    pub fn next_change(&self, now: Duration) -> Option<Duration> {
        self.published.next_change(now)
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
