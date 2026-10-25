// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::units::ScaleFactor;

#[test]
fn a_scaled_reflowing_node_scales_about_the_centre_of_its_glass() -> TestResult {
    const HALF: Positive = crate::constant!(Positive::try_new(0.5));
    let wide = State::new(false);
    let target = wide.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let scaled = Modifier::new().visual_scale(HALF);
        island(ui, "island", 20.dp(), scaled, |ui| pieces(ui, target.get()));
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    let glass = glass(&scene)?;
    near(center(glass), Point::new(105.dp(), 30.dp()));
    assert_eq!(glass.width, 85.dp());
    Ok(())
}

#[test]
fn a_reflowing_node_leaves_scaling_about_the_centre_of_its_glass() -> TestResult {
    let (wide, shown) = (State::new(false), State::new(true));
    let (target, present) = (wide.clone(), shown.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            let restarting = linear().preserve_velocity(false);
            let leaving = Modifier::new().transition(
                Transition::new(restarting)
                    .reflow()
                    .exit(Presence::fade().scale(crate::constant!(ScaleFactor::try_new(0.5)))),
            );
            island(ui, "island", 20.dp(), leaving, |ui| {
                pieces(ui, target.get());
            });
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    let glass = fills(&scene, ISLAND)?.remove(0);
    close(glass.rect.width / glass.scale.0, 195.dp());
    near(center(glass.rect), Point::new(117.5.dp(), 30.dp()));
    Ok(())
}
