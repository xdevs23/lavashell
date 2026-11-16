// SPDX-License-Identifier: GPL-3.0-only
use super::smallest::{RecordedProbe, nested, probe_recording, weighted};
use super::*;
use crate::animation::{Animation, Easing, Tween};
use crate::geometry::Axis;
use crate::ui::NodeRef;
use std::time::Duration;

fn cards(ui: &mut Ui) {
    let wide = NodeRef::new();
    ui.layout(Axis::Vertical, Modifier::new(), |ui| {
        ui.spacer(
            Dp::ZERO,
            20.dp(),
            Modifier::new().width(wide.width()).tag("narrow"),
        );
        ui.spacer(
            120.dp(),
            20.dp(),
            Modifier::new().node_ref(&wide).tag("wide"),
        );
    });
}

#[test]
fn a_card_takes_the_width_of_a_card_declared_after_it_in_one_composition() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), cards);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(
        view.bounds_of(&"narrow"),
        Some(Rect::new(0.dp(), 0.dp(), 120.dp(), 20.dp()))
    );
    assert_eq!(
        view.bounds_of(&"wide"),
        Some(Rect::new(0.dp(), 20.dp(), 120.dp(), 20.dp()))
    );
    assert_eq!(view.stats().root_compositions, 1);
    Ok(())
}

#[test]
fn a_probe_measures_the_sizes_that_a_node_reads() {
    let mut view = View::new(crate::platform::fixture::facts(), cards);
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(
        view.size_that_fits(open, &mut Metrics),
        Ok(Size::new(120.dp(), 40.dp()))
    );
}

#[test]
fn a_height_that_follows_an_animation_lays_out_each_frame_without_composing() -> TestResult {
    let height = Animation::new(76.dp())?;
    let panel = height.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Vertical, Modifier::new().fill_max_height(), |ui| {
            ui.spacer(
                10.dp(),
                Dp::ZERO,
                Modifier::new().weight(1.0).tag("content"),
            );
            ui.spacer(
                10.dp(),
                Dp::ZERO,
                Modifier::new().height(&panel).tag("panel"),
            );
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let linear = Tween::new(Duration::from_millis(1000)).easing(Easing::Linear);
    height.animate_to(276.dp(), linear)?;
    for (time, expected) in [(500, 176.dp()), (1000, 276.dp())] {
        advance(&mut scene, &mut view, WINDOW_SIZE, time)?;
        let (content, panel) = (
            view.tagged_bounds(&"content")?,
            view.tagged_bounds(&"panel")?,
        );
        assert_eq!(panel.height, expected);
        assert_eq!(content.y + content.height, panel.y);
    }
    assert_eq!(view.stats().root_compositions, 1);
    Ok(())
}

#[test]
fn a_scoped_component_measures_again_when_a_size_it_reads_from_outside_changes() -> TestResult {
    let width = State::new(120.dp());
    let source_width = width.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let wide = NodeRef::new();
        let (source, read, source_width) = (wide.clone(), wide, source_width.clone());
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.scoped("wide", Modifier::new(), move |ui| {
                ui.spacer(
                    source_width.get(),
                    20.dp(),
                    Modifier::new().node_ref(&source),
                );
            });
            ui.scoped("card", Modifier::new(), move |ui| {
                ui.spacer(
                    Dp::ZERO,
                    20.dp(),
                    Modifier::new().width(read.width()).tag("card"),
                );
            });
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds(&"card")?.width, 120.dp());
    width.set(160.dp());
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds(&"card")?.width, 160.dp());
    let stats = view.stats();
    assert_eq!(
        (stats.root_compositions, stats.component_compositions),
        (1, 1)
    );
    Ok(())
}

fn probe_row(mut row: impl FnMut(&mut Ui) + 'static) -> Result<Size, TestError> {
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), &mut row);
    });
    let width = Proposal {
        width: Some(300.dp()),
        height: None,
    };
    Ok(view.size_that_fits(width, &mut Metrics)?)
}

#[test]
fn a_weighted_child_reads_the_last_share_of_a_weighted_child_declared_after_it() -> TestResult {
    let size = probe_row(|ui| {
        let source = NodeRef::new();
        let height = Modifier::new().height(source.width());
        ui.spacer(Dp::ZERO, Dp::ZERO, height.weight(2.0));
        ui.spacer(
            Dp::ZERO,
            10.dp(),
            Modifier::new().node_ref(&source).weight(1.0),
        );
    })?;
    assert_eq!(size, Size::new(300.dp(), 100.dp()));
    Ok(())
}

#[test]
fn a_probe_measures_a_reader_again_under_equal_constraints_after_its_source_changed() -> TestResult
{
    let size = probe_row(|ui| {
        let share = NodeRef::new();
        ui.spacer(
            Dp::ZERO,
            10.dp(),
            Modifier::new().node_ref(&share).weight(1.0),
        );
        let limit = Modifier::new().measurement_limit(Size::new(50.dp(), 500.dp()));
        ui.layout(
            Arrangement::Overlay,
            Modifier::new().weight(2.0).then(limit),
            |ui| {
                let height = Modifier::new().height(share.width());
                ui.spacer(10.dp(), Dp::ZERO, height);
            },
        );
    })?;
    assert_eq!(size, Size::new(300.dp(), 100.dp()));
    Ok(())
}

fn reading_row(ui: &mut Ui) {
    let source = NodeRef::new();
    ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
        ui.layout(Axis::Horizontal, Modifier::new().weight(1.0), |ui| {
            ui.spacer(
                10.dp(),
                10.dp(),
                Modifier::new().node_ref(&source).fill_max_height(),
            );
            ui.spacer(Dp::ZERO, 10.dp(), Modifier::new().width(source.height()));
        });
        nested(ui, 2, weighted, &Modifier::new());
    });
}

#[test]
fn a_probe_measures_the_width_alone_only_of_a_child_of_a_row_without_reads() -> TestResult {
    let tall = Proposal {
        width: Some(0.dp()),
        height: Some(1000.dp()),
    };
    let texts = 3;
    let RecordedProbe { size, measured } = probe_recording(reading_row, tall)?;
    assert_eq!(size, Size::new(2020.dp(), 1000.dp()));
    assert!(measured.len() <= 2 * texts, "{measured:?}");
    Ok(())
}

fn reading_column(ui: &mut Ui) {
    let source = NodeRef::new();
    ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(
                10.dp(),
                10.dp(),
                Modifier::new().node_ref(&source).weight(1.0),
            );
            ui.spacer(10.dp(), 90.dp(), Modifier::new().weight(1.0));
            nested(ui, 1, weighted, &Modifier::new());
        });
        ui.spacer(Dp::ZERO, 10.dp(), Modifier::new().width(source.height()));
    });
}

#[test]
fn a_probe_measures_again_only_the_child_of_a_column_whose_height_is_read() -> TestResult {
    let height = Proposal {
        width: None,
        height: Some(0.dp()),
    };
    let texts = 2;
    let RecordedProbe { size, measured } = probe_recording(reading_column, height)?;
    assert_eq!(size, Size::new(270.dp(), 270.dp()));
    assert!(measured.len() <= 2 * texts, "{measured:?}");
    Ok(())
}

fn reading_through(ui: &mut Ui, depth: usize) {
    let source = NodeRef::new();
    ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            nested(ui, depth, weighted, &Modifier::new().node_ref(&source));
        });
        ui.spacer(Dp::ZERO, 10.dp(), Modifier::new().width(source.height()));
    });
}

#[test]
fn a_probe_measures_each_text_at_most_twice_with_a_read_across_nested_stacks() -> TestResult {
    let narrow = Proposal {
        width: Some(0.dp()),
        height: None,
    };
    for depth in 0..=12 {
        let measured = probe_recording(move |ui| reading_through(ui, depth), narrow)?.measured;
        assert!(measured.len() <= 2 * (depth + 1), "{depth}");
    }
    for (depth, size) in [
        (4, Size::new(480.dp(), 380.dp())),
        (5, Size::new(480.dp(), 420.dp())),
    ] {
        let probed = probe_recording(move |ui| reading_through(ui, depth), narrow)?.size;
        assert_eq!(probed, size);
    }
    Ok(())
}
