// SPDX-License-Identifier: GPL-3.0-only
use crate::app::Platform;

mod identity;

use super::TransferId;
use crate::Error;
use identity::PipeIdentity;
use lavashell_core::transfer::Cancellation;
use lavashell_runtime::{Progress, ReadBuffer, write_available};
use sctk::{
    data_device_manager::{ReadPipe, WritePipe},
    reexports::{
        calloop::{PostAction, RegistrationToken},
        client::protocol::{wl_data_source::WlDataSource, wl_seat::WlSeat, wl_surface::WlSurface},
    },
};
use smithay_client_toolkit as sctk;
use std::{os::fd::AsFd, sync::Arc};

type Received = Box<dyn FnOnce(&mut Platform, Result<Vec<u8>, Error>)>;
type Sent = Box<dyn FnOnce(&mut Platform, Result<(), Error>)>;

/// The callback that receives the result of a transfer once its pipe is closed or fails.
enum Completion {
    /// Receives the bytes read from the pipe. `bytes` collects them until the writing client closes the pipe.
    Receive {
        bytes: ReadBuffer,
        done: Received,
    },
    Send(Sent),
}

impl Completion {
    fn finish(self, platform: &mut Platform, result: Result<(), Error>) {
        match self {
            Self::Receive { bytes, done } => done(platform, result.map(|()| bytes.into_bytes())),
            Self::Send(done) => done(platform, result),
        }
    }
}

pub(super) struct Job {
    pub id: u64,
    pub token: RegistrationToken,
    pub seat: WlSeat,
    pub surface: WlSurface,
    pub source: Option<WlDataSource>,
    incoming: Option<TransferId>,
    pipe: Option<PipeIdentity>,
    done: Completion,
}

pub(super) struct Scope {
    pub seat: WlSeat,
    pub surface: WlSurface,
    pub source: Option<WlDataSource>,
    pub incoming: Option<TransferId>,
}

pub(super) fn nonblocking(file: &impl AsFd) -> rustix::io::Result<()> {
    let flags = rustix::fs::fcntl_getfl(file)?;
    rustix::fs::fcntl_setfl(file, flags | rustix::fs::OFlags::NONBLOCK)
}

impl Platform {
    /// Adds a job for transfer `id`, whose pipe is registered in the event loop as `registered`. A failed
    /// registration finishes the transfer with that error.
    fn track_transfer(
        &mut self,
        id: u64,
        scope: Scope,
        pipe: Option<PipeIdentity>,
        done: Completion,
        registered: Result<RegistrationToken, Error>,
    ) {
        match registered {
            Ok(token) => self.clipboard.jobs.push(Job {
                id,
                token,
                seat: scope.seat,
                surface: scope.surface,
                source: scope.source,
                incoming: scope.incoming,
                pipe,
                done,
            }),
            Err(error) => done.finish(self, Err(error)),
        }
    }

    pub(super) fn receive_transfer(&mut self, pipe: ReadPipe, scope: Scope, done: Received) {
        if let Err(errno) = nonblocking(&pipe) {
            done(self, Err(Error::ConfigurePipe(errno)));
            return;
        }
        let id = self.clipboard.id();
        let identity = scope.incoming.and_then(|_| PipeIdentity::read(&pipe));
        let result = self
            .loop_handle
            .insert_source(pipe, move |(), file, platform| {
                let Some(bytes) = platform.received_bytes(id) else {
                    return PostAction::Remove;
                };
                match bytes.read_from(&mut &**file) {
                    Ok(Progress::Pending) => PostAction::Continue,
                    Ok(Progress::Complete) => {
                        platform.finish_transfer(id, Ok(()));
                        PostAction::Remove
                    }
                    Err(error) => {
                        platform.finish_transfer(id, Err(error.into()));
                        PostAction::Remove
                    }
                }
            })
            .map_err(|failure| Error::WatchPipe(failure.error));
        let done = Completion::Receive {
            bytes: ReadBuffer::default(),
            done,
        };
        self.track_transfer(id, scope, identity, done, result);
    }

    pub(super) fn send_transfer(
        &mut self,
        pipe: WritePipe,
        scope: Scope,
        bytes: Arc<[u8]>,
        done: Sent,
    ) {
        if let Err(errno) = nonblocking(&pipe) {
            done(self, Err(Error::ConfigurePipe(errno)));
            return;
        }
        let id = self.clipboard.id();
        let identity = scope.incoming.and_then(|_| PipeIdentity::read(&pipe));
        let mut written = 0;
        let result = self
            .loop_handle
            .insert_source(pipe, move |(), file, platform| {
                match write_available(&mut &**file, &bytes, &mut written) {
                    Ok(Progress::Pending) => PostAction::Continue,
                    Ok(Progress::Complete) => {
                        platform.finish_transfer(id, Ok(()));
                        PostAction::Remove
                    }
                    Err(error) => {
                        platform.finish_transfer(id, Err(error.into()));
                        PostAction::Remove
                    }
                }
            })
            .map_err(|failure| Error::WatchPipe(failure.error));
        self.track_transfer(id, scope, identity, Completion::Send(done), result);
    }

    /// The bytes that the pipe of the received transfer `id` delivered so far.
    fn received_bytes(&mut self, id: u64) -> Option<&mut ReadBuffer> {
        (self.clipboard.jobs.iter_mut()).find_map(|job| match &mut job.done {
            Completion::Receive { bytes, .. } if job.id == id => Some(bytes),
            _ => None,
        })
    }

    fn finish_transfer(&mut self, id: u64, result: Result<(), Error>) {
        if let Some(index) = self.clipboard.jobs.iter().position(|job| job.id == id) {
            let job = self.clipboard.jobs.remove(index);
            let result = if self.transfer_surface_live(&job.surface) {
                result
            } else {
                Err(Cancellation::SurfaceClosed.into())
            };
            job.done.finish(self, result);
        }
    }

    pub(crate) fn cancel_transfer(&mut self, id: u64, reason: Cancellation) {
        if let Some(token) = (self.clipboard.jobs.iter())
            .find(|job| job.id == id)
            .map(|job| job.token)
        {
            self.loop_handle.remove(token);
        }
        self.finish_transfer(id, Err(reason.into()));
    }

    pub(super) fn local_receiving_transfer(
        &self,
        pipe: &impl AsFd,
        seat: &WlSeat,
    ) -> Option<TransferId> {
        let identity = PipeIdentity::read(pipe)?;
        let mut matches = self.clipboard.jobs.iter().filter_map(|job| {
            (job.pipe == Some(identity) && &job.seat == seat)
                .then_some(job.incoming)
                .flatten()
        });
        let id = matches.next()?;
        matches.next().is_none().then_some(id)
    }
}
