// SPDX-License-Identifier: GPL-3.0-only
mod host_surface;
mod input;
mod motion;
mod stream_tests;

use crate::platform::TestError;
use log::SetLoggerError;
use std::{cell::RefCell, sync::OnceLock};

thread_local! {
    static WARNINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct Recorder;

impl log::Log for Recorder {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            WARNINGS.with_borrow_mut(|warnings| warnings.push(record.args().to_string()));
        }
    }

    fn flush(&self) {}
}

fn warnings(run: impl FnOnce() -> Result<(), TestError>) -> Result<Vec<String>, TestError> {
    static INSTALL: OnceLock<Result<(), SetLoggerError>> = OnceLock::new();
    let installed = INSTALL.get_or_init(|| {
        log::set_logger(&Recorder)?;
        log::set_max_level(log::LevelFilter::Warn);
        Ok(())
    });
    if let Err(error) = installed {
        return Err(TestError::InstallLogger(error));
    }
    WARNINGS.with_borrow_mut(Vec::clear);
    run()?;
    Ok(WARNINGS.take())
}
