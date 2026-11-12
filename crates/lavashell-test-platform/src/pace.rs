// SPDX-License-Identifier: GPL-3.0-only
use crate::FrameInterval;
use lavashell_actor::Latest;
use std::sync::Arc;
use std::time::Duration;

/// The bridge through which steps that play on the views of an app tell the host of a test platform when to move its
/// clock. The steps publish on the thread of the views, and the host reads the newest publication once the actors
/// have nothing left to do.
#[derive(Clone, Default)]
pub struct Pace(Arc<Latest<StepsPace>>);

/// What the steps that play on the views of an app wait for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepsPace {
    /// The next step waits for a frame: at `due` or later when it has a due time, and each `every` while it samples a
    /// motion.
    Waiting {
        due: Option<Duration>,
        every: Option<FrameInterval>,
    },
    /// An action of the steps waits for the next frame of its view.
    Queued,
    /// Every step ran, or a step failed.
    Ended,
}

impl Pace {
    pub fn publish(&self, steps: StepsPace) {
        self.0.publish(steps);
    }

    pub(crate) fn read(&self) -> Option<StepsPace> {
        self.0.read().as_deref().copied()
    }
}
