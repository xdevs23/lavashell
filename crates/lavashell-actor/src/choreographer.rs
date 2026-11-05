// SPDX-License-Identifier: GPL-3.0-only
use crate::{Effect, Merge};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::mem;

/// Merges the frames sent by a fast producer until a slower consumer takes them, and hands them out only as whole
/// frames. An overwrite in the open frame replaces the earlier write of its key in that frame that no fence of the
/// key follows. When the frame ends, each of its overwrites replaces the waiting write of its key that no fence of the
/// key follows, at the place of that write. Every other item is appended.
pub struct Choreographer<T: Merge> {
    open: Pending<T>,
    waiting: Option<Pending<T>>,
    restart: bool,
}

/// The frames merged since the last take. After a restart, the items start from nothing and do not build on the
/// items handed out before.
pub struct Frame<T> {
    pub restart: bool,
    pub items: Vec<T>,
}

impl<T: Merge> Default for Choreographer<T> {
    fn default() -> Self {
        Self {
            open: Pending::default(),
            waiting: None,
            restart: false,
        }
    }
}

impl<T: Merge> Choreographer<T> {
    pub fn push(&mut self, item: T) {
        self.open.push(item);
    }

    /// Ends the open frame and merges it into the frames waiting to be taken.
    pub fn end_frame(&mut self) {
        let ended = mem::take(&mut self.open);
        if let Some(waiting) = &mut self.waiting {
            waiting.append(ended);
        } else {
            self.waiting = Some(ended);
        }
    }

    /// Drops the waiting frames and the open frame, so that the next taken frame starts from nothing.
    pub fn restart(&mut self) {
        *self = Self {
            restart: true,
            ..Self::default()
        };
    }

    /// The waiting frames merged into one, or `None` when no frame ended since the last take.
    pub fn take(&mut self) -> Option<Frame<T>> {
        let waiting = self.waiting.take()?;
        Some(Frame {
            restart: mem::take(&mut self.restart),
            items: waiting.items,
        })
    }
}

/// Items in the order in which each key was first written since its last fence, with every other item at the place
/// where it was pushed. `index` holds the place of each key that a later overwrite replaces.
struct Pending<T: Merge> {
    items: Vec<T>,
    index: HashMap<T::Key, usize>,
}

impl<T: Merge> Default for Pending<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<T: Merge> Pending<T> {
    fn push(&mut self, item: T) {
        match item.effect() {
            Effect::BuildsOn => {}
            Effect::Overwrites(key) => match self.index.entry(key) {
                Entry::Occupied(slot) => {
                    self.items[*slot.get()].absorb(item);
                    return;
                }
                Entry::Vacant(slot) => {
                    let _place = slot.insert(self.items.len());
                }
            },
            Effect::Fences(key) => {
                let _fenced = self.index.remove(&key);
            }
        }
        self.items.push(item);
    }

    fn append(&mut self, later: Self) {
        for item in later.items {
            self.push(item);
        }
    }
}

#[cfg(test)]
mod tests;
