// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::ui::ModifierValue;

fn bounds(
    compose: impl FnMut(&mut Ui) + 'static,
    window: Size,
    tag: &str,
) -> Result<Rect, TestError> {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), compose);
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(window, Density::ONE),
        &mut Metrics,
    )?;
    view.tagged_bounds(tag)
}

#[test]
fn a_maximum_height_leaves_short_content_short_and_bounds_tall_content() -> TestResult {
    let column = |height: Dp| {
        move |ui: &mut Ui| {
            ui.layout(Axis::Vertical, Modifier::new(), |ui| {
                ui.spacer(
                    50.dp(),
                    height,
                    Modifier::new().tag("bounded").max_height(100.dp()),
                );
            });
        }
    };
    let window = Size::new(300.dp(), 400.dp());
    let short = bounds(column(50.dp()), window, "bounded")?;
    assert_eq!(short.size(), Size::new(50.dp(), 50.dp()));
    let tall = bounds(column(300.dp()), window, "bounded")?;
    assert_eq!(tall.size(), Size::new(50.dp(), 100.dp()));
    Ok(())
}

fn ranged(content: Size) -> impl FnMut(&mut Ui) + 'static {
    move |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(
                content.width,
                content.height,
                Modifier::new()
                    .tag("ranged")
                    .size_in(Size::new(240.dp(), 120.dp()), Size::new(600.dp(), 400.dp())),
            );
        });
    }
}

#[test]
fn a_size_range_raises_small_content_and_bounds_large_content() -> TestResult {
    let window = Size::new(800.dp(), 800.dp());
    let small = bounds(ranged(Size::new(10.dp(), 10.dp())), window, "ranged")?;
    assert_eq!(small.size(), Size::new(240.dp(), 120.dp()));
    let large = bounds(ranged(Size::new(1000.dp(), 1000.dp())), window, "ranged")?;
    assert_eq!(large.size(), Size::new(600.dp(), 400.dp()));
    Ok(())
}

#[test]
fn the_constraints_of_the_parent_win_over_a_size_range_when_they_are_stricter() -> TestResult {
    let window = Size::new(200.dp(), 100.dp());
    let bounded = bounds(ranged(Size::new(10.dp(), 10.dp())), window, "ranged")?;
    assert_eq!(bounded.size(), Size::new(200.dp(), 100.dp()));
    Ok(())
}

#[test]
fn a_maximum_below_the_minimum_of_a_size_range_wins() -> TestResult {
    let crossed = |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(
                10.dp(),
                10.dp(),
                Modifier::new()
                    .tag("crossed")
                    .size_in(Size::new(240.dp(), 120.dp()), Size::new(100.dp(), 400.dp())),
            );
        });
    };
    let window = Size::new(800.dp(), 800.dp());
    let bounded = bounds(crossed, window, "crossed")?;
    assert_eq!(bounded.size(), Size::new(100.dp(), 120.dp()));
    Ok(())
}

#[test]
fn an_infinite_minimum_size_fails_its_composition() -> TestResult {
    let compose = |ui: &mut Ui| {
        let modifier = Modifier::new().tag("sized").min_size(Dp::INFINITY, 0.dp());
        ui.spacer(10.dp(), 10.dp(), modifier);
    };
    let result = bounds(compose, Size::new(100.dp(), 100.dp()), "sized");
    assert!(matches!(
        failure_causes(result)?.as_slice(),
        [ComponentError::Layout(LayoutError::Modifier {
            value: ModifierValue::MinimumSize,
            cause: NumberError::NotFinite,
            ..
        })]
    ));
    Ok(())
}
