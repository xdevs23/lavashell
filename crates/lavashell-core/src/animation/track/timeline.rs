// SPDX-License-Identifier: GPL-3.0-only
//! Pausing, resuming and seeking along the timeline of a motion.

use super::{Animation, Duration};
use crate::animation::Animatable;

impl<T: Animatable> Animation<T> {
    pub fn pause(&self) {
        self.0.synchronize();
        let mut t = self.0.track.borrow_mut();
        let pauses = t.program.is_some() && !t.finished && t.timing.paused.is_none();
        if pauses {
            t.timing.paused = Some(t.now);
        }
        drop(t);
        self.0.publish_status();
        if pauses {
            self.0.revise();
        }
    }

    pub fn resume(&self) {
        self.0.synchronize();
        let mut t = self.0.track.borrow_mut();
        let paused = t.timing.paused.take();
        if let Some(paused) = paused {
            let duration = t.now.saturating_sub(paused);
            t.timing.start += duration;
        }
        drop(t);
        self.0.publish_status();
        if paused.is_some() {
            self.0.revise();
        }
    }

    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.0.track.borrow().elapsed()
    }

    /// Moves the animation to the elapsed time `elapsed` of its program and samples the program exactly at that
    /// time. A paused animation remains paused, and the shared clock epoch does not change.
    pub fn seek(&self, elapsed: Duration) {
        self.0.synchronize();
        let mut t = self.0.track.borrow_mut();
        if t.program.is_none() {
            return;
        }
        t.timing.start = t.timing.paused.unwrap_or(t.now);
        t.timing.offset = elapsed;
        t.finished = false;
        t.sample();
        self.0.publish(t);
        self.0.revise();
    }
}
