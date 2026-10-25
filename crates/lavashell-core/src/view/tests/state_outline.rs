// SPDX-License-Identifier: GPL-3.0-only
use super::material_motion::linear;
use super::*;
use crate::style::StateStyles;
use crate::style::{BorderStyle, INTERACTION_STYLE, InteractionAppearance, InteractionStyle};
use crate::units::NonNegativeDp;

#[test]
fn the_outline_of_a_focused_state_is_the_only_focus_ring_of_its_node() -> TestResult {
    let ring = |width: Dp| -> Result<BorderStyle, NumberError> {
        Ok(BorderStyle {
            color: Color::WHITE,
            width: NonNegativeDp::try_new(width)?,
            radius: 4.dp(),
        })
    };
    let focused = ring(2.dp())?;
    let style = InteractionStyle {
        focus: Some(ring(5.dp())?),
        states: Some(StateStyles::new(move |state| {
            InteractionAppearance::default().outline(if state.focus.focused {
                focused.clone()
            } else {
                BorderStyle::NONE
            })
        })),
        ..InteractionStyle::default()
    };
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.provide(&INTERACTION_STYLE, style.clone(), |ui| {
            ui.spacer(100.dp(), 40.dp(), Modifier::new().on_click(|| {}));
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(TAB);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(stroke_widths(&scene.commands()), [2.dp()]);
    Ok(())
}

#[test]
fn a_focus_ring_that_appears_keeps_its_corners_while_it_fades_in() -> TestResult {
    let ring = BorderStyle {
        color: Color::WHITE,
        width: crate::constant!(NonNegativeDp::try_new(Dp::new(4.0))),
        radius: 12.dp(),
    };
    let styles = StateStyles::new(move |state| {
        InteractionAppearance::default().outline(if state.focus.focused {
            ring.clone()
        } else {
            BorderStyle::NONE
        })
    })
    .motion(linear());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.spacer(
            100.dp(),
            40.dp(),
            Modifier::new().on_click(|| {}).state_styles(styles.clone()),
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(TAB);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let strokes: Vec<_> = (scene.commands().iter())
        .filter_map(|command| match command {
            DrawCommand::Stroke {
                shape: Shape::Rounded(rounded),
                stroke,
            } => Some((rounded.corners, *stroke)),
            _ => None,
        })
        .collect();
    let [(corners, stroke)] = strokes.exactly::<1>("rounded strokes")?;
    assert_eq!(corners.top_left, 12.dp());
    assert!((stroke.width.get() - 2.dp()).abs() < 0.01.dp());
    assert!((stroke.color.alpha() - 0.5).abs() < 0.01);
    Ok(())
}

#[test]
fn a_focused_node_whose_state_paints_no_outline_shows_the_focus_ring_of_its_style() -> TestResult {
    let style = InteractionStyle {
        focus: Some(BorderStyle {
            color: Color::WHITE,
            width: crate::constant!(NonNegativeDp::try_new(Dp::new(5.0))),
            radius: 4.dp(),
        }),
        ..InteractionStyle::default()
    };
    let hover = StateStyles::new(|state| {
        let tint = Color::WHITE.with_alpha(if state.pointer.hovered { 0.5 } else { 0.0 });
        InteractionAppearance::default().tint(tint, 4.dp())
    });
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let hover = hover.clone();
        ui.provide(&INTERACTION_STYLE, style.clone(), |ui| {
            ui.spacer(
                100.dp(),
                40.dp(),
                Modifier::new().on_click(|| {}).state_styles(hover),
            );
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert!(stroke_widths(&scene.commands()).is_empty());
    view.input(TAB);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(stroke_widths(&scene.commands()), [5.dp()]);
    Ok(())
}

#[test]
fn a_hover_tint_keeps_its_corners_while_the_ring_fades_in_and_out() -> TestResult {
    let ring = BorderStyle {
        color: Color::WHITE,
        width: crate::constant!(NonNegativeDp::try_new(Dp::new(4.0))),
        radius: 12.dp(),
    };
    let styles = StateStyles::new(move |state| {
        let tint = Color::WHITE.with_alpha(if state.pointer.hovered { 0.5 } else { 0.0 });
        InteractionAppearance::default()
            .tint(tint, 12.dp())
            .outline(if state.focus.focused {
                ring.clone()
            } else {
                BorderStyle::NONE
            })
    })
    .motion(linear());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            for tag in ["first", "second"] {
                ui.spacer(
                    100.dp(),
                    40.dp(),
                    Modifier::new()
                        .tag(tag)
                        .on_click(|| {})
                        .state_styles(styles.clone()),
                );
            }
        });
    });
    let tint_corners = |scene: &Scene| -> Vec<Corners> {
        (scene.commands().iter())
            .filter_map(|command| match command {
                DrawCommand::Fill {
                    shape: Shape::Rounded(rounded),
                    ..
                } => Some(rounded.corners),
                _ => None,
            })
            .collect()
    };
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(InputEvent::Move(Point::new(20.dp(), 20.dp())));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(tint_corners(&scene), [Corners::all(12.dp())]);
    view.input(TAB);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(stroke_widths(&scene.commands()).len(), 1);
    assert_eq!(tint_corners(&scene), [Corners::all(12.dp())]);
    view.input(TAB);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 2000)?;
    assert_eq!(stroke_widths(&scene.commands()).len(), 2);
    assert_eq!(tint_corners(&scene), [Corners::all(12.dp())]);
    Ok(())
}
