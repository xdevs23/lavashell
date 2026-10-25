// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::float_asserts::assert_f64_eq;
use crate::prelude::*;
use crate::test_error::{Required, TestResult};
use crate::view::test_scene::{Scene, advance_with, advance_with_timing};
use std::{cell::Cell, rc::Rc, time::Duration};

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(100.0), Dp::new(100.0)), Density::ONE);

struct Measure;

impl TextMeasurer for Measure {
    fn measure(&mut self, text: &str, style: &TextStyle<Dp>, width: Dp) -> Size {
        let natural: Dp = text.chars().map(|_| style.size).sum();
        Size::new(
            natural.min(width),
            style.size * (natural / width.max(1.dp())).ceil().max(1.0),
        )
    }
}

#[test]
fn overrides_survive_system_changes_and_equal_values_do_not_notify() -> TestResult {
    let compositions = Rc::new(Cell::new(0));
    let reduced = Rc::new(Cell::new(false));
    let (count, seen) = (compositions.clone(), reduced.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (count, seen) = (count.clone(), seen.clone());
        ui.scoped("reader", Modifier::new(), move |ui| {
            count.set(count.get() + 1);
            seen.set(ui.local(&REDUCED_MOTION));
        });
    });
    let mut source = PreferenceSource::default();
    source.set_overrides(PreferenceOverrides {
        reduced_motion: Some(false),
        ..Default::default()
    });
    view.provide_preferences(&source.values());
    advance_with(&mut scene, &mut view, WINDOW, &mut Measure)?;
    source.set_system(PreferenceValues {
        reduced_motion: true,
        ..Default::default()
    });
    view.provide_preferences(&source.values());
    advance_with(&mut scene, &mut view, WINDOW, &mut Measure)?;
    assert_eq!((compositions.get(), reduced.get()), (1, false));
    source.set_overrides(PreferenceOverrides::default());
    view.provide_preferences(&source.values());
    advance_with(&mut scene, &mut view, WINDOW, &mut Measure)?;
    assert_eq!((compositions.get(), reduced.get()), (2, true));
    Ok(())
}

#[test]
fn scaling_validates_inputs_and_supports_non_linear_policy() -> TestResult {
    for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(TextScaling::linear(value).is_none());
    }
    let table = TextScaling::table([(10.sp(), 20.dp()), (20.sp(), 30.dp())]).required("a table")?;
    assert_eq!(table.to_dp(5.sp()), 10.dp());
    assert_eq!(table.to_dp(10.sp()), 20.dp());
    assert_eq!(table.to_dp(15.sp()), 25.dp());
    assert_eq!(table.to_dp(20.sp()), 30.dp());
    assert_eq!(table.to_dp(40.sp()), 60.dp());
    assert_eq!(
        TextScaling::linear(1.5).required("scaling")?.to_dp(12.sp()),
        18.dp()
    );
    let overflowing = TextScaling::linear(f64::MAX).required("scaling")?;
    assert_eq!(overflowing.to_dp(12.sp()), 12.dp());
    Ok(())
}

#[test]
fn a_table_needs_positive_sizes_that_increase() {
    assert!(TextScaling::table([]).is_none());
    assert!(TextScaling::table([(10.sp(), 20.dp()), (10.sp(), 30.dp())]).is_none());
    assert!(TextScaling::table([(20.sp(), 20.dp()), (10.sp(), 30.dp())]).is_none());
    assert!(TextScaling::table([(0.sp(), 1.dp())]).is_none());
    assert!(TextScaling::table([(10.sp(), Dp::INFINITY)]).is_none());
}

#[test]
fn tables_compare_by_value() {
    let points = [(10.sp(), 20.dp()), (20.sp(), 30.dp())];
    assert_eq!(TextScaling::table(points), TextScaling::table(points));
    assert_ne!(
        TextScaling::table(points),
        TextScaling::table([(10.sp(), 20.dp())])
    );
    assert_ne!(TextScaling::table(points), Some(TextScaling::default()));
}

#[test]
fn runtime_scaling_reflows_scoped_content_without_cumulative_scaling() -> TestResult {
    let compositions = Rc::new(Cell::new(0));
    let count = compositions.clone();
    let state = State::new(false);
    let signal = state.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let count = count.clone();
        let signal = signal.clone();
        ui.scoped("text", Modifier::new(), move |ui| {
            signal.with(|_| {});
            count.set(count.get() + 1);
            ui.text("abcdefgh", Modifier::new().font_size(10.sp()).tag("text"));
        });
    });
    let window = Viewport::new(Size::new(100.dp(), 200.dp()), Density::ONE);
    advance_with(&mut scene, &mut view, window, &mut Measure)?;
    let text_height = |view: &View| {
        view.bounds_of("text")
            .required("the text")
            .map(|text| text.height)
    };
    assert_eq!(text_height(&view)?, 10.dp());
    view.provide(&TEXT_SCALING, TextScaling::linear(2.0).required("scaling")?);
    assert!(view.needs_frame());
    advance_with(&mut scene, &mut view, window, &mut Measure)?;
    assert_eq!(text_height(&view)?, 40.dp());
    let scaled_compositions = compositions.get();
    advance_with(&mut scene, &mut view, window, &mut Measure)?;
    assert_eq!(compositions.get(), scaled_compositions);
    assert_eq!(text_height(&view)?, 40.dp());
    state.set(true);
    advance_with(&mut scene, &mut view, window, &mut Measure)?;
    assert_eq!(text_height(&view)?, 40.dp());
    view.provide(&TEXT_SCALING, TextScaling::default());
    advance_with(&mut scene, &mut view, window, &mut Measure)?;
    assert_eq!(text_height(&view)?, 10.dp());
    Ok(())
}

#[test]
fn tooltips_and_canvas_text_follow_the_text_size_setting() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let tooltip = TooltipStyle {
            text: TextStyle::new("sans-serif", 10.sp(), Color::WHITE),
            background: Color::WHITE,
            border: BorderStyle {
                color: Color::WHITE,
                width: constant!(NonNegativeDp::try_new(Dp::new(1.0))),
                radius: 4.dp(),
            },
            padding: Insets::all(4.dp()),
            gap: 4.dp(),
        };
        let style = InteractionStyle {
            tooltip: Some(tooltip),
            ..Default::default()
        };
        ui.provide(&INTERACTION_STYLE, style, |ui| {
            ui.spacer(
                40.dp(),
                20.dp(),
                Modifier::new().on_click(|| {}).tooltip("hint"),
            );
        });
        ui.canvas(Size::new(40.dp(), 20.dp()), Modifier::new(), |drawing| {
            drawing.text(
                "label",
                Rect::new(0.dp(), 0.dp(), 40.dp(), 20.dp()),
                TextStyle::new("sans-serif", 10.sp(), Color::WHITE),
            );
        });
    });
    view.provide(&TEXT_SCALING, TextScaling::linear(1.5).required("scaling")?);
    let window = Viewport::new(Size::new(200.dp(), 200.dp()), Density::ONE);
    let mut scene = Scene::default();
    scene.show(view.frame(window, &mut Measure)?)?;
    view.input(InputEvent::Move(Point::new(10.dp(), 10.dp())));
    scene.show(view.frame(window, &mut Measure)?)?;
    let text_sizes: Vec<_> = (scene.commands().into_iter())
        .filter_map(|command| match command {
            DrawCommand::Text { text, style, .. } => Some((text, style.size)),
            _ => None,
        })
        .collect();
    assert_eq!(
        text_sizes,
        [("label".to_owned(), 15.dp()), ("hint".to_owned(), 15.dp())]
    );
    Ok(())
}

#[test]
fn reduced_motion_at_the_root_ends_a_running_motion_and_makes_later_changes_instant() -> TestResult
{
    let animation = Animation::new(0.0_f64)?;
    let bound = animation.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.text("motion", Modifier::new().animated_opacity(bound.clone()));
    });
    let at = |milliseconds| FrameTime {
        presentation: Duration::from_millis(milliseconds),
        ..Default::default()
    };
    advance_with_timing(&mut scene, &mut view, WINDOW, &mut Measure, at(0))?;
    animation.animate_to(1.0, Tween::new(Duration::from_secs(10)))?;
    advance_with_timing(&mut scene, &mut view, WINDOW, &mut Measure, at(1000))?;
    assert!(animation.is_running());
    view.provide(&REDUCED_MOTION, true);
    advance_with_timing(&mut scene, &mut view, WINDOW, &mut Measure, at(1000))?;
    assert_f64_eq(animation.get(), 1.0);
    assert!(!animation.is_running());
    animation.animate_to(0.0, Tween::new(Duration::from_secs(10)))?;
    assert_f64_eq(animation.get(), 0.0);
    assert!(!animation.is_running());
    advance_with_timing(&mut scene, &mut view, WINDOW, &mut Measure, at(2000))?;
    assert!(!view.motion().is_active());
    Ok(())
}
