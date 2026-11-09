// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use std::io::Cursor;
use std::os::unix::net::UnixStream;

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("the transfer failed")]
    Transfer(#[from] Error),
    #[error("a socket failed")]
    Socket(#[from] io::Error),
}

type TestResult = Result<(), TestError>;

/// Writes to the nonblocking `writer` until it is full, and returns the number of bytes written.
fn fill(writer: &mut impl Write) -> Result<usize, io::Error> {
    let mut written = 0;
    loop {
        match writer.write(&[42; 1024]) {
            Ok(count) => written += count,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(written),
            Err(error) => return Err(error),
        }
    }
}

#[test]
fn reading_keeps_binary_data_until_the_writer_closes() -> TestResult {
    let expected: Vec<u8> = (0..=u8::MAX).cycle().take(5000).collect();
    let mut reader = Cursor::new(&expected);
    let mut buffer = ReadBuffer::default();
    assert!(matches!(buffer.read_from(&mut reader)?, Progress::Pending));
    assert!(matches!(buffer.read_from(&mut reader)?, Progress::Complete));
    assert_eq!(buffer.into_bytes(), expected);
    Ok(())
}

#[test]
fn writing_waits_for_room_and_fails_once_the_reader_is_gone() -> TestResult {
    let (mut sender, mut receiver) = UnixStream::pair()?;
    sender.set_nonblocking(true)?;
    let filled = fill(&mut sender)?;
    let mut written = 0;
    assert!(matches!(
        write_available(&mut sender, b"data", &mut written)?,
        Progress::Pending
    ));
    assert_eq!(written, 0);
    receiver.read_exact(&mut vec![0; filled])?;
    assert!(matches!(
        write_available(&mut sender, b"data", &mut written)?,
        Progress::Complete
    ));
    drop(receiver);
    assert!(write_available(&mut sender, b"data", &mut 0).is_err());
    Ok(())
}
