// SPDX-License-Identifier: GPL-3.0-only
use super::{TestResult, app, growing_bar, observed_times, recorded_times};
use crate::linux_key_code::convert_to_linux_key_code;
use crate::test_renderer::TestPlatformWithoutDrawing;
use crate::{Error, FrameInterval, HostScript, Pace, PacedLinks};
use lavashell_core::keyboard::Key;
use lavashell_core::prelude::{Density, constant};
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{KeyAction, SurfaceEvent};
use lavashell_runtime::{RunError, run};
use std::time::Duration;
use std::{cell::RefCell, mem, rc::Rc};

fn publishing(
    links: &PacedLinks,
    steps: impl Fn(&View) -> Pace + 'static,
) -> impl FnMut(SurfaceId, &View) + 'static {
    let pace = links.pace.clone();
    move |_, view| pace.publish(steps(view))
}

fn paced(links: PacedLinks) -> HostScript {
    HostScript::new(Density::ONE).record_paced(links)
}

#[test]
fn paced_play_moves_the_clock_to_the_due_time_of_the_steps() -> TestResult {
    let (links, due) = (PacedLinks::default(), Duration::from_millis(250));
    let framing = app(|_| {})?.on_frame(|_| {});
    let steps = publishing(&links, move |view| {
        if view.motion().now() < due {
            Pace::Wake {
                at: Some(due),
                every: None,
            }
        } else {
            Pace::Finished
        }
    });
    assert_eq!(
        observed_times(paced(links), framing, steps)?,
        [Duration::ZERO, due]
    );
    Ok(())
}

#[test]
fn paced_play_samples_a_motion_at_the_interval_of_the_steps_until_it_rests() -> TestResult {
    let links = PacedLinks::default();
    let every = Some(constant!(FrameInterval::per_second(40)));
    let steps = publishing(&links, move |_| Pace::Wake { at: None, every });
    assert_eq!(
        observed_times(paced(links), growing_bar()?, steps)?,
        [0, 25, 50, 75, 100].map(Duration::from_millis)
    );
    Ok(())
}

#[test]
fn paced_play_presents_at_an_unchanged_time_while_an_action_waits_for_its_frame() -> TestResult {
    let links = PacedLinks::default();
    let bar = growing_bar()?;
    let actions = bar.view_actions();
    let published = links.pace.clone();
    let mut first = true;
    let shown = observed_times(paced(links), bar, move |surface, _| {
        if mem::take(&mut first) {
            actions.dispatch(surface, View::invalidate);
            published.publish(Pace::FrameAtSameTime);
        } else {
            published.publish(Pace::Finished);
        }
    });
    assert_eq!(shown?, [Duration::ZERO; 2]);
    Ok(())
}

#[test]
fn paced_play_without_steps_follows_the_deadlines_until_the_run_rests() -> TestResult {
    assert_eq!(
        observed_times(paced(PacedLinks::default()), growing_bar()?, |_, _| {})?,
        [0, 100].map(Duration::from_millis)
    );
    Ok(())
}

#[test]
fn frames_that_keep_arriving_at_an_unchanged_time_fail_paced_play() -> TestResult {
    let app = app(|_| {})?.on_frame(|_| {});
    let (ran, _frames_before_the_failure) =
        paced(PacedLinks::default()).serve(|host| run::<TestPlatformWithoutDrawing>(app, host));
    assert!(matches!(ran, Err(RunError::Platform(Error::NoRest { time })) if time.is_zero()));
    Ok(())
}

#[test]
fn paced_play_reports_the_keys_that_the_steps_press_through_the_pace() -> TestResult {
    let (links, keys) = (PacedLinks::default(), Rc::new(RefCell::new(Vec::new())));
    let pressed = Rc::new(RefCell::new(Ok(())));
    let (pressing, seen, pressed_in_view) = (links.clone(), Rc::clone(&keys), Rc::clone(&pressed));
    let mut first = true;
    let app = app(|_| {})?
        .on_keyboard(move |key| {
            seen.borrow_mut().push((Some(key.code), key.action));
            false
        })
        .view_observer(move |surface, _, _, _| {
            if mem::take(&mut first) {
                *pressed_in_view.borrow_mut() = pressing.input.press(surface, &[Key::Enter]);
            }
            pressing.pace.publish(Pace::Finished);
        });
    let (ran, _frames_of_the_empty_window) =
        paced(links).serve(|host| run::<TestPlatformWithoutDrawing>(app, host));
    ran?;
    pressed.replace(Ok(()))?;
    let enter = convert_to_linux_key_code(Key::Enter);
    assert_eq!(
        *keys.borrow(),
        [(enter, KeyAction::Pressed), (enter, KeyAction::Released)]
    );
    Ok(())
}

#[test]
fn paced_play_records_the_frames_shown_after_a_request_to_record_and_at_the_end() -> TestResult {
    let (links, asked) = (PacedLinks::default(), Duration::from_millis(50));
    let every = Some(constant!(FrameInterval::per_second(40)));
    let steps = links.clone();
    let app = growing_bar()?.view_observer(move |_, timing, _, _| {
        if timing.presentation == asked {
            steps.record_requests.request();
        }
        steps.pace.publish(Pace::Wake { at: None, every });
    });
    assert_eq!(
        recorded_times(paced(links), app)?,
        [asked, Duration::from_millis(100)]
    );
    Ok(())
}

#[test]
fn paced_play_records_the_frame_of_a_window_closed_during_the_step() -> TestResult {
    let links = PacedLinks::default();
    let closing = links.input.clone();
    let mut first = true;
    let app = app(|_| {})?.view_observer(move |surface, _, _, _| {
        if mem::take(&mut first) {
            closing.report((surface, SurfaceEvent::Closed));
        }
    });
    assert_eq!(recorded_times(paced(links), app)?, [Duration::ZERO]);
    Ok(())
}
