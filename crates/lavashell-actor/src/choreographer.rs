// SPDX-License-Identifier: GPL-3.0-only
use crate::Merge;
use crate::pending::{Fence, Pending};
use std::mem;

/// Merges the frames sent by a fast producer until a slower consumer takes them, and returns only whole frames. An
/// overwrite in the open frame replaces the earlier write with the same key in that frame, unless a fence of the key
/// came after that write. When the frame ends, each overwrite in it replaces the waiting write with the same key at
/// the place of that write, unless a fence of the key came after that write. Every other item is appended. An end
/// value given with a frame replaces the end value of the waiting frames.
pub struct Choreographer<T: Merge, E> {
    open: Pending<T>,
    /// Whether the open frame follows a restart.
    open_restarts: bool,
    waiting: Option<Waiting<T, E>>,
}

/// The frames merged since the previous call of [`Choreographer::take`], and the newest end value given with them.
/// After a restart, the items do not depend on the items returned by earlier calls.
pub struct Frame<T, E> {
    pub restart: bool,
    pub items: Vec<T>,
    pub end: Option<E>,
}

/// The frames that ended since the previous take.
struct Waiting<T: Merge, E> {
    items: Pending<T>,
    restart: bool,
    end: Option<E>,
}

impl<T: Merge, E> Default for Choreographer<T, E> {
    fn default() -> Self {
        Self {
            open: Pending::new(Fence::NamedKey),
            open_restarts: false,
            waiting: None,
        }
    }
}

impl<T: Merge, E> Choreographer<T, E> {
    pub fn push(&mut self, item: T) {
        self.open.push(item);
    }

    /// Ends the open frame and merges it into the frames waiting to be taken. `end`, when given, replaces the end value
    /// of the waiting frames.
    pub fn end_frame(&mut self, end: Option<E>) {
        let waiting = self.waiting.get_or_insert_with(|| Waiting::ending(None));
        for item in self.open.take() {
            waiting.items.push(item);
        }
        waiting.restart |= mem::take(&mut self.open_restarts);
        waiting.end = end.or(waiting.end.take());
    }

    /// Drops the waiting frames and the open frame, so that the next frame that ends does not depend on earlier
    /// frames. The end value of the waiting frames stays, and the next take returns it.
    pub fn restart(&mut self) {
        let end = self.waiting.take().and_then(|waiting| waiting.end);
        *self = Self {
            open_restarts: true,
            waiting: end.map(|end| Waiting::ending(Some(end))),
            ..Self::default()
        };
    }

    /// The number of items in the waiting frames that do not overwrite.
    #[must_use]
    pub fn waiting_builds_on(&self) -> usize {
        (self.waiting.as_ref()).map_or(0, |waiting| waiting.items.builds_on())
    }

    /// The waiting frames merged into one, or `None` when nothing waits since the previous call.
    pub fn take(&mut self) -> Option<Frame<T, E>> {
        let mut waiting = self.waiting.take()?;
        Some(Frame {
            restart: waiting.restart,
            items: waiting.items.take(),
            end: waiting.end,
        })
    }
}

impl<T: Merge, E> Waiting<T, E> {
    /// Returns waiting frames without items, with `end` as the end value.
    fn ending(end: Option<E>) -> Self {
        Self {
            items: Pending::new(Fence::NamedKey),
            restart: false,
            end,
        }
    }
}

#[cfg(test)]
mod tests;
