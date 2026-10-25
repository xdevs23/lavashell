// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn animated_backgrounds_paint_colors_in_the_default_and_a_chosen_space() -> TestResult {
    use crate::animation::Animation;
    use crate::color::ColorInterpolation;
    use crate::color::samples::{BLUE, VIOLET};
    let default = Animation::new(BLUE)?;
    let chosen = Animation::new(VIOLET.interpolated(ColorInterpolation::Srgb))?;
    let (a, b) = (default.clone(), chosen.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.spacer(
            20.dp(),
            20.dp(),
            Modifier::new().animated_background(a.clone()),
        );
        ui.spacer(
            20.dp(),
            20.dp(),
            Modifier::new().animated_background(b.clone()),
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let painted: Vec<Color> = scene
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Fill {
                brush: Brush::Solid(color),
                ..
            } => Some(*color),
            _ => None,
        })
        .collect();
    assert_eq!(painted, [default.get(), chosen.get().color]);
    Ok(())
}

#[test]
fn transition_color_interpolation_chooses_the_hue_path_of_the_background() -> TestResult {
    use crate::animation::{Easing, Tween};
    use crate::color::samples::{BLUE, VIOLET, assert_halfway_from_blue_to_violet};
    use crate::color::{ColorInterpolation, HuePath};
    let changed = State::new(false);
    let state = changed.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.spacer(
            100.dp(),
            80.dp(),
            Modifier::new()
                .background(if state.get() { VIOLET } else { BLUE })
                .transition(
                    Transition::new(
                        Tween::new(std::time::Duration::from_secs(1)).easing(Easing::Linear),
                    )
                    .color_interpolation(ColorInterpolation::Oklch(HuePath::Longer)),
                ),
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    changed.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let color = scene
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Fill {
                brush: Brush::Solid(color),
                ..
            } => Some(*color),
            _ => None,
        })
        .required("a solid fill")?;
    assert_halfway_from_blue_to_violet(color, HuePath::Longer);
    Ok(())
}

#[test]
fn an_exit_after_a_color_change_fades_without_turning_the_hue() -> TestResult {
    use crate::animation::{Easing, Tween};
    use crate::color::samples::{BLUE, VIOLET};
    use crate::color::{ColorInterpolation, HuePath};
    let shown = State::new(true);
    let changed = State::new(false);
    let (visible, violet) = (shown.clone(), changed.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if visible.get() {
            ui.key(
                "fading",
                Modifier::new()
                    .background(if violet.get() { VIOLET } else { BLUE })
                    .transition(
                        Transition::new(
                            Tween::new(std::time::Duration::from_secs(1)).easing(Easing::Linear),
                        )
                        .exit(Presence::fade())
                        .color_interpolation(ColorInterpolation::Oklch(HuePath::Longer)),
                    ),
                |ui| {
                    ui.spacer(80.dp(), 40.dp(), Modifier::new());
                },
            );
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    changed.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    let settled = view.transitions.only_animation()?.get().appearance;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    let fading = view.transitions.only_animation()?.get();
    let opacity = fading.presence.alpha();
    assert!(opacity > 0.0 && opacity < 1.0);
    assert_eq!(
        fading.appearance.background.color.to_argb8(),
        settled.background.color.to_argb8()
    );
    Ok(())
}
