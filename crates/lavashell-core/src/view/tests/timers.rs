// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::preferences::REDUCED_MOTION;
use std::time::Duration;

#[test]
fn a_view_reports_its_timers_to_the_host_and_runs_them_at_the_frame_time() -> TestResult {
    let runs = State::new(0);
    let count = runs.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |_| {});
    let timer = view.motion().timer(move || count.update(|runs| *runs += 1));
    view.provide(&REDUCED_MOTION, true);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    timer.start(Duration::from_millis(70));
    assert_eq!(view.next_deadline(), Some(Duration::from_millis(70)));
    assert!(!view.needs_visual_frame());
    advance(&mut scene, &mut view, WINDOW_SIZE, 69)?;
    assert_eq!(runs.get(), 0);
    advance(&mut scene, &mut view, WINDOW_SIZE, 70)?;
    assert_eq!(runs.get(), 1);
    assert_eq!(view.next_deadline(), None);
    timer.start(Duration::ZERO);
    assert!(view.needs_visual_frame());
    advance(&mut scene, &mut view, WINDOW_SIZE, 70)?;
    assert_eq!(runs.get(), 2);
    assert!(!view.needs_visual_frame());
    Ok(())
}
