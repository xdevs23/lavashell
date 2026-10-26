// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::scroll::ScrollState;

#[test]
fn an_offset_places_a_menu_under_the_bounds_of_a_button() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let button = NodeRef::new();
        let below = button
            .bounds()
            .map(|bounds| Point::new(bounds.x, bounds.y + bounds.height));
        ui.layout(Axis::Vertical, Modifier::new().padding(10.dp()), |ui| {
            ui.spacer(80.dp(), 30.dp(), Modifier::new());
            ui.spacer(60.dp(), 20.dp(), Modifier::new().node_ref(&button));
        });
        ui.spacer(
            100.dp(),
            50.dp(),
            Modifier::new()
                .offset(below.map(|at| at.x), below.map(|at| at.y))
                .tag("menu"),
        );
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(
        view.bounds_of("menu"),
        Some(Rect::new(10.dp(), 60.dp(), 100.dp(), 50.dp()))
    );
    Ok(())
}

#[test]
fn bounds_follow_the_scroll_of_the_nodes_around_them() -> TestResult {
    let scroll = ScrollState::new(Axis::Vertical);
    let scrolled = scroll.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let button = NodeRef::new();
        let top = button.bounds().map(|bounds| bounds.y);
        ui.scroll(scrolled.clone(), Modifier::new(), |ui| {
            ui.spacer(50.dp(), 200.dp(), Modifier::new());
            ui.spacer(50.dp(), 20.dp(), Modifier::new().node_ref(&button));
            ui.spacer(50.dp(), 400.dp(), Modifier::new());
        });
        ui.spacer(
            10.dp(),
            10.dp(),
            Modifier::new().offset(Dp::ZERO, top).tag("marker"),
        );
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds("marker")?.y, 200.dp());
    scroll.jump_to(150.dp());
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds("marker")?.y, 50.dp());
    Ok(())
}

#[test]
fn a_scoped_component_paints_an_offset_that_reads_bounds_moved_by_a_scroll_outside_it() -> TestResult
{
    let scroll = ScrollState::new(Axis::Vertical);
    let scrolled = scroll.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let button = NodeRef::new();
        let top = button.bounds().map(|bounds| bounds.y);
        ui.scroll(scrolled.clone(), Modifier::new(), |ui| {
            ui.spacer(50.dp(), 200.dp(), Modifier::new());
            ui.spacer(50.dp(), 20.dp(), Modifier::new().node_ref(&button));
            ui.spacer(50.dp(), 400.dp(), Modifier::new());
        });
        ui.scoped("marker", Modifier::new(), move |ui| {
            let marker = Modifier::new().offset(Dp::ZERO, top.clone());
            ui.spacer(10.dp(), 10.dp(), marker.background(Color::WHITE));
        });
    });
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    scroll.jump_to(150.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let fills: Vec<Rect> = (scene.commands().into_iter())
        .filter_map(|command| match command {
            DrawCommand::Fill { shape, .. } => Some(shape.bounds()),
            _ => None,
        })
        .collect();
    assert_eq!(fills, [Rect::new(0.dp(), 50.dp(), 10.dp(), 10.dp())]);
    assert_eq!(view.stats().reused_layouts, 1);
    Ok(())
}

/// The error returned by the first frame of a view built with `compose`, or
/// `TestError::FrameSucceeded` when that frame succeeds.
fn first_frame_error(compose: impl FnMut(&mut Ui) + 'static) -> Result<FrameError, TestError> {
    let mut view = View::new(crate::platform::fixture::facts(), compose);
    let viewport = Viewport::new(WINDOW_SIZE, Density::ONE);
    let Err(error) = view.frame(viewport, &mut Metrics) else {
        return Err(TestError::FrameSucceeded);
    };
    Ok(error)
}

fn tag(name: &NodeName) -> Option<&str> {
    name.tag.as_deref()
}

/// For each layout read in `reads`, the tag of the node that reads a size and the tag of the node
/// whose size it reads.
fn tags(reads: &[LayoutRead]) -> Vec<(Option<&str>, Option<&str>)> {
    (reads.iter())
        .map(|read| (tag(&read.reader.node), tag(&read.source)))
        .collect()
}

#[test]
fn sizes_that_read_each_other_name_the_cycle() -> TestResult {
    let error = first_frame_error(|ui| {
        let (a, b) = (NodeRef::new(), NodeRef::new());
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            let width = Modifier::new().width(b.width());
            ui.spacer(Dp::ZERO, 10.dp(), width.node_ref(&a).tag("a"));
            let width = Modifier::new().width(a.width());
            ui.spacer(Dp::ZERO, 10.dp(), width.node_ref(&b).tag("b"));
        });
    })?;
    let FrameError::Layout(LayoutError::SizeCycle(LayoutReads(reads))) = error else {
        return Err(error.into());
    };
    assert_eq!(
        tags(&reads),
        [(Some("a"), Some("b")), (Some("b"), Some("a"))]
    );
    Ok(())
}

#[test]
fn a_size_cannot_read_the_node_around_it() -> TestResult {
    let error = first_frame_error(|ui| {
        let card = NodeRef::new();
        ui.layout(Axis::Vertical, Modifier::new().node_ref(&card), |ui| {
            let width = Modifier::new().width(card.width());
            ui.spacer(Dp::ZERO, 10.dp(), width);
        });
    })?;
    assert!(matches!(
        error,
        FrameError::Layout(LayoutError::RelativeSizeRead(_))
    ));
    Ok(())
}

#[test]
fn a_reference_is_set_on_one_node() -> TestResult {
    let error = first_frame_error(|ui| {
        let shared = NodeRef::new();
        ui.spacer(10.dp(), 10.dp(), Modifier::new().node_ref(&shared).tag("a"));
        ui.spacer(10.dp(), 10.dp(), Modifier::new().node_ref(&shared).tag("b"));
    })?;
    let FrameError::Layout(LayoutError::DuplicateNodeRef { first, second }) = error else {
        return Err(error.into());
    };
    assert_eq!((tag(&first), tag(&second)), (Some("a"), Some("b")));
    Ok(())
}

#[test]
fn a_reference_read_is_set_on_a_node() -> TestResult {
    let error = first_frame_error(|ui| {
        let missing = NodeRef::new();
        let width = Modifier::new().width(missing.width());
        ui.spacer(Dp::ZERO, 10.dp(), width.tag("card"));
    })?;
    let FrameError::Layout(LayoutError::UnsetNodeRef(value)) = error else {
        return Err(error.into());
    };
    assert_eq!(tag(&value.node), Some("card"));
    Ok(())
}

#[test]
fn an_offset_cannot_read_bounds_that_it_moves() -> TestResult {
    let error = first_frame_error(|ui| {
        let title = NodeRef::new();
        let x = title.bounds().map(|bounds| bounds.x);
        ui.layout(
            Axis::Vertical,
            Modifier::new().offset(x, Dp::ZERO).tag("card"),
            |ui| {
                ui.spacer(
                    10.dp(),
                    10.dp(),
                    Modifier::new().node_ref(&title).tag("title"),
                );
            },
        );
    })?;
    let FrameError::Layout(LayoutError::BoundsCycle(LayoutReads(reads))) = error else {
        return Err(error.into());
    };
    assert_eq!(tags(&reads), [(Some("card"), Some("title"))]);
    Ok(())
}

#[test]
fn a_child_without_a_weight_cannot_read_a_child_with_one() -> TestResult {
    let error = first_frame_error(|ui| {
        let bar = NodeRef::new();
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            let width = Modifier::new().width(bar.width());
            ui.spacer(Dp::ZERO, 10.dp(), width.tag("label"));
            ui.spacer(
                Dp::ZERO,
                10.dp(),
                Modifier::new().weight(1.0).node_ref(&bar).tag("bar"),
            );
        });
    })?;
    let FrameError::Layout(LayoutError::WeightedReadOrder(read)) = error else {
        return Err(error.into());
    };
    assert_eq!(tags(&[read]), [(Some("label"), Some("bar"))]);
    Ok(())
}

#[test]
fn a_condition_cannot_read_the_size_of_its_own_node() -> TestResult {
    let error = first_frame_error(|ui| {
        let card = NodeRef::new();
        let fits = card.width().map(|width| width <= 100.dp());
        let shown = Modifier::new().node_ref(&card).shown_when(fits);
        ui.spacer(120.dp(), 10.dp(), shown);
    })?;
    assert!(matches!(
        error,
        FrameError::Layout(LayoutError::RelativeSizeRead(_))
    ));
    Ok(())
}

#[test]
fn the_condition_of_a_child_with_a_weight_cannot_read_another_child_with_one() -> TestResult {
    let error = first_frame_error(|ui| {
        let bar = NodeRef::new();
        let fits = bar.width().map(|width| width >= 50.dp());
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            let shown = Modifier::new().weight(1.0).shown_when(fits);
            ui.spacer(Dp::ZERO, 10.dp(), shown.tag("label"));
            let weighted = Modifier::new().weight(1.0).node_ref(&bar);
            ui.spacer(Dp::ZERO, 10.dp(), weighted.tag("bar"));
        });
    })?;
    let FrameError::Layout(LayoutError::WeightedReadOrder(read)) = error else {
        return Err(error.into());
    };
    assert_eq!(tags(&[read]), [(Some("label"), Some("bar"))]);
    Ok(())
}

#[test]
fn a_natural_width_read_by_a_condition_keeps_the_order_of_weights() -> TestResult {
    let error = first_frame_error(|ui| {
        let (label, bar) = (NodeRef::new(), NodeRef::new());
        let fits = label.natural_width().map(|width| width <= 200.dp());
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            let shown = Modifier::new().weight(1.0).shown_when(fits);
            ui.layout(
                Axis::Horizontal,
                shown.node_ref(&label).tag("label"),
                |ui| {
                    ui.spacer(Dp::ZERO, 10.dp(), Modifier::new().width(bar.width()));
                },
            );
            let weighted = Modifier::new().weight(1.0).node_ref(&bar);
            ui.spacer(Dp::ZERO, 10.dp(), weighted.tag("bar"));
        });
    })?;
    let FrameError::Layout(LayoutError::WeightedReadOrder(read)) = error else {
        return Err(error.into());
    };
    assert_eq!(tags(&[read]), [(Some("label"), Some("bar"))]);
    Ok(())
}

#[test]
fn a_condition_that_decides_whether_an_overlay_fills_cannot_read_inside_it() -> TestResult {
    let error = first_frame_error(|ui| {
        let mark = NodeRef::new();
        let fits = mark.width().map(|width| width > Dp::ZERO);
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            ui.layout(Arrangement::Overlay, Modifier::new(), |ui| {
                let filling = Modifier::new().fill_max_width().shown_when(fits);
                ui.spacer(Dp::ZERO, 10.dp(), filling.tag("filling"));
                let marked = Modifier::new().node_ref(&mark);
                ui.spacer(10.dp(), 10.dp(), marked.tag("mark"));
            });
        });
    })?;
    let FrameError::Layout(LayoutError::WeightedReadOrder(read)) = error else {
        return Err(error.into());
    };
    assert_eq!(tags(&[read]), [(Some("filling"), Some("mark"))]);
    Ok(())
}

#[test]
fn a_condition_in_a_cycle_names_the_cycle() -> TestResult {
    let error = first_frame_error(|ui| {
        let (a, b) = (NodeRef::new(), NodeRef::new());
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            let shown = Modifier::new().shown_when(b.width().map(|width| width > Dp::ZERO));
            ui.spacer(10.dp(), 10.dp(), shown.node_ref(&a).tag("a"));
            let width = Modifier::new().width(a.width());
            ui.spacer(Dp::ZERO, 10.dp(), width.node_ref(&b).tag("b"));
        });
    })?;
    let FrameError::Layout(LayoutError::SizeCycle(LayoutReads(reads))) = error else {
        return Err(error.into());
    };
    assert_eq!(
        tags(&reads),
        [(Some("a"), Some("b")), (Some("b"), Some("a"))]
    );
    Ok(())
}

#[test]
fn a_node_shown_by_its_natural_width_still_takes_part_in_a_cycle() -> TestResult {
    let error = first_frame_error(|ui| {
        let (x, y) = (NodeRef::new(), NodeRef::new());
        let fits = x.natural_width().map(|width| width <= 200.dp());
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            let shown = Modifier::new().node_ref(&x).shown_when(fits);
            ui.layout(Axis::Horizontal, shown.tag("x"), |ui| {
                let width = Modifier::new().width(y.width());
                ui.spacer(Dp::ZERO, 10.dp(), width.tag("inside"));
            });
            let width = Modifier::new().width(x.width());
            ui.spacer(Dp::ZERO, 10.dp(), width.node_ref(&y).tag("y"));
        });
    })?;
    let FrameError::Layout(LayoutError::SizeCycle(LayoutReads(reads))) = error else {
        return Err(error.into());
    };
    assert_eq!(
        tags(&reads),
        [(Some("inside"), Some("y")), (Some("y"), Some("x"))]
    );
    Ok(())
}

#[test]
fn natural_widths_whose_probes_read_each_other_name_the_cycle() -> TestResult {
    let error = first_frame_error(|ui| {
        let (x, y) = (NodeRef::new(), NodeRef::new());
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.layout(
                Axis::Horizontal,
                Modifier::new().node_ref(&x).tag("x"),
                |ui| {
                    let width = Modifier::new().width(y.natural_width());
                    ui.spacer(Dp::ZERO, 10.dp(), width.tag("x1"));
                },
            );
            ui.layout(
                Axis::Horizontal,
                Modifier::new().node_ref(&y).tag("y"),
                |ui| {
                    let width = Modifier::new().width(x.natural_width());
                    ui.spacer(Dp::ZERO, 10.dp(), width.tag("y1"));
                },
            );
        });
    })?;
    let FrameError::Layout(LayoutError::SizeCycle(LayoutReads(reads))) = error else {
        return Err(error.into());
    };
    assert_eq!(
        tags(&reads),
        [(Some("y1"), Some("x")), (Some("x1"), Some("y"))]
    );
    Ok(())
}

#[test]
fn reads_from_different_depths_that_order_one_container_both_ways_name_the_cycle() -> TestResult {
    let error = first_frame_error(|ui| {
        let (x1, y) = (NodeRef::new(), NodeRef::new());
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.layout(Axis::Vertical, Modifier::new(), |ui| {
                ui.spacer(10.dp(), 10.dp(), Modifier::new().node_ref(&x1).tag("x1"));
                ui.layout(Axis::Vertical, Modifier::new(), |ui| {
                    let width = Modifier::new().width(y.width());
                    ui.spacer(Dp::ZERO, 10.dp(), width.tag("x2"));
                });
            });
            ui.layout(Axis::Vertical, Modifier::new(), |ui| {
                let width = Modifier::new().width(x1.width());
                ui.spacer(Dp::ZERO, 10.dp(), width.node_ref(&y).tag("y"));
            });
        });
    })?;
    let FrameError::Layout(LayoutError::SizeCycle(LayoutReads(reads))) = error else {
        return Err(error.into());
    };
    assert_eq!(
        tags(&reads),
        [(Some("x2"), Some("y")), (Some("y"), Some("x1"))]
    );
    Ok(())
}

#[test]
fn a_composition_that_changes_what_it_reads_fails_the_frame() -> TestResult {
    let count = State::new(0);
    let error = first_frame_error(move |ui| {
        count.set(count.get() + 1);
        ui.text("restless", Modifier::new());
    })?;
    assert!(matches!(error, FrameError::CompositionCycle { .. }));
    Ok(())
}

#[test]
fn a_frame_after_a_layout_error_reports_it_again() {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let missing = NodeRef::new();
        ui.spacer(Dp::ZERO, 10.dp(), Modifier::new().width(missing.width()));
    });
    let viewport = Viewport::new(WINDOW_SIZE, Density::ONE);
    for _ in 0..2 {
        assert!(matches!(
            view.frame(viewport, &mut Metrics),
            Err(FrameError::Layout(LayoutError::UnsetNodeRef(_)))
        ));
    }
}
