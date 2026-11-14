// SPDX-License-Identifier: GPL-3.0-only
use super::{
    KEYBOARD, TestResult, app, growing_bar, key_a, observed_times, recorded_times, size_and_middle,
};
use crate::{Error, FrameInterval, HostScript, Pace, StepsPace, TestPlatform};
use lavashell_core::prelude::{Density, constant};
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{DeviceEvent, SurfaceEvent};
use lavashell_renderer::skia::ISize;
use lavashell_runtime::{RunError, run};
use std::time::Duration;
use std::{cell::Cell, mem, rc::Rc};

/// A view observer that publishes to `pace` what `steps` returns for each frame.
fn publishing(
    pace: &Pace,
    steps: impl Fn(&View) -> StepsPace + 'static,
) -> impl FnMut(SurfaceId, &View) + 'static {
    let pace = pace.clone();
    move |_, view| pace.publish(steps(view))
}

fn paced(pace: Pace) -> HostScript {
    HostScript::new(Density::ONE).record_paced(pace)
}

#[test]
fn paced_play_moves_the_clock_to_the_due_time_of_the_steps() -> TestResult {
    let (pace, due) = (Pace::default(), Duration::from_millis(250));
    let framing = app(|_| {})?.on_frame(|_| {});
    let steps = publishing(&pace, move |view| {
        if view.motion().now() < due {
            StepsPace::Waiting {
                due: Some(due),
                every: None,
            }
        } else {
            StepsPace::Ended
        }
    });
    assert_eq!(
        observed_times(paced(pace), framing, steps)?,
        [Duration::ZERO, due]
    );
    Ok(())
}

#[test]
fn paced_play_samples_a_motion_at_the_interval_of_the_steps_until_it_rests() -> TestResult {
    let pace = Pace::default();
    let every = Some(constant!(FrameInterval::per_second(40)));
    let steps = publishing(&pace, move |_| StepsPace::Waiting { due: None, every });
    assert_eq!(
        observed_times(paced(pace), growing_bar()?, steps)?,
        [0, 25, 50, 75, 100].map(Duration::from_millis)
    );
    Ok(())
}

#[test]
fn paced_play_presents_at_an_unchanged_time_while_an_action_waits_for_its_frame() -> TestResult {
    let pace = Pace::default();
    let bar = growing_bar()?;
    let actions = bar.view_actions();
    let published = pace.clone();
    let mut first = true;
    let shown = observed_times(paced(pace), bar, move |surface, _| {
        if mem::take(&mut first) {
            actions.dispatch(surface, View::invalidate);
            published.publish(StepsPace::Queued);
        } else {
            published.publish(StepsPace::Ended);
        }
    });
    assert_eq!(shown?, [Duration::ZERO; 2]);
    Ok(())
}

#[test]
fn paced_play_without_steps_follows_the_deadlines_until_the_run_rests() -> TestResult {
    assert_eq!(
        observed_times(paced(Pace::default()), growing_bar()?, |_, _| {})?,
        [0, 100].map(Duration::from_millis)
    );
    Ok(())
}

#[test]
fn frames_that_keep_arriving_at_an_unchanged_time_fail_paced_play() -> TestResult {
    let app = app(|_| {})?.on_frame(|_| {});
    let (ran, _frames_before_the_failure) =
        paced(Pace::default()).serve(|| run::<TestPlatform>(app));
    assert!(matches!(ran, Err(RunError::Platform(Error::NoRest { time })) if time.is_zero()));
    Ok(())
}

#[test]
fn paced_play_reports_the_input_that_the_steps_hand_to_the_pace() -> TestResult {
    let (pace, pressed) = (Pace::default(), Rc::new(Cell::new(0)));
    let (handed, seen) = (pace.clone(), Rc::clone(&pressed));
    let mut first = true;
    let app = app(|_| {})?
        .on_keyboard(move |_| {
            seen.set(seen.get() + 1);
            false
        })
        .view_observer(move |surface, _, _, _| {
            if mem::take(&mut first) {
                handed.report(DeviceEvent::KeyboardFocus {
                    keyboard: KEYBOARD,
                    surface: Some(surface),
                });
                handed.report(key_a());
            }
            handed.publish(StepsPace::Ended);
        });
    let (ran, _frames_of_the_empty_window) = paced(pace).serve(|| run::<TestPlatform>(app));
    ran?;
    assert_eq!(pressed.get(), 1);
    Ok(())
}

#[test]
fn paced_play_records_the_frames_shown_after_a_request_to_record_and_at_the_end() -> TestResult {
    let (pace, asked) = (Pace::default(), Duration::from_millis(50));
    let every = Some(constant!(FrameInterval::per_second(40)));
    let steps = pace.clone();
    let app = growing_bar()?.view_observer(move |_, timing, _, _| {
        if timing.presentation == asked {
            steps.record();
        }
        steps.publish(StepsPace::Waiting { due: None, every });
    });
    assert_eq!(
        recorded_times(paced(pace), app)?,
        [asked, Duration::from_millis(100)]
    );
    Ok(())
}

#[test]
fn paced_play_records_the_frame_of_a_window_closed_during_the_step() -> TestResult {
    let pace = Pace::default();
    let closing = pace.clone();
    let mut first = true;
    let app = app(|_| {})?.view_observer(move |surface, _, _, _| {
        if mem::take(&mut first) {
            closing.report((surface, SurfaceEvent::Closed));
        }
    });
    assert_eq!(recorded_times(paced(pace), app)?, [Duration::ZERO]);
    Ok(())
}

#[test]
fn a_default_pace_records_one_frame_at_the_end_decoded_at_the_painted_size() -> TestResult {
    let app = app(|_| {})?;
    let (ran, frames) = paced(Pace::default()).serve(|| run::<TestPlatform>(app));
    ran?;
    let sizes: Vec<_> = (frames?.frames().iter())
        .map(|frame| size_and_middle(frame).map(|(size, _)| size))
        .collect();
    assert_eq!(sizes, [Some(ISize::new(100, 100))]);
    Ok(())
}
