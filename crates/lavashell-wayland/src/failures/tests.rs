// SPDX-License-Identifier: GPL-3.0-only
use super::log_failures;
use crate::preference_reader::NoText;
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use lavashell_test_fixtures::FrameFailure;
use log::SetLoggerError;
use std::{cell::RefCell, sync::OnceLock};

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
    static INSTALLED: OnceLock<Result<(), SetLoggerError>> = OnceLock::new();
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
fn each_failed_component_is_logged_as_an_error_with_its_causes() -> Result<(), TestError> {
    let mut view = View::new(lavashell_test_fixtures::facts(), |ui| {
        ui.scoped("forecast", Modifier::new().tag("forecast"), |ui| {
            ui.fail(NumberError::OutOfRange);
        });
        ui.scoped("radar", Modifier::new().tag("radar"), |ui| {
            ui.fail(NumberError::NotFinite);
        });
    });
    let viewport = Viewport::new(Size::new(10.dp(), 10.dp()), Density::ONE);
    let frame = (view.frame(viewport, &mut NoText)).map_err(FrameFailure::Frame)?;
    let expected = [
        "component \"forecast\" failed: a component composes a number that is not valid: \
         the number is outside the range of its type",
        "component \"radar\" failed: a component composes a number that is not valid: \
         the number must be finite",
    ]
    .map(|message| (log::Level::Error, message.to_owned()));
    assert_eq!(recorded(|| log_failures(frame.failures))?, expected);
    Ok(())
}
