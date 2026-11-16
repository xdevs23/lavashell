// SPDX-License-Identifier: GPL-3.0-only
mod error;
pub(crate) mod surface;

pub use error::AospError;

use crate::native_window::NativeWindow;
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
    Requests,
    Streams(StreamTask<AospStreams>),
}

pub struct HostLink {
    messages: mpsc::Receiver<HostMessage>,
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
        let Self { messages, channel } = self;
        let PlatformLinks { reports, requests } = links;
        let wake = channel.sender.clone();
        requests.on_arrival(move || {
            let _ended_platform_reads_nothing = wake.send(HostMessage::Requests);
        });
        let mut surface = HostSurface::default();
        let served = execute_messages(&messages, &requests, &reports, &mut surface);
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
    surface: &mut HostSurface,
) -> Result<(), AospError> {
    for message in messages {
        match message {
            HostMessage::Requests => loop {
                match requests.take_arrived() {
                    Ok(arrived) if arrived.is_empty() => break,
                    Ok(arrived) => {
                        for request in arrived {
                            surface.execute(request, reports)?;
                        }
                    }
                    Err(Closed) => return Ok(()),
                }
            },
            HostMessage::SurfaceCreated(window) => surface.created(window, reports)?,
            HostMessage::SurfaceChanged => surface.changed(reports)?,
            HostMessage::SurfaceDestroyed(retired) => surface.destroyed(retired, reports),
            HostMessage::Frame => surface.frame(reports),
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
                let _returned_callback = retired.send(());
            }
        }
    }
}

struct Channel {
    sender: mpsc::Sender<HostMessage>,
    unserved: Mutex<Option<mpsc::Receiver<HostMessage>>>,
    awaiting_run_end: Mutex<Option<AwaitingRunEnd>>,
}

#[derive(Clone)]
pub struct AospHost(Arc<Channel>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the run that was started inside `AospHost::serve` has ended")]
pub struct RunEnded;

impl AospHost {
    #[must_use]
    pub fn new() -> Self {
        let (sender, messages) = mpsc::channel();
        Self(Arc::new(Channel {
            sender,
            unserved: Mutex::new(Some(messages)),
            awaiting_run_end: Mutex::new(None),
        }))
    }

    pub fn serve<T>(self, run: impl FnOnce(HostLink) -> T) -> Result<T, AospError> {
        let messages =
            (lock_unpoisoned(&self.0.unserved)?.take()).ok_or(AospError::HostAlreadyServed)?;
        let _served = Served(Arc::clone(&self.0));
        Ok(run(HostLink {
            messages,
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

impl Default for AospHost {
    fn default() -> Self {
        Self::new()
    }
}
