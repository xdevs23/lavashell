// SPDX-License-Identifier: GPL-3.0-only
use super::material_motion::painted_fills;
use super::*;
use crate::animation::{Animation, Easing, Tween};
use crate::geometry::{Axis, Color};
use crate::scene::{FontWeight, TextAlignment};
use crate::scroll::ScrollState;
use std::time::Duration;

fn linear(ms: u64) -> Tween {
    Tween::new(Duration::from_millis(ms))
        .easing(Easing::Linear)
        .preserve_velocity(false)
}

fn shown(scene: &Scene) -> Result<Rect, TestError> {
    let fills = painted_fills(&scene.commands())?;
    assert_eq!(fills.len(), 1);
    Ok(fills[0].rect)
}

fn shared(ui: &mut Ui) {
    ui.shared(
        "shared",
        Modifier::new()
            .background(Color::WHITE)
            .transition(linear(1000))
            .tag("shared"),
        |ui| {
            ui.spacer(60.dp(), 40.dp(), Modifier::new());
        },
    );
}

fn column(ui: &mut Ui, right: bool, filled: bool, content: fn(&mut Ui)) {
    ui.layout(
        Axis::Vertical,
        Modifier::new().offset(if right { 300.dp() } else { 10.dp() }, 20.dp()),
        move |ui| {
            if filled {
                content(ui);
            }
        },
    );
}

#[test]
fn a_shared_node_in_a_sliding_parent_ends_where_it_is_laid_out() -> TestResult {
    let right = State::new(false);
    let moved = right.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let holder = moved.get();
        let slide = Presence::default().slide(crate::constant!(slide_offset(200.0, 0.0)));
        ui.layout(
            Axis::Vertical,
            Modifier::new().transition(Transition::new(linear(1000)).enter(slide)),
            move |ui| {
                for right in [false, true] {
                    column(ui, right, holder == right, shared);
                }
            },
        );
    });
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    right.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert!(!view.needs_frame());
    assert_eq!(view.next_deadline(), Some(Duration::from_secs(1)));
    let before = shown(scene.at(1000))?;
    let rest = frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(rest.len(), 1, "only the slot of the parent changes");
    assert_eq!(shown(&scene)?, before);
    let before = shown(scene.at(1500))?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(shown(&scene)?, before);
    assert_eq!(view.bounds_of(&"shared"), Some(before));
    assert_eq!(before.x, 300.dp());
    Ok(())
}

fn in_component(ui: &mut Ui) {
    ui.spacer(10.dp(), 30.dp(), Modifier::new());
    ui.scoped("component", Modifier::new(), shared);
}

#[test]
fn a_shared_node_in_a_component_moves_between_the_places_where_it_is_laid_out() -> TestResult {
    let right = State::new(false);
    let moved = right.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let holder = moved.get();
        for right in [false, true] {
            column(ui, right, holder == right, in_component);
        }
    });
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let start = shown(&scene)?;
    assert_eq!(view.bounds_of(&"shared"), Some(start));
    assert_eq!((start.x, start.y), (10.dp(), 50.dp()));
    right.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(shown(&scene)?, start);
    assert_eq!(shown(scene.at(500))?.x, 155.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    let end = shown(&scene)?;
    assert_eq!(view.bounds_of(&"shared"), Some(end));
    assert_eq!(end.x, 300.dp());
    Ok(())
}

fn show_shared_title(ui: &mut Ui, style: TextStyle) {
    let in_place = Transition {
        bounds: BoundsMotion::Instant,
        ..Transition::new(linear(1000))
    };
    ui.provide(&TEXT_STYLE, style, |ui| {
        ui.shared("title", Modifier::new().transition(linear(1000)), |ui| {
            ui.text("Title", Modifier::new().transition(in_place));
        });
    });
}

fn painted_text(scene: &Scene) -> Result<(Rect, TextStyle<Dp>), TestError> {
    (scene.commands().into_iter())
        .find_map(|command| match command {
            DrawCommand::Text { rect, style, .. } => Some((rect, style)),
            _ => None,
        })
        .required("a painted text")
}

fn painted_text_style(scene: &Scene) -> Result<TextStyle<Dp>, TestError> {
    painted_text(scene).map(|(_, style)| style)
}

#[test]
fn a_shared_text_node_blends_its_text_style_while_its_size_follows_the_scaled_move() -> TestResult {
    use crate::color::HuePath;
    use crate::color::samples::{
        BLUE, VIOLET, assert_halfway_from_blue_to_violet, assert_shown_alike,
    };
    let right = State::new(false);
    let moved = right.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let holder = moved.get();
        column(ui, false, !holder, |ui| {
            show_shared_title(ui, TextStyle::new("sans-serif", 16.sp(), BLUE));
        });
        column(ui, true, holder, |ui| {
            show_shared_title(
                ui,
                TextStyle::new("sans-serif", 24.sp(), VIOLET).weight(FontWeight::BOLD),
            );
        });
    });
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    right.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let first = painted_text_style(&scene)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let halfway = painted_text_style(&scene)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    let end = painted_text_style(&scene)?;
    assert_eq!(first.weight, FontWeight::NORMAL);
    assert_shown_alike(first.color, BLUE);
    assert_eq!(halfway.weight, FontWeight::try_new(550.0)?);
    assert_halfway_from_blue_to_violet(halfway.color, HuePath::Shorter);
    assert_eq!(end.weight, FontWeight::BOLD);
    assert_shown_alike(end.color, VIOLET);
    assert_eq!(
        first.size, end.size,
        "the move scales the size, so the size is the same at both ends"
    );
    Ok(())
}

fn text_with_transition(style: State<TextStyle>, transition: State<Transition>) -> View {
    View::new(crate::platform::fixture::facts(), move |ui| {
        let transition = transition.get();
        ui.provide(&TEXT_STYLE, style.get(), |ui| {
            ui.text("Title", Modifier::new().transition(transition));
        });
    })
}

#[test]
fn a_text_node_fades_its_color_in_once_its_transition_blends_the_appearance() -> TestResult {
    use crate::color::samples::{BLUE, assert_shown_alike};
    let style = State::new(TextStyle::new("sans-serif", 16.sp(), BLUE));
    let transition = State::new(Transition::new(linear(1000)).bounds_only());
    let mut view = text_with_transition(style, transition.clone());
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    transition.set(Transition::new(linear(1000)));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let first = painted_text_style(&scene)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    let end = painted_text_style(&scene)?;
    crate::float_asserts::assert_f64_eq(first.color.alpha(), 0.0);
    assert_shown_alike(end.color, BLUE);
    Ok(())
}

#[test]
fn a_text_style_change_starts_a_motion_only_when_it_changes_a_blended_part() -> TestResult {
    use crate::color::samples::{BLUE, VIOLET};
    let blue = TextStyle::new("sans-serif", 16.sp(), BLUE);
    let style = State::new(blue.clone());
    let transition = State::new(Transition::new(linear(1000)));
    let mut view = text_with_transition(style.clone(), transition);
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    style.set(TextStyle {
        max_lines: Some(1),
        ..blue.clone()
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert!(!view.needs_frame(), "a new line limit applies at once");
    style.set(blue.color(VIOLET));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert!(view.needs_frame(), "a new color blends");
    Ok(())
}

struct SpacedMetrics;

impl TextMeasurer for SpacedMetrics {
    fn measure(&mut self, t: &str, s: &TextStyle<Dp>, w: Dp) -> Size {
        let each = s.size * 0.5 + s.letter_spacing.to_dp(s.size);
        Size::new(repeated(each, t.len()).min(w), s.size * 1.3)
    }
}

fn show_spaced_frame(scene: &mut Scene, view: &mut View, time_ms: u64) -> TestResult {
    let time = Duration::from_millis(time_ms);
    let timing = FrameTime {
        now: time,
        presentation: time,
        ..Default::default()
    };
    let viewport = Viewport::new(WINDOW_SIZE, Density::ONE);
    advance_with_timing(
        scene.at(time_ms),
        view,
        viewport,
        &mut SpacedMetrics,
        timing,
    )
}

fn show_letter_spacing_halfway_to_none(spaced: TextStyle) -> Result<(View, Scene), TestError> {
    let style = State::new(spaced.clone());
    let in_place = Transition {
        bounds: BoundsMotion::Instant,
        ..Transition::new(linear(1000))
    };
    let mut view = text_with_transition(style.clone(), State::new(in_place));
    let mut scene = Scene::default();
    show_spaced_frame(&mut scene, &mut view, 0)?;
    style.set(spaced.letter_spacing(crate::units::Em::ZERO));
    show_spaced_frame(&mut scene, &mut view, 0)?;
    show_spaced_frame(&mut scene, &mut view, 500)?;
    Ok((view, scene))
}

#[test]
fn a_text_node_paints_into_the_width_its_blended_letter_spacing_needs() -> TestResult {
    use crate::color::samples::BLUE;
    use crate::units::EmExt;
    let spaced = TextStyle::new("sans-serif", 16.sp(), BLUE).letter_spacing(0.2.em());
    let (_view_after_the_blend, scene) = show_letter_spacing_halfway_to_none(spaced)?;
    let (rect, halfway) = painted_text(&scene)?;
    let needed = SpacedMetrics.measure("Title", &halfway, Dp::INFINITY).width;
    assert_eq!(rect.width, needed);
    Ok(())
}

#[test]
fn the_accessibility_geometry_recorded_for_a_blending_text_node_follows_its_widened_box()
-> TestResult {
    use crate::color::samples::BLUE;
    use crate::units::EmExt;
    let spaced = TextStyle {
        alignment: TextAlignment::Center,
        ..TextStyle::new("sans-serif", 16.sp(), BLUE).letter_spacing(0.2.em())
    };
    let (view, scene) = show_letter_spacing_halfway_to_none(spaced)?;
    let (widened, _blended_style) = painted_text(&scene)?;
    let node = (view.nodes.iter())
        .find(|node| node.text_style().is_some())
        .required("the text node")?;
    let geometry = (view.accessibility.geometry.get(&node.id))
        .required("the accessibility geometry recorded for the text node")?;
    assert_eq!(geometry.content, widened);
    Ok(())
}

#[test]
fn a_shared_node_in_moved_content_shows_where_it_is_laid_out() -> TestResult {
    fn content(ui: &mut Ui) {
        ui.spacer(100.dp(), 100.dp(), Modifier::new());
        shared(ui);
        ui.spacer(100.dp(), 400.dp(), Modifier::new());
    }
    let scroll = ScrollState::new(Axis::Vertical);
    let list = scroll.clone();
    let scrolled = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.scroll(
            list.clone(),
            Modifier::new().size(100.dp(), 200.dp()),
            content,
        );
    });
    let offset = Animation::new(Point::new(Dp::ZERO, -50.dp()))?;
    let moving = offset.clone();
    let offset_by = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(
            Axis::Vertical,
            Modifier::new().animated_offset(moving.clone()),
            content,
        );
    });
    for (case, mut view) in [("scroll", scrolled), ("offset", offset_by)] {
        let mut scene = Scene::default();
        scroll.jump_to(50.dp());
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        assert_eq!(shown(&scene)?.y, 50.dp(), "{case}");
    }
    Ok(())
}

#[test]
fn a_shared_node_does_not_move_on_the_screen_while_the_host_moves_the_viewport_with_the_content()
-> TestResult {
    let grown = State::new(false);
    let read = grown.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let top = if read.get() { 300.dp() } else { 0.dp() };
        ui.layout(
            Axis::Vertical,
            Modifier::new().offset(10.dp(), 20.dp() + top),
            shared,
        );
    });
    view.set_viewport_origin(Point::new(0.dp(), 300.dp()));
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    grown.set(true);
    view.set_viewport_origin(Point::default());
    let window = Size::new(WINDOW_SIZE.width, WINDOW_SIZE.height + 300.dp());
    show_frame(&mut scene, &mut view, window, 0)?;
    let rest = view.bounds_of(&"shared");
    assert_eq!(rest, Some(shown(scene.at(500))?));
    Ok(())
}
