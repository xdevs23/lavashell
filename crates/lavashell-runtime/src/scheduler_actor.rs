// SPDX-License-Identifier: GPL-3.0-only
use crate::failures::log_failure_to_stop;
use crate::scheduler::{FrameScheduler, Redraw, SchedulerInput, SchedulerOutput};
use lavashell_actor::{
    Actor, Clock, Closed, Flow, Handle, Merge, MergeEffect, Outlet, Sender, mailbox_on, spawn,
};
use lavashell_core::surface::SurfaceId;
use lavashell_platform::TimingEvent;
use std::convert::Infallible;
use std::sync::Arc;

mod error;

pub use error::Error;

pub type SchedulerObserver = Box<dyn FnMut(SchedulerOutput) -> Result<(), Closed> + Send>;

pub(crate) struct SchedulerThread {
    inputs: Sender<SchedulerItem>,
    actor: Option<Handle<SchedulerActor>>,
}

enum SchedulerItem {
    Input(SchedulerInput),
    Stop,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum InputKey {
    Timing(SurfaceId),
    Published(SurfaceId),
    OutputRefresh(SurfaceId),
}

struct SchedulerActor {
    scheduler: FrameScheduler,
    clock: Arc<dyn Clock>,
    observer: SchedulerObserver,
}

impl SchedulerThread {
    pub fn start(
        redraw: Redraw,
        clock: Arc<dyn Clock>,
        observer: SchedulerObserver,
    ) -> Result<Self, Error> {
        let (inputs, inbox) = mailbox_on(clock.as_ref());
        let actor = spawn("frame scheduler", inbox, move || {
            Ok(SchedulerActor {
                scheduler: FrameScheduler::new(redraw),
                clock,
                observer,
            })
        })
        .map_err(Error::StartScheduler)?;
        Ok(Self {
            inputs,
            actor: Some(actor),
        })
    }

    pub(crate) fn timing_outlet(&self) -> Outlet<TimingEvent> {
        (self.inputs).outlet(|event| SchedulerItem::Input(SchedulerInput::Timing(event)))
    }

    pub fn send(&self, input: SchedulerInput) -> Result<(), Closed> {
        self.inputs.push(SchedulerItem::Input(input))
    }

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
        log_failure_to_stop(self.stop());
    }
}

impl Merge for SchedulerItem {
    type Key = InputKey;

    fn effect(&self) -> MergeEffect<InputKey> {
        let Self::Input(input) = self else {
            return MergeEffect::Appends;
        };
        match *input {
            SchedulerInput::Timing(event) => event.effect().map(InputKey::Timing),
            SchedulerInput::Published { surface, .. } => {
                MergeEffect::Overwrites(InputKey::Published(surface))
            }
            SchedulerInput::OutputRefresh { surface, .. } => {
                MergeEffect::Overwrites(InputKey::OutputRefresh(surface))
            }
            SchedulerInput::Wants { .. }
            | SchedulerInput::Opened(_)
            | SchedulerInput::Built { .. }
            | SchedulerInput::StateChanged(_)
            | SchedulerInput::Committed(_)
            | SchedulerInput::Rendered { .. }
            | SchedulerInput::Unrendered { .. }
            | SchedulerInput::Closed(_) => MergeEffect::Appends,
        }
    }
}

impl Actor for SchedulerActor {
    type Input = SchedulerItem;
    type Error = Infallible;

    fn handle(&mut self, item: SchedulerItem) -> Result<Flow, Infallible> {
        let SchedulerItem::Input(input) = item else {
            return Ok(Flow::Stop);
        };
        let now = self.clock.now();
        let mut outputs = Vec::new();
        self.scheduler.handle(now, input, &mut outputs);
        let observed = outputs.into_iter().try_for_each(&mut self.observer);
        Ok(observed.map_or(Flow::Stop, |()| Flow::Continue))
    }
}

#[cfg(test)]
mod tests;
