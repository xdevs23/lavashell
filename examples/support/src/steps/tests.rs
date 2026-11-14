// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::platform;
use std::cell::Cell;

fn timing(sequence: u64, milliseconds: u64) -> FrameTime {
    FrameTime {
        presentation: Duration::from_millis(milliseconds),
        sequence,
        ..FrameTime::default()
    }
}

fn presented(surface: u64, sequence: u64, milliseconds: u64) -> RecordedFrame {
    RecordedFrame {
        surface: SurfaceId(surface),
        time: timing(sequence, milliseconds),
        png: Vec::new(),
    }
}

fn marked(surface: u64, sequence: u64, milliseconds: u64) -> Marked {
    Marked {
        surface: SurfaceId(surface),
        time: timing(sequence, milliseconds),
    }
}

fn labels(labeled: Vec<(&'static str, RecordedFrame)>) -> Vec<(&'static str, u64, u64)> {
    (labeled.into_iter())
        .map(|(label, frame)| (label, frame.surface.0, frame.time.sequence))
        .collect()
}

fn view() -> View {
    View::new(platform(), |_| {})
}

fn milliseconds(count: u64) -> Duration {
    Duration::from_millis(count)
}

/// Runs the steps that a frame of a view at the start of its clock starts, as the view observer of a run does.
fn observe_frame<C: 'static, L: 'static>(
    steps: Steps<C, L, Error>,
) -> Rc<RefCell<Playing<C, L, Error>>> {
    let playing = Rc::new(RefCell::new(steps.into_playing()));
    let actions = App::new(platform()).view_actions();
    let viewport = Viewport::new(Size::new(10.dp(), 20.dp()), Density::ONE);
    observe(&playing, &actions, marked(1, 1, 0), viewport, &view());
    playing
}

#[test]
fn each_frame_shows_the_newest_mark_started_by_it_or_by_an_earlier_frame() {
    let started = [(marked(1, 2, 12), "a"), (marked(1, 3, 20), "c")];
    let frames = vec![
        presented(1, 1, 5),
        presented(1, 2, 12),
        presented(1, 3, 20),
        presented(1, 4, 30),
    ];
    assert_eq!(
        labels(label(frames, &started)),
        [("a", 1, 2), ("c", 1, 3), ("c", 1, 4)]
    );
}

#[test]
fn a_mark_labels_the_frame_that_started_it_and_not_an_earlier_frame_at_the_same_time() {
    let started = [(marked(1, 2, 20), "action")];
    let frames = vec![presented(1, 1, 20), presented(1, 2, 20)];
    assert_eq!(labels(label(frames, &started)), [("action", 1, 2)]);
}

#[test]
fn a_frame_of_another_surface_is_labeled_by_its_presentation_time() {
    let started = [(marked(1, 1, 10), "a"), (marked(1, 2, 20), "b")];
    let frames = vec![
        presented(1, 1, 10),
        presented(2, 1, 15),
        presented(1, 2, 20),
    ];
    assert_eq!(
        labels(label(frames, &started)),
        [("a", 1, 1), ("a", 2, 1), ("b", 1, 2)]
    );
}

#[test]
fn a_mark_without_a_later_frame_is_shown_by_the_newest_frame_of_each_surface() {
    let started = [
        (marked(1, 1, 12), "a"),
        (marked(1, 2, 15), "b"),
        (marked(1, 2, 15), "c"),
    ];
    let frames = vec![
        presented(1, 1, 12),
        presented(2, 1, 14),
        presented(1, 2, 15),
        presented(1, 3, 25),
    ];
    assert_eq!(
        labels(label(frames, &started)),
        [
            ("a", 1, 1),
            ("a", 2, 1),
            ("b", 1, 2),
            ("b", 2, 1),
            ("c", 1, 2),
            ("c", 1, 3)
        ]
    );
}

#[test]
fn marks_that_start_in_the_same_frame_each_show_that_frame() {
    let started = [(marked(1, 2, 20), "b"), (marked(1, 2, 20), "c")];
    let frames = vec![presented(1, 1, 10), presented(1, 2, 20)];
    assert_eq!(labels(label(frames, &started)), [("b", 1, 2), ("c", 1, 2)]);
}

#[test]
fn next_starts_in_the_frame_of_the_step_before() -> Result<(), Error> {
    let view = view();
    let due = Start::NEXT.due(&view)?;
    assert_eq!(due, None);
    assert!(Start::NEXT.starts(&view, Duration::ZERO, due));
    Ok(())
}

#[test]
fn after_starts_once_its_delay_passed_since_the_frame_of_the_step_before() -> Result<(), Error> {
    let (view, after) = (view(), Start::after(milliseconds(10)));
    view.motion().advance(milliseconds(5));
    let due = after.due(&view)?;
    assert!(!after.starts(&view, milliseconds(14), due));
    assert!(after.starts(&view, milliseconds(15), due));
    assert!(
        after.starts(&view, Duration::ZERO, None),
        "a first step starts at once"
    );
    Ok(())
}

#[test]
fn after_reading_reads_its_delay_from_the_view_of_the_frame_of_the_step_before() -> Result<(), Error>
{
    let view = view();
    view.motion().advance(milliseconds(6));
    let reading = Start::after_reading(|view| Some(view.motion().now() / 2));
    assert_eq!(reading.due(&view)?, Some(milliseconds(9)));
    let unread = Start::after_reading(|_| None).due(&view);
    assert!(matches!(unread, Err(Error::UnreadDelay)));
    Ok(())
}

#[test]
fn when_starts_once_its_condition_holds() {
    let view = view();
    assert!(!Start::when(|_| false).starts(&view, Duration::ZERO, None));
    assert!(Start::when(|_| true).starts(&view, Duration::ZERO, None));
}

#[test]
fn at_rest_waits_while_the_view_needs_a_frame() {
    assert!(!Start::at_rest().starts(&view(), Duration::ZERO, None));
}

#[test]
fn the_steps_wait_for_the_due_time_and_the_interval_of_the_next_step() {
    let every = constant!(FrameInterval::per_second(60));
    let mut steps = Steps::<(), &str, Error>::new(());
    steps.mark(Start::after(milliseconds(10)), "late");
    let (mut playing, view) = (steps.into_playing(), view());
    playing.step_ran(Ok(()));
    assert_eq!(playing.waits_for(), StepsPace::Queued);
    assert!(playing.take_started(&view, Duration::ZERO).is_none());
    let due = Some(milliseconds(10));
    assert_eq!(playing.waits_for(), StepsPace::Waiting { due, every: None });
    playing.waiting[0].start = Start::sampled(every, |_| false);
    playing.step_ran(Ok(()));
    assert!(playing.take_started(&view, Duration::ZERO).is_none());
    let every = Some(every);
    assert_eq!(playing.waits_for(), StepsPace::Waiting { due: None, every });
}

#[test]
fn a_frame_runs_each_check_and_mark_that_it_starts_until_the_steps_end() {
    let seen = Rc::new(Cell::new(None));
    let checked = Rc::clone(&seen);
    let mut steps = Steps::<u8, &str, Error>::new(7);
    steps.check(Start::NEXT, move |_, viewport, context| {
        checked.set(Some((viewport.size, *context)));
        Ok(())
    });
    steps.mark(Start::NEXT, "shown");
    let played = observe_frame(steps);
    let played = played.borrow();
    assert_eq!(seen.get(), Some((Size::new(10.dp(), 20.dp()), 7)));
    let started: Vec<_> = (played.started.iter())
        .map(|(marked, label)| (marked.surface, marked.time.sequence, *label))
        .collect();
    assert_eq!(started, [(SurfaceId(1), 1, "shown")]);
    assert_eq!(played.waits_for(), StepsPace::Ended);
}

#[test]
fn a_failed_check_ends_the_steps_before_the_steps_after_it() {
    let mut steps = Steps::<(), &str, Error>::new(());
    steps.check(Start::NEXT, |_, _, ()| Err(Error::UnplayedSteps(1)));
    steps.mark(Start::NEXT, "after the failure");
    let played = observe_frame(steps);
    let played = played.borrow();
    assert!(matches!(
        played.phase,
        Phase::Failed(Error::UnplayedSteps(1))
    ));
    assert!(played.started.is_empty());
    assert_eq!(played.waits_for(), StepsPace::Ended);
}

#[test]
fn an_action_waits_for_its_frame_and_a_later_step_asks_for_a_frame_at_its_due_time() {
    let mut steps = Steps::<(), &str, Error>::new(());
    steps.mark(Start::NEXT, "first");
    steps.mark(Start::after(milliseconds(10)), "late");
    let waiting = observe_frame(steps);
    let waiting = waiting.borrow();
    assert!(waiting.frame_requested);
    let due = Some(milliseconds(10));
    assert_eq!(waiting.waits_for(), StepsPace::Waiting { due, every: None });

    let mut steps = Steps::<(), &str, Error>::new(());
    steps.push(Start::NEXT, |_, _, ()| Ok(()));
    assert_eq!(observe_frame(steps).borrow().waits_for(), StepsPace::Queued);
}

#[test]
fn a_press_of_keys_waits_for_the_frame_that_shows_them() {
    let mut steps = Steps::<(), &str, Error>::new(());
    let enter = KeyboardKey {
        code: 28,
        key: Key::Enter,
    };
    steps.press(Start::NEXT, [enter]);
    steps.mark(Start::NEXT, "after the keys");
    let pressed = observe_frame(steps);
    let pressed = pressed.borrow();
    assert!(pressed.started.is_empty());
    assert_eq!(pressed.waits_for(), StepsPace::Queued);
}

/// The label and presentation time of each frame that `steps` record while they play on a window of 10 by 10 dp.
fn play_labeled_times(
    steps: Steps<(), &'static str, Error>,
) -> Result<Vec<(&'static str, Duration)>, Error> {
    let mut app = App::new(platform());
    app.window(
        Window::new("test", "test.steps").size(10.dp(), 10.dp()),
        |_| {},
    )?;
    Ok((steps.play(app, Density::ONE)?.into_iter())
        .map(|(label, frame)| (label, frame.time.presentation))
        .collect())
}

#[test]
fn a_mark_asks_the_pace_to_record_the_frame_that_started_it() -> Result<(), Error> {
    let mut steps = Steps::<(), &str, Error>::new(());
    steps.mark(Start::NEXT, "first");
    steps.mark(Start::after(milliseconds(100)), "late");
    assert_eq!(
        play_labeled_times(steps)?,
        [("first", Duration::ZERO), ("late", milliseconds(100))]
    );
    Ok(())
}

#[test]
fn a_sequence_mark_asks_the_pace_to_record_each_frame_until_the_next_mark() -> Result<(), Error> {
    let mut steps = Steps::<(), &str, Error>::new(());
    steps.mark_sequence(Start::NEXT, "sequence");
    steps.check(Start::after(milliseconds(25)), |_, _, ()| Ok(()));
    steps.mark(Start::after(milliseconds(25)), "end");
    steps.check(Start::after(milliseconds(25)), |_, _, ()| Ok(()));
    let expected = [("sequence", 0), ("sequence", 25), ("end", 50), ("end", 75)];
    assert_eq!(
        play_labeled_times(steps)?,
        expected.map(|(label, at)| (label, milliseconds(at)))
    );
    Ok(())
}

#[test]
fn a_delay_counts_from_the_frame_of_the_step_before_or_of_its_action() -> Result<(), Error> {
    let mut steps = Steps::<(), &str, Error>::new(());
    steps.mark(Start::NEXT, "first");
    steps.mark(Start::after(milliseconds(25)), "fixed");
    steps.mark(
        Start::after_reading(|view| Some(view.motion().now())),
        "read",
    );
    steps.push(Start::NEXT, |_, _, ()| Ok(()));
    steps.mark(Start::after(milliseconds(10)), "after the action");
    let expected = [
        ("first", 0),
        ("fixed", 25),
        ("read", 50),
        ("after the action", 60),
    ];
    assert_eq!(
        play_labeled_times(steps)?,
        expected.map(|(label, at)| (label, milliseconds(at)))
    );
    Ok(())
}

#[test]
fn a_delay_that_the_view_does_not_give_fails_the_steps() {
    let mut steps = Steps::<(), &str, Error>::new(());
    steps.mark(Start::NEXT, "first");
    steps.mark(Start::after_reading(|_| None), "never");
    assert!(matches!(play_labeled_times(steps), Err(Error::UnreadDelay)));
}
