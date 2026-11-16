// SPDX-License-Identifier: GPL-3.0-only
use crate::error::{FileOperation, RoundTripError};
use lavashell_core::error::message_chain;
use std::fs::File;
use tempfile::NamedTempFile;

pub struct RunLog(NamedTempFile);

impl RunLog {
    pub fn create() -> Result<Self, RoundTripError> {
        tempfile::Builder::new()
            .prefix("lavashell-emulator-")
            .suffix(".log")
            .tempfile()
            .map(Self)
            .map_err(RoundTripError::file(
                FileOperation::CreateTemporary,
                std::env::temp_dir(),
            ))
    }

    pub fn duplicate_handle(&self) -> Result<File, RoundTripError> {
        self.0
            .as_file()
            .try_clone()
            .map_err(RoundTripError::ShareLog)
    }

    pub fn keep_and_print_path(self) {
        match self.0.keep() {
            Ok((_log_file, path)) => eprintln!(
                "the log of the emulator and the adb server is {}",
                path.display()
            ),
            Err(failure) => eprintln!(
                "{}",
                message_chain(RoundTripError::file(
                    FileOperation::KeepLog,
                    failure.file.path()
                )(failure.error))
            ),
        }
    }
}
