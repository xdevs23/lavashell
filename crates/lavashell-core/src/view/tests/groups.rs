// SPDX-License-Identifier: GPL-3.0-only
use super::material_motion::{linear, painted_fills};
use super::*;
use crate::retained::Extent;
use crate::scene::tree::{Change, DamageTracker};
use crate::scroll::ScrollState;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// A view with a list 120 dp high that scrolls `scroll`. The list contains six rows 40 dp high,
/// each in a scoped component. A tap on a row sets `tapped` to the index of the row.
fn rows(scroll: &ScrollState, tapped: &Rc<Cell<Option<u32>>>) -> View {
    let (list, tapped) = (scroll.clone(), tapped.clone());
    View::new(crate::platform::fixture::facts(), move |ui| {
        let tapped = tapped.clone();
        ui.scroll(
            list.clone(),
            Modifier::new().size(100.dp(), 120.dp()),
            move |ui| {
                for row in 0..6 {
                    let tapped = tapped.clone();
                    ui.scoped(row, Modifier::new(), move |ui| {
                        let tapped = tapped.clone();
                        ui.spacer(
                            100.dp(),
                            40.dp(),
                            Modifier::new()
                                .on_click(move || tapped.set(Some(row)))
                                .background(Color::WHITE),
                        );
                    });
                }
            },
        );
    })
}

#[test]
fn a_scroll_step_changes_one_slot() -> TestResult {
    let scroll = ScrollState::new(Axis::Vertical);
    let tapped = Rc::new(Cell::new(None));
    let mut view = rows(&scroll, &tapped);
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let before = view.stats();
    view.input(InputEvent::Scroll {
        at: Point::new(10.dp(), 10.dp()),
        delta: Point::new(Dp::ZERO, 30.dp()),
    });
    let changes = frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], Change::SetSlot { .. }));
    assert_eq!(view.stats().painted_components, before.painted_components);
    assert_eq!(view.stats().reused_components, before.reused_components + 6);
    tap_at(&mut view, Point::new(10.dp(), 55.dp()));
    assert_eq!(tapped.get(), Some(2));
    Ok(())
}

#[test]
fn a_frame_that_changes_nothing_sends_no_change() -> TestResult {
    let mut scene = Scene::default();
    let mut view = rows(&ScrollState::new(Axis::Vertical), &Rc::default());
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.invalidate();
    assert!(frame(&mut scene, &mut view, WINDOW_SIZE, 0)?.is_empty());
    Ok(())
}

#[test]
fn a_root_composition_changes_only_the_leaves_that_paint_differently() -> TestResult {
    let text = State::new("before");
    let read = text.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.text(read.get(), Modifier::new());
        ui.scoped("card", Modifier::new(), |ui| {
            ui.spacer(100.dp(), 40.dp(), Modifier::new().background(Color::WHITE));
        });
    });
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    text.set("after");
    let changes = frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.stats().root_compositions, 2);
    assert!(!changes.is_empty());
    assert!((changes.iter()).all(|change| matches!(change, Change::SetContent { .. })));
    let texts: Vec<_> = (scene.commands().into_iter())
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["after"]);
    Ok(())
}

/// A blur filters everything drawn inside its effect layer as one image. The text of a scoped
/// component inside the blurred node must therefore be drawn inside that layer.
#[test]
fn a_component_inside_a_blur_paints_inside_it() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(Axis::Vertical, Modifier::new().blur(4.dp()), |ui| {
            ui.scoped("inside", Modifier::new(), |ui| {
                ui.text("blurred", Modifier::new());
            });
        });
    });
    let mut scene = Scene::default();
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let commands = scene.commands();
    let at = |found: fn(&DrawCommand) -> bool| commands.iter().position(found);
    let blur =
        at(|command| matches!(command, DrawCommand::EffectLayer { .. })).required("a blur")?;
    let text = at(|command| matches!(command, DrawCommand::Text { .. })).required("a text")?;
    let depth = (commands[blur..text].iter()).fold(0, |depth: i32, command| match command {
        DrawCommand::Restore => depth - 1,
        DrawCommand::Save | DrawCommand::EffectLayer { .. } => depth + 1,
        _ => depth,
    });
    assert!(blur < text && depth > 0);
    Ok(())
}

#[test]
fn a_component_with_a_transition_moves_on_it_when_a_sibling_before_it_grows() -> TestResult {
    let wide = State::new(false);
    let read = wide.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let read = read.clone();
        ui.layout(Axis::Horizontal, Modifier::new(), move |ui| {
            let read = read.clone();
            ui.scoped("sibling", Modifier::new(), move |ui| {
                let width = if read.get() { 60.dp() } else { 20.dp() };
                ui.spacer(width, 20.dp(), Modifier::new());
            });
            ui.scoped(
                "card",
                Modifier::new()
                    .background(Color::WHITE)
                    .transition(Transition::new(linear()))
                    .tag("card"),
                |ui| {
                    ui.spacer(40.dp(), 40.dp(), Modifier::new());
                },
            );
        });
    });
    let card = |scene: &Scene| painted_fills(&scene.commands()).map(|fills| fills[0].rect.x);
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    wide.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_eq!(card(&scene)?, 40.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(card(&scene)?, 60.dp());
    assert_eq!(view.tagged_bounds("card")?.x, 60.dp());
    Ok(())
}

#[test]
fn a_node_moved_by_an_offset_damages_only_where_it_paints() -> TestResult {
    let shift = State::new(0.0);
    let read = shift.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let read = read.clone();
        ui.scoped(
            "moved",
            Modifier::new().offset(200.dp(), 100.dp()),
            move |ui| {
                let read = read.clone();
                ui.canvas(Size::new(20.dp(), 20.dp()), Modifier::new(), move |d| {
                    d.circle(
                        Point::new(Dp::new(10.0 + read.get()), 10.dp()),
                        4.dp(),
                        Color::WHITE,
                    );
                });
            },
        );
    });
    let viewport = Viewport::new(WINDOW_SIZE, Density::ONE);
    let (mut scene, mut presented) = (Scene::default(), DamageTracker::default());
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let baseline = scene.snapshot();
    let _baseline_damage = presented.damage(baseline.at(Duration::ZERO), viewport);
    shift.set(2.0);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let shifted = scene.snapshot();
    let damage = presented
        .damage(shifted.at(Duration::ZERO), viewport)
        .required("damage")?;
    assert!(
        damage.x >= 190.dp() && damage.y >= 90.dp() && damage.width < 60.dp(),
        "{damage:?}"
    );
    Ok(())
}

#[test]
fn a_leaf_at_the_top_level_covers_what_it_paints_only() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.canvas(Size::new(20.dp(), 20.dp()), Modifier::new(), |d| {
            d.circle(Point::new(10.dp(), 10.dp()), 4.dp(), Color::WHITE);
        });
    });
    let mut scene = Scene::default();
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let outputs = (scene.snapshot().at(Duration::ZERO).paint_order().leaves())
        .map(|leaf| leaf.packet.footprint().output)
        .collect::<Vec<_>>();
    let painted = Rect::new(6.dp(), 6.dp(), 8.dp(), 8.dp());
    assert_eq!(outputs, [Some(Extent::Within(painted))]);
    Ok(())
}
