// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::accessibility::{AccessibilityAction, ActionKind, ActionOutcome, ActionRequest};
use crate::scroll::ScrollState;
use std::cell::RefCell;
use std::rc::Rc;

/// A row 100 dp wide that scrolls `scroll` and contains five spacers 60 dp wide. Each spacer has
/// the tag and the accessible name "tab 0" to "tab 4" and counts its clicks in `clicks`.
fn tab_row(ui: &mut Ui, scroll: ScrollState, clicks: &State<u32>) {
    ui.scroll(scroll, Modifier::new().width(100.dp()), |ui| {
        for index in 0..5 {
            let clicks = clicks.clone();
            ui.spacer(
                60.dp(),
                40.dp(),
                Modifier::new()
                    .tag(format!("tab {index}"))
                    .accessible_name(format!("tab {index}"))
                    .on_click(move || clicks.update(|count| *count += 1)),
            );
        }
    });
}

fn tabs(scene: &mut Scene, scroll: ScrollState) -> Result<View, TestError> {
    let clicks = State::new(0);
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        tab_row(ui, scroll.clone(), &clicks);
    });
    advance(scene, &mut view, WINDOW_SIZE, 0)?;
    Ok(view)
}

/// A view with a list 100 dp high that scrolls `list`, painted once into `scene`. The list contains
/// the row from `tab_row`, which scrolls `row`, then a spacer tagged "slider" that appends the
/// phase of each horizontal drag to `drags`, then a spacer 200 dp high.
fn page(
    scene: &mut Scene,
    list: ScrollState,
    row: ScrollState,
    clicks: State<u32>,
    drags: Rc<RefCell<Vec<DragPhase>>>,
) -> Result<View, TestError> {
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let drags = drags.clone();
        ui.scroll(list.clone(), Modifier::new().height(100.dp()), |ui| {
            tab_row(ui, row.clone(), &clicks);
            ui.spacer(
                100.dp(),
                40.dp(),
                Modifier::new()
                    .tag("slider")
                    .on_drag(Axis::Horizontal, move |input| {
                        drags.borrow_mut().push(input.phase);
                    }),
            );
            ui.spacer(100.dp(), 200.dp(), Modifier::new());
        });
    });
    advance(scene, &mut view, WINDOW_SIZE, 0)?;
    Ok(view)
}

fn swipe(view: &mut View, point: i32, from: Point, to: Point) {
    let sample = |at| TouchSample {
        id: touch(point),
        at,
        kind: PointerKind::Finger,
    };
    view.input(InputEvent::TouchDown(sample(from)));
    view.input(InputEvent::TouchMove(sample(to)));
    view.input(InputEvent::TouchUp(sample(to)));
}

fn key(view: &mut View, key: Key) {
    view.input(InputEvent::Key { key, shift: false });
}

#[test]
fn a_row_that_scrolls_lays_out_past_its_width_and_shifts_its_content_by_the_offset() -> TestResult {
    let scroll = ScrollState::new(Axis::Horizontal);
    let mut scene = Scene::default();
    let mut view = tabs(&mut scene, scroll.clone())?;
    assert_eq!(scroll.viewport().width, 100.dp());
    assert_eq!(scroll.maximum(), 200.dp());
    scroll.jump_to(50.dp());
    advance(&mut scene, &mut view, WINDOW_SIZE, 16)?;
    assert_eq!(view.tagged_bounds("tab 1")?.x, 10.dp());
    Ok(())
}

#[test]
fn reveal_scrolls_a_row_the_least_distance_that_shows_the_span() -> TestResult {
    let scroll = ScrollState::new(Axis::Horizontal);
    let mut scene = Scene::default();
    let _view = tabs(&mut scene, scroll.clone())?;
    scroll.reveal(180.dp(), 240.dp());
    assert_eq!(scroll.offset(), 140.dp());
    scroll.reveal(60.dp(), 120.dp());
    assert_eq!(scroll.offset(), 60.dp());
    Ok(())
}

#[test]
fn each_axis_of_a_wheel_scrolls_the_nearest_node_that_scrolls_along_it() -> TestResult {
    let (list, row) = (
        ScrollState::new(Axis::Vertical),
        ScrollState::new(Axis::Horizontal),
    );
    let mut scene = Scene::default();
    let mut view = page(
        &mut scene,
        list.clone(),
        row.clone(),
        State::new(0),
        Rc::default(),
    )?;
    let tab = center(view.tagged_bounds("tab 0")?);
    view.input(InputEvent::Scroll {
        at: tab,
        delta: Point::new(30.dp(), Dp::ZERO),
    });
    assert_eq!((row.offset(), list.offset()), (30.dp(), Dp::ZERO));
    view.input(InputEvent::Scroll {
        at: tab,
        delta: Point::new(Dp::ZERO, 20.dp()),
    });
    assert_eq!((row.offset(), list.offset()), (30.dp(), 20.dp()));
    Ok(())
}

#[test]
fn shift_turns_a_vertical_wheel_into_a_sideways_one() -> TestResult {
    let (list, row) = (
        ScrollState::new(Axis::Vertical),
        ScrollState::new(Axis::Horizontal),
    );
    let mut scene = Scene::default();
    let mut view = page(
        &mut scene,
        list.clone(),
        row.clone(),
        State::new(0),
        Rc::default(),
    )?;
    view.input(InputEvent::Modifiers(InputModifiers {
        control: false,
        shift: true,
        alt: false,
    }));
    view.input(InputEvent::Scroll {
        at: center(view.tagged_bounds("tab 0")?),
        delta: Point::new(Dp::ZERO, 20.dp()),
    });
    assert_eq!((row.offset(), list.offset()), (20.dp(), Dp::ZERO));
    Ok(())
}

#[test]
fn with_shift_a_wheel_over_a_list_scrolls_it_by_the_vertical_turn_only() -> TestResult {
    let (list, row) = (
        ScrollState::new(Axis::Vertical),
        ScrollState::new(Axis::Horizontal),
    );
    let mut scene = Scene::default();
    let mut view = page(
        &mut scene,
        list.clone(),
        row.clone(),
        State::new(0),
        Rc::default(),
    )?;
    view.input(InputEvent::Modifiers(InputModifiers {
        control: false,
        shift: true,
        alt: false,
    }));
    let slider = center(view.tagged_bounds("slider")?);
    view.input(InputEvent::Scroll {
        at: slider,
        delta: Point::new(Dp::ZERO, 20.dp()),
    });
    assert_eq!((row.offset(), list.offset()), (Dp::ZERO, 20.dp()));
    view.input(InputEvent::Scroll {
        at: slider,
        delta: Point::new(30.dp(), Dp::ZERO),
    });
    assert_eq!((row.offset(), list.offset()), (Dp::ZERO, 20.dp()));
    Ok(())
}

#[test]
fn left_and_right_scroll_a_row_and_up_and_down_scroll_the_list_around_it() -> TestResult {
    let (list, row) = (
        ScrollState::new(Axis::Vertical),
        ScrollState::new(Axis::Horizontal),
    );
    let mut scene = Scene::default();
    let mut view = page(
        &mut scene,
        list.clone(),
        row.clone(),
        State::new(0),
        Rc::default(),
    )?;
    // The height of one line measured by `Metrics` at 14 sp, the text size set by the platform
    // fixture.
    let line = Dp::new(14.0 * 1.3);
    tap(&mut view, "tab 0")?;
    key(&mut view, Key::Right);
    assert_eq!(row.offset(), line);
    key(&mut view, Key::Left);
    assert_eq!(row.offset(), Dp::ZERO);
    key(&mut view, Key::Down);
    assert_eq!((row.offset(), list.offset()), (Dp::ZERO, line));
    key(&mut view, Key::End);
    assert_eq!((row.offset(), list.offset()), (200.dp(), line));
    Ok(())
}

#[test]
fn assistive_technology_scrolls_a_row_sideways_and_reveals_a_tab_along_it() -> TestResult {
    let scroll = ScrollState::new(Axis::Horizontal);
    let mut scene = Scene::default();
    let mut view = tabs(&mut scene, scroll.clone())?;
    let row = (view.accessibility().nodes.values())
        .find(|node| node.actions.contains(&ActionKind::Scroll(Axis::Horizontal)))
        .required("a node that scrolls sideways")?
        .id;
    let request = |view: &mut View, target, action| {
        view.accessibility_action(ActionRequest { target, action })
    };
    let vertical = AccessibilityAction::Scroll(Axis::Vertical, 40.dp());
    assert_eq!(
        request(&mut view, row, vertical),
        Ok(ActionOutcome::Ignored)
    );
    let sideways = AccessibilityAction::Scroll(Axis::Horizontal, 40.dp());
    assert_eq!(
        request(&mut view, row, sideways),
        Ok(ActionOutcome::Performed)
    );
    assert_eq!(scroll.offset(), 40.dp());
    advance(&mut scene, &mut view, WINDOW_SIZE, 16)?;
    let last = (view.accessibility().nodes.values())
        .find(|node| node.name.as_deref() == Some("tab 4"))
        .required("the last tab")?
        .id;
    assert_eq!(
        request(&mut view, last, AccessibilityAction::ScrollIntoView),
        Ok(ActionOutcome::Performed)
    );
    assert_eq!(scroll.offset(), 200.dp());
    Ok(())
}

#[test]
fn a_sideways_swipe_on_a_tab_scrolls_the_row_containing_it_and_an_upright_swipe_scrolls_the_list()
-> TestResult {
    let (list, row, clicks) = (
        ScrollState::new(Axis::Vertical),
        ScrollState::new(Axis::Horizontal),
        State::new(0),
    );
    let mut scene = Scene::default();
    let mut view = page(
        &mut scene,
        list.clone(),
        row.clone(),
        clicks.clone(),
        Rc::default(),
    )?;
    let start = center(view.tagged_bounds("tab 0")?);
    swipe(&mut view, 1, start, start.offset(-30.dp(), 2.dp()));
    assert_eq!((row.offset(), list.offset()), (22.dp(), Dp::ZERO));
    swipe(&mut view, 2, start, start.offset(2.dp(), -30.dp()));
    assert_eq!((row.offset(), list.offset()), (22.dp(), 22.dp()));
    assert_eq!(clicks.get(), 0);
    Ok(())
}

#[test]
fn a_slider_below_the_tabs_receives_a_sideways_swipe_and_an_upright_swipe_scrolls_the_list()
-> TestResult {
    let (list, row) = (
        ScrollState::new(Axis::Vertical),
        ScrollState::new(Axis::Horizontal),
    );
    let drags = Rc::new(RefCell::new(Vec::new()));
    let mut scene = Scene::default();
    let mut view = page(
        &mut scene,
        list.clone(),
        row.clone(),
        State::new(0),
        drags.clone(),
    )?;
    let start = center(view.tagged_bounds("slider")?);
    swipe(&mut view, 1, start, start.offset(30.dp(), 2.dp()));
    assert_eq!(*drags.borrow(), [DragPhase::Started, DragPhase::Ended]);
    assert_eq!((row.offset(), list.offset()), (Dp::ZERO, Dp::ZERO));
    swipe(&mut view, 2, start, start.offset(2.dp(), -30.dp()));
    assert_eq!(drags.borrow().len(), 2);
    assert_eq!((row.offset(), list.offset()), (Dp::ZERO, 22.dp()));
    Ok(())
}
