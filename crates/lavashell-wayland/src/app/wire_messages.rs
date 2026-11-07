// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use std::{io::Read, os::unix::net::UnixStream};

pub(super) fn requests(peer: &mut UnixStream, id: u32) -> Result<Vec<(u16, Vec<u32>)>, TestError> {
    Ok(messages(peer)?
        .into_iter()
        .filter_map(|(object, opcode, words)| (object == id).then_some((opcode, words)))
        .collect())
}

pub(super) fn messages(peer: &mut UnixStream) -> Result<Vec<(u32, u16, Vec<u32>)>, TestError> {
    // The client flushed before this read, so every request it sent is in the socket already.
    let mut bytes = Vec::new();
    match peer.read_to_end(&mut bytes) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
        Err(error) => return Err(TestError::ReadProtocol(error)),
    }
    let word = |bytes: &[u8]| Ok::<_, TestError>(u32::from_ne_bytes(bytes.try_into()?));
    let mut offset = 0;
    let mut result = Vec::new();
    while offset < bytes.len() {
        let header = word(&bytes[offset + 4..offset + 8])?;
        let size = usize::try_from(header >> 16)?;
        assert!(size >= 8 && offset + size <= bytes.len());
        result.push((
            word(&bytes[offset..offset + 4])?,
            u16::try_from(header & u32::from(u16::MAX))?,
            bytes[offset + 8..offset + size]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| u32::from_ne_bytes(*bytes))
                .collect(),
        ));
        offset += size;
    }
    Ok(result)
}
