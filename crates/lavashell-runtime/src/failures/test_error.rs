// SPDX-License-Identifier: GPL-3.0-only

use log::SetLoggerError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum FailuresTestError {
    #[error("the test process has a logger that is not the recorder of the test")]
    Logger(#[source] &'static SetLoggerError),
}
