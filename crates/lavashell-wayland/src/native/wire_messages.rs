// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use crate::test_error::TestError;
use crate::{ConnectionError, Error};
use sctk::reexports::client::Connection;
use std::io::{ErrorKind, Read};
use std::os::unix::net::UnixStream;
use test_error::WireTestError;

pub(crate) mod test_error;

const WORD_SIZE: usize = size_of::<u32>();
const HEADER_SIZE: usize = 2 * WORD_SIZE;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct WireMessage {
    pub(super) object: u32,
    pub(super) opcode: u16,
    pub(super) arguments: Vec<u32>,
}

impl WireMessage {
    pub(super) fn new(object: u32, opcode: u16, arguments: &[u32]) -> Self {
        Self {
            object,
            opcode,
            arguments: arguments.to_vec(),
        }
    }
}

pub(super) struct FakeCompositor {
    pub(super) connection: Connection,
    peer: UnixStream,
}

impl FakeCompositor {
    pub(super) fn new() -> Result<Self, TestError> {
        let (socket, peer) = UnixStream::pair().map_err(TestError::CreateSocketPair)?;
        peer.set_nonblocking(true)
            .map_err(WireTestError::ConfigureSocket)?;
        let connection = Connection::from_socket(socket)
            .map_err(ConnectionError::Connect)
            .map_err(Error::from)?;
        Ok(Self { connection, peer })
    }

    pub(super) fn flush(&mut self) -> Result<Vec<WireMessage>, TestError> {
        self.connection
            .flush()
            .map_err(ConnectionError::Flush)
            .map_err(Error::from)?;
        read_messages(&mut self.peer)
    }

    pub(super) fn flush_object(&mut self, object: u32) -> Result<Vec<WireMessage>, TestError> {
        let mut messages = self.flush()?;
        messages.retain(|message| message.object == object);
        Ok(messages)
    }
}

fn read_messages(peer: &mut UnixStream) -> Result<Vec<WireMessage>, TestError> {
    let mut bytes = Vec::new();
    match peer.read_to_end(&mut bytes) {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::WouldBlock => {}
        Err(error) => return Err(WireTestError::ReadProtocol(error).into()),
    }
    let word = |bytes: &[u8]| Ok::<_, WireTestError>(u32::from_ne_bytes(bytes.try_into()?));
    let mut offset = 0;
    let mut result = Vec::new();
    while offset < bytes.len() {
        let header = word(&bytes[offset + WORD_SIZE..offset + HEADER_SIZE])?;
        let size = usize::try_from(header >> u16::BITS)?;
        assert!(size >= HEADER_SIZE && offset + size <= bytes.len());
        result.push(WireMessage {
            object: word(&bytes[offset..offset + WORD_SIZE])?,
            opcode: u16::try_from(header & u32::from(u16::MAX))?,
            arguments: bytes[offset + HEADER_SIZE..offset + size]
                .as_chunks::<WORD_SIZE>()
                .0
                .iter()
                .map(|bytes| u32::from_ne_bytes(*bytes))
                .collect(),
        });
        offset += size;
    }
    Ok(result)
}
