// SPDX-License-Identifier: GPL-3.0-only
use super::material_group::inside_clip;
use super::material_motion::painted_fills;
use super::*;
use crate::animation::Animation;
use crate::color::samples::BLUE;
use crate::scene::FontWeight;
use crate::ui::Deferred;

const WINDOW: Viewport = Viewport::new(WINDOW_SIZE, Density::ONE);
const HEADER: Rect = Rect::new(Dp::new(0.0), Dp::new(0.0), Dp::new(200.0), Dp::new(40.0));
const ROW: Rect = Rect::new(Dp::new(40.0), Dp::new(100.0), Dp::new(100.0), Dp::new(20.0));
const QUARTER: Rect = Rect::new(Dp::new(10.0), Dp::new(25.0), Dp::new(175.0), Dp::new(35.0));
const SLIDER: Rect = Rect::new(Dp::new(0.0), Dp::new(60.0), Dp::new(250.0), Dp::new(8.0));
const CLIP: Rect = Rect::new(Dp::new(0.0), Dp::new(0.0), Dp::new(100.0), Dp::new(40.0));

type Tag = (&'static str, &'static str);

fn shared_at(ui: &mut Ui, tag: Tag, at: Rect, inside: impl FnOnce(&mut Ui)) {
    shared_with(ui, tag, at, Modifier::new(), inside);
}

fn shared_with(ui: &mut Ui, tag: Tag, at: Rect, modifier: Modifier, inside: impl FnOnce(&mut Ui)) {
    let key = tag.1;
    let modifier = modifier.offset(at.x, at.y).background(BLUE).tag(tag);
    ui.shared(key, modifier, |ui| {
        ui.spacer(at.width, at.height, Modifier::new());
        inside(ui);
    });
}

fn cross_views(
    ui: &mut Ui,
    progress: &Animation<f64>,
    [page_at, list_at]: [Modifier; 2],
    page: impl FnOnce(&mut Ui),
    list: impl FnOnce(&mut Ui),
) {
    let from = ui.view("page", page_at, page);
    let to = ui.view("list", list_at, list);
    ui.crossing(from, to, progress);
}

fn crossing_in(
    progress: &Animation<f64>,
    [page_at, list_at]: [Modifier; 2],
    page: impl Fn(&mut Ui) + 'static,
    list: impl Fn(&mut Ui) + 'static,
) -> View {
    let progress = progress.clone();
    View::new(crate::platform::fixture::facts(), move |ui| {
        let offsets = [page_at.clone(), list_at.clone()];
        cross_views(ui, &progress, offsets, &page, &list);
    })
}

fn crossing(
    progress: &Animation<f64>,
    page: impl Fn(&mut Ui) + 'static,
    list: impl Fn(&mut Ui) + 'static,
) -> View {
    crossing_in(progress, [Modifier::new(), Modifier::new()], page, list)
}

fn clipped_settings(progress: &Animation<f64>) -> View {
    let progress = progress.clone();
    View::new(crate::platform::fixture::facts(), move |ui| {
        ui.key("clipped", Modifier::new().clip_rect(CLIP, 0.dp()), |ui| {
            let offsets = [Modifier::new(), Modifier::new()];
            cross_views(ui, &progress, offsets, settings_page, settings_list);
        });
    })
}

fn settings_page(ui: &mut Ui) {
    shared_at(ui, ("page", "title"), HEADER, |_| {});
    shared_at(ui, ("page", "slider"), SLIDER, |_| {});
}

fn settings_list(ui: &mut Ui) {
    shared_at(ui, ("list", "title"), ROW, |_| {});
}

fn settings(progress: &Animation<f64>) -> View {
    crossing(progress, settings_page, settings_list)
}

fn advance_at(
    view: &mut View,
    progress: &Animation<f64>,
    at: f64,
    scene: &mut Scene,
) -> TestResult {
    progress.snap(at)?;
    advance_with(scene, view, WINDOW, &mut Metrics)
}

fn frame_at(view: &mut View, progress: &Animation<f64>, at: f64) -> TestResult {
    advance_at(view, progress, at, &mut Scene::default())
}

fn painted_at(view: &mut View, progress: &Animation<f64>, at: f64) -> Result<Vec<Rect>, TestError> {
    let mut scene = Scene::default();
    advance_at(view, progress, at, &mut scene)?;
    let fills = painted_fills(&scene.commands())?;
    Ok(fills.into_iter().map(|fill| fill.rect).collect())
}

#[test]
fn a_shared_key_moves_between_both_views_also_backwards() -> TestResult {
    let progress = Animation::new(0.0)?;
    let mut view = settings(&progress);
    let three_quarters = Rect::new(30.dp(), 75.dp(), 125.dp(), 25.dp());
    for (at, flown) in [(0.25, QUARTER), (0.75, three_quarters), (0.25, QUARTER)] {
        assert_eq!(painted_at(&mut view, &progress, at)?, [SLIDER, flown]);
        assert_eq!(view.tagged_bounds(&("page", "title"))?, flown);
        assert_eq!(view.tagged_bounds(&("list", "title"))?, flown);
    }
    Ok(())
}

#[test]
fn a_paired_element_is_painted_once_where_its_flight_puts_it() -> TestResult {
    let progress = Animation::new(0.0)?;
    let mut view = settings(&progress);
    assert_eq!(painted_at(&mut view, &progress, 0.25)?, [SLIDER, QUARTER]);
    assert_eq!(
        painted_at(&mut view, &progress, 0.0)?,
        [HEADER, SLIDER, ROW]
    );
    Ok(())
}

#[test]
fn views_moved_by_their_offsets_blend_the_rects_where_they_are_painted() -> TestResult {
    let progress = Animation::new(0.0)?;
    let slid = |x_at: fn(f64) -> Dp| {
        Modifier::new().offset(Deferred::<f64>::from(&progress).map(x_at), Dp::ZERO)
    };
    let offsets = [slid(|at| 200.dp() * at), slid(|at| 50.dp() * (at - 1.0))];
    let mut view = crossing_in(&progress, offsets, settings_page, settings_list);
    let slider = SLIDER.translated(Point::new(50.dp(), Dp::ZERO));
    let flown = Rect::new(38.125.dp(), 25.dp(), 175.dp(), 35.dp());
    assert_eq!(painted_at(&mut view, &progress, 0.25)?, [slider, flown]);
    Ok(())
}

#[test]
fn a_flight_is_clipped_by_the_clip_of_the_node_around_both_views() -> TestResult {
    let progress = Animation::new(0.0)?;
    let mut view = clipped_settings(&progress);
    let mut scene = Scene::default();
    advance_at(&mut view, &progress, 0.25, &mut scene)?;
    let commands = scene.commands();
    let clip = (commands.iter())
        .position(|command| matches!(command, DrawCommand::Clip { rect, .. } if *rect == CLIP))
        .required("the clip of the node around both views")?;
    let flown = (painted_fills(&commands)?.into_iter())
        .find(|fill| fill.rect == QUARTER)
        .required("the fill of the flight")?;
    assert!(inside_clip(&commands, clip, flown.command));
    Ok(())
}

fn scoped_at(ui: &mut Ui, tag: Tag, at: Rect) {
    ui.scoped(tag, Modifier::new(), move |ui| {
        shared_at(ui, tag, at, |_| {});
    });
}

#[test]
fn a_reused_component_paints_its_paired_element_once_where_it_moves() -> TestResult {
    let progress = Animation::new(0.0)?;
    let mut view = crossing(
        &progress,
        |ui| scoped_at(ui, ("page", "title"), HEADER),
        |ui| scoped_at(ui, ("list", "title"), ROW),
    );
    assert_eq!(painted_at(&mut view, &progress, 0.25)?, [QUARTER]);
    let reused = view.stats().reused_components;
    assert_eq!(painted_at(&mut view, &progress, 0.25)?, [QUARTER]);
    assert_eq!(view.stats().reused_components, reused + 2);
    assert_eq!(painted_at(&mut view, &progress, 0.0)?, [HEADER, ROW]);
    Ok(())
}

#[test]
fn at_progress_exactly_zero_or_one_each_element_is_at_its_own_bounds() -> TestResult {
    let progress = Animation::new(0.0)?;
    let mut view = settings(&progress);
    for at in [0.0, 1.0] {
        assert_eq!(painted_at(&mut view, &progress, at)?, [HEADER, SLIDER, ROW]);
    }
    Ok(())
}

#[test]
fn a_key_in_only_one_view_does_not_move() -> TestResult {
    let progress = Animation::new(0.0)?;
    let mut view = settings(&progress);
    frame_at(&mut view, &progress, 0.4)?;
    assert_eq!(view.tagged_bounds(&("page", "slider"))?, SLIDER);
    Ok(())
}

#[test]
fn a_hidden_shared_element_leaves_the_element_of_the_other_view_in_place() -> TestResult {
    let progress = Animation::new(0.0)?;
    let mut view = crossing(&progress, settings_page, |ui| {
        let hidden = Modifier::new().shown_when(false);
        shared_with(ui, ("list", "title"), ROW, hidden, |_| {});
    });
    assert_eq!(painted_at(&mut view, &progress, 0.25)?, [HEADER, SLIDER]);
    Ok(())
}

#[test]
fn a_nested_shared_element_moves_inside_the_element_around_it() -> TestResult {
    let progress = Animation::new(0.0)?;
    let mut view = crossing(
        &progress,
        |ui| {
            shared_at(ui, ("page", "row"), HEADER, |ui| {
                let icon = Rect::new(10.dp(), 5.dp(), 30.dp(), 30.dp());
                shared_at(ui, ("page", "icon"), icon, |_| {});
            });
        },
        |ui| {
            shared_at(ui, ("list", "row"), ROW, |ui| {
                let icon = Rect::new(4.dp(), 2.dp(), 16.dp(), 16.dp());
                shared_at(ui, ("list", "icon"), icon, |_| {});
            });
        },
    );
    let icon = Rect::new(18.5.dp(), 29.25.dp(), 26.5.dp(), 26.5.dp());
    assert_eq!(painted_at(&mut view, &progress, 0.25)?, [QUARTER, icon]);
    Ok(())
}

fn titled(ui: &mut Ui, tag: Tag, at: Rect, weight: FontWeight) {
    let style = TextStyle::new("sans-serif", 16.sp(), BLUE).weight(weight);
    ui.provide(&TEXT_STYLE, style, |ui| {
        shared_at(ui, tag, at, |ui| ui.text("Display", Modifier::new()));
    });
}

#[test]
fn the_text_inside_a_paired_element_blends_its_weight_by_the_progress() -> TestResult {
    let progress = Animation::new(0.0)?;
    let heavy = FontWeight::try_new(800.0)?;
    let mut view = crossing(
        &progress,
        move |ui| titled(ui, ("page", "title"), HEADER, heavy),
        |ui| titled(ui, ("list", "title"), ROW, FontWeight::NORMAL),
    );
    let mut scene = Scene::default();
    advance_at(&mut view, &progress, 0.25, &mut scene)?;
    let weights: Vec<FontWeight> = (scene.commands().into_iter())
        .filter_map(|command| match command {
            DrawCommand::Text { style, .. } => Some(style.weight),
            _ => None,
        })
        .collect();
    assert_eq!(weights, [FontWeight::try_new(700.0)?]);
    Ok(())
}

fn clicks_at(
    view: &mut View,
    progress: &Animation<f64>,
    at: f64,
    clicks: &State<u32>,
) -> Result<u32, TestError> {
    frame_at(view, progress, at)?;
    let before = clicks.get();
    for point in [ROW.center(), QUARTER.center()] {
        view.input(InputEvent::Press(point));
        view.input(InputEvent::Release(point));
    }
    Ok(clicks.get() - before)
}

#[test]
fn a_click_on_a_paired_element_counts_only_outside_its_flight() -> TestResult {
    let progress = Animation::new(0.0)?;
    let clicks = State::new(0);
    let counted = clicks.clone();
    let mut view = crossing(&progress, settings_page, move |ui| {
        let counted = counted.clone();
        let click = Modifier::new().on_click(move || counted.update(|count| *count += 1));
        shared_with(ui, ("list", "title"), ROW, click, |_| {});
    });
    assert_eq!(clicks_at(&mut view, &progress, 0.25, &clicks)?, 0);
    assert_eq!(clicks_at(&mut view, &progress, 1.0, &clicks)?, 1);
    Ok(())
}
