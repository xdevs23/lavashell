// SPDX-License-Identifier: GPL-3.0-only
use crate::pipe_transfers::Error;
use std::io::{self, Read, Write};
use std::sync::Arc;

pub(crate) enum Progress {
    Pending,
    Complete,
}

#[derive(Default)]
pub(crate) struct ReadBuffer {
    buffer: Vec<u8>,
    filled: usize,
}

impl ReadBuffer {
    pub fn read_from(&mut self, reader: &mut impl Read) -> Result<Progress, Error> {
        loop {
            if self.filled == self.buffer.len() {
                let additional = self.buffer.len().max(1);
                self.buffer
                    .try_reserve(additional)
                    .map_err(Error::ReserveTransfer)?;
                self.buffer.resize(self.buffer.capacity(), 0);
            }
            let free = self.buffer.len() - self.filled;
            match reader.read(&mut self.buffer[self.filled..]) {
                Ok(0) => return Ok(Progress::Complete),
                Ok(count) => {
                    self.filled += count;
                    if count < free {
                        return Ok(Progress::Pending);
                    }
                }
                Err(error) if retried(&error) => return Ok(Progress::Pending),
                Err(error) => return Err(Error::ReadPipe(error)),
            }
        }
    }

    #[must_use]
    pub fn into_bytes(mut self) -> Vec<u8> {
        self.buffer.truncate(self.filled);
        self.buffer
    }
}

pub(crate) struct WriteBuffer {
    bytes: Arc<[u8]>,
    written: usize,
}

impl WriteBuffer {
    #[must_use]
    pub fn new(bytes: Arc<[u8]>) -> Self {
        Self { bytes, written: 0 }
    }

    pub fn write_to(&mut self, writer: &mut impl Write) -> Result<Progress, Error> {
        if self.written < self.bytes.len() {
            match writer.write(&self.bytes[self.written..]) {
                Ok(0) => return Err(Error::WritePipe(io::ErrorKind::WriteZero.into())),
                Ok(count) => self.written += count,
                Err(error) if retried(&error) => {}
                Err(error) => return Err(Error::WritePipe(error)),
            }
        }
        Ok(if self.written == self.bytes.len() {
            Progress::Complete
        } else {
            Progress::Pending
        })
    }
}

fn retried(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
    )
}

#[cfg(test)]
mod tests;
