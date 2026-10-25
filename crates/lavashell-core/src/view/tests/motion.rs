// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::float_asserts::assert_f64_eq;

#[test]
fn automatic_geometry_transitions_keep_hit_testing_at_visible_bounds() -> TestResult {
    use crate::animation::Tween;
    let moved = State::new(false);
    let clicked = State::new(0);
    let target = moved.clone();
    let count = clicked.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let x = if target.get() { 200.dp() } else { 20.dp() };
        let count = count.clone();
        ui.spacer(
            80.dp(),
            40.dp(),
            Modifier::new()
                .offset(x, 20.dp())
                .clip(Corners::all(10.dp()))
                .background(Color::WHITE)
                .transition(Tween::new(std::time::Duration::from_secs(1)))
                .on_click(move || count.update(|value| *value += 1))
                .tag("moving"),
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    moved.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds("moving")?.x, 20.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let rect = view.tagged_bounds("moving")?;
    assert!((rect.x - 110.dp()).abs() < 0.01.dp());
    let at = Point::new(rect.x + 20.dp(), rect.y + 20.dp());
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
    assert_eq!(clicked.get(), 1);
    view.input(InputEvent::Press(Point::new(240.dp(), 40.dp())));
    view.input(InputEvent::Release(Point::new(240.dp(), 40.dp())));
    assert_eq!(clicked.get(), 1);
    moved.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert!((view.tagged_bounds("moving")?.x - 110.dp()).abs() < 0.01.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1600)?;
    assert_eq!(view.tagged_bounds("moving")?.x, 20.dp());
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn shared_elements_keep_global_geometry_when_reparented() -> TestResult {
    use crate::animation::Tween;
    let other = State::new(false);
    let state = other.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let right = state.get();
        for destination in [false, true] {
            ui.key(
                destination,
                Modifier::new().offset(if destination { 300.dp() } else { 10.dp() }, 20.dp()),
                |ui| {
                    if right == destination {
                        ui.shared(
                            "shared",
                            Modifier::new()
                                .transition(Tween::new(std::time::Duration::from_secs(1)))
                                .tag("shared"),
                            |ui| {
                                ui.spacer(60.dp(), 40.dp(), Modifier::new());
                            },
                        );
                    }
                },
            );
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    other.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert!((view.tagged_bounds("shared")?.x - 10.dp()).abs() < 0.01.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert!((view.tagged_bounds("shared")?.x - 155.dp()).abs() < 0.01.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert!((view.tagged_bounds("shared")?.x - 300.dp()).abs() < 0.01.dp());
    Ok(())
}

#[test]
fn entering_and_exiting_nodes_finish_and_removed_nodes_cannot_receive_input() -> TestResult {
    use crate::animation::Tween;
    let shown = State::new(true);
    let state = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if state.get() {
            ui.key(
                "fading",
                Modifier::new().background(Color::WHITE).transition(
                    Transition::new(Tween::new(std::time::Duration::from_millis(200)))
                        .enter(Presence::fade())
                        .exit(Presence::fade()),
                ),
                |ui| {
                    ui.spacer(80.dp(), 40.dp(), Modifier::new());
                },
            );
        }
    });
    let millis = std::time::Duration::from_millis;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert!(!view.needs_frame());
    assert_eq!(view.next_deadline(), Some(millis(200)));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 250)?;
    assert!(!view.needs_frame());
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 250)?;
    assert_eq!(view.next_deadline(), Some(millis(450)));
    assert!(view.hits.is_empty());
    assert!(
        scene
            .commands()
            .iter()
            .any(|cmd| matches!(cmd, DrawCommand::Fill { .. }))
    );
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert!(!view.needs_frame());
    assert!(scene.commands().is_empty());
    Ok(())
}

#[test]
fn animated_modifiers_update_paint_without_recomposing_or_measuring() -> TestResult {
    use crate::animation::Animation;
    use std::cell::Cell;
    use std::time::Duration;
    let offset = Animation::new(Point::new(0.dp(), 0.dp()))?;
    let animation = offset.clone();
    let count = std::rc::Rc::new(Cell::new(0));
    let composed = count.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        composed.set(composed.get() + 1);
        ui.spacer(
            20.dp(),
            20.dp(),
            Modifier::new()
                .animated_offset(animation.clone())
                .tag("animated"),
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    offset.animate_to(
        Point::new(100.dp(), 0.dp()),
        crate::spring!(Duration::from_millis(450), 0.85),
    )?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    assert!(view.tagged_bounds("animated")?.x > Dp::ZERO);
    assert_eq!(count.get(), 1);
    Ok(())
}

#[test]
fn automatic_appearance_preserves_ordered_nested_decorations() -> TestResult {
    use crate::animation::Tween;
    let changed = State::new(false);
    let state = changed.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.spacer(
            100.dp(),
            80.dp(),
            Modifier::new()
                .clip(Corners::all(if state.get() { 30.dp() } else { 10.dp() }))
                .background(if state.get() {
                    Color::rgb(0)
                } else {
                    Color::WHITE
                })
                .opacity(if state.get() { 0.8 } else { 0.4 })
                .padding(8.dp())
                .clip(Corners::all(3.dp()))
                .background(Color::rgb(0x00ff_0000))
                .opacity(0.5)
                .transition(Tween::new(std::time::Duration::from_secs(1))),
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    changed.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let clips: Vec<_> = scene
        .commands()
        .iter()
        .filter_map(|c| {
            if let DrawCommand::Clip { corners, .. } = c {
                Some(corners.top_left.x.get())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(clips, [20.0, 3.0]);
    let colors: Vec<_> = scene
        .commands()
        .iter()
        .filter_map(|c| {
            if let DrawCommand::Fill {
                brush: Brush::Solid(color),
                ..
            } = c
            {
                Some(*color)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        colors.iter().map(|c| c.to_argb8()).collect::<Vec<_>>(),
        [
            Color::rgb(0x0063_6363).to_argb8(),
            Color::rgb(0x00ff_0000).to_argb8()
        ]
    );
    let alphas: Vec<_> = scene
        .commands()
        .iter()
        .filter_map(|c| {
            if let DrawCommand::Opacity(alpha) = c {
                Some(*alpha)
            } else {
                None
            }
        })
        .collect();
    assert_f64_eq(alphas[0], 0.6);
    assert_f64_eq(alphas[1], 0.5);
    Ok(())
}

#[test]
fn scaled_rounded_clips_use_elliptical_corners_for_input() {
    let clip = HitClip::new(
        Rect::new(0.dp(), 0.dp(), 100.dp(), 100.dp()),
        Corners::all(40.dp()),
        Transform {
            scale: Scale { x: 2.0, y: 0.5 },
            translation: Point::new(10.dp(), 20.dp()),
        },
    );
    assert!(!clip.shape.contains(Point::new(30.dp(), 25.dp())));
    assert!(clip.shape.contains(Point::new(50.dp(), 30.dp())));
    assert!(clip.shape.contains(Point::new(110.dp(), 21.dp())));
}

#[test]
fn rounded_clips_take_input_inside_each_corner_radius() {
    let corners = Corners::new(40.dp(), 0.dp(), 20.dp(), 0.dp());
    let clip = HitClip::new(
        Rect::new(0.dp(), 0.dp(), 100.dp(), 100.dp()),
        corners,
        Transform::IDENTITY,
    );
    assert!(!clip.shape.contains(Point::new(5.dp(), 5.dp())));
    assert!(clip.shape.contains(Point::new(99.dp(), 1.dp())));
    assert!(!clip.shape.contains(Point::new(97.dp(), 97.dp())));
    assert!(clip.shape.contains(Point::new(90.dp(), 90.dp())));
    assert!(clip.shape.contains(Point::new(1.dp(), 99.dp())));
}

#[test]
fn automatic_corner_radius_is_not_scaled_twice_when_bounds_change() -> TestResult {
    use crate::animation::Tween;
    let expanded = State::new(false);
    let state = expanded.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let expanded = state.get();
        ui.spacer(
            if expanded { 300.dp() } else { 100.dp() },
            if expanded { 100.dp() } else { 60.dp() },
            Modifier::new()
                .clip(Corners::all(if expanded { 30.dp() } else { 20.dp() }))
                .on_click(|| {})
                .transition(Tween::new(std::time::Duration::from_secs(1))),
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    expanded.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let radius = view.hits[0].clips[0].shape.corners().top_left;
    assert!((radius.x - 20.dp()).abs() < 0.001.dp() && (radius.y - 20.dp()).abs() < 0.001.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let radius = view.hits[0].clips[0].shape.corners().top_left;
    assert!((radius.x - 25.dp()).abs() < 0.001.dp() && (radius.y - 25.dp()).abs() < 0.001.dp());
    Ok(())
}

#[test]
fn interrupted_entering_fades_keep_velocity_and_ancestor_clipping_on_exit() -> TestResult {
    use std::time::Duration;
    let shown = State::new(true);
    let state = shown.clone();
    let clicked = State::new(0);
    let count = clicked.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let count = count.clone();
        ui.layout(
            Arrangement::Overlay,
            Modifier::new()
                .size(60.dp(), 60.dp())
                .clip(Corners::all(8.dp())),
            |ui| {
                if state.get() {
                    ui.key(
                        "fade",
                        Modifier::new()
                            .background(Color::WHITE)
                            .on_click(move || count.update(|value| *value += 1))
                            .transition(
                                Transition::new(crate::spring!(Duration::from_millis(450), 0.85))
                                    .enter(Presence::fade())
                                    .exit(Presence::fade()),
                            ),
                        |ui| {
                            ui.spacer(100.dp(), 80.dp(), Modifier::new());
                        },
                    );
                }
            },
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 80)?;
    let entering = view.transitions.only_animation()?.get().presence.alpha();
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 80)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 90)?;
    let exiting = view.transitions.only_animation()?.get().presence.alpha();
    assert!(exiting > entering);
    let alpha = (scene.groups().iter())
        .map(|group| group.opacity)
        .product::<f64>();
    assert!((alpha - exiting).abs() < 0.001);
    let clip = scene.commands().iter().position(|command| {
        matches!(
            command,
            DrawCommand::Clip { rect, corners }
                if rect.width == 60.dp() && corners.top_left.x == 8.dp()
        )
    });
    let layer = scene
        .commands()
        .iter()
        .position(|command| matches!(command, DrawCommand::Opacity(_)));
    assert!(clip.is_some() && clip < layer);
    assert!(view.hits.is_empty());
    view.input(InputEvent::Press(Point::new(30.dp(), 30.dp())));
    view.input(InputEvent::Release(Point::new(30.dp(), 30.dp())));
    assert_eq!(clicked.get(), 0, "removed nodes cannot receive input");
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 3000)?;
    assert_eq!(view.transitions.count(), 0);
    Ok(())
}

#[test]
fn a_child_exit_inside_a_leaving_parent_paints_once() -> TestResult {
    use crate::animation::Tween;
    let shown = State::new(true);
    let state = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if state.get() {
            let fade = Transition::new(Tween::new(std::time::Duration::from_millis(200)))
                .enter(Presence::fade())
                .exit(Presence::fade());
            ui.key(
                "parent",
                Modifier::new().background(Color::rgb(0)).transition(fade),
                |ui| {
                    ui.key(
                        "child",
                        Modifier::new().background(Color::WHITE).transition(fade),
                        |ui| {
                            ui.spacer(20.dp(), 20.dp(), Modifier::new());
                        },
                    );
                },
            );
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 250)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 250)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 300)?;
    assert_eq!(
        scene
            .commands()
            .iter()
            .filter(|command| {
                matches!(command, DrawCommand::Fill { brush: Brush::Solid(color), .. }
            if color.to_argb8() == Color::WHITE.to_argb8())
            })
            .count(),
        1
    );
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 600)?;
    assert_eq!(view.transitions.count(), 0);
    Ok(())
}

#[test]
fn a_clip_takes_input_where_its_fitted_corners_are_drawn() {
    let clip = HitClip::new(
        Rect::new(0.dp(), 0.dp(), 200.dp(), 40.dp()),
        Corners::all(60.dp()),
        Transform::IDENTITY,
    );
    assert!(clip.shape.contains(Point::new(30.dp(), 1.dp())));
    assert!(!clip.shape.contains(Point::new(3.dp(), 3.dp())));
}
