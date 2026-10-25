// SPDX-License-Identifier: GPL-3.0-only
use std::os::fd::AsFd;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct PipeIdentity {
    device: u64,
    inode: u64,
}

impl PipeIdentity {
    pub fn read(file: &impl AsFd) -> Option<Self> {
        let stat = rustix::fs::fstat(file).ok()?;
        (rustix::fs::FileType::from_raw_mode(stat.st_mode) == rustix::fs::FileType::Fifo).then_some(
            Self {
                device: stat.st_dev,
                inode: stat.st_ino,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;

    #[test]
    fn real_pipe_ends_match_and_unrelated_pipes_do_not() -> Result<(), TestError> {
        let (reader, writer) = rustix::pipe::pipe().map_err(TestError::CreatePipe)?;
        let (other, _) = rustix::pipe::pipe().map_err(TestError::CreatePipe)?;
        let identity = PipeIdentity::read(&reader).ok_or(TestError::NotPipe)?;
        assert!(PipeIdentity::read(&writer) == Some(identity));
        assert!(PipeIdentity::read(&other) != Some(identity));
        let executable = std::env::current_exe().map_err(TestError::CurrentExecutable)?;
        let executable = std::fs::File::open(executable).map_err(TestError::OpenExecutable)?;
        assert!(PipeIdentity::read(&executable).is_none());
        Ok(())
    }
}
