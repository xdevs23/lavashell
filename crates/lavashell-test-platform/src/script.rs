// SPDX-License-Identifier: GPL-3.0-only
use crate::platform::ScriptedHost;
use crate::{Error, FrameInterval, PacedLinks, RecordedFrames};
use lavashell_core::units::Density;
use lavashell_platform::{KeyboardDevice, PlatformEvent};
use std::sync::mpsc;
use std::time::Duration;

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
    RecordPaced(PacedLinks),
}

impl HostScript {
    #[must_use]
    pub fn new(density: Density) -> Self {
        Self {
            density,
            steps: Vec::new(),
        }
    }

    #[must_use]
    pub fn report(mut self, event: impl Into<PlatformEvent>) -> Self {
        self.steps.push(Step::Report(event.into()));
        self
    }

    #[must_use]
    pub fn focus_keyboard(mut self, keyboard: KeyboardDevice, app_id: impl Into<String>) -> Self {
        self.steps.push(Step::FocusKeyboard {
            keyboard,
            app_id: app_id.into(),
        });
        self
    }

    #[must_use]
    pub fn play_until(mut self, time: Duration) -> Self {
        self.steps.push(Step::PlayUntil(time));
        self
    }

    #[must_use]
    pub fn record_until_rest(mut self, every: FrameInterval) -> Self {
        self.steps.push(Step::RecordUntilRest(every));
        self
    }

    #[must_use]
    pub fn record_paced(mut self, links: PacedLinks) -> Self {
        self.steps.push(Step::RecordPaced(links));
        self
    }

    pub fn serve<T>(
        self,
        run: impl FnOnce(ScriptedHost) -> T,
    ) -> (T, Result<RecordedFrames, Error>) {
        let (recorded, frames) = mpsc::channel();
        let ran = run(ScriptedHost {
            script: self,
            recorded,
        });
        (ran, frames.recv().map_err(Error::NotServed))
    }
}

impl Step {
    pub(crate) fn records(&self) -> bool {
        matches!(self, Self::RecordUntilRest(_) | Self::RecordPaced(_))
    }
}
