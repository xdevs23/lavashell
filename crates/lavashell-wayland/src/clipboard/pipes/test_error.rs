// SPDX-License-Identifier: GPL-3.0-only
use std::io;

#[derive(Debug, thiserror::Error)]
pub(crate) enum PipesTestError {
    #[error("the test executable could not be opened")]
    OpenExecutable(#[source] io::Error),
    #[error("the file is not a pipe")]
    NotPipe,
}
