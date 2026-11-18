// SPDX-License-Identifier: GPL-3.0-only
mod error;
pub(crate) mod surface;
mod surface_in_window;

pub use error::AospError;

use crate::input::{MotionEvent, MotionTranslator};
use crate::layout_size::LayoutSize;
use crate::native_window::NativeWindow;
use crate::output::{HostDisplay, report_added, report_display};
use crate::platform::AospBackend;
use crate::settings::{HostConfiguration, report_configuration};
use crate::streams::AospStreams;
use lavashell_actor::{Closed, lock_ignoring_poison, lock_unpoisoned};
use lavashell_core::units::Density;
use lavashell_platform::{PlatformLinks, PlatformRequest, Reports, StreamTask};
use lavashell_renderer::Painter;
use std::ops::ControlFlow;
use std::sync::{Arc, Mutex, mpsc};
use surface::HostSurface;

pub(crate) type AospReports = Reports<NativeWindow, AospBackend>;
pub(crate) type AospLinks = PlatformLinks<NativeWindow, AospBackend>;

pub(crate) enum HostMessage {
    SurfaceCreated(NativeWindow),
    SurfaceChanged,
    SurfaceDestroyed(mpsc::SyncSender<()>),
    Frame,
    Motion(MotionEvent),
    DisplayChanged(HostDisplay),
    ConfigurationChanged(HostConfiguration),
    LayoutSize(Option<LayoutSize>),
    Requests,
    Streams(StreamTask<AospStreams>),
}

struct HostStart {
    messages: mpsc::Receiver<HostMessage>,
    display: HostDisplay,
    configuration: HostConfiguration,
}

pub struct HostLink {
    start: HostStart,
    channel: Arc<Channel>,
}

struct AwaitingRunEnd {
    messages: mpsc::Receiver<HostMessage>,
    destroying: Vec<mpsc::SyncSender<()>>,
}

impl HostLink {
    pub(crate) fn serve(self, links: AospLinks) -> Result<(), AospError> {
        let Self { start, channel } = self;
        let HostStart {
            messages,
            display,
            configuration,
        } = start;
        let PlatformLinks { reports, requests } = links;
        report_added(display, &reports);
        report_configuration(configuration, &reports);
        let wake = channel.sender.clone();
        requests.on_arrival(move || {
            let _ended_platform_reads_nothing = wake.send(HostMessage::Requests);
        });
        let mut state = HostState::new(display.density);
        let served = execute_messages(&messages, &requests, &reports, &mut state);
        let awaiting = AwaitingRunEnd {
            messages,
            destroying: state.surface.into_destroying(),
        };
        let kept =
            lock_unpoisoned(&channel.awaiting_run_end).map(|mut slot| *slot = Some(awaiting));
        served.and(kept.map_err(AospError::from))
    }
}

fn execute_messages(
    messages: &mpsc::Receiver<HostMessage>,
    requests: &lavashell_actor::Receiver<PlatformRequest>,
    reports: &AospReports,
    state: &mut HostState,
) -> Result<(), AospError> {
    for message in messages {
        if state.handle(message, requests, reports)?.is_break() {
            break;
        }
    }
    Ok(())
}

pub(crate) struct HostState {
    pub(crate) surface: HostSurface,
    translator: MotionTranslator,
}

impl HostState {
    pub(crate) fn new(density: Density) -> Self {
        Self {
            surface: HostSurface::new(density),
            translator: MotionTranslator::default(),
        }
    }

    pub(crate) fn handle(
        &mut self,
        message: HostMessage,
        requests: &lavashell_actor::Receiver<PlatformRequest>,
        reports: &AospReports,
    ) -> Result<ControlFlow<()>, AospError> {
        let surface = &mut self.surface;
        match message {
            HostMessage::Requests => loop {
                match requests.take_arrived() {
                    Ok(arrived) if arrived.is_empty() => break,
                    Ok(arrived) => {
                        for request in arrived {
                            surface.execute(request, reports)?;
                        }
                    }
                    Err(Closed) => return Ok(ControlFlow::Break(())),
                }
            },
            HostMessage::SurfaceCreated(window) => {
                surface.created(window, reports)?;
            }
            HostMessage::SurfaceChanged => surface.changed(reports)?,
            HostMessage::SurfaceDestroyed(retired) => surface.destroyed(retired, reports),
            HostMessage::Frame => surface.frame(reports),
            HostMessage::Motion(event) => {
                (self.translator).report(event, surface.input_target(), reports);
            }
            HostMessage::DisplayChanged(display) => {
                report_display(display, reports);
                surface.change_density(display.density, reports)?;
            }
            HostMessage::LayoutSize(size) => surface.change_layout_size(size, reports)?,
            HostMessage::ConfigurationChanged(configuration) => {
                report_configuration(configuration, reports);
            }
            HostMessage::Streams(task) => reports.run_on_streams(task),
        }
        Ok(ControlFlow::Continue(()))
    }
}

struct Served(Arc<Channel>);

impl Drop for Served {
    fn drop(&mut self) {
        let awaiting = lock_ignoring_poison(&self.0.awaiting_run_end).take();
        if let Some(AwaitingRunEnd {
            messages,
            destroying,
        }) = awaiting
        {
            drop(messages);
            for retired in destroying {
                let _retiring_caller_may_have_left = retired.send(());
            }
        }
    }
}

struct Channel {
    sender: mpsc::Sender<HostMessage>,
    unserved: Mutex<Option<HostStart>>,
    awaiting_run_end: Mutex<Option<AwaitingRunEnd>>,
}

#[derive(Clone)]
pub struct AospHost(Arc<Channel>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the run that was started inside `AospHost::serve` has ended")]
pub struct RunEnded;

impl AospHost {
    #[must_use]
    pub fn new(display: HostDisplay, configuration: HostConfiguration) -> Self {
        let (sender, messages) = mpsc::channel();
        let start = HostStart {
            messages,
            display,
            configuration,
        };
        Self(Arc::new(Channel {
            sender,
            unserved: Mutex::new(Some(start)),
            awaiting_run_end: Mutex::new(None),
        }))
    }

    pub fn serve<T>(self, run: impl FnOnce(HostLink) -> T) -> Result<T, AospError> {
        let start =
            (lock_unpoisoned(&self.0.unserved)?.take()).ok_or(AospError::HostAlreadyServed)?;
        let _awaiting_messages_end_on_drop = Served(Arc::clone(&self.0));
        Ok(run(HostLink {
            start,
            channel: self.0,
        }))
    }

    pub fn surface_created(&self, window: NativeWindow) {
        self.send(HostMessage::SurfaceCreated(window));
    }

    pub fn surface_changed(&self) {
        self.send(HostMessage::SurfaceChanged);
    }

    pub fn surface_destroyed(&self) {
        let (retired, waiter) = mpsc::sync_channel(1);
        self.send(HostMessage::SurfaceDestroyed(retired));
        let _retired_or_ended = waiter.recv();
    }

    pub fn frame(&self) {
        self.send(HostMessage::Frame);
    }

    pub fn display_changed(&self, display: HostDisplay) {
        self.send(HostMessage::DisplayChanged(display));
    }

    pub fn set_layout_size(&self, size: Option<LayoutSize>) {
        self.send(HostMessage::LayoutSize(size));
    }

    pub fn configuration_changed(&self, configuration: HostConfiguration) {
        self.send(HostMessage::ConfigurationChanged(configuration));
    }

    pub fn motion(&self, event: MotionEvent) {
        self.send(HostMessage::Motion(event));
    }

    pub fn run_on_streams<T: Send + 'static>(
        &self,
        task: impl FnOnce(&mut AospStreams, &Painter) -> T + Send + 'static,
    ) -> Result<T, RunEnded> {
        let (reply, result) = mpsc::sync_channel(1);
        let task: StreamTask<AospStreams> = Box::new(move |streams, painter| {
            let _caller_left = reply.send(task(streams, painter));
        });
        self.send(HostMessage::Streams(task));
        result.recv().map_err(|mpsc::RecvError| RunEnded)
    }

    fn send(&self, message: HostMessage) {
        let _ended_run_reads_nothing = self.0.sender.send(message);
    }
}
