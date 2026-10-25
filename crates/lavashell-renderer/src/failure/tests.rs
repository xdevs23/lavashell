// SPDX-License-Identifier: GPL-3.0-only
use super::{DrawFailure, FailedDraw, Failures};
use crate::Error;
use crate::test_error::TestError;
use lavashell_core::prelude::NumberError;
use std::{cell::RefCell, rc::Rc, sync::OnceLock};

impl Failures {
    pub(crate) fn count(&self, failure: DrawFailure) -> u64 {
        self.0.borrow().get(&failure).copied().unwrap_or_default()
    }
}

impl FailedDraw {
    pub(crate) fn cause(&self) -> Option<&Error> {
        self.cause.as_deref()
    }
}

thread_local! {
    static RECORDS: RefCell<Vec<(log::Level, String)>> = const { RefCell::new(Vec::new()) };
}

struct Recorder;

impl log::Log for Recorder {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        RECORDS
            .with_borrow_mut(|records| records.push((record.level(), record.args().to_string())));
    }

    fn flush(&self) {}
}

fn recorded(run: impl FnOnce()) -> Result<Vec<(log::Level, String)>, TestError> {
    static INSTALLED: OnceLock<Result<(), log::SetLoggerError>> = OnceLock::new();
    INSTALLED
        .get_or_init(|| {
            let installed = log::set_logger(&Recorder);
            log::set_max_level(log::LevelFilter::Trace);
            installed
        })
        .as_ref()
        .copied()
        .map_err(TestError::Logger)?;
    RECORDS.with_borrow_mut(Vec::clear);
    run();
    Ok(RECORDS.take())
}

#[test]
fn a_failure_is_logged_as_a_warning_whenever_its_count_doubles() -> Result<(), TestError> {
    let failures = Failures::default();
    let logged = (1..=9)
        .map(|_| {
            recorded(|| failures.record(DrawFailure::StreamStale)).map(|records| records.len())
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(logged, [1, 1, 0, 1, 0, 0, 0, 1, 0]);

    let records = recorded(|| {
        failures.record_caused(
            DrawFailure::ImageSize,
            &std::io::Error::other("a 4x4 image"),
        );
    })?;
    let [(level, message)] = records.as_slice() else {
        return Err(TestError::Unexpected("one log record"));
    };
    assert_eq!(*level, log::Level::Warn);
    assert!(message.contains("a 4x4 image"));
    Ok(())
}

#[test]
fn a_logged_cause_names_every_source_below_it() -> Result<(), TestError> {
    let source = NumberError::Negative;
    let cause = Error::TexelCount(source);
    let records = recorded(|| Failures::default().record_caused(DrawFailure::ImageSize, &cause))?;
    let [(_, message)] = records.as_slice() else {
        return Err(TestError::Unexpected("one log record"));
    };
    assert!(message.contains(&format!("{cause}: {source}")));
    Ok(())
}

#[test]
fn a_failed_draw_is_logged_with_its_cause() -> Result<(), TestError> {
    let source = NumberError::Negative;
    let cause = Rc::new(Error::TexelCount(source));
    let failed = FailedDraw::caused(DrawFailure::ImageSize, Rc::clone(&cause));
    let failures = Failures::default();
    let records = recorded(|| failures.record(failed))?;
    let [(_, message)] = records.as_slice() else {
        return Err(TestError::Unexpected("one log record"));
    };
    assert!(message.contains(&format!("{cause}: {source}")));
    assert_eq!(failures.count(DrawFailure::ImageSize), 1);
    Ok(())
}
