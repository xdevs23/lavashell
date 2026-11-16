// SPDX-License-Identifier: GPL-3.0-only
use std::time::Duration;

#[derive(Default)]
pub struct RunEnd {
    ended: bool,
    exit_after: Option<Duration>,
    exit_when: Option<Box<dyn FnMut() -> bool>>,
}

impl RunEnd {
    pub fn exit_after(&mut self, duration: Duration) {
        self.exit_after = Some(duration);
    }

    pub fn exit_when(&mut self, predicate: impl FnMut() -> bool + 'static) {
        self.exit_when = Some(Box::new(predicate));
    }

    pub fn end(&mut self) {
        self.ended = true;
    }

    pub fn finished(&mut self, elapsed: Duration, anything_open: bool) -> bool {
        self.ended
            || !anything_open
            || self.exit_after.is_some_and(|limit| elapsed >= limit)
            || self.exit_when.as_mut().is_some_and(|predicate| predicate())
    }

    #[must_use]
    pub fn exit_in(&self, elapsed: Duration) -> Option<Duration> {
        (self.exit_after).map(|limit| limit.saturating_sub(elapsed))
    }
}

#[cfg(test)]
mod tests;
