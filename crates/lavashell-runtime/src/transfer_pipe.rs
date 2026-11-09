// SPDX-License-Identifier: GPL-3.0-only
use crate::pipe_transfers::Error;
use std::io::{self, Read, Write};

/// Whether a transfer through a pipe moved every byte.
pub(crate) enum Progress {
    Pending,
    Complete,
}

/// The bytes read from a pipe so far. The buffer doubles once it is full. The bytes after `filled` are zero and free.
#[derive(Default)]
pub(crate) struct ReadBuffer {
    buffer: Vec<u8>,
    filled: usize,
}

impl ReadBuffer {
    /// Reads the bytes that `reader` holds, until one `read` call fills less than the free part of the buffer, the
    /// nonblocking reader is empty, or the writer closed the pipe. The memory for the buffer is reserved with
    /// `try_reserve`, so a transfer too large for the memory fails with an error and does not abort the app.
    ///
    /// # Errors
    ///
    /// Fails when the memory for the buffer cannot be reserved, or when `reader` fails.
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

/// Writes once from the bytes after `written` and advances `written` by the number of bytes written.
///
/// # Errors
///
/// Fails when `writer` fails or accepts zero bytes.
pub(crate) fn write_available(
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

/// Whether `error` means that the call moved nothing and only has to be repeated: the call was interrupted, or the
/// nonblocking pipe was not ready.
fn retried(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
    )
}

#[cfg(test)]
mod tests;
