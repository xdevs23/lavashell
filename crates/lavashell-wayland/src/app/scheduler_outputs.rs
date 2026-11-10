// SPDX-License-Identifier: GPL-3.0-only
use super::{Platform, actor_link};
use crate::Error;
use lavashell_runtime::{SchedulerObserver, SchedulerOutput};
use sctk::reexports::calloop::{EventLoop, LoopHandle, RegistrationToken};
use smithay_client_toolkit as sctk;
use std::time::Duration;

/// Watches the outputs of the frame scheduler in the event loop, and returns the observer that sends each output
/// there.
pub(super) fn install(
    handle: &LoopHandle<'static, Platform>,
    sources: &mut Vec<RegistrationToken>,
) -> Result<SchedulerObserver, Error> {
    let observer = actor_link::watch(
        handle,
        sources,
        Platform::scheduler_output,
        |platform| platform.scheduler_stopped = true,
        Error::WatchScheduler,
    )?;
    Ok(Box::new(observer))
}

impl Platform {
    /// Stores each started frame for the next render and the time at which the views report again, and passes each
    /// report to the frame observer of the app.
    fn scheduler_output(&mut self, output: SchedulerOutput) {
        match output {
            SchedulerOutput::Frame {
                surface,
                kind,
                time,
            } => {
                let _older_frame = self.started_frames.insert(surface, (kind, time));
            }
            SchedulerOutput::Report(report) => {
                if let Some(observer) = &mut self.config.observer {
                    observer(*report);
                }
            }
            SchedulerOutput::WakeAt(wake) => self.scheduler_wake = wake,
        }
    }

    /// Joins the thread of the stopped frame scheduler, and returns its failure, or `SchedulerStopped` when it did not
    /// fail.
    pub(super) fn stopped_scheduler_error(&mut self) -> Error {
        actor_link::stopped(self.scheduler.stop(), Error::SchedulerStopped)
    }

    /// Stops the frame scheduler after the inputs sent before, and handles its remaining outputs, so that the frame
    /// observer of the app receives every report.
    ///
    /// # Errors
    ///
    /// Fails when the scheduler failed or the event loop cannot dispatch the outputs.
    pub(super) fn drain_scheduler(
        &mut self,
        event_loop: &mut EventLoop<'static, Self>,
    ) -> Result<(), Error> {
        self.scheduler.stop().map_err(Error::Runtime)?;
        while !self.scheduler_stopped {
            (event_loop.dispatch(Some(Duration::ZERO), self)).map_err(Error::DispatchEvents)?;
        }
        Ok(())
    }
}
