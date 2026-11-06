// SPDX-License-Identifier: GPL-3.0-only
use crate::scheduler::FrameScheduler;
use crate::{Error, Redraw, SchedulerInput, SchedulerOutput};
use lavashell_actor::{Actor, Closed, Effect, Flow, Handle, Merge, Sender, mailbox, spawn};
use lavashell_core::surface::SurfaceId;
use std::convert::Infallible;
use std::time::Instant;

/// Receives each output of the frame scheduler. The scheduler stops once it fails.
pub type SchedulerObserver = Box<dyn FnMut(SchedulerOutput) -> Result<(), Closed> + Send>;

/// Runs the frame scheduler on its own thread. While the scheduler is busy, an input replaces the earlier input of the
/// same kind for the same surface, unless an input that builds on the earlier inputs came in between.
pub struct SchedulerThread {
    inputs: Sender<SchedulerItem>,
    actor: Option<Handle<SchedulerActor>>,
}

/// An item in the mailbox of the scheduler. A view report starts a frame from the pacing state set by the inputs before
/// it, so it stays in order with them.
enum SchedulerItem {
    Input(SchedulerInput),
    Stop,
}

/// The kind of input whose newest value replaces the earlier ones for a surface. These inputs only set pacing state,
/// so their order among each other does not change what the scheduler does.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum InputKey {
    Timing(SurfaceId),
    Published(SurfaceId),
    OutputRefresh(SurfaceId),
}

/// The frame scheduler with the clock of the app, which sends each output to the observer.
struct SchedulerActor {
    scheduler: FrameScheduler,
    /// The start of the clock of the app, from which every time of the scheduler is measured.
    start: Instant,
    observer: SchedulerObserver,
}

impl SchedulerThread {
    /// Starts the frame scheduler for `redraw`, which measures time from `start` and passes each output to `observer`.
    ///
    /// # Errors
    ///
    /// Fails when the thread of the scheduler cannot start.
    pub fn start(
        redraw: Redraw,
        start: Instant,
        observer: SchedulerObserver,
    ) -> Result<Self, Error> {
        let (inputs, inbox) = mailbox();
        let actor = spawn("frame scheduler", inbox, move || {
            Ok(SchedulerActor {
                scheduler: FrameScheduler::new(redraw),
                start,
                observer,
            })
        })
        .map_err(Error::StartScheduler)?;
        Ok(Self {
            inputs,
            actor: Some(actor),
        })
    }

    /// # Errors
    ///
    /// Fails when the scheduler is gone.
    pub fn send(&self, input: SchedulerInput) -> Result<(), Closed> {
        self.inputs.push(SchedulerItem::Input(input))
    }

    /// Stops the scheduler after the inputs sent before, and waits until the scheduler thread ends.
    ///
    /// # Errors
    ///
    /// Fails when the thread of the scheduler panicked.
    pub fn stop(&mut self) -> Result<(), Error> {
        let Some(actor) = self.actor.take() else {
            return Ok(());
        };
        let _scheduler_gone = self.inputs.push(SchedulerItem::Stop);
        actor.join().map_err(Error::RunScheduler)
    }
}

impl Drop for SchedulerThread {
    fn drop(&mut self) {
        let _failure = self.stop();
    }
}

impl Merge for SchedulerItem {
    type Key = InputKey;

    fn effect(&self) -> Effect<InputKey> {
        let Self::Input(input) = self else {
            return Effect::BuildsOn;
        };
        match *input {
            SchedulerInput::Timing(event) => event.effect().map(InputKey::Timing),
            SchedulerInput::Published { surface, .. } => {
                Effect::Overwrites(InputKey::Published(surface))
            }
            SchedulerInput::OutputRefresh { surface, .. } => {
                Effect::Overwrites(InputKey::OutputRefresh(surface))
            }
            SchedulerInput::Wants { .. }
            | SchedulerInput::Opened(_)
            | SchedulerInput::Built { .. }
            | SchedulerInput::Rendered { .. }
            | SchedulerInput::Closed(_) => Effect::BuildsOn,
        }
    }
}

impl Actor for SchedulerActor {
    type Input = SchedulerItem;
    type Error = Infallible;

    /// Passes the outputs that follow from the input to the observer. Stops once the observer is gone.
    fn handle(&mut self, item: SchedulerItem) -> Result<Flow, Infallible> {
        let SchedulerItem::Input(input) = item else {
            return Ok(Flow::Stop);
        };
        let now = self.start.elapsed();
        let mut outputs = Vec::new();
        self.scheduler.handle(now, input, &mut outputs);
        let observed = outputs.into_iter().try_for_each(&mut self.observer);
        Ok(observed.map_or(Flow::Stop, |()| Flow::Continue))
    }
}

#[cfg(test)]
mod tests;
