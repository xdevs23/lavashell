// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::scene::test_sink::{Received, TestSink, TestSinkError};
use std::collections::BTreeSet;
use std::time::Duration;

const AREA: Viewport = Viewport::new(WINDOW_SIZE, Density::ONE);

/// A view of `count` white squares, each in a scoped component.
fn squares(count: State<usize>) -> View {
    View::new(crate::platform::fixture::facts(), move |ui| {
        for square in 0..count.get() {
            ui.scoped(square, Modifier::new(), |ui| {
                ui.spacer(20.dp(), 20.dp(), Modifier::new().background(Color::WHITE));
            });
        }
    })
}

fn commands(sink: &TestSink) -> Vec<DrawCommand> {
    (sink.tree.at(Duration::ZERO).leaf_commands())
        .cloned()
        .collect()
}

/// Whether `received` inserts at least one node and then ends the frame.
fn inserts_a_scene(received: &[Received]) -> bool {
    match received {
        [changes @ .., Received::End] => {
            !changes.is_empty() && changes.iter().all(|change| *change == Received::Insert)
        }
        _ => false,
    }
}

/// Whether `received` restarts the sink, then inserts at least one node and then ends the frame.
fn restarts_and_inserts_a_scene(received: &[Received]) -> bool {
    matches!(received, [Received::Restart, rest @ ..] if inserts_a_scene(rest))
}

fn planned(call: Received) -> FrameError<TestSinkError> {
    FrameError::Sink(TestSinkError::Planned(call))
}

#[test]
fn the_first_frame_into_a_sink_restarts_it_and_the_next_frame_sends_only_the_end() -> TestResult {
    let mut view = squares(State::new(1));
    let mut sink = TestSink::default();
    (view.frame_into(AREA, &mut Metrics, &mut sink)).without_failures()?;
    assert!(restarts_and_inserts_a_scene(&std::mem::take(
        &mut sink.received
    )));
    (view.frame_into(AREA, &mut Metrics, &mut sink)).without_failures()?;
    assert_eq!(sink.received, [Received::End]);
    Ok(())
}

#[test]
fn a_frame_into_another_sink_restarts_that_sink_and_sends_the_whole_scene() -> TestResult {
    let mut view = squares(State::new(1));
    let (mut first, mut second) = (TestSink::default(), TestSink::default());
    (view.frame_into(AREA, &mut Metrics, &mut first)).without_failures()?;
    (view.frame_into(AREA, &mut Metrics, &mut second)).without_failures()?;
    assert!(restarts_and_inserts_a_scene(&second.received));
    Ok(())
}

#[test]
fn a_change_that_the_sink_rejects_restarts_the_sink_and_the_next_frame_sends_all() -> TestResult {
    let count = State::new(1);
    let mut view = squares(count.clone());
    let mut sink = TestSink::default();
    (view.frame_into(AREA, &mut Metrics, &mut sink)).without_failures()?;
    sink.received.clear();
    sink.failing = BTreeSet::from([0]);
    count.set(2);
    assert!(matches!(
        view.frame_into(AREA, &mut Metrics, &mut sink),
        Err(FrameError::Sink(TestSinkError::Planned(_)))
    ));
    assert!(matches!(sink.received.as_slice(), [_, Received::Restart]));
    assert!(view.needs_visual_frame());
    sink.received.clear();
    (view.frame_into(AREA, &mut Metrics, &mut sink)).without_failures()?;
    assert!(inserts_a_scene(&sink.received));
    let mut expected = TestSink::default();
    (squares(count).frame_into(AREA, &mut Metrics, &mut expected)).without_failures()?;
    assert_eq!(commands(&sink), commands(&expected));
    Ok(())
}

#[test]
fn a_failed_end_of_a_frame_restarts_the_sink_and_the_next_frame_sends_all() -> TestResult {
    let mut view = squares(State::new(1));
    let mut sink = TestSink::default();
    (view.frame_into(AREA, &mut Metrics, &mut sink)).without_failures()?;
    sink.received.clear();
    sink.failing = BTreeSet::from([0]);
    let failed = view.frame_into(AREA, &mut Metrics, &mut sink);
    assert_eq!(failed, Err(planned(Received::End)));
    assert_eq!(sink.received, [Received::End, Received::Restart]);
    sink.received.clear();
    (view.frame_into(AREA, &mut Metrics, &mut sink)).without_failures()?;
    assert!(inserts_a_scene(&sink.received));
    Ok(())
}

#[test]
fn a_failed_restart_replaces_the_error_and_the_next_frame_restarts_again() -> TestResult {
    let count = State::new(1);
    let mut view = squares(count.clone());
    let mut sink = TestSink::default();
    (view.frame_into(AREA, &mut Metrics, &mut sink)).without_failures()?;
    sink.received.clear();
    sink.failing = BTreeSet::from([0, 1]);
    count.set(2);
    let failed = view.frame_into(AREA, &mut Metrics, &mut sink);
    assert_eq!(failed, Err(planned(Received::Restart)));
    sink.received.clear();
    (view.frame_into(AREA, &mut Metrics, &mut sink)).without_failures()?;
    assert!(restarts_and_inserts_a_scene(&sink.received));
    Ok(())
}

#[test]
fn a_failed_first_restart_sends_nothing_and_the_next_frame_restarts_again() -> TestResult {
    let mut view = squares(State::new(1));
    let mut sink = TestSink::default();
    sink.failing = BTreeSet::from([0]);
    let failed = view.frame_into(AREA, &mut Metrics, &mut sink);
    assert_eq!(failed, Err(planned(Received::Restart)));
    assert_eq!(std::mem::take(&mut sink.received), [Received::Restart]);
    (view.frame_into(AREA, &mut Metrics, &mut sink)).without_failures()?;
    assert!(restarts_and_inserts_a_scene(&sink.received));
    Ok(())
}

#[test]
fn the_next_frame_into_a_sink_returns_the_failures_of_a_frame_that_failed() -> TestResult {
    let broken = State::new(false);
    let reads = broken.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let broken = reads.clone();
        ui.scoped(0, Modifier::new(), move |ui| {
            if broken.get() {
                ui.fail(CompositionError::DuplicateItemKey);
            }
            ui.spacer(20.dp(), 20.dp(), Modifier::new().background(Color::WHITE));
        });
    });
    let mut sink = TestSink::default();
    (view.frame_into(AREA, &mut Metrics, &mut sink)).without_failures()?;
    sink.received.clear();
    sink.failing = BTreeSet::from([0]);
    broken.set(true);
    assert!(matches!(
        view.frame_into(AREA, &mut Metrics, &mut sink),
        Err(FrameError::Sink(TestSinkError::Planned(_)))
    ));
    let failures = view.frame_into(AREA, &mut Metrics, &mut sink)?;
    let causes: Vec<_> = (failures.into_iter())
        .map(|failure| failure.cause)
        .collect();
    assert_eq!(
        causes,
        [ComponentError::Composition(
            CompositionError::DuplicateItemKey
        )]
    );
    Ok(())
}
