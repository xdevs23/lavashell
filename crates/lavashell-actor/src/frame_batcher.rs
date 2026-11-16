// SPDX-License-Identifier: GPL-3.0-only
use crate::Merge;
use crate::pending::{Fence, Pending};
use std::mem;

pub struct FrameBatcher<T: Merge, E> {
    open: Pending<T>,
    open_restarts: bool,
    waiting: Option<Waiting<T, E>>,
}

pub struct Frame<T, E> {
    pub restart: bool,
    pub items: Vec<T>,
    pub end: Option<E>,
}

struct Waiting<T: Merge, E> {
    items: Pending<T>,
    restart: bool,
    end: Option<E>,
}

impl<T: Merge, E> Default for FrameBatcher<T, E> {
    fn default() -> Self {
        Self {
            open: Pending::new(Fence::NamedKey),
            open_restarts: false,
            waiting: None,
        }
    }
}

impl<T: Merge, E> FrameBatcher<T, E> {
    pub fn push(&mut self, item: T) {
        self.open.push(item);
    }

    pub fn end_frame(&mut self, end: Option<E>) {
        let waiting = self.waiting.get_or_insert_with(|| Waiting::ending(None));
        for item in self.open.take() {
            waiting.items.push(item);
        }
        waiting.restart |= mem::take(&mut self.open_restarts);
        waiting.end = end.or(waiting.end.take());
    }

    pub fn restart(&mut self) {
        let end = self.waiting.take().and_then(|waiting| waiting.end);
        *self = Self {
            open_restarts: true,
            waiting: end.map(|end| Waiting::ending(Some(end))),
            ..Self::default()
        };
    }

    #[must_use]
    pub fn waiting_appended_items(&self) -> usize {
        (self.waiting.as_ref()).map_or(0, |waiting| waiting.items.appended_items())
    }

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
