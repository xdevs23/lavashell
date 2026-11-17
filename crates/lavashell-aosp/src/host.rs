// SPDX-License-Identifier: GPL-3.0-only
mod error;
pub(crate) mod surface;

pub use error::AospError;

use crate::input::{MotionEvent, MotionTranslator};
use crate::native_window::NativeWindow;
use crate::output::{HostDisplay, HostOutput};
use crate::settings::{HostConfiguration, report_configuration};
use crate::streams::AospStreams;
use lavashell_actor::{Closed, lock_ignoring_poison, lock_unpoisoned};
use lavashell_platform::{PlatformLinks, PlatformRequest, Reports, StreamTask};
use lavashell_renderer::Painter;
use std::sync::{Arc, Mutex, mpsc};
use surface::HostSurface;

enum HostMessage {
    SurfaceCreated(NativeWindow),
    SurfaceChanged,
    SurfaceDestroyed(mpsc::SyncSender<()>),
    Frame,
    Motion(MotionEvent),
    DisplayChanged(HostDisplay),
    ConfigurationChanged(HostConfiguration),
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
    pub(crate) fn serve(
        self,
        links: PlatformLinks<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        let Self { start, channel } = self;
        let HostStart {
            messages,
            display,
            configuration,
        } = start;
        let PlatformLinks { reports, requests } = links;
        let mut output = HostOutput::report_added(display, &reports);
        report_configuration(configuration, &reports);
        let wake = channel.sender.clone();
        requests.on_arrival(move || {
            let _ended_platform_reads_nothing = wake.send(HostMessage::Requests);
        });
        let mut surface = HostSurface::default();
        let mut translator = MotionTranslator::default();
        let served = execute_messages(
            &messages,
            &requests,
            &reports,
            &mut output,
            &mut surface,
            &mut translator,
        );
        let awaiting = AwaitingRunEnd {
            messages,
            destroying: surface.into_destroying(),
        };
        let kept =
            lock_unpoisoned(&channel.awaiting_run_end).map(|mut slot| *slot = Some(awaiting));
        served.and(kept.map_err(AospError::from))
    }
}

fn execute_messages(
    messages: &mpsc::Receiver<HostMessage>,
    requests: &lavashell_actor::Receiver<PlatformRequest>,
    reports: &Reports<NativeWindow, AospStreams>,
    output: &mut HostOutput,
    surface: &mut HostSurface,
    translator: &mut MotionTranslator,
) -> Result<(), AospError> {
    for message in messages {
        match message {
            HostMessage::Requests => loop {
                match requests.take_arrived() {
                    Ok(arrived) if arrived.is_empty() => break,
                    Ok(arrived) => {
                        for request in arrived {
                            surface.execute(request, output.density(), reports)?;
                        }
                    }
                    Err(Closed) => return Ok(()),
                }
            },
            HostMessage::SurfaceCreated(window) => {
                surface.created(window, output.density(), reports)?;
            }
            HostMessage::SurfaceChanged => surface.changed(reports)?,
            HostMessage::SurfaceDestroyed(retired) => surface.destroyed(retired, reports),
            HostMessage::Frame => surface.frame(reports),
            HostMessage::Motion(event) => translator.report(event, surface.shown(), reports),
            HostMessage::DisplayChanged(display) => {
                if let Some(density) = output.report_change(display, reports) {
                    surface.change_density(density, reports);
                }
            }
            HostMessage::ConfigurationChanged(configuration) => {
                report_configuration(configuration, reports);
            }
            HostMessage::Streams(task) => reports.run_on_streams(task),
        }
    }
    Ok(())
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
