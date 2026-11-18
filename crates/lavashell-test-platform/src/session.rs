// SPDX-License-Identifier: GPL-3.0-only
use crate::capture::CaptureMode;
use crate::clock::VirtualClock;
use crate::screen::Screen;
use crate::script::Step;
use crate::windows::{WindowRecording, Windows};
use crate::{Error, FrameCapture, FrameInterval, RecordedFrames, is_at_rest};
use lavashell_actor::{Clock, mailbox};
use lavashell_core::frame::PresentationFlags;
use lavashell_core::units::Density;
use lavashell_platform::{
    DeviceEvent, PlatformEvent, PlatformLinks, PlatformRequest, Role, SurfaceEvent, SurfaceRequest,
    TimingEvent,
};
use lavashell_vulkan_renderer::HeadlessBackend;
use std::collections::BTreeSet;
use std::time::Duration;

mod paced;

pub(crate) struct Session {
    clock: VirtualClock,
    links: PlatformLinks<FrameCapture, HeadlessBackend>,
    screen: Screen,
    windows: Windows,
}

enum Stop {
    RunEnded,
    Failed(Error),
}

impl From<Error> for Stop {
    fn from(error: Error) -> Self {
        Self::Failed(error)
    }
}

impl Session {
    pub(crate) fn new(
        clock: VirtualClock,
        links: PlatformLinks<FrameCapture, HeadlessBackend>,
        density: Density,
    ) -> Self {
        let (shown_by_controls, shown) = mailbox();
        Self {
            clock,
            links,
            screen: Screen::new(shown),
            windows: Windows::new(density, shown_by_controls),
        }
    }

    pub(crate) fn into_frames(self) -> RecordedFrames {
        let WindowRecording { app_ids, requests } = self.windows.into_recorded();
        RecordedFrames::new(self.screen.into_recorded(), app_ids, requests)
    }

    pub(crate) fn play(&mut self, steps: Vec<Step>) -> Result<(), Error> {
        match self.play_steps(steps) {
            Ok(()) | Err(Stop::RunEnded) => Ok(()),
            Err(Stop::Failed(error)) => Err(error),
        }
    }

    fn play_steps(&mut self, mut steps: Vec<Step>) -> Result<(), Stop> {
        let recording = steps
            .iter()
            .rposition(Step::records)
            .map_or(0, |last| last + 1);
        let after_recording = steps.split_off(recording);
        for (group, mode) in [
            (steps, CaptureMode::WithPixels),
            (after_recording, CaptureMode::WithoutPixels),
        ] {
            self.windows.capture(&self.links.reports, mode);
            for step in group {
                self.play_step(step)?;
            }
        }
        self.close()
    }

    fn play_step(&mut self, step: Step) -> Result<(), Stop> {
        self.screen.start_step();
        match step {
            Step::Report(event) => self.report_settled(|_| Ok(event)),
            Step::FocusKeyboard { keyboard, app_id } => self.report_settled(|session| {
                let surface = Some(session.windows.find(&app_id)?);
                Ok(DeviceEvent::KeyboardFocus { keyboard, surface }.into())
            }),
            Step::PlayUntil(time) => self.play_until(time).map(drop),
            Step::RecordUntilRest(every) => self.record_until_rest(every),
            Step::RecordPaced(links) => self.record_paced(&links),
        }
    }

    fn report_settled(
        &mut self,
        event: impl FnOnce(&Self) -> Result<PlatformEvent, Error>,
    ) -> Result<(), Stop> {
        self.settle()?;
        let event = event(self)?;
        self.links.reports.report(event);
        self.settle()
    }

    fn settle(&mut self) -> Result<(), Stop> {
        self.settle_until_next_deadline().map(drop)
    }

    fn settle_until_next_deadline(&mut self) -> Result<Option<Duration>, Stop> {
        loop {
            let deadline = self.clock.next_deadline_when_idle();
            self.screen.take_shown();
            let Ok(requests) = self.links.requests.take_arrived() else {
                return Err(Stop::RunEnded);
            };
            if requests.is_empty() {
                return Ok(deadline);
            }
            requests
                .into_iter()
                .try_for_each(|request| self.handle(request))?;
        }
    }

    fn play_until(&mut self, time: Duration) -> Result<Option<Duration>, Stop> {
        while let Some(deadline) = self
            .settle_until_next_deadline()?
            .filter(|&wait| wait < time)
        {
            self.advance(deadline)?;
        }
        self.advance(time)?;
        self.settle_until_next_deadline()
    }

    fn advance(&mut self, time: Duration) -> Result<(), Stop> {
        let presented = self.screen.take_unpresented();
        self.clock.advance_to(time);
        self.settle()?;
        for (surface, frame) in presented {
            self.links.reports.time(TimingEvent::DisplayReady(surface));
            self.links.reports.time(TimingEvent::Presented {
                surface,
                frame,
                time: Some(frame.now),
                refresh: None,
                flags: PresentationFlags::default(),
            });
        }
        Ok(())
    }

    fn record_until_rest(&mut self, every: FrameInterval) -> Result<(), Stop> {
        self.settle()?;
        loop {
            self.screen.record_new()?;
            let recorded = self.screen.newest_frames();
            let deadline = self.play_until(self.clock.now() + every.get())?;
            if is_at_rest(self.screen.newest_frames() != recorded, deadline) {
                return Ok(());
            }
        }
    }

    fn close(&mut self) -> Result<(), Stop> {
        self.settle()?;
        let mut closed = BTreeSet::new();
        loop {
            for surface in self.windows.surfaces() {
                if closed.insert(surface) {
                    self.links.reports.report((surface, SurfaceEvent::Closed));
                }
            }
            let Ok(requests) = self.links.requests.take() else {
                return Ok(());
            };
            requests
                .into_iter()
                .try_for_each(|request| self.handle(request))?;
        }
    }

    fn handle(&mut self, request: PlatformRequest) -> Result<(), Error> {
        let reports = &self.links.reports;
        match request {
            PlatformRequest::Surface(SurfaceRequest::Create { surface, spec })
                if matches!(spec.role, Role::Window(_)) =>
            {
                self.windows.create(reports, surface, *spec);
            }
            PlatformRequest::Surface(SurfaceRequest::Destroy(surface)) => {
                self.windows.destroy(reports, surface);
                self.screen.remove(surface);
            }
            PlatformRequest::WindowControl(request) => self.windows.control(reports, request)?,
            PlatformRequest::Surface(SurfaceRequest::Release(_)) | PlatformRequest::Input(_) => {}
            request => return Err(Error::Unsupported(Box::new(request))),
        }
        Ok(())
    }
}
