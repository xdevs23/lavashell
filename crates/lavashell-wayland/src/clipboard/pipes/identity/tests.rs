// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::clipboard::pipes::test_error::PipesTestError;
use crate::test_error::TestError;

#[test]
fn real_pipe_ends_match_and_unrelated_pipes_do_not() -> Result<(), TestError> {
    let (reader, writer) = rustix::pipe::pipe().map_err(TestError::CreatePipe)?;
    let (other, _other_writer_is_never_compared) =
        rustix::pipe::pipe().map_err(TestError::CreatePipe)?;
    let identity = PipeIdentity::read(&reader).ok_or(PipesTestError::NotPipe)?;
    assert!(PipeIdentity::read(&writer) == Some(identity));
    assert!(PipeIdentity::read(&other) != Some(identity));
    let executable = lavashell_test_fixtures::test_bus::current_test_executable()?;
    let executable = std::fs::File::open(executable).map_err(PipesTestError::OpenExecutable)?;
    assert!(PipeIdentity::read(&executable).is_none());
    Ok(())
}
