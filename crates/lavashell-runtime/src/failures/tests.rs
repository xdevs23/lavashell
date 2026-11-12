// SPDX-License-Identifier: GPL-3.0-only
use super::log_failures;
use super::test_error::FailuresTestError;
use crate::observers::Observers;
use crate::test_error::TestResult;
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::{ComponentFailure, Failures};
use lavashell_test_fixtures::{FrameFailure, NoText};
use log::SetLoggerError;
use std::rc::Rc;
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

fn recorded(run: impl FnOnce()) -> TestResult<Vec<(log::Level, String)>> {
    static INSTALLED: OnceLock<Result<(), SetLoggerError>> = OnceLock::new();
    INSTALLED
        .get_or_init(|| {
            let installed = log::set_logger(&Recorder);
            log::set_max_level(log::LevelFilter::Trace);
            installed
        })
        .as_ref()
        .copied()
        .map_err(FailuresTestError::Logger)?;
    RECORDS.with_borrow_mut(Vec::clear);
    run();
    Ok(RECORDS.take())
}

/// The failures of a frame of a view whose components "forecast" and "radar" fail.
fn failed_frame() -> TestResult<Failures> {
    let mut view = View::new(lavashell_test_fixtures::facts(), |ui| {
        ui.scoped("forecast", Modifier::new().tag("forecast"), |ui| {
            ui.fail(NumberError::OutOfRange);
        });
        ui.scoped("radar", Modifier::new().tag("radar"), |ui| {
            ui.fail(NumberError::NotFinite);
        });
    });
    let viewport = Viewport::new(Size::new(10.dp(), 10.dp()), Density::ONE);
    let mut scene = SceneBuilder::default();
    let frame = view.frame_into(viewport, &mut NoText, &mut scene);
    Ok(frame.map_err(FrameFailure::Frame)?)
}

#[test]
fn each_failed_component_is_logged_as_an_error_with_its_causes() -> TestResult {
    let failures = failed_frame()?;
    let expected = [
        "component \"forecast\" failed: a component composes a number that is not valid: \
         the number is outside the range of its type",
        "component \"radar\" failed: a component composes a number that is not valid: \
         the number must be finite",
    ]
    .map(|message| (log::Level::Error, message.to_owned()));
    assert_eq!(recorded(|| log_failures(failures))?, expected);
    Ok(())
}

#[test]
fn a_failure_observer_receives_each_failed_component_in_place_of_the_log() -> TestResult {
    let failures = failed_frame()?;
    let surface = SurfaceId(7);
    let expected: Vec<_> = (failures.clone().into_iter())
        .map(|failure| (surface, failure))
        .collect();
    let received = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&received);
    let mut observers = Observers {
        failure: Some(Box::new(
            move |surface: SurfaceId, failure: ComponentFailure| {
                observed.borrow_mut().push((surface, failure));
            },
        )),
        ..Observers::default()
    };
    assert!(recorded(|| observers.report_failures(surface, failures))?.is_empty());
    assert_eq!(received.take(), expected);
    Ok(())
}
