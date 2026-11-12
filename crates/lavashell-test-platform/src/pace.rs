// SPDX-License-Identifier: GPL-3.0-only
use crate::FrameInterval;
use lavashell_actor::{Latest, Receiver, Sender, mailbox};
use lavashell_platform::PlatformEvent;
use std::sync::Arc;
use std::time::Duration;

/// The bridge through which steps that play on the views of an app tell the host of a test platform when to move its
/// clock, and hand it the events to report to the app. The steps publish on the thread of the views, and the host
/// reads the newest publication once the actors have nothing left to do.
#[derive(Clone)]
pub struct Pace {
    steps: Arc<Latest<StepsPace>>,
    input: Sender<PlatformEvent>,
    reported: Arc<Receiver<PlatformEvent>>,
}

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

impl Default for Pace {
    fn default() -> Self {
        let (input, reported) = mailbox();
        Self {
            steps: Arc::default(),
            input,
            reported: Arc::new(reported),
        }
    }
}

impl Pace {
    pub fn publish(&self, steps: StepsPace) {
        self.steps.publish(steps);
    }

    /// Hands `event` to the host, which reports it to the app before it presents the shown frames again.
    pub fn report(&self, event: impl Into<PlatformEvent>) {
        let _open_while_the_pace_has_a_sender = self.input.push(event.into());
    }

    pub(crate) fn read(&self) -> Option<StepsPace> {
        self.steps.read().as_deref().copied()
    }

    /// The events handed to the host since the last call, in the order of their report.
    pub(crate) fn take_input(&self) -> Vec<PlatformEvent> {
        (self.reported.take_arrived().into_iter().flatten()).collect()
    }
}
