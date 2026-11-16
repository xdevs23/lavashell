// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::{Easing, Tween};
use std::time::Duration;

fn fading_in() -> Modifier {
    let tween = Tween::new(Duration::from_millis(1000)).easing(Easing::Linear);
    Modifier::new().transition(Transition::new(tween).enter(Presence::fade()))
}

fn presences(scene: &Scene) -> Vec<f64> {
    (scene.groups().into_iter())
        .map(|group| group.opacity)
        .collect()
}

#[test]
fn rows_of_the_first_composition_skip_their_enter_and_later_rows_fade_in() -> TestResult {
    let later = State::new(false);
    let shown = later.clone();
    let mut view =
        View::new_skipping_first_frame_enters(crate::platform::fixture::facts(), move |ui| {
            ui.spacer(20.dp(), 20.dp(), fading_in());
            if shown.get() {
                ui.spacer(20.dp(), 20.dp(), fading_in());
            }
        });
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(presences(&scene), [1.0]);
    later.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(presences(&scene), [1.0, 0.0]);
    Ok(())
}

#[test]
fn rows_left_after_a_failed_paint_in_the_first_frame_skip_their_enter() {
    let later = State::new(false);
    let shown = later.clone();
    let mut view =
        View::new_skipping_first_frame_enters(crate::platform::fixture::facts(), move |ui| {
            ui.spacer(20.dp(), 20.dp(), fading_in());
            ui.scoped("radar", fading_in(), |ui| {
                ui.spacer(10.dp(), 10.dp(), fading_in().clip(Dp::new(f64::NAN)));
            });
            if shown.get() {
                ui.spacer(20.dp(), 20.dp(), fading_in());
            }
        });
    let failed = |result| matches!(result, Err(TestError::Failed(_)));
    let mut scene = Scene::default();
    assert!(failed(show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)));
    assert_eq!(presences(&scene), [1.0, 1.0, 1.0]);
    later.set(true);
    assert!(failed(show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)));
    assert_eq!(presences(&scene), [1.0, 1.0, 1.0, 0.0]);
}
