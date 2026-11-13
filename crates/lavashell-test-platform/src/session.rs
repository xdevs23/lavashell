// SPDX-License-Identifier: GPL-3.0-only
use crate::capture::{CaptureMode, ShownFrame};
use crate::clock::VirtualClock;
use crate::script::Step;
use crate::{Error, FrameCapture, FrameInterval, RecordedFrame, RecordedFrames, is_at_rest};
use lavashell_actor::{Clock, Receiver, Sender, mailbox};
use lavashell_core::frame::{FrameTime, PresentationFlags};
use lavashell_core::geometry::PixelScale;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_platform::{
    DeviceEvent, PlatformEvent, PlatformLinks, PlatformRequest, Role, SurfaceAttachment,
    SurfaceEvent, SurfaceRequest, SurfaceSpec, TimingEvent,
};
use lavashell_renderer::{NoStreams, PresentationPolicy, SurfaceTiming};
use lavashell_vulkan::HeadlessWindow;
use std::collections::{BTreeMap, BTreeSet};
use std::mem;
use std::sync::Arc;
use std::time::Duration;

/// The host of a test platform while it plays its script.
pub(crate) struct Session {
    clock: VirtualClock,
    links: PlatformLinks<FrameCapture, NoStreams>,
    density: Density,
    /// The capture mode of the controls, also of each control created later.
    capture: CaptureMode,
    shown_by_controls: Sender<ShownFrame>,
    shown: Receiver<ShownFrame>,
    /// The windows created and not destroyed yet, in the order of their creation.
    windows: Vec<(SurfaceId, Role)>,
    /// The newest frame shown by each window.
    screen: BTreeMap<SurfaceId, ShownFrame>,
    /// The frames shown since the last advance, presented by the display with the next advance.
    unpresented: Vec<(SurfaceId, FrameTime)>,
    recorded: Vec<RecordedFrame>,
}

/// How the run stands once its actors have nothing left to do at the current time.
enum Settled {
    /// The actors wait for the time on the clock, or only for an event without one.
    Waiting(Option<Duration>),
    /// The runtime ended its requests, so the run is over.
    Ended,
}

impl Session {
    pub(crate) fn new(
        clock: VirtualClock,
        links: PlatformLinks<FrameCapture, NoStreams>,
        density: Density,
    ) -> Self {
        let (shown_by_controls, shown) = mailbox();
        Self {
            clock,
            links,
            density,
            capture: CaptureMode::WithoutPixels,
            shown_by_controls,
            shown,
            windows: Vec::new(),
            screen: BTreeMap::new(),
            unpresented: Vec::new(),
            recorded: Vec::new(),
        }
    }

    pub(crate) fn into_frames(self) -> RecordedFrames {
        RecordedFrames::new(self.recorded)
    }

    /// Plays `steps` while the run lasts, then closes every window and handles the requests from the runtime until
    /// the runtime ends them. The controls capture pixels until the last step that records, since a frame shown
    /// before that step is recorded if no later frame replaces it.
    ///
    /// # Errors
    ///
    /// Fails when the runtime makes a request that the test platform cannot carry out, when a frame cannot be
    /// encoded or when a step names no open window.
    pub(crate) fn play(&mut self, mut steps: Vec<Step>) -> Result<(), Error> {
        let recording = steps
            .iter()
            .rposition(Step::records)
            .map_or(0, |last| last + 1);
        let after_recording = steps.split_off(recording);
        for (group, mode) in [
            (steps, CaptureMode::WithPixels),
            (after_recording, CaptureMode::WithoutPixels),
        ] {
            self.capture(mode);
            for step in group {
                if let Settled::Ended = self.play_step(step)? {
                    return Ok(());
                }
            }
        }
        self.close()
    }

    fn play_step(&mut self, step: Step) -> Result<Settled, Error> {
        match step {
            Step::Report(event) => self.report_settled(|_| Ok(event)),
            Step::FocusKeyboard { keyboard, app_id } => self.report_settled(|session| {
                let surface = Some(session.window(&app_id)?);
                Ok(DeviceEvent::KeyboardFocus { keyboard, surface }.into())
            }),
            Step::PlayUntil(time) => self.play_until(time),
            Step::RecordUntilRest(every) => self.record_until_rest(every),
        }
    }

    /// Reports the event made by `event` once the actors have nothing left to do, and waits until they handled it.
    fn report_settled(
        &mut self,
        event: impl FnOnce(&Self) -> Result<PlatformEvent, Error>,
    ) -> Result<Settled, Error> {
        if let Settled::Ended = self.settle()? {
            return Ok(Settled::Ended);
        }
        let event = event(self)?;
        self.links.reports.report(event);
        self.settle()
    }

    /// Handles the requests from the runtime until its actors have nothing left to do at the current time, and takes
    /// the frames shown meanwhile. The frames are taken before the requests, so a destroyed window drops its last
    /// frame.
    fn settle(&mut self) -> Result<Settled, Error> {
        loop {
            let deadline = self.clock.next_deadline_when_idle();
            self.take_shown();
            let Ok(requests) = self.links.requests.take_arrived() else {
                return Ok(Settled::Ended);
            };
            if requests.is_empty() {
                return Ok(Settled::Waiting(deadline));
            }
            requests
                .into_iter()
                .try_for_each(|request| self.handle(request))?;
        }
    }

    fn take_shown(&mut self) {
        // The session has a sender, so the mailbox never closes.
        for frame in self.shown.take_arrived().into_iter().flatten() {
            self.unpresented.push((frame.surface, frame.time));
            let _older_frame = self.screen.insert(frame.surface, frame);
        }
    }

    /// Advances the clock to each time before `time` for which the actors wait, and then to `time`.
    fn play_until(&mut self, time: Duration) -> Result<Settled, Error> {
        loop {
            let Settled::Waiting(deadline) = self.settle()? else {
                return Ok(Settled::Ended);
            };
            match deadline.filter(|&wait| wait < time) {
                Some(deadline) => {
                    if let Settled::Ended = self.advance(deadline)? {
                        return Ok(Settled::Ended);
                    }
                }
                None => return self.advance(time),
            }
        }
    }

    /// Moves the clock to `time` while the actors have nothing left to do, and then presents the frames shown before,
    /// so that the frame scheduler starts the next frames at `time`.
    fn advance(&mut self, time: Duration) -> Result<Settled, Error> {
        let presented = mem::take(&mut self.unpresented);
        self.clock.advance_to(time);
        if let Settled::Ended = self.settle()? {
            return Ok(Settled::Ended);
        }
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
        self.settle()
    }

    /// Records the newest frame of each window now, and again each time the clock advanced by `every`, until the run
    /// is at rest.
    fn record_until_rest(&mut self, every: FrameInterval) -> Result<Settled, Error> {
        let mut settled = self.settle()?;
        while let Settled::Waiting(_) = settled {
            self.record();
            let recorded = self.newest_frames();
            settled = self.play_until(self.clock.now() + every.get())?;
            if let Settled::Waiting(deadline) = settled
                && is_at_rest(self.newest_frames() != recorded, deadline)
            {
                break;
            }
        }
        Ok(settled)
    }

    /// Records the newest frame of each window, whose pixels the controls captured.
    fn record(&mut self) {
        self.recorded
            .extend(self.screen.values().filter_map(|frame| {
                Some(RecordedFrame {
                    surface: frame.surface,
                    time: frame.time,
                    png: frame.png.clone()?,
                })
            }));
    }

    /// The sequence of the newest frame of each window.
    fn newest_frames(&self) -> Vec<(SurfaceId, u64)> {
        (self.screen.values())
            .map(|frame| (frame.surface, frame.time.sequence))
            .collect()
    }

    /// Makes the controls of the windows, and of every window created later, capture frames in `mode`.
    fn capture(&mut self, mode: CaptureMode) {
        self.capture = mode;
        for &(surface, _) in &self.windows {
            self.links.reports.attach(SurfaceAttachment::Control {
                surface,
                message: mode,
            });
        }
    }

    /// Closes every window, also each window created meanwhile, and handles the requests from the runtime until it
    /// ends them.
    fn close(&mut self) -> Result<(), Error> {
        if let Settled::Ended = self.settle()? {
            return Ok(());
        }
        let mut closed = BTreeSet::new();
        loop {
            for &(surface, _) in &self.windows {
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

    /// The open window with `app_id` that was created first.
    fn window(&self, app_id: &str) -> Result<SurfaceId, Error> {
        (self.windows.iter())
            .find(|(_, role)| matches!(role, Role::Window(window) if window.app_id == app_id))
            .map(|&(surface, _)| surface)
            .ok_or_else(|| Error::NoWindow {
                app_id: app_id.to_owned(),
            })
    }

    fn handle(&mut self, request: PlatformRequest) -> Result<(), Error> {
        match request {
            PlatformRequest::Surface(SurfaceRequest::Create {
                surface,
                spec,
                gpu_budget,
            }) if matches!(spec.role, Role::Window(_)) => self.create(surface, *spec, gpu_budget),
            PlatformRequest::Surface(SurfaceRequest::Destroy(surface)) => self.destroy(surface),
            // A headless surface needs no cleanup on detach and never shows a cursor.
            PlatformRequest::Surface(SurfaceRequest::Release(_)) | PlatformRequest::Input(_) => {}
            request => return Err(Error::Unsupported(Box::new(request))),
        }
        Ok(())
    }

    /// Creates a headless window of `spec` for `surface`, reports the density of the host for it, and configures it
    /// at the size of `spec`.
    fn create(&mut self, surface: SurfaceId, spec: SurfaceSpec, gpu_budget: Option<Duration>) {
        let control = FrameCapture::new(surface, self.capture, self.shown_by_controls.clone());
        let timing = SurfaceTiming::new(PresentationPolicy::Automatic);
        let attach = spec.attach(
            surface,
            Arc::new(HeadlessWindow),
            timing,
            gpu_budget,
            control,
        );
        self.links.reports.attach(SurfaceAttachment::Attach {
            attach: Box::new(attach),
            scale: PixelScale::SQUARE,
        });
        self.links
            .reports
            .report((surface, SurfaceEvent::DensityChanged(self.density)));
        self.links.reports.report((
            surface,
            SurfaceEvent::Configured {
                width: Some(spec.size.width),
                height: Some(spec.size.height),
                token: None,
            },
        ));
        self.windows.push((surface, spec.role));
    }

    fn destroy(&mut self, surface: SurfaceId) {
        self.windows.retain(|&(window, _)| window != surface);
        let _last_frame = self.screen.remove(&surface);
        self.unpresented.retain(|&(shown, _)| shown != surface);
        self.links
            .reports
            .attach(SurfaceAttachment::Detach(surface));
    }
}
