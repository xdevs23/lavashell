// SPDX-License-Identifier: GPL-3.0-only
use super::material_motion::linear;
use super::*;
use crate::color::HuePath;
use crate::color::samples::{BLUE, VIOLET, assert_halfway_from_blue_to_violet};
use crate::preferences::REDUCED_MOTION;
use crate::scene::{Icon, IconPath, Segment};
use crate::style::{BorderStyle, InteractionAppearance, InteractionState, StateStyles};
use crate::ui::IconOptions;
use crate::units::{Fraction, NonNegativeDp, ScaleFactor};

const CENTER: Point = Point::new(Dp::new(100.0), Dp::new(20.0));

static LINE: Icon = Icon {
    grid: Size::new(Dp::new(24.0), Dp::new(24.0)),
    paths: &[IconPath {
        start: Point::new(Dp::new(4.0), Dp::new(12.0)),
        segments: &[Segment::Line(Point::new(Dp::new(20.0), Dp::new(12.0)))],
        closed: false,
        stroke: Some(2.0),
    }],
};

fn text_color(scene: &Scene, value: &str) -> Result<Color, TestError> {
    (scene.commands().into_iter())
        .filter_map(|command| match command {
            DrawCommand::Text { text, style, .. } if text == value => Some(style.color),
            _ => None,
        })
        .next_back()
        .ok_or_else(|| TestError::NoText(value.to_owned()))
}

fn icon_color(scene: &Scene) -> Result<Color, TestError> {
    (scene.commands().into_iter())
        .filter_map(|command| match command {
            DrawCommand::Path {
                brush: Brush::Solid(color),
                ..
            } => Some(color),
            _ => None,
        })
        .next_back()
        .required("an icon")
}

fn assert_same_color(a: Color, b: Color) {
    assert_eq!(a.to_argb8(), b.to_argb8(), "{a:?} != {b:?}");
}

fn assert_scale_and_opacity(
    scene: &Scene,
    expected_scale: f64,
    expected_opacity: f64,
) -> TestResult {
    let (mut scale, mut opacity) = (None, None);
    for command in scene.commands() {
        match command {
            DrawCommand::Transform(affine) => scale = scale.or(Some(affine.linear)),
            DrawCommand::Opacity(alpha) => opacity = Some(alpha),
            _ => {}
        }
    }
    let [sx, _, _, sy] = scale.required("a scale")?;
    assert!(
        (sx - expected_scale).abs() < 1e-4 && (sy - expected_scale).abs() < 1e-4,
        "{sx} {sy}"
    );
    let opacity = opacity.required("an opacity")?;
    assert!((opacity - expected_opacity).abs() < 1e-4, "{opacity}");
    Ok(())
}

fn pressed(state: InteractionState) -> InteractionAppearance {
    InteractionAppearance::default()
        .content(if state.pointer.active { VIOLET } else { BLUE })
        .scale(if state.pointer.active {
            crate::constant!(ScaleFactor::try_new(0.96))
        } else {
            ScaleFactor::ONE
        })
        .opacity(if state.pointer.active {
            crate::constant!(Fraction::try_new(0.5))
        } else {
            crate::constant!(Fraction::try_new(1.0))
        })
}

/// A view with a clickable row tagged "button" inside a scoped component keyed "control", painted
/// once into the returned scene. The row has the state styles `styles` and contains an icon, the
/// text "label" and a scoped component with the text "nested", all white. The white text "outside"
/// follows the component "control".
fn button_view(styles: StateStyles) -> Result<(View, Scene), TestError> {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let styles = styles.clone();
        ui.scoped("control", Modifier::new(), move |ui| {
            ui.layout(
                Axis::Horizontal,
                Modifier::new()
                    .tag("button")
                    .on_click(|| {})
                    .state_styles(styles.clone())
                    .size(200.dp(), 40.dp()),
                |ui| {
                    ui.icon(
                        &LINE,
                        IconOptions::default(),
                        Modifier::new().color(Color::WHITE),
                    );
                    ui.text("label", Modifier::new().color(Color::WHITE));
                    ui.scoped("nested", Modifier::new(), |ui| {
                        ui.text("nested", Modifier::new().color(Color::WHITE));
                    });
                },
            );
        });
        ui.text("outside", Modifier::new().color(Color::WHITE));
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    Ok((view, scene))
}

#[test]
fn text_and_icons_inside_a_node_move_to_the_content_color_of_its_state() -> TestResult {
    let (mut view, mut scene) = button_view(StateStyles::new(pressed).motion(linear()))?;
    for value in ["label", "nested"] {
        assert_same_color(text_color(&scene, value)?, BLUE);
    }
    assert_same_color(icon_color(&scene)?, BLUE);
    assert_same_color(text_color(&scene, "outside")?, Color::WHITE);
    view.input(InputEvent::Move(CENTER));
    view.input(InputEvent::Press(CENTER));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    for value in ["label", "nested"] {
        assert_halfway_from_blue_to_violet(text_color(&scene, value)?, HuePath::Shorter);
    }
    assert_halfway_from_blue_to_violet(icon_color(&scene)?, HuePath::Shorter);
    assert_same_color(text_color(&scene, "outside")?, Color::WHITE);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_same_color(text_color(&scene, "nested")?, VIOLET);
    Ok(())
}

#[test]
fn a_press_scales_and_fades_the_node_without_composing_or_measuring_it() -> TestResult {
    let (mut view, mut scene) = button_view(StateStyles::new(pressed).motion(linear()))?;
    let (before, bounds) = (view.stats(), view.bounds_of("button"));
    view.input(InputEvent::Move(CENTER));
    view.input(InputEvent::Press(CENTER));
    for milliseconds in [0, 250, 500] {
        show_frame(&mut scene, &mut view, WINDOW_SIZE, milliseconds)?;
    }
    // The scale moves linearly in its logarithm. Halfway through the motion, the scale is the
    // geometric mean of 1 and the pressed scale of 0.96.
    assert_scale_and_opacity(&scene, 0.96_f64.sqrt(), 0.75)?;
    assert_eq!(view.bounds_of("button"), bounds);
    let after = view.stats();
    assert_eq!(after.root_compositions, before.root_compositions);
    assert_eq!(after.component_compositions, before.component_compositions);
    assert_eq!(after.measured_components, before.measured_components);
    assert!(after.reused_layouts > before.reused_layouts);
    Ok(())
}

/// A view with the blue text "label" inside a clickable node whose state styles set the content
/// color to violet while the pointer presses the node. The node is pressed at 0 ms. The returned
/// scene shows the frame at 500 ms, halfway to violet. The motion follows the linear easing
/// exactly, so a new target starts a new linear motion.
fn pressed_halfway_to_violet() -> Result<(View, Scene), TestError> {
    let styles = StateStyles::new(|state| {
        let appearance = InteractionAppearance::default();
        if state.pointer.active {
            appearance.content(VIOLET)
        } else {
            appearance
        }
    })
    .motion(linear().preserve_velocity(false));
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(
            Axis::Horizontal,
            Modifier::new()
                .on_click(|| {})
                .state_styles(styles.clone())
                .size(200.dp(), 40.dp()),
            |ui| {
                ui.text("label", Modifier::new().color(BLUE));
            },
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(InputEvent::Move(CENTER));
    view.input(InputEvent::Press(CENTER));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    Ok((view, scene))
}

#[test]
fn a_content_color_that_appears_moves_from_the_color_of_the_text() -> TestResult {
    let (mut view, mut scene) = pressed_halfway_to_violet()?;
    assert_halfway_from_blue_to_violet(text_color(&scene, "label")?, HuePath::Shorter);
    view.input(InputEvent::Release(CENTER));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 2000)?;
    assert_same_color(text_color(&scene, "label")?, BLUE);
    Ok(())
}

#[test]
fn a_content_color_released_mid_flight_turns_back_to_the_color_of_the_text() -> TestResult {
    let (mut view, mut scene) = pressed_halfway_to_violet()?;
    let halfway = text_color(&scene, "label")?;
    view.input(InputEvent::Release(CENTER));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_same_color(text_color(&scene, "label")?, halfway);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 750)?;
    let returning = text_color(&scene, "label")?;
    let hue = |color: Color| color.to_oklch()[2];
    let (blue, violet) = (hue(BLUE), hue(VIOLET));
    assert!(
        blue < hue(returning) && hue(returning) < hue(halfway) && hue(halfway) < violet,
        "{returning:?}"
    );
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 2000)?;
    assert_same_color(text_color(&scene, "label")?, BLUE);
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn the_content_color_reaches_down_to_the_next_node_that_sets_its_own() -> TestResult {
    let outer = StateStyles::new(|_| InteractionAppearance::default().content(VIOLET));
    let inner = StateStyles::new(|_| InteractionAppearance::default().content(BLUE));
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let inner = inner.clone();
        ui.layout(
            Axis::Vertical,
            Modifier::new().on_click(|| {}).state_styles(outer.clone()),
            |ui| {
                ui.text("outer", Modifier::new().color(Color::WHITE));
                ui.layout(
                    Axis::Horizontal,
                    Modifier::new().on_click(|| {}).state_styles(inner),
                    |ui| {
                        ui.text("inner", Modifier::new().color(Color::WHITE));
                    },
                );
            },
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_same_color(text_color(&scene, "outer")?, VIOLET);
    assert_same_color(text_color(&scene, "inner")?, BLUE);
    Ok(())
}

#[test]
fn reduced_motion_changes_the_content_color_and_the_scale_at_once() -> TestResult {
    let (mut view, mut scene) = button_view(StateStyles::new(pressed).motion(linear()))?;
    view.provide(&REDUCED_MOTION, true);
    view.input(InputEvent::Move(CENTER));
    view.input(InputEvent::Press(CENTER));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_same_color(text_color(&scene, "label")?, VIOLET);
    assert_scale_and_opacity(&scene, 0.96, 0.5)?;
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn an_inner_content_color_fades_in_from_the_content_color_around_it() -> TestResult {
    let outer = StateStyles::new(|_| InteractionAppearance::default().content(VIOLET));
    let inner = StateStyles::new(|state| {
        let appearance = InteractionAppearance::default();
        if state.pointer.active {
            appearance.content(BLUE)
        } else {
            appearance
        }
    })
    .motion(linear());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let inner = inner.clone();
        ui.layout(
            Axis::Vertical,
            Modifier::new().on_click(|| {}).state_styles(outer.clone()),
            |ui| {
                ui.layout(
                    Axis::Horizontal,
                    Modifier::new()
                        .on_click(|| {})
                        .state_styles(inner)
                        .size(200.dp(), 40.dp()),
                    |ui| {
                        ui.text("inner", Modifier::new().color(Color::WHITE));
                    },
                );
            },
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(InputEvent::Move(CENTER));
    view.input(InputEvent::Press(CENTER));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_halfway_from_blue_to_violet(text_color(&scene, "inner")?, HuePath::Shorter);
    Ok(())
}

#[test]
fn a_scale_overshooting_a_small_target_never_mirrors_the_node() -> TestResult {
    let styles = StateStyles::new(|state| {
        InteractionAppearance::default().scale(if state.pointer.active {
            crate::constant!(ScaleFactor::try_new(0.05))
        } else {
            ScaleFactor::ONE
        })
    })
    .motion(crate::spring!(std::time::Duration::from_millis(400), 0.2));
    let (mut view, mut scene) = button_view(styles)?;
    view.input(InputEvent::Move(CENTER));
    view.input(InputEvent::Press(CENTER));
    let mut smallest = f64::INFINITY;
    for time in (0..=3000).step_by(10) {
        show_frame(&mut scene, &mut view, WINDOW_SIZE, time)?;
        for command in scene.commands() {
            if let DrawCommand::Transform(affine) = command {
                let [sx, _, _, sy] = affine.linear;
                assert!(sx > 0.0 && sy > 0.0, "{time}: {affine:?}");
                smallest = smallest.min(sx);
            }
        }
    }
    assert!(
        smallest < 0.05,
        "the press overshoots its scale: {smallest}"
    );
    Ok(())
}

#[test]
fn opacity_and_corners_overshooting_their_range_paint_the_nearest_valid_values() -> TestResult {
    use crate::shape::{RoundedRect, Shape};
    let ring = |radius: Dp| BorderStyle {
        color: BLUE,
        width: crate::constant!(NonNegativeDp::try_new(Dp::new(2.0))),
        radius,
    };
    let styles = StateStyles::new(move |state| {
        let (opacity, radius) = if state.pointer.active {
            (Fraction::ZERO, 0.dp())
        } else {
            (crate::constant!(Fraction::try_new(1.0)), 12.dp())
        };
        InteractionAppearance::default()
            .tint(VIOLET, radius)
            .outline(ring(radius))
            .opacity(opacity)
    })
    .motion(crate::spring!(std::time::Duration::from_millis(400), 0.2));
    let (mut view, mut scene) = button_view(styles)?;
    view.input(InputEvent::Move(CENTER));
    view.input(InputEvent::Press(CENTER));
    for time in (0..=3000).step_by(10) {
        show_frame(&mut scene, &mut view, WINDOW_SIZE, time)?;
        for command in scene.commands() {
            match command {
                DrawCommand::Opacity(alpha) => {
                    assert!((0.0..=1.0).contains(&alpha), "{time}: opacity {alpha}");
                }
                DrawCommand::Fill {
                    shape: Shape::Rounded(RoundedRect { corners, .. }),
                    ..
                }
                | DrawCommand::Stroke {
                    shape: Shape::Rounded(RoundedRect { corners, .. }),
                    ..
                } => {
                    assert!(corners.top_left >= Dp::ZERO, "{time}: corner {corners:?}");
                }
                _ => {}
            }
        }
    }
    Ok(())
}
