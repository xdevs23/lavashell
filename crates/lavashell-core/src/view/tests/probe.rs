// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::units::NumberError;

pub(super) const WINDOW: Viewport =
    Viewport::new(Size::new(Dp::new(600.0), Dp::new(400.0)), Density::ONE);

/// Measures a text as 10 dp wide for each character and 20 dp high for each line. The text wraps
/// into as many lines as it needs at the given width.
pub(super) struct Wrapping;

impl TextMeasurer for Wrapping {
    fn measure(&mut self, text: &str, _: &TextStyle<Dp>, max_width: Dp) -> Size {
        let natural = repeated(10.dp(), text.chars().count());
        let lines = (natural / max_width).ceil().max(1.0);
        Size::new(natural.min(max_width), 20.dp() * lines)
    }
}

fn probe(compose: impl FnMut(&mut Ui) + 'static, proposal: Proposal) -> Result<Size, TestError> {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), compose);
    advance_with(&mut scene, &mut view, WINDOW, &mut Wrapping)?;
    Ok(view.size_that_fits(proposal, &mut Wrapping)?)
}

fn settings_row(ui: &mut Ui) {
    ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
        ui.text("Brightness", Modifier::new());
        ui.spacer(Dp::ZERO, Dp::ZERO, Modifier::new().weight(1.0));
        ui.spacer(150.dp(), 30.dp(), Modifier::new());
    });
}

#[test]
fn a_weighted_spacer_leaves_an_open_width_at_the_content_size() -> TestResult {
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(settings_row, open)?, Size::new(250.dp(), 30.dp()));
    let proposal = Proposal {
        width: Some(400.dp()),
        height: None,
    };
    assert_eq!(probe(settings_row, proposal)?, Size::new(400.dp(), 30.dp()));
    Ok(())
}

fn stack(ui: &mut Ui) {
    ui.layout(Axis::Vertical, Modifier::new(), |ui| {
        ui.spacer(40.dp(), 30.dp(), Modifier::new());
        ui.spacer(Dp::ZERO, Dp::ZERO, Modifier::new().weight(1.0));
        ui.spacer(60.dp(), 50.dp(), Modifier::new());
    });
}

#[test]
fn a_weighted_child_takes_its_share_only_of_a_proposed_height() -> TestResult {
    let width = Proposal {
        width: Some(300.dp()),
        height: None,
    };
    assert_eq!(probe(stack, width)?, Size::new(60.dp(), 80.dp()));
    let height = Proposal {
        width: None,
        height: Some(300.dp()),
    };
    assert_eq!(probe(stack, height)?, Size::new(60.dp(), 300.dp()));
    Ok(())
}

fn filling(ui: &mut Ui) {
    ui.layout(Axis::Vertical, Modifier::new(), |ui| {
        ui.spacer(
            30.dp(),
            10.dp(),
            Modifier::new().fill_max_width().fill_max_height(),
        );
        ui.spacer(50.dp(), 20.dp(), Modifier::new());
    });
}

#[test]
fn a_filling_child_takes_the_length_left_by_the_other_children_and_the_minimum_length_on_an_open_axis()
-> TestResult {
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(filling, open)?, Size::new(50.dp(), 30.dp()));
    let width = Proposal {
        width: Some(200.dp()),
        height: None,
    };
    assert_eq!(probe(filling, width)?, Size::new(200.dp(), 30.dp()));
    let height = Proposal {
        width: None,
        height: Some(100.dp()),
    };
    assert_eq!(probe(filling, height)?, Size::new(50.dp(), 100.dp()));
    Ok(())
}

#[test]
fn a_keyed_filling_child_takes_the_length_left_by_the_other_children_and_the_minimum_length_on_an_open_axis()
-> TestResult {
    let keyed = |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.key("filling", Modifier::new(), |ui| {
                ui.spacer(30.dp(), 10.dp(), Modifier::new().fill_max_height());
            });
            ui.spacer(50.dp(), 20.dp(), Modifier::new());
        });
    };
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(keyed, open)?, Size::new(50.dp(), 30.dp()));
    let height = Proposal {
        width: None,
        height: Some(100.dp()),
    };
    assert_eq!(probe(keyed, height)?, Size::new(50.dp(), 100.dp()));
    Ok(())
}

#[test]
fn an_overlay_gives_a_filling_child_the_proposed_size_and_its_minimum_on_an_open_axis() -> TestResult
{
    let overlay = |ui: &mut Ui| {
        ui.layout(Arrangement::Overlay, Modifier::new(), |ui| {
            ui.spacer(
                30.dp(),
                10.dp(),
                Modifier::new().fill_max_width().fill_max_height(),
            );
            ui.spacer(50.dp(), 20.dp(), Modifier::new());
        });
    };
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(overlay, open)?, Size::new(50.dp(), 20.dp()));
    let height = Proposal {
        width: None,
        height: Some(100.dp()),
    };
    assert_eq!(probe(overlay, height)?, Size::new(50.dp(), 100.dp()));
    Ok(())
}

#[test]
fn a_fixed_child_keeps_its_size_within_a_proposal() -> TestResult {
    let fixed = |ui: &mut Ui| {
        ui.spacer(30.dp(), 10.dp(), Modifier::new());
    };
    let proposal = Proposal {
        width: Some(200.dp()),
        height: Some(100.dp()),
    };
    assert_eq!(probe(fixed, proposal)?, Size::new(30.dp(), 10.dp()));
    Ok(())
}

#[test]
fn a_fixed_child_larger_than_the_proposal_keeps_its_size() -> TestResult {
    let fixed = |ui: &mut Ui| {
        ui.spacer(300.dp(), 50.dp(), Modifier::new());
    };
    let proposal = Proposal {
        width: Some(200.dp()),
        height: Some(20.dp()),
    };
    assert_eq!(probe(fixed, proposal)?, Size::new(300.dp(), 50.dp()));
    let wide = |ui: &mut Ui| {
        ui.spacer(10.dp(), 10.dp(), Modifier::new().width(300.dp()));
    };
    assert_eq!(probe(wide, proposal)?, Size::new(300.dp(), 10.dp()));
    let narrow = Proposal {
        width: Some(200.dp()),
        height: None,
    };
    assert_eq!(probe(settings_row, narrow)?, Size::new(250.dp(), 30.dp()));
    Ok(())
}

#[test]
fn padding_adds_to_the_content_and_shrinks_the_proposal() -> TestResult {
    let padded = |ui: &mut Ui| {
        ui.text("Brightness", Modifier::new().padding(5.dp()));
    };
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(padded, open)?, Size::new(110.dp(), 30.dp()));
    let narrow = Proposal {
        width: Some(65.dp()),
        height: None,
    };
    assert_eq!(probe(padded, narrow)?, Size::new(65.dp(), 50.dp()));
    Ok(())
}

fn card(ui: &mut Ui) {
    ui.layout(Axis::Vertical, Modifier::new().padding(10.dp()), |ui| {
        ui.layout(Axis::Horizontal, Modifier::new().fill_max_width(), |ui| {
            ui.text("Name", Modifier::new());
            ui.spacer(Dp::ZERO, Dp::ZERO, Modifier::new().weight(1.0));
            ui.spacer(20.dp(), 20.dp(), Modifier::new());
        });
        ui.spacer(Dp::ZERO, Dp::ZERO, Modifier::new().weight(1.0));
        ui.text("Subtitle", Modifier::new());
    });
}

#[test]
fn nested_containers_follow_the_rules_of_the_proposal_on_each_axis() -> TestResult {
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(card, open)?, Size::new(100.dp(), 60.dp()));
    let width = Proposal {
        width: Some(200.dp()),
        height: None,
    };
    assert_eq!(probe(card, width)?, Size::new(200.dp(), 60.dp()));
    Ok(())
}

#[test]
fn a_scoped_component_is_measured_at_its_content_size_after_a_frame_cached_its_layout() -> TestResult
{
    let scoped = |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.scoped("row", Modifier::new(), settings_row);
        });
    };
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), scoped);
    advance_with(&mut scene, &mut view, WINDOW, &mut Wrapping)?;
    view.invalidate();
    advance_with(&mut scene, &mut view, WINDOW, &mut Wrapping)?;
    assert_eq!(view.stats().reused_layouts, 1);
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(
        view.size_that_fits(open, &mut Wrapping),
        Ok(Size::new(250.dp(), 30.dp()))
    );
    Ok(())
}

fn limited(ui: &mut Ui) {
    ui.layout(
        Axis::Vertical,
        Modifier::new().measurement_limit(Size::new(300.dp(), 500.dp())),
        |ui| {
            ui.spacer(40.dp(), 20.dp(), Modifier::new());
            ui.spacer(Dp::ZERO, Dp::ZERO, Modifier::new().weight(1.0));
        },
    );
}

#[test]
fn a_measurement_limit_leaves_an_open_axis_unbounded() -> TestResult {
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(limited, open)?, Size::new(40.dp(), 20.dp()));
    Ok(())
}

#[test]
fn a_size_range_bounds_content_on_open_axes() -> TestResult {
    let ranged = |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(
                1000.dp(),
                10.dp(),
                Modifier::new()
                    .size_in(Size::new(240.dp(), 120.dp()), Size::new(600.dp(), 400.dp())),
            );
        });
    };
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(ranged, open)?, Size::new(600.dp(), 120.dp()));
    Ok(())
}

#[test]
fn a_maximum_height_bounds_content_on_an_open_height() -> TestResult {
    let tall = |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(50.dp(), 300.dp(), Modifier::new().max_height(100.dp()));
        });
    };
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(tall, open)?, Size::new(50.dp(), 100.dp()));
    Ok(())
}

#[test]
fn a_maximum_width_leaves_an_open_height_open() -> TestResult {
    let limited = |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new().max_width(300.dp()), |ui| {
            ui.spacer(40.dp(), 20.dp(), Modifier::new());
            ui.spacer(Dp::ZERO, Dp::ZERO, Modifier::new().weight(1.0));
        });
    };
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(limited, open)?, Size::new(40.dp(), 20.dp()));
    Ok(())
}

fn list(ui: &mut Ui) {
    ui.scroll(Axis::Vertical, Modifier::new(), |ui| {
        for _ in 0..10 {
            ui.spacer(80.dp(), 30.dp(), Modifier::new());
        }
    });
}

#[test]
fn a_scroll_container_reports_its_content_within_a_proposed_height() -> TestResult {
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(list, open)?, Size::new(80.dp(), 300.dp()));
    let height = Proposal {
        width: None,
        height: Some(100.dp()),
    };
    assert_eq!(probe(list, height)?, Size::new(80.dp(), 100.dp()));
    Ok(())
}

fn scrolling(ui: &mut Ui) {
    ui.scroll(Axis::Vertical, Modifier::new(), |ui| {
        ui.spacer(50.dp(), 40.dp(), Modifier::new().fill_max_height());
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(30.dp(), 10.dp(), Modifier::new());
            ui.spacer(Dp::ZERO, Dp::ZERO, Modifier::new().weight(1.0));
        });
    });
}

#[test]
fn children_of_a_scroll_container_are_measured_on_an_unbounded_height() -> TestResult {
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(scrolling, open)?, Size::new(50.dp(), 50.dp()));
    Ok(())
}

#[test]
fn text_wraps_at_the_proposed_width() -> TestResult {
    let title = |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new().padding(10.dp()), |ui| {
            ui.text("Display brightness", Modifier::new());
        });
    };
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(probe(title, open)?, Size::new(200.dp(), 40.dp()));
    let narrow = Proposal {
        width: Some(100.dp()),
        height: None,
    };
    assert_eq!(probe(title, narrow)?, Size::new(100.dp(), 80.dp()));
    Ok(())
}

#[test]
fn text_under_a_minimum_width_above_the_proposal_wraps_at_that_minimum() -> TestResult {
    let title = |ui: &mut Ui| {
        ui.text(
            "Display brightness",
            Modifier::new().min_size(300.dp(), 0.dp()),
        );
    };
    let narrow = Proposal {
        width: Some(100.dp()),
        height: None,
    };
    assert_eq!(probe(title, narrow)?, Size::new(300.dp(), 20.dp()));
    Ok(())
}

#[test]
fn an_infinite_proposed_length_is_unbounded() -> TestResult {
    let infinite = Proposal {
        width: Some(Dp::INFINITY),
        height: None,
    };
    assert_eq!(probe(settings_row, infinite)?, Size::new(250.dp(), 30.dp()));
    Ok(())
}

#[test]
fn a_negative_proposed_length_fails_the_probe() {
    let negative = Proposal {
        width: Some(-1.dp()),
        height: None,
    };
    assert!(matches!(
        probe(settings_row, negative),
        Err(TestError::Number(NumberError::Negative))
    ));
}

#[test]
fn a_proposed_length_that_is_not_a_number_fails_the_probe() {
    let not_a_number = Proposal {
        width: None,
        height: Some(Dp::new(f64::NAN)),
    };
    assert!(matches!(
        probe(settings_row, not_a_number),
        Err(TestError::Number(NumberError::NotFinite))
    ));
}

fn half_width(ui: &mut Ui) {
    let width = ui.viewport().size.width;
    ui.spacer(
        width / 2.0,
        10.dp(),
        Modifier::new().background(Color::WHITE),
    );
}

#[test]
fn a_view_that_was_never_framed_is_composed_at_its_viewport() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), half_width);
    view.provide(&VIEWPORT, WINDOW);
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(
        view.size_that_fits(open, &mut Wrapping),
        Ok(Size::new(300.dp(), 10.dp()))
    );
    let mut plain = View::new(crate::platform::fixture::facts(), half_width);
    assert_eq!(
        painted(view.frame(WINDOW, &mut Wrapping).without_failures()?)?,
        painted(plain.frame(WINDOW, &mut Wrapping).without_failures()?)?
    );
    Ok(())
}
