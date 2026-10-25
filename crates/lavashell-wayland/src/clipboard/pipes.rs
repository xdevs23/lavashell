// SPDX-License-Identifier: GPL-3.0-only
use crate::app::Platform;

mod identity;

use super::TransferId;
use crate::{Cancellation, Error};
use identity::PipeIdentity;
use sctk::{
    data_device_manager::{ReadPipe, WritePipe},
    reexports::{
        calloop::{PostAction, RegistrationToken},
        client::protocol::{wl_data_source::WlDataSource, wl_seat::WlSeat, wl_surface::WlSurface},
    },
};
use smithay_client_toolkit as sctk;
use std::{
    io::{self, Read, Write},
    os::fd::AsFd,
    sync::Arc,
};

type Received = Box<dyn FnOnce(&mut Platform, Result<Vec<u8>, Error>)>;
type Sent = Box<dyn FnOnce(&mut Platform, Result<(), Error>)>;

/// The callback that receives the result of a transfer once its pipe is closed or fails.
enum Completion {
    /// Receives the bytes read from the pipe. `bytes` collects them until the writing client closes the pipe.
    Receive {
        bytes: Vec<u8>,
        done: Received,
    },
    Send(Sent),
}

impl Completion {
    fn finish(self, platform: &mut Platform, result: Result<(), Error>) {
        match self {
            Self::Receive { bytes, done } => done(platform, result.map(|()| bytes)),
            Self::Send(done) => done(platform, result),
        }
    }
}

pub(super) struct Job {
    pub id: u64,
    pub token: RegistrationToken,
    pub seat: WlSeat,
    pub surface: Option<WlSurface>,
    pub source: Option<WlDataSource>,
    incoming: Option<TransferId>,
    pipe: Option<PipeIdentity>,
    done: Completion,
}

pub(super) struct Scope {
    pub seat: WlSeat,
    pub surface: Option<WlSurface>,
    pub source: Option<WlDataSource>,
    pub incoming: Option<TransferId>,
}

enum Progress {
    Pending,
    Complete,
}

fn nonblocking(file: &impl AsFd) -> Result<(), Error> {
    let flags = rustix::fs::fcntl_getfl(file).map_err(Error::ConfigurePipe)?;
    rustix::fs::fcntl_setfl(file, flags | rustix::fs::OFlags::NONBLOCK)
        .map_err(Error::ConfigurePipe)
}

/// Reads from the pipe once into `chunk` and appends the bytes read to `bytes`. The memory for them is reserved
/// with `try_reserve`, so a transfer too large for the memory fails with an error and does not abort the app.
fn read_available(
    reader: &mut impl Read,
    chunk: &mut [u8],
    bytes: &mut Vec<u8>,
) -> Result<Progress, Error> {
    match reader.read(chunk) {
        Ok(0) => Ok(Progress::Complete),
        Ok(count) => {
            bytes.try_reserve(count).map_err(Error::ReserveTransfer)?;
            bytes.extend_from_slice(&chunk[..count]);
            Ok(Progress::Pending)
        }
        Err(error) if retried(&error) => Ok(Progress::Pending),
        Err(error) => Err(Error::ReadPipe(error)),
    }
}

/// Writes once from the bytes after `written` and advances `written` by the number of bytes written.
fn write_available(
    writer: &mut impl Write,
    bytes: &[u8],
    written: &mut usize,
) -> Result<Progress, Error> {
    if *written < bytes.len() {
        match writer.write(&bytes[*written..]) {
            Ok(0) => return Err(Error::WritePipe(io::ErrorKind::WriteZero.into())),
            Ok(count) => *written += count,
            Err(error) if retried(&error) => {}
            Err(error) => return Err(Error::WritePipe(error)),
        }
    }
    Ok(if *written == bytes.len() {
        Progress::Complete
    } else {
        Progress::Pending
    })
}

/// Whether `error` means that the call moved no data and only has to be repeated: the call was interrupted, or the
/// nonblocking pipe was not ready. The event loop calls again once the pipe is ready.
fn retried(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
    )
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
        let capacity = nonblocking(&pipe)
            .and_then(|()| rustix::pipe::fcntl_getpipe_size(&pipe).map_err(Error::PipeCapacity));
        let capacity = match capacity {
            Ok(capacity) => capacity,
            Err(error) => {
                done(self, Err(error));
                return;
            }
        };
        let id = self.clipboard.id();
        let identity = scope.incoming.and_then(|_| PipeIdentity::read(&pipe));
        let mut chunk = vec![0; capacity];
        let result = self
            .loop_handle
            .insert_source(pipe, move |(), file, platform| {
                let Some(bytes) = platform.received_bytes(id) else {
                    return PostAction::Remove;
                };
                match read_available(&mut &**file, &mut chunk, bytes) {
                    Ok(Progress::Pending) => PostAction::Continue,
                    Ok(Progress::Complete) => {
                        platform.finish_transfer(id, Ok(()));
                        PostAction::Remove
                    }
                    Err(error) => {
                        platform.finish_transfer(id, Err(error));
                        PostAction::Remove
                    }
                }
            })
            .map_err(|failure| Error::WatchPipe(failure.error));
        let done = Completion::Receive {
            bytes: Vec::new(),
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
        if let Err(error) = nonblocking(&pipe) {
            done(self, Err(error));
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
                        platform.finish_transfer(id, Err(error));
                        PostAction::Remove
                    }
                }
            })
            .map_err(|failure| Error::WatchPipe(failure.error));
        self.track_transfer(id, scope, identity, Completion::Send(done), result);
    }

    /// The bytes that the pipe of the received transfer `id` delivered so far.
    fn received_bytes(&mut self, id: u64) -> Option<&mut Vec<u8>> {
        (self.clipboard.jobs.iter_mut()).find_map(|job| match &mut job.done {
            Completion::Receive { bytes, .. } if job.id == id => Some(bytes),
            _ => None,
        })
    }

    fn finish_transfer(&mut self, id: u64, result: Result<(), Error>) {
        if let Some(index) = self.clipboard.jobs.iter().position(|job| job.id == id) {
            let job = self.clipboard.jobs.remove(index);
            let result = if job
                .surface
                .as_ref()
                .is_some_and(|surface| !self.transfer_surface_live(surface))
            {
                Err(Cancellation::SurfaceClosed.into())
            } else {
                result
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;
    use std::fs::File;
    use std::io::Cursor;

    fn pipe_capacity() -> Result<usize, TestError> {
        let (_, writer) = rustix::pipe::pipe().map_err(TestError::CreatePipe)?;
        Ok(rustix::pipe::fcntl_getpipe_size(&writer).map_err(Error::PipeCapacity)?)
    }

    #[test]
    fn reading_takes_one_chunk_per_call_and_preserves_binary_data() -> Result<(), TestError> {
        let capacity = pipe_capacity()?;
        let expected: Vec<u8> = (0..=u8::MAX).cycle().take(capacity + 1).collect();
        let mut reader = Cursor::new(&expected);
        let mut chunk = vec![0; capacity];
        let mut bytes = Vec::new();
        let mut read = || read_available(&mut reader, &mut chunk, &mut bytes);
        assert!(matches!(read()?, Progress::Pending));
        assert!(matches!(read()?, Progress::Pending));
        assert!(matches!(read()?, Progress::Complete));
        assert_eq!(bytes, expected);
        Ok(())
    }

    #[test]
    fn writing_waits_for_room_in_a_full_pipe_and_closed_receivers_report_failure()
    -> Result<(), TestError> {
        let (reader, writer) = rustix::pipe::pipe().map_err(TestError::CreatePipe)?;
        let capacity = rustix::pipe::fcntl_getpipe_size(&writer).map_err(Error::PipeCapacity)?;
        nonblocking(&writer)?;
        let data = vec![42; capacity + 1];
        let mut writer = File::from(writer);
        let mut written = 0;
        assert!(matches!(
            write_available(&mut writer, &data, &mut written)?,
            Progress::Pending
        ));
        assert_eq!(written, capacity);
        assert!(matches!(
            write_available(&mut writer, &data, &mut written)?,
            Progress::Pending
        ));
        assert_eq!(written, capacity);
        let mut reader = File::from(reader);
        reader
            .read_exact(&mut vec![0; capacity])
            .map_err(Error::ReadPipe)?;
        assert!(matches!(
            write_available(&mut writer, &data, &mut written)?,
            Progress::Complete
        ));
        let (mut sender, receiver) =
            std::os::unix::net::UnixStream::pair().map_err(TestError::CreateSocketPair)?;
        drop(receiver);
        assert!(write_available(&mut sender, b"data", &mut 0).is_err());
        Ok(())
    }
}
