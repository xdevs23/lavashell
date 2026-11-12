// SPDX-License-Identifier: GPL-3.0-only
use super::{TestResult, app, growing_bar, presentation_times};
use crate::{Error, FrameInterval, HostScript, Pace, StepsPace, TestPlatform};
use lavashell_core::prelude::{Density, constant};
use lavashell_core::view::View;
use lavashell_runtime::{App, RunError, run};
use std::mem;
use std::time::Duration;

/// `app` with a view observer that publishes to `pace` what `steps` returns for each frame.
fn publishing(app: App, pace: &Pace, steps: impl Fn(&View) -> StepsPace + 'static) -> App {
    let pace = pace.clone();
    app.view_observer(move |_, _, _, view| pace.publish(steps(view)))
}

fn paced(pace: Pace) -> HostScript {
    HostScript::new(Density::ONE).record_paced(pace)
}

#[test]
fn paced_play_moves_the_clock_to_the_due_time_of_the_steps() -> TestResult {
    let (pace, due) = (Pace::default(), Duration::from_millis(250));
    let framing = app(|_| {})?.on_frame(|_| {});
    let app = publishing(framing, &pace, move |view| {
        if view.motion().now() < due {
            StepsPace::Waiting {
                due: Some(due),
                every: None,
            }
        } else {
            StepsPace::Ended
        }
    });
    assert_eq!(presentation_times(paced(pace), app)?, [Duration::ZERO, due]);
    Ok(())
}

#[test]
fn paced_play_samples_a_motion_at_the_interval_of_the_steps_until_it_rests() -> TestResult {
    let pace = Pace::default();
    let every = Some(constant!(FrameInterval::per_second(40)));
    let app = publishing(growing_bar()?, &pace, move |_| StepsPace::Waiting { due: None, every });
    assert_eq!(
        presentation_times(paced(pace), app)?,
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
    let app = bar.view_observer(move |surface, _, _, _| {
        if mem::take(&mut first) {
            actions.dispatch(surface, View::invalidate);
            published.publish(StepsPace::Queued);
        } else {
            published.publish(StepsPace::Ended);
        }
    });
    assert_eq!(presentation_times(paced(pace), app)?, [Duration::ZERO; 2]);
    Ok(())
}

#[test]
fn paced_play_without_steps_follows_the_deadlines_until_the_run_rests() -> TestResult {
    assert_eq!(
        presentation_times(paced(Pace::default()), growing_bar()?)?,
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
