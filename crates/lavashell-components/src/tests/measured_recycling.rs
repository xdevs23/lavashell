// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::tests::{TestError, render};
use lavashell_core::numbers::count_f64;
use std::cell::Cell;
use std::iter::repeat_n;

struct Metrics;

impl TextMeasurer for Metrics {
    fn measure(&mut self, _: &str, _: &TextStyle<Dp>, _: Dp) -> Size {
        Size::new(10.dp(), 20.dp())
    }
}

#[test]
fn ten_thousand_variable_rows_recycle_and_measure_without_building_the_history()
-> Result<(), TestError> {
    let list = MeasuredRecycler::new();
    list.set_keys((0..10_000).collect())?;
    let state = list.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        state.compose(
            ui,
            Modifier::new().fill_max_width().fill_max_height(),
            |ui, item| {
                ui.spacer(
                    item.size.width,
                    24.dp() + repeat_n(12.dp(), item.index % 5).sum::<Dp>(),
                    Modifier::new(),
                );
            },
        );
    });
    let size = Size::new(500.dp(), 400.dp());
    let viewport = Viewport::new(size, Density::ONE);
    for _ in 0..3 {
        render(&mut view, viewport, &mut Metrics)?;
    }
    assert!(list.stats().active < 24);
    for step in 1..12_usize {
        list.scroll.jump_to(1500.dp() * count_f64(step)?);
        for _ in 0..3 {
            render(&mut view, viewport, &mut Metrics)?;
        }
    }
    assert_eq!(list.stats().items, 10_000);
    assert!(list.stats().holders < 48);
    assert!(list.stats().rebound > 0);
    Ok(())
}

#[test]
fn only_the_first_row_is_composed_until_a_row_is_measured_and_the_others_take_the_mean()
-> Result<(), TestError> {
    let list = MeasuredRecycler::new();
    list.set_keys((0..10).collect())?;
    let compositions = Rc::new(RefCell::new(Vec::new()));
    let (state, log) = (list.clone(), compositions.clone());
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        let composed = Rc::new(RefCell::new(Vec::new()));
        log.borrow_mut().push(composed.clone());
        state.compose(
            ui,
            Modifier::new().fill_max_width().fill_max_height(),
            move |ui, item| {
                composed.borrow_mut().push(item.index);
                ui.spacer(
                    300.dp(),
                    30.dp() + repeat_n(20.dp(), item.index).sum::<Dp>(),
                    Modifier::new(),
                );
            },
        );
    });
    let viewport = Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE);
    render(&mut view, viewport, &mut Metrics)?;
    assert_eq!(*compositions.borrow()[0].borrow(), [0]);
    while view.needs_frame() {
        render(&mut view, viewport, &mut Metrics)?;
    }
    assert!(list.stats().active < list.stats().items);
    Ok(())
}

#[test]
fn a_single_row_can_grow_beyond_its_estimated_extent() -> Result<(), TestError> {
    let list = MeasuredRecycler::new();
    list.set_keys(vec![1])?;
    let state = list.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        state.compose(
            ui,
            Modifier::new().fill_max_width().fill_max_height(),
            |ui, _| {
                ui.spacer(300.dp(), 2000.dp(), Modifier::new());
            },
        );
    });
    for _ in 0..3 {
        render(
            &mut view,
            Viewport::new(Size::new(300.dp(), 400.dp()), Density::ONE),
            &mut Metrics,
        )?;
    }
    assert_eq!(list.scroll.content().height, 2000.dp());
    list.bottom();
    assert_eq!(list.scroll.offset(), 1600.dp());
    Ok(())
}

#[test]
fn growing_rows_follow_the_bottom_and_preserve_a_reader_above_it() -> Result<(), TestError> {
    let list = MeasuredRecycler::new();
    list.set_keys(vec![0, 1])?;
    let heights = State::new([100.dp(), 100.dp()]);
    let values = heights.clone();
    let state = list.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        state.compose(
            ui,
            Modifier::new().fill_max_width().fill_max_height(),
            |ui, item| {
                ui.spacer(300.dp(), values.get()[item.index], Modifier::new());
            },
        );
    });
    let size = Size::new(300.dp(), 100.dp());
    let viewport = Viewport::new(size, Density::ONE);
    for _ in 0..3 {
        render(&mut view, viewport, &mut Metrics)?;
    }
    list.bottom();
    heights.set([100.dp(), 600.dp()]);
    for _ in 0..3 {
        render(&mut view, viewport, &mut Metrics)?;
    }
    assert_eq!(list.scroll.offset(), 600.dp());
    assert!(list.at_bottom(), "growth must not lose bottom following");
    list.scroll.jump_to(150.dp());
    heights.set([100.dp(), 800.dp()]);
    for _ in 0..3 {
        render(&mut view, viewport, &mut Metrics)?;
    }
    assert_eq!(
        list.scroll.offset(),
        150.dp(),
        "do not pull a reader to the end"
    );
    Ok(())
}

#[test]
fn a_row_above_the_reader_that_grew_while_it_was_away_keeps_the_reader_in_place()
-> Result<(), TestError> {
    let list = MeasuredRecycler::new();
    list.set_keys(vec![0, 1, 2, 3])?;
    let heights = State::new([100.dp(); 4]);
    let values = heights.clone();
    let state = list.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        state.compose(
            ui,
            Modifier::new().fill_max_width().fill_max_height(),
            |ui, item| {
                ui.spacer(300.dp(), values.get()[item.index], Modifier::new());
            },
        );
    });
    let viewport = Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE);
    let settle = |view: &mut View| -> Result<(), TestError> {
        while view.needs_frame() {
            render(view, viewport, &mut Metrics)?;
        }
        Ok(())
    };
    settle(&mut view)?;
    list.scroll.jump_to(250.dp());
    settle(&mut view)?;
    heights.set([300.dp(), 100.dp(), 100.dp(), 100.dp()]);
    settle(&mut view)?;
    assert_eq!(list.scroll.offset(), 250.dp(), "the row is not mounted");
    list.scroll.jump_to(150.dp());
    settle(&mut view)?;
    assert_eq!(
        list.scroll.offset(),
        350.dp(),
        "the row at the top stays where it was, 50 dp into it"
    );
    Ok(())
}

#[test]
fn measurements_keep_the_visible_anchor_and_survive_key_reordering() -> Result<(), TestError> {
    let list = MeasuredRecycler::new();
    list.set_keys(vec![1, 2, 3, 4])?;
    list.rows.update(|rows| rows.change_width(300.dp()));
    let state = list.clone();
    let first_height = State::new(100.dp());
    let measured_height = first_height.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        state.compose(
            ui,
            Modifier::new().fill_max_width().fill_max_height(),
            |ui, item| {
                ui.spacer(
                    300.dp(),
                    if item.index == 0 {
                        measured_height.get()
                    } else {
                        100.dp()
                    },
                    Modifier::new(),
                );
            },
        );
    });
    let viewport = Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE);
    render(&mut view, viewport, &mut Metrics)?;
    list.scroll.jump_to(220.dp());
    first_height.set(150.dp());
    for _ in 0..3 {
        render(&mut view, viewport, &mut Metrics)?;
    }
    assert_eq!(list.scroll.offset(), 270.dp());
    list.set_keys(vec![4, 1, 2, 3])?;
    let taken = |measured: Dp| {
        list.rows
            .peek_with(|rows| rows.accepted(&1, 300.dp(), measured))
    };
    assert_eq!(taken(150.dp()), None, "the height of the key moved with it");
    list.measured(&1, 400.dp(), 500.dp(), false);
    assert_eq!(taken(150.dp()), None, "stale width ignored");
    assert_eq!(taken(90.dp()), Some((1, 90.dp())));
    Ok(())
}

/// The component "latest" reads `at_bottom` of the list, as a button that jumps to the latest message does.
#[test]
fn the_end_of_the_list_follows_growth_and_recomposes_its_reader_only_when_it_changes()
-> Result<(), TestError> {
    let list = MeasuredRecycler::new();
    list.set_keys((0..5).collect())?;
    let last_height = State::new(100.dp());
    let latest = Rc::new(Cell::new(0));
    let (state, height, composed) = (list.clone(), last_height.clone(), latest.clone());
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        let (rows, height, ends, composed) = (
            state.clone(),
            height.clone(),
            state.clone(),
            composed.clone(),
        );
        ui.scoped("transcript", Modifier::new(), move |ui| {
            let height = height.clone();
            rows.compose(
                ui,
                Modifier::new().size(300.dp(), 100.dp()),
                move |ui, item| {
                    ui.spacer(
                        300.dp(),
                        if item.index == 4 {
                            height.get()
                        } else {
                            100.dp()
                        },
                        Modifier::new(),
                    );
                },
            );
        });
        ui.scoped("latest", Modifier::new(), move |ui| {
            composed.set(composed.get() + 1);
            if !ends.at_bottom() {
                ui.spacer(20.dp(), 20.dp(), Modifier::new());
            }
        });
    });
    let viewport = Viewport::new(Size::new(300.dp(), 200.dp()), Density::ONE);
    let settle = |view: &mut View| -> Result<(), TestError> {
        while view.needs_frame() {
            render(view, viewport, &mut Metrics)?;
        }
        Ok(())
    };
    settle(&mut view)?;
    list.bottom();
    settle(&mut view)?;
    assert!(list.at_bottom());
    last_height.set(300.dp());
    settle(&mut view)?;
    assert_eq!(list.scroll.offset(), list.scroll.maximum());
    assert!(
        list.at_bottom(),
        "the list still follows the end after growth"
    );
    let composed = latest.get();
    for offset in [550_u16, 500, 450, 400] {
        list.scroll.jump_to(Dp::new(f64::from(offset)));
        settle(&mut view)?;
    }
    assert!(!list.at_bottom());
    assert_eq!(latest.get(), composed + 1);
    let reader = list.scroll.offset();
    last_height.set(500.dp());
    settle(&mut view)?;
    assert_eq!(
        list.scroll.offset(),
        reader,
        "growth does not pull a reader to the end"
    );
    Ok(())
}
