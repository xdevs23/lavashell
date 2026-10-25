// SPDX-License-Identifier: GPL-3.0-only
use super::material_motion::painted_fills;
use super::*;
use crate::animation::{Easing, Tween};
use crate::units::{Fraction, ScaleFactor};
use std::time::Duration;

fn linear(ms: u64) -> Transition {
    Transition::new(Tween::new(Duration::from_millis(ms)).easing(Easing::Linear))
}

fn painted_colors(commands: &[DrawCommand]) -> Vec<u32> {
    commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Fill {
                brush: Brush::Solid(color),
                ..
            } => Some(color.to_argb8()),
            _ => None,
        })
        .collect()
}

fn presence_alpha(scene: &Scene) -> Option<f64> {
    scene.groups().first().map(|group| group.opacity)
}

fn finite(affine: &Affine) -> bool {
    affine.linear.iter().all(|v| v.is_finite())
        && affine.translation.x.is_finite()
        && affine.translation.y.is_finite()
}

fn close(a: Rect, b: Rect) -> bool {
    [a.x - b.x, a.y - b.y, a.width - b.width, a.height - b.height]
        .iter()
        .all(|d| d.abs() < 0.001.dp())
}

#[test]
fn entering_nodes_start_at_their_presence_and_settle_in_place() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let entering = Presence::fade()
            .slide(crate::constant!(slide_offset(0.0, 12.0)))
            .scale(crate::constant!(ScaleFactor::try_new(0.5)));
        let modifier = Modifier::new()
            .offset(20.dp(), 20.dp())
            .transition(linear(1000).enter(entering))
            .tag("entering");
        ui.spacer(100.dp(), 40.dp(), modifier);
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert!(close(
        view.tagged_bounds("entering")?,
        Rect::new(45.dp(), 42.dp(), 50.dp(), 20.dp())
    ));
    assert_eq!(presence_alpha(&scene), Some(0.0));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let scale = 0.5_f64.sqrt();
    assert!(close(
        view.tagged_bounds("entering")?,
        Rect::new(
            70.dp() - 50.dp() * scale,
            46.dp() - 20.dp() * scale,
            100.dp() * scale,
            40.dp() * scale
        )
    ));
    assert!((presence_alpha(&scene).required("a presence opacity")? - 0.5).abs() < 1e-4);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert!(close(
        view.tagged_bounds("entering")?,
        Rect::new(20.dp(), 20.dp(), 100.dp(), 40.dp())
    ));
    assert_eq!(presence_alpha(&scene), Some(1.0));
    Ok(())
}

#[test]
fn leaving_nodes_keep_their_place_among_their_siblings_until_their_exit_ends() -> TestResult {
    let shown = State::new(vec!["a", "b", "c", "d"]);
    let state = shown.clone();
    let colors = [
        ("a", 0x00ff_0000),
        ("b", 0x0000_ff00),
        ("c", 0x0000_00ff),
        ("d", 0x00ff_ffff),
    ];
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let shown = state.get();
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            for (key, color) in colors {
                if !shown.contains(&key) {
                    continue;
                }
                let modifier = Modifier::new().background(Color::rgb(color)).tag(key);
                let modifier = if key == "a" {
                    modifier
                } else {
                    modifier.transition(linear(200).exit(Presence::fade()))
                };
                ui.key(key, modifier, |ui| {
                    ui.spacer(20.dp(), 20.dp(), Modifier::new());
                });
            }
        });
    });
    let argb = |colors: &[u32]| {
        colors
            .iter()
            .map(|c| Color::rgb(*c).to_argb8())
            .collect::<Vec<_>>()
    };
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    shown.set(vec!["a", "c", "d"]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    assert_eq!(
        painted_colors(&scene.commands()),
        argb(&[0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0x00ff_ffff])
    );
    shown.set(vec!["d"]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 150)?;
    assert_eq!(
        painted_colors(&scene.commands()),
        argb(&[0x0000_ff00, 0x0000_00ff, 0x00ff_ffff])
    );
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 400)?;
    assert_eq!(painted_colors(&scene.commands()), argb(&[0x00ff_ffff]));
    Ok(())
}

#[test]
fn leaving_nodes_follow_their_parent_inside_its_live_clip() -> TestResult {
    let shown = State::new(true);
    let state = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let shown = state.get();
        let parent = Modifier::new()
            .offset(if shown { 0.dp() } else { 50.dp() }, 0.dp())
            .clip(8.dp());
        ui.layout(Axis::Horizontal, parent, |ui| {
            ui.spacer(20.dp(), 20.dp(), Modifier::new());
            if shown {
                let exit = Presence::default().slide(crate::constant!(slide_offset(30.0, 0.0)));
                let leaving = Modifier::new().background(Color::WHITE);
                let leaving = leaving.transition(linear(1000).exit(exit));
                ui.key("leaving", leaving, |ui| {
                    ui.spacer(20.dp(), 20.dp(), Modifier::new());
                });
            }
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let clip = scene.commands().iter().position(|command| {
        matches!(command, DrawCommand::Clip { rect, .. } if rect.x == 50.dp() && rect.width == 20.dp())
    });
    let departure =
        (scene.commands().iter()).position(|command| matches!(command, DrawCommand::Fill { .. }));
    assert!(clip.is_some() && clip < departure, "{:?}", scene.commands());
    let [fill] =
        painted_fills(&scene.commands())?.exactly::<1>("fills painted by the departure")?;
    assert!(
        (fill.rect.x - 85.dp()).abs() < 0.001.dp(),
        "{:?}",
        fill.rect
    );
    Ok(())
}

#[test]
fn leaving_nodes_inside_a_component_disappear_when_their_exit_ends() -> TestResult {
    let shown = State::new(true);
    let state = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let state = state.clone();
        ui.scoped("panel", Modifier::new(), move |ui| {
            if state.get() {
                let exit = Presence::default().slide(crate::constant!(slide_offset(30.0, 0.0)));
                let transition = Transition::new(Tween::new(Duration::from_secs(2))).exit(exit);
                let leaving = Modifier::new()
                    .background(Color::WHITE)
                    .transition(transition);
                ui.key("leaving", leaving, |ui| {
                    ui.spacer(20.dp(), 20.dp(), Modifier::new());
                });
            }
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1999)?;
    assert_eq!(painted_colors(&scene.commands()), [Color::WHITE.to_argb8()]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 2000)?;
    assert!(painted_colors(&scene.commands()).is_empty());
    Ok(())
}

#[test]
fn a_leaving_shared_node_that_returns_under_another_parent_paints_once() -> TestResult {
    let place = State::new(Some(false));
    let state = place.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let place = state.get();
        for second in [false, true] {
            ui.key(second, Modifier::new(), |ui| {
                if place == Some(second) {
                    let modifier = Modifier::new()
                        .background(Color::WHITE)
                        .transition(linear(200).exit(Presence::fade()));
                    ui.shared("shared", modifier, |ui| {
                        ui.spacer(20.dp(), 20.dp(), Modifier::new());
                    });
                }
            });
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    place.set(None);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 50)?;
    place.set(Some(true));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    assert_eq!(painted_colors(&scene.commands()), [Color::WHITE.to_argb8()]);
    Ok(())
}

const ROW: [(&str, u32); 3] = [("a", 0x00ff_0000), ("b", 0x0000_ff00), ("c", 0x0000_00ff)];

fn row(ui: &mut Ui, shown: &[&str]) {
    ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
        for (key, color) in ROW {
            if !shown.contains(&key) {
                continue;
            }
            let modifier = Modifier::new().background(Color::rgb(color));
            let exit = Presence::fade().slide(crate::constant!(slide_offset(0.0, 20.0)));
            let modifier = if key == "b" {
                modifier.transition(linear(1000).exit(exit))
            } else {
                modifier
            };
            ui.key(key, modifier, |ui| {
                ui.spacer(20.dp(), 20.dp(), Modifier::new());
            });
        }
    });
}

fn row_colors() -> Vec<u32> {
    ROW.iter().map(|(_, c)| Color::rgb(*c).to_argb8()).collect()
}

fn presence_and_target(view: &View) -> Result<(Presence, Presence), TestError> {
    let animation = view.transitions.only_animation()?;
    Ok((animation.get().presence, animation.target().presence))
}

#[test]
fn a_node_brought_back_halfway_through_its_exit_returns_from_where_it_was() -> TestResult {
    let shown = State::new(vec!["a", "b", "c"]);
    let state = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        row(ui, &state.get());
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    shown.set(vec!["a", "c"]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let (left, _) = presence_and_target(&view)?;
    shown.set(vec!["a", "b", "c"]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_eq!(painted_colors(&scene.commands()), row_colors());
    assert_eq!(presence_and_target(&view)?, (left, Presence::default()));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(presence_and_target(&view)?.0, Presence::default());
    Ok(())
}

#[test]
fn a_node_brought_back_and_removed_again_before_a_paint_continues_its_exit_in_its_slot()
-> TestResult {
    let compositions = State::new(vec![vec!["a", "b", "c"]]);
    let state = compositions.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let mut pending = state.get();
        let shown = pending.remove(0);
        if !pending.is_empty() {
            state.set(pending);
        }
        row(ui, &shown);
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    compositions.set(vec![vec!["a", "c"]]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let (left, exit) = presence_and_target(&view)?;
    let before = view.stats().root_compositions;
    compositions.set(vec![vec!["a", "b", "c"], vec!["a", "c"]]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_eq!(view.stats().root_compositions, before + 2);
    assert_eq!(painted_colors(&scene.commands()), row_colors());
    assert_eq!(presence_and_target(&view)?, (left, exit));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(
        painted_colors(&scene.commands()),
        [row_colors()[0], row_colors()[2]]
    );
    Ok(())
}

#[test]
fn an_entering_clipped_node_scales_its_corner_radius_with_its_presence() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let entering = Presence::default().scale(crate::constant!(ScaleFactor::try_new(0.5)));
        let modifier = Modifier::new()
            .clip(10.dp())
            .transition(linear(1000).enter(entering));
        ui.spacer(100.dp(), 40.dp(), modifier);
    });
    let visible_radius = |scene: &Scene| {
        let scale = (scene.groups().first()).map(|group| group.transform.linear[0])?;
        let radius = scene.commands().iter().find_map(|command| match command {
            DrawCommand::Clip { corners, .. } => Some(corners.top_left.x.get()),
            _ => None,
        })?;
        Some(radius * scale)
    };
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert!((visible_radius(&scene).required("a clip radius")? - 5.0).abs() < 1e-4);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert!((visible_radius(&scene).required("a clip radius")? - 10.0).abs() < 1e-4);
    Ok(())
}

#[test]
fn an_overshooting_exit_toward_a_small_scale_keeps_transforms_and_clips_positive() -> TestResult {
    let shown = State::new(true);
    let state = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let shown = state.get();
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            if shown {
                let spring = crate::spring!(Duration::from_millis(400), 0.2);
                let exit = Presence::default().scale(crate::constant!(ScaleFactor::try_new(0.05)));
                let leaving = Modifier::new()
                    .clip(8.dp())
                    .transition(Transition::new(spring).exit(exit));
                ui.key("leaving", leaving, |ui| {
                    ui.spacer(40.dp(), 40.dp(), Modifier::new());
                });
            }
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    shown.set(false);
    let mut smallest = f64::INFINITY;
    for time in (0..=3000).step_by(10) {
        show_frame(&mut scene, &mut view, WINDOW_SIZE, time)?;
        for command in &scene.commands() {
            match command {
                DrawCommand::Transform(c) => {
                    assert!(finite(c), "{time}: {c:?}");
                    assert!(c.linear[0] > 0.0 && c.linear[3] > 0.0, "{time}: {c:?}");
                    smallest = smallest.min(c.linear[0]);
                }
                DrawCommand::Clip { corners, .. } => {
                    let radius = corners.top_left;
                    assert!(
                        radius.x.get().is_finite() && radius.y.get().is_finite(),
                        "{time}"
                    );
                    assert!(radius.x.get() > 0.0 && radius.y.get() > 0.0, "{time}");
                }
                _ => {}
            }
        }
    }
    assert!(
        smallest < 0.05,
        "the exit overshoots its target scale: {smallest}"
    );
    Ok(())
}

#[test]
fn nested_nodes_at_the_ends_of_the_scale_range_paint_finite_transforms_and_hits() -> TestResult {
    const DEPTH: usize = 4;
    fn nest(ui: &mut Ui, presence: Presence, depth: usize) {
        let entering = Modifier::new()
            .on_click(|| {})
            .transition(linear(1000).enter(presence));
        if depth == 0 {
            ui.spacer(20.dp(), 20.dp(), entering.clone());
            ui.shared("shared", entering, |ui| {
                ui.spacer(20.dp(), 20.dp(), Modifier::new());
            });
            return;
        }
        let offset = Modifier::new().offset(400.dp(), 250.dp()).then(entering);
        ui.layout(Axis::Horizontal, offset, |ui| nest(ui, presence, depth - 1));
    }
    for scale in [*ScaleFactor::RANGE.start(), *ScaleFactor::RANGE.end()] {
        let presence = Presence::default().scale(ScaleFactor::try_new(scale)?);
        let mut scene = Scene::default();
        let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
            nest(ui, presence, DEPTH);
        });
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        let transforms: Vec<_> = scene
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Transform(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert_eq!(transforms.len(), DEPTH + 2);
        for c in transforms {
            assert!(finite(&c), "{scale}: {c:?}");
            assert!(c.linear[0] != 0.0 && c.linear[3] != 0.0, "{scale}: {c:?}");
        }
        assert_eq!(view.input_hits.len(), DEPTH + 2);
        for hit in &view.input_hits {
            let r = hit.rect;
            assert!(
                [r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite()),
                "{scale}: {r:?}"
            );
            assert!(r.width != 0.dp() && r.height != 0.dp(), "{scale}: {r:?}");
        }
    }
    Ok(())
}

#[test]
fn an_entering_node_starts_at_the_opacity_of_its_presence() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let entering = Presence::default().opacity(crate::constant!(Fraction::try_new(0.4)));
        let modifier = Modifier::new().transition(linear(1000).enter(entering));
        ui.spacer(100.dp(), 40.dp(), modifier);
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(presence_alpha(&scene), Some(0.4));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(presence_alpha(&scene), Some(1.0));
    Ok(())
}

#[test]
fn the_shadow_of_a_fading_node_fades_with_it_outside_its_bounds() -> TestResult {
    let shadow = Shadow::new(Color::rgb(0x0010_2030), 6.dp(), Point::new(0.dp(), 30.dp()))?;
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let modifier = Modifier::new()
            .shadow(shadow, 8.dp())
            .background(Color::WHITE)
            .transition(linear(1000).enter(Presence::fade()));
        ui.spacer(100.dp(), 40.dp(), modifier);
    });
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let card = Rect::new(0.dp(), 0.dp(), 100.dp(), 40.dp());
    for ms in [250, 750] {
        let opacity = presence_alpha(scene.at(ms)).required("a presence opacity")?;
        assert!(opacity > 0.0 && opacity < 1.0, "{ms} ms");
        let expected = vec![
            DrawCommand::Save,
            DrawCommand::Opacity(opacity),
            DrawCommand::Shadow {
                shape: Shape::rounded(card, 8.dp()),
                shadow,
            },
            DrawCommand::Fill {
                shape: card.into(),
                brush: Color::WHITE.into(),
            },
            DrawCommand::Restore,
            DrawCommand::Restore,
        ];
        assert!(scene.commands() == expected, "{ms} ms");
    }
    Ok(())
}
