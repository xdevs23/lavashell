// SPDX-License-Identifier: GPL-3.0-only
use crate::platform::{Host, ScopedHost};
use crate::{Error, FrameInterval, Pace, RecordedFrames};
use lavashell_core::units::Density;
use lavashell_platform::{KeyboardDevice, PlatformEvent};
use std::sync::mpsc;
use std::time::Duration;

/// What the host of a [`TestPlatform`](crate::TestPlatform) does during a run, step by step, on a virtual clock that
/// starts at zero. Before each step, the actors of the run handle everything that is due. After the last step, the
/// host closes every surface, which ends the run.
pub struct HostScript {
    pub(crate) density: Density,
    pub(crate) steps: Vec<Step>,
}

pub(crate) enum Step {
    Report(PlatformEvent),
    FocusKeyboard {
        keyboard: KeyboardDevice,
        app_id: String,
    },
    PlayUntil(Duration),
    RecordUntilRest(FrameInterval),
    RecordPaced(Pace),
}

impl HostScript {
    /// A script for a host whose output has `density`, which the host reports for each surface it creates.
    #[must_use]
    pub fn new(density: Density) -> Self {
        Self {
            density,
            steps: Vec::new(),
        }
    }

    /// Reports `event` to the app, such as an input event.
    #[must_use]
    pub fn report(mut self, event: impl Into<PlatformEvent>) -> Self {
        self.steps.push(Step::Report(event.into()));
        self
    }

    /// Gives the focus of `keyboard` to the open window with `app_id` that was created first.
    #[must_use]
    pub fn focus_keyboard(mut self, keyboard: KeyboardDevice, app_id: impl Into<String>) -> Self {
        self.steps.push(Step::FocusKeyboard {
            keyboard,
            app_id: app_id.into(),
        });
        self
    }

    /// Advances the clock to `time`, stopping at each time before it for which the actors wait.
    #[must_use]
    pub fn play_until(mut self, time: Duration) -> Self {
        self.steps.push(Step::PlayUntil(time));
        self
    }

    /// Records the newest frame of each window now, and again each time the clock advanced by `every`, until the
    /// run is at rest as [`is_at_rest`](crate::is_at_rest) describes.
    #[must_use]
    pub fn record_until_rest(mut self, every: FrameInterval) -> Self {
        self.steps.push(Step::RecordUntilRest(every));
        self
    }

    /// Plays the run at the pace of the steps that publish to `pace`. Once the actors have nothing left to do, the
    /// clock moves to the earliest of their next deadline, the due time of the steps and, while the steps sample a
    /// motion, the next sample time. While an action of the steps waits for its frame, the frames shown are presented
    /// without moving the clock. The events that the steps hand to `pace` are reported to the app as soon as the actors
    /// have nothing left to do. The step ends once the steps end, once the runtime ends, or once the run is at rest as
    /// [`is_at_rest`](crate::is_at_rest) describes and the steps do not wait for a later due time. When nothing
    /// publishes to `pace`, the step plays the deadlines of the actors until the run is at rest.
    ///
    /// The step records only the newest frame of each window, when it is new, once the actors have nothing left to do
    /// after the steps asked `pace` to record, and once more when the step ends. The record at the end also contains
    /// the newest frame of each window destroyed during the step.
    #[must_use]
    pub fn record_paced(mut self, pace: Pace) -> Self {
        self.steps.push(Step::RecordPaced(pace));
        self
    }

    /// Runs `run` with this script as the host of a test platform that connects on the calling thread, and returns
    /// what `run` returned with the frames recorded by the script.
    ///
    /// # Errors
    ///
    /// The frames are [`Error::NotServed`] when no test platform connected inside `run`.
    pub fn serve<T>(self, run: impl FnOnce() -> T) -> (T, Result<RecordedFrames, Error>) {
        let (recorded, frames) = mpsc::channel();
        let ran = {
            let _host_until_run_returns = ScopedHost::set(Host {
                script: self,
                recorded,
            });
            run()
        };
        (ran, frames.recv().map_err(Error::NotServed))
    }
}

impl Step {
    pub(crate) fn records(&self) -> bool {
        matches!(self, Self::RecordUntilRest(_) | Self::RecordPaced(_))
    }
}
