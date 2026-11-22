// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::{Animation, Easing, Tween};
use crate::geometry::{Axis, Color, Viewport};
use crate::ui::Overflow;
use std::time::Duration;

const CARD_WIDTH: f64 = 50.0;
const CARD_HEIGHTS: [f64; 3] = [40.0, 30.0, 40.0];
const CARD_GAP: f64 = 10.0;

fn card_fills(overflow: Overflow, height: Dp) -> Result<Vec<Rect>, TestError> {
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let column = Modifier::new().height(height).gap(CARD_GAP.dp());
        let axis = Axis::Vertical;
        ui.layout(Arrangement::Along { axis, overflow }, column, |ui| {
            for card in CARD_HEIGHTS {
                let filled = Modifier::new().background(Color::WHITE);
                ui.spacer(CARD_WIDTH.dp(), card.dp(), filled);
            }
        });
    });
    fill_bounds(&mut Scene::default(), &mut view, WINDOW_SIZE)
}

fn stacked(cards: &[f64]) -> Vec<Rect> {
    let tops = cards.iter().scan(0.0, |top, height| {
        let card = *top;
        *top += height + CARD_GAP;
        Some(card)
    });
    (tops.zip(cards))
        .map(|(top, height)| Rect::new(0.dp(), top.dp(), CARD_WIDTH.dp(), height.dp()))
        .collect()
}

#[test]
fn a_column_hides_whole_children_at_the_start_until_the_rest_fit() -> TestResult {
    let cases = [
        (130.dp(), &CARD_HEIGHTS[..]),
        (100.dp(), &CARD_HEIGHTS[1..]),
        (50.dp(), &CARD_HEIGHTS[2..]),
    ];
    for (height, shown) in cases {
        let fills = card_fills(Overflow::HideStart, height)?;
        assert_eq!(fills, stacked(shown), "{height:?}");
    }
    Ok(())
}

#[test]
fn a_column_shrinks_a_last_child_longer_than_its_span_to_the_span() -> TestResult {
    let span = 20.dp();
    let fills = card_fills(Overflow::HideStart, span)?;
    assert_eq!(fills, [Rect::new(0.dp(), 0.dp(), CARD_WIDTH.dp(), span)]);
    Ok(())
}

#[test]
fn a_column_hides_whole_children_at_the_end_until_the_rest_fit() -> TestResult {
    let fills = card_fills(Overflow::HideEnd, 100.dp())?;
    assert_eq!(fills, stacked(&CARD_HEIGHTS[..2]));
    Ok(())
}

#[test]
fn a_column_with_visible_overflow_shows_every_child() -> TestResult {
    let fills = card_fills(Overflow::Visible, 100.dp())?;
    assert_eq!(fills, stacked(&CARD_HEIGHTS));
    Ok(())
}

#[test]
fn a_child_with_a_weight_gets_the_room_left_after_hiding() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let row = Modifier::new().width(100.dp()).gap(10.dp());
        let hiding = Arrangement::Along {
            axis: Axis::Horizontal,
            overflow: Overflow::HideStart,
        };
        ui.layout(hiding, row, |ui| {
            ui.spacer(Dp::ZERO, 20.dp(), Modifier::new().weight(1.0).tag("share"));
            ui.spacer(60.dp(), 20.dp(), Modifier::new().tag("hidden"));
            ui.spacer(50.dp(), 20.dp(), Modifier::new().tag("last"));
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(
        view.tagged_bounds(&"share")?,
        Rect::new(0.dp(), 0.dp(), 40.dp(), 20.dp())
    );
    assert_eq!(view.tagged_bounds(&"hidden")?.size(), Size::ZERO);
    assert_eq!(
        view.tagged_bounds(&"last")?,
        Rect::new(50.dp(), 0.dp(), 50.dp(), 20.dp())
    );
    Ok(())
}

#[test]
fn a_row_hiding_at_the_end_gives_the_room_left_to_a_child_with_a_weight() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let row = Modifier::new().width(100.dp()).gap(10.dp());
        let hiding = Arrangement::Along {
            axis: Axis::Horizontal,
            overflow: Overflow::HideEnd,
        };
        ui.layout(hiding, row, |ui| {
            ui.spacer(50.dp(), 20.dp(), Modifier::new().tag("first"));
            ui.spacer(60.dp(), 20.dp(), Modifier::new().tag("hidden"));
            ui.spacer(Dp::ZERO, 20.dp(), Modifier::new().weight(1.0).tag("share"));
        });
    });
    advance(&mut Scene::default(), &mut view, WINDOW_SIZE, 0)?;
    let bounds = [
        view.tagged_bounds(&"first")?,
        view.tagged_bounds(&"hidden")?,
        view.tagged_bounds(&"share")?,
    ];
    let expected = [
        Rect::new(0.dp(), 0.dp(), 50.dp(), 20.dp()),
        Rect::default(),
        Rect::new(60.dp(), 0.dp(), 40.dp(), 20.dp()),
    ];
    assert_eq!(bounds, expected);
    Ok(())
}

#[test]
fn a_column_neither_counts_nor_hides_children_out_of_flow_or_hidden_by_a_condition() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let hiding = Arrangement::Along {
            axis: Axis::Vertical,
            overflow: Overflow::HideStart,
        };
        let column = Modifier::new().height(100.dp()).gap(CARD_GAP.dp());
        ui.layout(hiding, column, |ui| {
            ui.spacer(CARD_WIDTH.dp(), 40.dp(), Modifier::new().tag("first"));
            let out = Modifier::new().out_of_flow().tag("out");
            ui.spacer(CARD_WIDTH.dp(), 80.dp(), out);
            let unshown = Modifier::new().shown_when(false);
            ui.spacer(CARD_WIDTH.dp(), 40.dp(), unshown);
            ui.spacer(CARD_WIDTH.dp(), 30.dp(), Modifier::new().tag("second"));
            ui.spacer(CARD_WIDTH.dp(), 40.dp(), Modifier::new().tag("third"));
        });
    });
    advance(&mut Scene::default(), &mut view, WINDOW_SIZE, 0)?;
    let bounds = ["first", "out", "second", "third"].map(|tag| view.tagged_bounds(&tag));
    let expected = [
        Rect::default(),
        Rect::new(0.dp(), 0.dp(), CARD_WIDTH.dp(), 80.dp()),
        Rect::new(0.dp(), 0.dp(), CARD_WIDTH.dp(), 30.dp()),
        Rect::new(0.dp(), 40.dp(), CARD_WIDTH.dp(), 40.dp()),
    ];
    assert_eq!(bounds.into_iter().collect::<Result<Vec<_>, _>>()?, expected);
    Ok(())
}

#[test]
fn a_column_hides_the_children_inside_a_child_that_it_hides() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let hiding = Arrangement::Along {
            axis: Axis::Vertical,
            overflow: Overflow::HideStart,
        };
        ui.layout(hiding, Modifier::new().height(50.dp()), |ui| {
            ui.layout(Axis::Vertical, Modifier::new(), |ui| {
                let inner = Modifier::new().background(Color::WHITE).tag("inner");
                ui.spacer(CARD_WIDTH.dp(), 40.dp(), inner);
            });
            let last = Modifier::new().background(Color::WHITE);
            ui.spacer(CARD_WIDTH.dp(), 40.dp(), last);
        });
    });
    let fills = fill_bounds(&mut Scene::default(), &mut view, WINDOW_SIZE)?;
    assert_eq!(fills, stacked(&[40.0]));
    assert_eq!(view.tagged_bounds(&"inner")?, Rect::default());
    Ok(())
}

#[test]
fn a_child_hidden_by_overflow_does_not_report_a_size() -> TestResult {
    let reported = State::new(Vec::new());
    let recorded = reported.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let hiding = Arrangement::Along {
            axis: Axis::Vertical,
            overflow: Overflow::HideStart,
        };
        ui.layout(hiding, Modifier::new().height(50.dp()), |ui| {
            for card in ["first", "second"] {
                let recorded = recorded.clone();
                let report = move |_| recorded.update(|cards| cards.push(card));
                ui.spacer(50.dp(), 40.dp(), Modifier::new().on_measure(report));
            }
        });
    });
    advance(&mut Scene::default(), &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(reported.get(), ["second"]);
    Ok(())
}

struct Recording(Vec<String>);

impl TextMeasurer for Recording {
    fn measure(&mut self, text: &str, style: &TextStyle<Dp>, max_width: Dp) -> Size {
        self.0.push(text.to_owned());
        Metrics.measure(text, style, max_width)
    }
}

#[test]
fn a_column_measures_none_of_the_children_after_the_first_that_does_not_fit() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let hiding = Arrangement::Along {
            axis: Axis::Vertical,
            overflow: Overflow::HideEnd,
        };
        ui.layout(hiding, Modifier::new().height(30.dp()), |ui| {
            for entry in ["first", "second", "third"] {
                ui.text(entry, Modifier::new());
            }
        });
    });
    let mut recording = Recording(Vec::new());
    let viewport = Viewport::new(WINDOW_SIZE, Density::ONE);
    advance_with(&mut Scene::default(), &mut view, viewport, &mut recording)?;
    let measured = |entry: &str| recording.0.iter().any(|text| text == entry);
    assert_eq!(
        ["first", "second", "third"].map(measured),
        [true, true, false]
    );
    Ok(())
}

#[test]
fn a_component_hidden_by_overflow_shows_again_once_its_height_fits() -> TestResult {
    let height = Animation::new(40.dp())?;
    let first = height.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let first = first.clone();
        ui.scoped("cards", Modifier::new(), move |ui| {
            let first = first.clone();
            let hiding = Arrangement::Along {
                axis: Axis::Vertical,
                overflow: Overflow::HideStart,
            };
            let column = Modifier::new().height(60.dp()).gap(CARD_GAP.dp());
            ui.layout(hiding, column, move |ui| {
                let first = first.clone();
                ui.scoped("first", Modifier::new(), move |ui| {
                    let card = Modifier::new().height(&first).tag("first");
                    ui.spacer(CARD_WIDTH.dp(), Dp::ZERO, card);
                });
                ui.spacer(CARD_WIDTH.dp(), 40.dp(), Modifier::new().tag("second"));
            });
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds(&"first")?.size(), Size::ZERO);
    let linear = Tween::new(Duration::from_millis(1000)).easing(Easing::Linear);
    height.animate_to(10.dp(), linear)?;
    advance(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    let cards = [
        view.tagged_bounds(&"first")?,
        view.tagged_bounds(&"second")?,
    ];
    assert_eq!(stacked(&[10.0, 40.0]), cards);
    Ok(())
}
