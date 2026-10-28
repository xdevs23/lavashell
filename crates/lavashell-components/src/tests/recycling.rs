// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::frame::FrameTime;
use lavashell_core::numbers::count_f64;
use lavashell_core::scene::tree::Change;
use lavashell_core::view::{ComponentError, CompositionError};
use lavashell_test_platform::failure_causes;
use std::time::Duration;

const LIST: Viewport = Viewport::new(Size::new(Dp::new(100.0), Dp::new(120.0)), Density::ONE);

/// Twenty rows of 40 dp in a list 120 dp high, where each row logs its index in `composed` when it composes.
fn scoped_rows(
    recycler: &Recycler<usize>,
    composed: &Rc<RefCell<Vec<usize>>>,
    smooth: Option<Tween>,
) -> View {
    let (list, log) = (recycler.clone(), composed.clone());
    View::new(lavashell_test_platform::facts(), move |ui| {
        let (list, log) = (list.clone(), log.clone());
        ui.scoped("list", Modifier::new(), move |ui| {
            if let Some(tween) = smooth
                && let Err(error) = list.scroll.smooth(ui.motion(), tween)
            {
                ui.fail(error);
            }
            let log = log.clone();
            list.compose(
                ui,
                RecyclerItems {
                    count: 20,
                    key: |i| i,
                    view_type: |_| 0,
                },
                RecyclerLayout::list(constant!(PositiveDp::try_new(Dp::new(40.0)))),
                Modifier::new().size(100.dp(), 120.dp()),
                move |ui, item| {
                    let (log, index, size) = (log.clone(), item.index, item.size);
                    ui.scoped(index, Modifier::new(), move |ui| {
                        log.borrow_mut().push(index);
                        ui.spacer(
                            size.width,
                            size.height,
                            Modifier::new().background(Color::WHITE),
                        );
                    });
                },
            );
        });
    })
}

fn frame_at(view: &mut View, ms: u64) -> Result<SceneUpdate, FrameFailure> {
    let time = Duration::from_millis(ms);
    let timing = FrameTime {
        now: time,
        presentation: time,
        ..FrameTime::default()
    };
    (view.frame_at(LIST, &mut Metrics, timing)).without_failures()
}

/// Makes frames of `view` at the time `ms` until the view needs no frame, and applies every update to `tree`.
fn settle(view: &mut View, tree: &mut SceneTree, ms: u64) -> Result<(), TestError> {
    while view.needs_frame() {
        tree.show(frame_at(view, ms)?)?;
    }
    Ok(())
}

/// At 10 dp and at 30 dp, rows 0 to 3 are in view, so the window contains the same rows.
#[test]
fn a_scroll_within_the_window_composes_nothing_and_changes_only_the_content_slot()
-> Result<(), TestError> {
    let recycler = Recycler::<usize>::new();
    let composed = Rc::default();
    let mut view = scoped_rows(&recycler, &composed, None);
    let mut tree = SceneTree::default();
    settle(&mut view, &mut tree, 0)?;
    recycler.scroll.jump_to(10.dp());
    settle(&mut view, &mut tree, 0)?;
    let before = view.stats();
    recycler.scroll.jump_to(30.dp());
    let update = frame_at(&mut view, 0)?;
    assert_eq!(
        view.stats().component_compositions,
        before.component_compositions
    );
    assert!(matches!(update.changes[..], [Change::SetSlot { .. }]));
    Ok(())
}

/// At 50 dp, row 4 comes into view and row 5 enters the overscan. Row 5 is the only row painted again.
#[test]
fn a_scroll_across_a_row_repaints_the_list_and_the_entering_row_only() -> Result<(), TestError> {
    let recycler = Recycler::<usize>::new();
    let composed = Rc::new(RefCell::new(Vec::new()));
    let mut view = scoped_rows(&recycler, &composed, None);
    let mut tree = SceneTree::default();
    recycler.scroll.jump_to(30.dp());
    settle(&mut view, &mut tree, 0)?;
    let before = view.stats();
    composed.borrow_mut().clear();
    recycler.scroll.jump_to(50.dp());
    tree.show(frame_at(&mut view, 0)?)?;
    assert_eq!(*composed.borrow(), Vec::from_iter(0..6));
    assert_eq!(
        view.stats().painted_components,
        before.painted_components + 2
    );
    Ok(())
}

/// The viewport shows 3 rows. A move by 2 rows composes 2 more rows on each side, and a jump by
/// more rows than the viewport shows composes no more than the 3 rows of the viewport on each side.
#[test]
fn a_move_composes_the_rows_that_it_passed_and_a_jump_no_more_than_a_viewport()
-> Result<(), TestError> {
    let recycler = Recycler::<usize>::new();
    let composed = Rc::new(RefCell::new(Vec::new()));
    let mut view = scoped_rows(&recycler, &composed, None);
    let mut tree = SceneTree::default();
    settle(&mut view, &mut tree, 0)?;
    composed.borrow_mut().clear();
    recycler.scroll.jump_to(80.dp());
    settle(&mut view, &mut tree, 0)?;
    assert_eq!(*composed.borrow(), Vec::from_iter(0..7));
    composed.borrow_mut().clear();
    recycler.scroll.jump_to(400.dp());
    settle(&mut view, &mut tree, 0)?;
    assert_eq!(*composed.borrow(), Vec::from_iter(7..16));
    Ok(())
}

/// From 40 dp on, row 5 comes into view and row 0 is the row that the scroll moved past. From 80 dp
/// on, row 0 is out of the window. The scroll in this test ends at 35 dp, before either change.
#[test]
fn a_smooth_scroll_composes_the_list_only_when_its_window_changes() -> Result<(), TestError> {
    let tween = Tween::new(Duration::from_millis(400))
        .easing(Easing::Linear)
        .preserve_velocity(false);
    let recycler = Recycler::<usize>::new();
    let composed = Rc::new(RefCell::new(Vec::new()));
    let mut view = scoped_rows(&recycler, &composed, Some(tween));
    let mut tree = SceneTree::default();
    recycler.scroll.jump_to(10.dp());
    settle(&mut view, &mut tree, 0)?;
    let before = view.stats();
    recycler.scroll.scroll_by(25.dp());
    tree.show(frame_at(&mut view, 0)?)?;
    for ms in (10..400).step_by(10) {
        assert!(frame_at(&mut view, ms)?.changes.is_empty(), "at {ms} ms");
    }
    let rest = frame_at(&mut view, 400)?;
    assert!((rest.changes.iter()).all(|change| matches!(change, Change::SetSlot { .. })));
    tree.show(rest)?;
    assert_eq!(recycler.scroll.offset(), 35.dp());
    assert_eq!(
        view.stats().component_compositions,
        before.component_compositions
    );
    recycler.scroll.scroll_by(80.dp());
    composed.borrow_mut().clear();
    let mut windows = vec![Vec::from_iter(0..5)];
    for ms in (400..=800).step_by(10) {
        tree.show(frame_at(&mut view, ms)?)?;
        let window = std::mem::take(&mut *composed.borrow_mut());
        if !window.is_empty() {
            assert_ne!(Some(&window), windows.last(), "at {ms} ms");
            windows.push(window);
        }
    }
    assert_eq!(
        windows,
        [
            Vec::from_iter(0..5),
            Vec::from_iter(0..6),
            Vec::from_iter(1..7)
        ]
    );
    Ok(())
}

#[test]
fn recycler_rebinds_a_bounded_pool_and_clears_old_item_memory() -> Result<(), TestError> {
    let recycler = Recycler::<usize>::new();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let log = observed.clone();
    let list = recycler.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        log.borrow_mut().clear();
        list.compose(
            ui,
            RecyclerItems {
                count: 100_000,
                key: |i| i,
                view_type: |_| 0,
            },
            RecyclerLayout::list(constant!(PositiveDp::try_new(Dp::new(40.0)))),
            Modifier::new().fill_max_width().fill_max_height(),
            |ui, item| {
                let remembered = ui.state("identity", || item.key).get();
                assert_eq!(
                    remembered, item.key,
                    "rebound holders must not retain another item's state"
                );
                log.borrow_mut().push((item.index, item.holder));
                ui.spacer(item.size.width, item.size.height, Modifier::new());
            },
        );
    });
    let size = Size::new(600.dp(), 400.dp());
    let viewport = Viewport::new(size, Density::ONE);
    render(&mut view, viewport, &mut Metrics)?;
    let first = observed.borrow().clone();
    assert_eq!(first.len(), 10, "the rows in view");
    for index in (20..100_000).step_by(1234) {
        recycler.scroll.jump_to(40.dp() * count_f64(index)?);
        render(&mut view, viewport, &mut Metrics)?;
        assert!(recycler.stats().active <= 30);
        assert!(recycler.stats().holders <= 30);
    }
    assert_eq!(recycler.stats().created, 30, "scrolling must reuse holders");
    assert!(recycler.stats().rebound > 500);
    recycler.scroll.jump_to(0.dp());
    render(&mut view, viewport, &mut Metrics)?;
    let before = observed.borrow().clone();
    recycler.scroll.scroll_by(40.dp());
    render(&mut view, viewport, &mut Metrics)?;
    let after = observed.borrow().clone();
    let overlapping: Vec<_> = (before.iter())
        .filter(|(index, _)| after.iter().any(|(other, _)| other == index))
        .collect();
    assert!(!overlapping.is_empty());
    for item in overlapping {
        assert!(
            after.contains(item),
            "overlapping items retain their holders"
        );
    }
    recycler.scroll.jump_to(Dp::MAX);
    render(&mut view, viewport, &mut Metrics)?;
    let last_index = observed
        .borrow()
        .last()
        .map(|(index, _)| *index)
        .ok_or(TestError::NothingRecorded("composed row"))?;
    assert_eq!(last_index, 99_999);
    assert_eq!(recycler.scroll.offset(), 3_999_600.dp());
    Ok(())
}

#[test]
fn recycled_grid_responds_to_layout_width_and_clamps_after_data_shrinks() -> Result<(), TestError> {
    let count = State::new(10_000);
    let recycler = Recycler::<usize>::new();
    let list = recycler.clone();
    let items = count.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        list.compose(
            ui,
            RecyclerItems {
                count: items.get(),
                key: |i| i,
                view_type: |i| (i % 2) as u64,
            },
            RecyclerLayout::grid(
                constant!(PositiveDp::try_new(Dp::new(100.0))),
                constant!(PositiveDp::try_new(Dp::new(150.0))),
                constant!(NonNegativeDp::try_new(Dp::new(10.0))),
            ),
            Modifier::new().fill_max_width().fill_max_height(),
            |ui, item| {
                ui.text(
                    item.key.to_string(),
                    Modifier::new()
                        .tag(item.key.to_string())
                        .size(item.size.width, item.size.height),
                );
            },
        );
    });
    render(
        &mut view,
        Viewport::new(Size::new(500.dp(), 300.dp()), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(composed_bounds(&view, "1")?.x, 170.dp());
    assert_eq!(composed_bounds(&view, "3")?.y, 110.dp());
    let viewport = Viewport::new(Size::new(320.dp(), 300.dp()), Density::ONE);
    render(&mut view, viewport, &mut Metrics)?;
    assert_eq!(composed_bounds(&view, "1")?.x, 165.dp());
    assert_eq!(composed_bounds(&view, "2")?.y, 110.dp());
    recycler.scroll.jump_to(200_000.0.dp());
    render(&mut view, viewport, &mut Metrics)?;
    count.set(1);
    render(&mut view, viewport, &mut Metrics)?;
    assert_eq!(recycler.scroll.offset(), 0.dp());
    assert_eq!(recycler.stats().active, 1);
    assert!(view.bounds_of("0").is_some());
    count.set(0);
    render(&mut view, viewport, &mut Metrics)?;
    assert_eq!(recycler.stats().active, 0);
    assert_eq!(
        recycler.stats().holders,
        1,
        "the holder released by the last binding stays as a spare"
    );
    Ok(())
}

#[test]
fn selection_preserves_keys_and_ranges_follow_current_sort_order() {
    let mut selection = Selection::new();
    let order = [10, 20, 30, 40, 50];
    selection.select(20, InputModifiers::default(), &order);
    selection.select(
        40,
        InputModifiers {
            shift: true,
            ..Default::default()
        },
        &order,
    );
    assert_eq!(
        selection.keys().iter().copied().collect::<Vec<_>>(),
        [20, 30, 40]
    );
    selection.select(
        30,
        InputModifiers {
            control: true,
            ..Default::default()
        },
        &order,
    );
    assert!(!selection.contains(&30));
    selection.select(
        50,
        InputModifiers {
            shift: true,
            ..Default::default()
        },
        &[50, 40, 30, 20, 10],
    );
    assert_eq!(
        selection.keys().iter().copied().collect::<Vec<_>>(),
        [30, 40, 50]
    );
    assert_eq!(
        selection.navigate(Shortcut::key(Key::Home), &order, 1, 10),
        Some(0)
    );
    assert!(selection.contains(&10));
    assert_eq!(
        selection.navigate(Shortcut::key(Key::End).shift(), &order, 1, 10),
        Some(4)
    );
    assert_eq!(selection.keys().len(), 5);
    selection.retain(|key| *key != 50);
    assert!(selection.current().is_none());
}

// On a target with 32-bit pointers every row count converts to `f64` exactly, so the composition cannot fail.
#[cfg(target_pointer_width = "64")]
#[test]
fn a_list_with_too_many_rows_to_count_exactly_fails_its_composition() -> Result<(), TestError> {
    let recycler = Recycler::<usize>::new();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        recycler.compose(
            ui,
            RecyclerItems {
                count: usize::MAX,
                key: |i| i,
                view_type: |_| 0,
            },
            RecyclerLayout::list(constant!(PositiveDp::try_new(Dp::new(40.0)))),
            Modifier::new(),
            |_, _| {},
        );
    });
    assert!(matches!(
        failure_causes(render(&mut view, LIST, &mut Metrics))?.as_slice(),
        [ComponentError::Composition(CompositionError::Number(
            NumberError::Count(_)
        ))]
    ));
    Ok(())
}
