// SPDX-License-Identifier: GPL-3.0-only
use super::material_motion::painted_fills;
use super::*;
use crate::animation::{Easing, Tween};
use crate::float_asserts::assert_f64_eq;
use crate::geometry::{Axis, Color};
use crate::scene::{Brush, DrawCommand, SaveStackChange};
use std::time::Duration;

const ROW: Color = Color::rgb(0x0040_a0ff);
const BADGE: Color = Color::rgb(0x00ff_8040);

fn painted_colors(scene: &Scene) -> Result<Vec<u32>, TestError> {
    let fills = painted_fills(&scene.commands())?;
    Ok(fills.iter().map(|fill| fill.color.to_argb8()).collect())
}

fn shown(scene: &Scene) -> Result<Rect, TestError> {
    let fills = painted_fills(&scene.commands())?;
    let titles = fills.into_iter().filter(|fill| fill.color == Color::WHITE);
    let [title] = titles
        .collect::<Vec<_>>()
        .exactly("one fill of the title")?;
    Ok(title.rect)
}

fn columns(right: &State<bool>, down: &State<Dp>) -> View {
    decorated_columns(right, down, |column| column)
}

fn decorated_columns(
    right: &State<bool>,
    down: &State<Dp>,
    decorate: fn(Modifier) -> Modifier,
) -> View {
    let (holder, lowered) = (right.clone(), down.clone());
    View::new(crate::platform::fixture::facts(), move |ui| {
        let (holder, y) = (holder.get(), lowered.get());
        for right in [false, true] {
            let (x, column) = if right {
                (300.dp(), decorate(Modifier::new()))
            } else {
                (10.dp(), Modifier::new())
            };
            ui.layout(Axis::Vertical, column.offset(x, y), move |ui| {
                if holder == right {
                    title(ui);
                }
                if right {
                    ui.spacer(60.dp(), 40.dp(), Modifier::new().background(ROW));
                }
            });
        }
    })
}

fn linear() -> Tween {
    Tween::new(Duration::from_millis(1000))
        .easing(Easing::Linear)
        .preserve_velocity(false)
}

fn title(ui: &mut Ui) {
    title_with(ui, &Transition::new(linear()).instant_at_rest());
}

fn title_with(ui: &mut Ui, transition: &Transition) {
    ui.shared(
        "title",
        Modifier::new()
            .background(Color::WHITE)
            .transition(*transition),
        |ui| {
            ui.spacer(60.dp(), 40.dp(), Modifier::new());
        },
    );
}

fn title_clipped_or_faded(commands: &[DrawCommand]) -> Option<bool> {
    let mut levels = vec![false];
    for command in commands {
        match command {
            DrawCommand::Fill {
                brush: Brush::Solid(color),
                ..
            } if *color == Color::WHITE => return Some(levels.contains(&true)),
            DrawCommand::Clip { .. } => *levels.last_mut()? = true,
            _ => {}
        }
        match command.save_stack_change() {
            SaveStackChange::Push => levels.push(match command {
                DrawCommand::Opacity(alpha) | DrawCommand::OpacityLayer { alpha, .. } => {
                    *alpha < 1.0
                }
                _ => false,
            }),
            SaveStackChange::Pop => {
                let _closed_level = levels.pop();
            }
            SaveStackChange::Unchanged => {}
        }
    }
    None
}

#[test]
fn a_node_instant_at_rest_moves_into_another_parent_and_then_follows_it_at_once() -> TestResult {
    let (right, down) = (State::new(false), State::new(Dp::ZERO));
    let mut view = columns(&right, &down);
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    right.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 600)?;
    assert_eq!(shown(&scene)?.x, 155.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    down.set(50.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1600)?;
    assert_eq!(
        shown(&scene)?,
        Rect::new(300.dp(), 50.dp(), 60.dp(), 40.dp())
    );
    Ok(())
}

#[test]
fn a_node_instant_at_rest_follows_its_parent_at_once_in_the_frame_its_arrival_ends() -> TestResult {
    let (right, down) = (State::new(false), State::new(Dp::ZERO));
    let mut view = columns(&right, &down);
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    right.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 600)?;
    down.set(50.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1100)?;
    assert_eq!(
        shown(&scene)?,
        Rect::new(300.dp(), 50.dp(), 60.dp(), 40.dp())
    );
    Ok(())
}

#[test]
fn a_node_instant_at_rest_carries_its_arrival_along_when_its_parent_moves() -> TestResult {
    let (right, down) = (State::new(false), State::new(Dp::ZERO));
    let mut view = columns(&right, &down);
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    right.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    down.set(50.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 600)?;
    assert_eq!(
        shown(&scene)?,
        Rect::new(155.dp(), 50.dp(), 60.dp(), 40.dp())
    );
    Ok(())
}

#[test]
fn a_node_instant_at_rest_paints_above_the_view_until_it_has_arrived() -> TestResult {
    let (right, down) = (State::new(false), State::new(Dp::ZERO));
    let mut view = columns(&right, &down);
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    right.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 600)?;
    let (title, row) = (Color::WHITE.to_argb8(), ROW.to_argb8());
    assert_eq!(painted_colors(&scene)?, [row, title]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(painted_colors(&scene)?, [title, row]);
    Ok(())
}

fn listed(right: &State<bool>) -> View {
    let holder = right.clone();
    View::new(crate::platform::fixture::facts(), move |ui| {
        let left = !holder.get();
        let left_column = Modifier::new().offset(10.dp(), Dp::ZERO);
        ui.layout(Axis::Vertical, left_column, move |ui| {
            if left {
                title(ui);
            }
        });
        let listing = holder.clone();
        let list = Modifier::new().offset(300.dp(), Dp::ZERO);
        ui.scoped("list", list, move |ui| {
            let right = listing.get();
            ui.layout(Axis::Vertical, Modifier::new(), move |ui| {
                if right {
                    title(ui);
                }
                ui.spacer(60.dp(), 40.dp(), Modifier::new().background(ROW));
            });
        });
    })
}

#[test]
fn a_component_around_an_arriving_node_is_reused_until_the_node_comes_to_rest() -> TestResult {
    let right = State::new(false);
    let mut view = listed(&right);
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    right.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    let (title, row) = (Color::WHITE.to_argb8(), ROW.to_argb8());
    let reused = view.stats().reused_components;
    for ms in [300, 600] {
        show_frame(&mut scene, &mut view, WINDOW_SIZE, ms)?;
        assert_eq!(painted_colors(&scene)?, [row, title], "at {ms} ms");
    }
    assert_eq!(view.stats().reused_components, reused + 2);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(painted_colors(&scene)?, [title, row]);
    Ok(())
}

#[test]
fn an_arriving_node_paints_outside_the_clip_and_opacity_of_its_new_parent() -> TestResult {
    let (right, down) = (State::new(false), State::new(Dp::ZERO));
    let mut view = decorated_columns(&right, &down, |column| {
        column.clip(Corners::default()).opacity(0.5)
    });
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    right.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 600)?;
    let commands = scene.commands();
    assert_eq!(title_clipped_or_faded(&commands), Some(false));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(title_clipped_or_faded(&scene.commands()), Some(true));
    Ok(())
}

#[test]
fn an_arriving_node_moves_toward_the_resting_place_of_a_moving_parent() -> TestResult {
    let (right, down) = (State::new(false), State::new(Dp::ZERO));
    let mut view = decorated_columns(&right, &down, |column| {
        column.transition(Transition::new(linear()))
    });
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    right.set(true);
    down.set(50.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 600)?;
    let title = shown(&scene)?;
    assert_f64_eq(title.x.get(), 155.0);
    assert_f64_eq(title.y.get(), 25.0);
    Ok(())
}

fn column_while_present(present: &State<bool>, node: impl Fn(&mut Ui) + 'static) -> View {
    let present = present.clone();
    View::new(crate::platform::fixture::facts(), move |ui| {
        let shown = present.get();
        let column = Modifier::new().offset(300.dp(), Dp::ZERO);
        ui.layout(Axis::Vertical, column, |ui| {
            if shown {
                node(ui);
            }
            ui.spacer(60.dp(), 40.dp(), Modifier::new().background(ROW));
        });
    })
}

fn returning(present: &State<bool>, away: Presence) -> View {
    column_while_present(present, move |ui| {
        title_with(ui, &Transition::new(linear()).instant_at_rest().exit(away));
    })
}

#[test]
fn a_node_that_comes_back_during_its_exit_arrives_from_where_the_exit_reached() -> TestResult {
    let present = State::new(true);
    let away = Presence::default().slide(slide_offset(0.0, 100.0)?);
    let mut view = returning(&present, away);
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    present.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 600)?;
    present.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 600)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1100)?;
    let (title, row) = (Color::WHITE.to_argb8(), ROW.to_argb8());
    assert_eq!(painted_colors(&scene)?, [row, title]);
    assert_eq!(
        shown(&scene)?,
        Rect::new(300.dp(), 25.dp(), 60.dp(), 40.dp())
    );
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1700)?;
    assert_eq!(painted_colors(&scene)?, [title, row]);
    assert_eq!(
        shown(&scene)?,
        Rect::new(300.dp(), Dp::ZERO, 60.dp(), 40.dp())
    );
    Ok(())
}

fn nested_arrivals(present: &State<bool>, entering: Presence) -> View {
    column_while_present(present, move |ui| {
        let arrival = Transition::new(linear()).instant_at_rest().enter(entering);
        let outer = Modifier::new().background(Color::WHITE).transition(arrival);
        ui.shared("outer", outer, |ui| {
            let inner = Modifier::new().background(BADGE).transition(arrival);
            ui.shared("inner", inner, |ui| {
                ui.spacer(20.dp(), 20.dp(), Modifier::new());
            });
        });
    })
}

#[test]
fn a_node_arriving_inside_an_arriving_node_paints_above_the_view_after_it() -> TestResult {
    let present = State::new(false);
    let entering = Presence::default().slide(slide_offset(0.0, 100.0)?);
    let mut view = nested_arrivals(&present, entering);
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    present.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 600)?;
    let (outer, inner, row) = (Color::WHITE.to_argb8(), BADGE.to_argb8(), ROW.to_argb8());
    assert_eq!(painted_colors(&scene)?, [row, outer, inner]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(painted_colors(&scene)?, [outer, inner, row]);
    Ok(())
}
