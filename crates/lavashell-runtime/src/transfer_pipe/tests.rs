// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestResult;
use std::io::Cursor;
use std::os::unix::net::UnixStream;

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
    let data = || WriteBuffer::new(Arc::from(b"data".as_slice()));
    let mut sent = data();
    assert!(matches!(sent.write_to(&mut sender)?, Progress::Pending));
    assert_eq!(sent.written, 0);
    receiver.read_exact(&mut vec![0; filled])?;
    assert!(matches!(sent.write_to(&mut sender)?, Progress::Complete));
    drop(receiver);
    assert!(data().write_to(&mut sender).is_err());
    Ok(())
}
