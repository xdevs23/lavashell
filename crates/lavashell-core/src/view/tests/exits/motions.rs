// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::float_asserts::assert_f64s_eq;

fn piece_leaving_with_a_quick_exit(shown: &State<bool>) -> View {
    let present = shown.clone();
    View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            let transition = Transition {
                bounds: BoundsMotion::Instant,
                ..Transition::new(linear(1000))
            }
            .exit_with(Presence::fade(), linear(200));
            ui.key(
                "piece",
                Modifier::new().background(PIECE).transition(transition),
                |ui| {
                    ui.spacer(20.dp(), 20.dp(), Modifier::new());
                },
            );
        }
    })
}

#[test]
fn a_node_with_an_exit_motion_leaves_with_it_and_not_with_its_transition_motion() -> TestResult {
    let shown = State::new(true);
    let mut scene = Scene::default();
    let mut view = piece_leaving_with_a_quick_exit(&shown);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    assert_f64s_eq(&presences(&scene), &[0.5]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 200)?;
    assert_eq!(fills(&scene, PIECE)?, []);
    assert_eq!(view.transitions.count(), 0);
    Ok(())
}

#[test]
fn a_node_regained_during_its_exit_motion_returns_with_its_transition_motion() -> TestResult {
    let shown = State::new(true);
    let mut scene = Scene::default();
    let mut view = piece_leaving_with_a_quick_exit(&shown);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    shown.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 600)?;
    assert_f64s_eq(&presences(&scene), &[0.75]);
    Ok(())
}
