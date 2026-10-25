// SPDX-License-Identifier: GPL-3.0-only
use super::material_group::inside_clip;
use super::material_motion::{PaintedFill, close, linear, painted_fills};
use super::*;
use crate::preferences::REDUCED_MOTION;
use crate::shape::RoundedRect;
use std::{cell::Cell, rc::Rc};

#[path = "reflow/clips.rs"]
mod clips;
#[path = "reflow/scaling.rs"]
mod scaling;

const ISLAND: Color = Color::rgb(0x0020_50a0);
const PIECE: Color = Color::rgb(0x0000_ff00);

/// A spacer of 100 by 20 dp filled with `PIECE`, keyed and tagged `key`, that fades in as it enters
/// and fades out as it leaves. `modifier` is applied after those settings.
fn piece(ui: &mut Ui, key: &str, modifier: Modifier) {
    ui.key(
        key,
        Modifier::new()
            .background(PIECE)
            .transition(
                Transition::new(linear())
                    .enter(Presence::fade())
                    .exit(Presence::fade()),
            )
            .tag(key)
            .then(modifier),
        |ui| {
            ui.spacer(100.dp(), 20.dp(), Modifier::new());
        },
    );
}

fn pieces(ui: &mut Ui, wide: bool) {
    piece(ui, "first", Modifier::new());
    if wide {
        piece(ui, "second", Modifier::new());
    }
}

/// A node keyed and tagged `key` around a row built by `content`, moved `x` to the right and 10 dp
/// down. The node has the `ISLAND` material and a clip, both with 20 dp corners, then 10 dp of
/// padding and a 2 dp border inside the padding. Its transition reflows the node over a second, as
/// `BoundsMotion::Reflow` describes. `modifier` is applied after all of these settings.
fn island(ui: &mut Ui, key: &str, x: Dp, modifier: Modifier, content: impl FnOnce(&mut Ui)) {
    ui.key(
        key,
        Modifier::new()
            .offset(x, 10.dp())
            .material(Material::new(ISLAND), 20.dp())
            .clip(20.dp())
            .padding(10.dp())
            .border(ISLAND, 2.dp(), 10.dp())
            .transition(Transition::new(linear()).reflow())
            .tag(key)
            .then(modifier),
        |ui| {
            ui.layout(Axis::Horizontal, Modifier::new(), content);
        },
    );
}

/// A view with an island at (20, 10) around the piece "first", and also around the piece "second"
/// while `wide` is true. The view is painted once into the returned scene. The returned counter
/// counts how often the view is composed.
fn island_view(wide: &State<bool>) -> Result<(View, Scene, Rc<Cell<u32>>), TestError> {
    let (wide, compositions) = (wide.clone(), Rc::new(Cell::new(0)));
    let count = compositions.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        count.set(count.get() + 1);
        island(ui, "island", 20.dp(), Modifier::new(), |ui| {
            pieces(ui, wide.get());
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    Ok((view, scene, compositions))
}

/// A view with an island around the piece "first", and also around the piece "second" while `wide`
/// is true. The island is placed so that its right edge is at 270 dp at either width, so it grows
/// to the left when the second piece appears.
fn left_growing_view(wide: &State<bool>) -> Result<(View, Scene), TestError> {
    let wide = wide.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let x = if wide.get() { 50.dp() } else { 150.dp() };
        island(ui, "island", x, Modifier::new(), |ui| {
            pieces(ui, wide.get());
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    Ok((view, scene))
}

fn halfway(view: &mut View, scene: &mut Scene, state: &State<bool>, value: bool) -> TestResult {
    state.set(value);
    show_frame(scene, view, WINDOW_SIZE, 0)?;
    show_frame(scene, view, WINDOW_SIZE, 500)?;
    Ok(())
}

fn fills(scene: &Scene, color: Color) -> Result<Vec<PaintedFill>, TestError> {
    let fills = painted_fills(&scene.commands())?;
    Ok(fills
        .into_iter()
        .filter(|fill| fill.color.to_argb8() == color.to_argb8())
        .collect())
}

fn glass(scene: &Scene) -> Result<Rect, TestError> {
    Ok(fills(scene, ISLAND)?[0].rect)
}

fn piece_rects(scene: &Scene) -> Result<Vec<Rect>, TestError> {
    Ok(fills(scene, PIECE)?.iter().map(|fill| fill.rect).collect())
}

fn clip(scene: &Scene) -> Result<(usize, Rect), TestError> {
    (scene.commands().iter().enumerate())
        .find_map(|(index, command)| match command {
            DrawCommand::Clip { rect, .. } => Some((index, *rect)),
            _ => None,
        })
        .required("a clip around the content of the island")
}

fn border(scene: &Scene) -> Result<Rect, TestError> {
    scene
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Stroke {
                shape: Shape::Rounded(RoundedRect { rect, .. }),
                ..
            } => Some(*rect),
            _ => None,
        })
        .required("a border on the island")
}

fn click(view: &mut View, at: Point) {
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
}

fn near(a: Point, b: Point) {
    close(a.x, b.x);
    close(a.y, b.y);
}

#[test]
fn a_reflowing_node_resizes_its_surfaces_around_content_laid_out_at_the_new_size() -> TestResult {
    let wide = State::new(false);
    let (mut view, mut scene, compositions) = island_view(&wide)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    let live = Rect::new(20.dp(), 10.dp(), 170.dp(), 40.dp());
    assert_eq!(glass(&scene)?, live);
    assert_eq!(fills(&scene, ISLAND)?[0].scale, (1.0, 1.0));
    assert_eq!(clip(&scene)?.1, live);
    assert_eq!(
        border(&scene)?,
        Rect::new(30.dp(), 20.dp(), 150.dp(), 20.dp())
    );
    assert_eq!(view.bounds_of("island"), Some(live));
    let pieces = fills(&scene, PIECE)?;
    let pieces: Vec<_> = pieces.iter().map(|fill| (fill.rect, fill.scale)).collect();
    assert_eq!(
        pieces,
        [
            (Rect::new(30.dp(), 20.dp(), 100.dp(), 20.dp()), (1.0, 1.0)),
            (Rect::new(130.dp(), 20.dp(), 100.dp(), 20.dp()), (1.0, 1.0))
        ]
    );
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(
        glass(&scene)?,
        Rect::new(20.dp(), 10.dp(), 220.dp(), 40.dp())
    );
    assert_eq!(compositions.get(), 2);
    Ok(())
}

#[test]
fn a_leaving_piece_fades_inside_the_shrinking_clip_of_a_reflowing_node() -> TestResult {
    let wide = State::new(true);
    let (mut view, mut scene, _) = island_view(&wide)?;
    halfway(&mut view, &mut scene, &wide, false)?;
    let (clip_at, clip) = clip(&scene)?;
    assert_eq!(clip, Rect::new(20.dp(), 10.dp(), 170.dp(), 40.dp()));
    assert_eq!(glass(&scene)?, clip);
    let leaving = (scene.commands().iter()).position(|command| {
        matches!(command, DrawCommand::Fill { shape, .. } if shape.bounds().x == 130.dp())
    });
    assert!(inside_clip(
        &scene.commands(),
        clip_at,
        leaving.required("the leaving fill")?
    ));
    assert_eq!(piece_rects(&scene)?.len(), 2);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(
        glass(&scene)?,
        Rect::new(20.dp(), 10.dp(), 120.dp(), 40.dp())
    );
    assert_eq!(piece_rects(&scene)?.len(), 1);
    Ok(())
}

#[test]
fn the_content_of_a_reflowing_node_moves_with_it_when_a_sibling_before_it_grows() -> TestResult {
    let wide = State::new(false);
    let lead = wide.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            let lead_width = if lead.get() { 200.dp() } else { 100.dp() };
            ui.spacer(lead_width, 20.dp(), Modifier::new());
            island(ui, "island", 20.dp(), Modifier::new(), |ui| {
                pieces(ui, false);
            });
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    let live = Rect::new(170.dp(), 10.dp(), 120.dp(), 40.dp());
    assert_eq!(view.bounds_of("island"), Some(live));
    assert_eq!(glass(&scene)?, live);
    assert_eq!(
        piece_rects(&scene)?,
        [Rect::new(180.dp(), 20.dp(), 100.dp(), 20.dp())]
    );
    Ok(())
}

#[test]
fn the_content_of_a_reflowing_node_follows_its_moving_left_edge() -> TestResult {
    let wide = State::new(false);
    let (mut view, mut scene) = left_growing_view(&wide)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    let live = Rect::new(100.dp(), 10.dp(), 170.dp(), 40.dp());
    assert_eq!(view.bounds_of("island"), Some(live));
    assert_eq!(glass(&scene)?, live);
    assert_eq!(
        piece_rects(&scene)?,
        [
            Rect::new(110.dp(), 20.dp(), 100.dp(), 20.dp()),
            Rect::new(210.dp(), 20.dp(), 100.dp(), 20.dp())
        ]
    );
    Ok(())
}

#[test]
fn a_reflow_retargeted_before_it_settles_moves_its_content_with_it() -> TestResult {
    let wide = State::new(false);
    let (mut view, mut scene) = left_growing_view(&wide)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    wide.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 750)?;
    let live = view.tagged_bounds("island")?;
    assert!(live.x > 50.dp() && live.x < 150.dp(), "{live:?}");
    assert_eq!(glass(&scene)?, live);
    assert_eq!(piece_rects(&scene)?[0].x, live.x + 10.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 3000)?;
    assert_eq!(
        glass(&scene)?,
        Rect::new(150.dp(), 10.dp(), 120.dp(), 40.dp())
    );
    assert_eq!(
        piece_rects(&scene)?,
        [Rect::new(160.dp(), 20.dp(), 100.dp(), 20.dp())]
    );
    Ok(())
}

#[test]
fn a_reflowing_node_takes_input_on_its_live_surface() -> TestResult {
    let wide = State::new(false);
    let (target, clicked) = (wide.clone(), State::new(0));
    let count = clicked.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let count = count.clone();
        let click = Modifier::new().on_click(move || count.update(|value| *value += 1));
        island(ui, "island", 20.dp(), click, |ui| pieces(ui, target.get()));
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    click(&mut view, Point::new(200.dp(), 30.dp()));
    assert_eq!(clicked.get(), 0);
    click(&mut view, Point::new(180.dp(), 30.dp()));
    assert_eq!(clicked.get(), 1);
    Ok(())
}

#[test]
fn an_entering_child_takes_input_only_inside_the_growing_clip() -> TestResult {
    let wide = State::new(false);
    let (target, clicked) = (wide.clone(), State::new(0));
    let count = clicked.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let wide = target.get();
        let count = count.clone();
        island(ui, "island", 20.dp(), Modifier::new(), |ui| {
            piece(ui, "first", Modifier::new());
            if wide {
                let click = Modifier::new().on_click(move || count.update(|value| *value += 1));
                piece(ui, "second", click);
            }
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    click(&mut view, Point::new(210.dp(), 30.dp()));
    assert_eq!(clicked.get(), 0);
    click(&mut view, Point::new(150.dp(), 30.dp()));
    assert_eq!(clicked.get(), 1);
    Ok(())
}

#[test]
fn a_reflowing_node_inside_another_follows_its_own_bounds_and_the_motion_of_its_parent()
-> TestResult {
    let wide = State::new(false);
    let target = wide.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let wide = target.get();
        let x = if wide { 20.dp() } else { 120.dp() };
        island(ui, "outer", x, Modifier::new(), |ui| {
            island(ui, "inner", 0.dp(), Modifier::new(), |ui| pieces(ui, wide));
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    let (outer, inner) = (
        Rect::new(70.dp(), 10.dp(), 190.dp(), 60.dp()),
        Rect::new(80.dp(), 30.dp(), 170.dp(), 40.dp()),
    );
    assert_eq!(view.bounds_of("outer"), Some(outer));
    assert_eq!(view.bounds_of("inner"), Some(inner));
    let glass: Vec<_> = fills(&scene, ISLAND)?
        .iter()
        .map(|fill| fill.rect)
        .collect();
    assert_eq!(glass, [outer, inner]);
    assert_eq!(
        piece_rects(&scene)?,
        [
            Rect::new(90.dp(), 40.dp(), 100.dp(), 20.dp()),
            Rect::new(190.dp(), 40.dp(), 100.dp(), 20.dp())
        ]
    );
    Ok(())
}

#[test]
fn a_shared_reflowing_node_moves_its_content_with_its_glass() -> TestResult {
    let wide = State::new(false);
    let state = wide.clone();
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
                                .material(Material::new(ISLAND), 20.dp())
                                .padding(10.dp())
                                .transition(Transition::new(linear()).reflow())
                                .tag("shared"),
                            |ui| {
                                ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
                                    pieces(ui, right);
                                });
                            },
                        );
                    }
                },
            );
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    let live = Rect::new(155.dp(), 20.dp(), 170.dp(), 40.dp());
    assert_eq!(view.bounds_of("shared"), Some(live));
    assert_eq!(glass(&scene)?, live);
    assert_eq!(
        piece_rects(&scene)?[0],
        Rect::new(165.dp(), 30.dp(), 100.dp(), 20.dp())
    );
    Ok(())
}

#[test]
fn a_leaving_piece_moves_with_the_content_of_a_moving_reflowing_node() -> TestResult {
    let wide = State::new(true);
    let (mut view, mut scene) = left_growing_view(&wide)?;
    halfway(&mut view, &mut scene, &wide, false)?;
    assert_eq!(
        glass(&scene)?,
        Rect::new(100.dp(), 10.dp(), 170.dp(), 40.dp())
    );
    assert_eq!(
        piece_rects(&scene)?,
        [
            Rect::new(110.dp(), 20.dp(), 100.dp(), 20.dp()),
            Rect::new(210.dp(), 20.dp(), 100.dp(), 20.dp())
        ]
    );
    Ok(())
}

#[test]
fn reduced_motion_settles_a_reflow_at_once() -> TestResult {
    let wide = State::new(false);
    let (mut view, mut scene, _) = island_view(&wide)?;
    view.provide(&REDUCED_MOTION, true);
    wide.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(
        glass(&scene)?,
        Rect::new(20.dp(), 10.dp(), 220.dp(), 40.dp())
    );
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn reflowing_islands_feed_their_live_bounds_into_the_material_group() -> TestResult {
    let wide = State::new(false);
    let target = wide.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let wide = target.get();
        let group = Modifier::new().material_group(Material::new(ISLAND), 24.dp());
        let island = || {
            Modifier::new()
                .island(20.dp())
                .transition(Transition::new(linear()).reflow())
        };
        ui.layout(Arrangement::Overlay, group, |ui| {
            ui.key("first", island(), |ui| {
                ui.spacer(
                    if wide { 300.dp() } else { 100.dp() },
                    40.dp(),
                    Modifier::new(),
                );
            });
            let second = Modifier::new().offset(if wide { 320.dp() } else { 240.dp() }, 0.dp());
            ui.key("second", second.then(island()), |ui| {
                ui.spacer(100.dp(), 40.dp(), Modifier::new());
            });
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    halfway(&mut view, &mut scene, &wide, true)?;
    let members = scene.commands().iter().find_map(|command| match command {
        DrawCommand::Fill {
            shape: Shape::Union(union),
            ..
        } => Some(union.members().to_vec()),
        _ => None,
    });
    let island = |x: Dp, width: Dp| {
        RoundedRect::new(Rect::new(x, 0.dp(), width, 40.dp()), Corners::all(20.dp()))
    };
    let islands: [Shape; 2] = [
        island(0.dp(), 200.dp()).into(),
        island(280.dp(), 100.dp()).into(),
    ];
    assert_eq!(members.required("a union fill")?, islands);
    Ok(())
}
