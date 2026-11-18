// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::geometry::Axis;
use crate::window::{
    ResizeEdge, WindowCapabilities, WindowControl, WindowRegion, WindowRequestQueue, WindowState,
};
use std::cell::Cell;
use std::rc::Rc;

fn at(x: i32, y: i32) -> Point {
    Point::new(x.dp(), y.dp())
}

fn provide_window_offering(view: &View, capabilities: WindowCapabilities) {
    let state = WindowState {
        capabilities,
        ..WindowState::default()
    };
    view.provide_window(WindowRequestQueue::default().handle(state));
}

fn movable_view(compose: impl FnMut(&mut Ui) + 'static) -> View {
    let view = View::new(crate::platform::fixture::facts(), compose);
    provide_window_offering(&view, WindowCapabilities::new([WindowControl::Move]));
    view
}

#[test]
fn a_drag_region_is_reported_only_in_a_window_that_offers_moves() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let region = Modifier::new().window_region(WindowRegion::Drag);
        ui.spacer(40.dp(), 40.dp(), region);
    });
    advance(&mut Scene::default(), &mut view, WINDOW_SIZE, 0)?;
    let in_region = at(20, 20);
    assert_eq!(view.window_region_at(in_region), None);
    provide_window_offering(&view, WindowCapabilities::new([WindowControl::Maximize]));
    assert_eq!(view.window_region_at(in_region), None);
    provide_window_offering(&view, WindowCapabilities::new([WindowControl::Move]));
    assert_eq!(view.window_region_at(in_region), Some(WindowRegion::Drag));
    Ok(())
}

#[test]
fn a_point_in_two_stacked_regions_reports_the_topmost_region_that_the_window_offers() -> TestResult
{
    let strip = WindowRegion::Resize(ResizeEdge::Top);
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let bar = Modifier::new().window_region(WindowRegion::Drag);
        ui.layout(Axis::Vertical, bar, |ui| {
            ui.spacer(40.dp(), 40.dp(), Modifier::new().window_region(strip));
        });
    });
    advance(&mut Scene::default(), &mut view, WINDOW_SIZE, 0)?;
    let in_strip_and_bar = at(20, 20);
    provide_window_offering(&view, WindowCapabilities::new([WindowControl::Move]));
    let reported = view.window_region_at(in_strip_and_bar);
    assert_eq!(reported, Some(WindowRegion::Drag));
    provide_window_offering(&view, WindowCapabilities::new([WindowControl::Resize]));
    assert_eq!(view.window_region_at(in_strip_and_bar), Some(strip));
    provide_window_offering(&view, WindowCapabilities::all());
    assert_eq!(view.window_region_at(in_strip_and_bar), Some(strip));
    Ok(())
}

#[test]
fn a_drag_region_is_where_its_node_was_laid_out_except_under_nodes_painted_above_it() -> TestResult
{
    let mut view = movable_view(|ui| {
        let bar = Modifier::new()
            .fill_max_width()
            .window_region(WindowRegion::Drag);
        ui.layout(Axis::Horizontal, bar, |ui| {
            ui.spacer(40.dp(), 40.dp(), Modifier::new());
            ui.spacer(40.dp(), 40.dp(), Modifier::new().on_activate(|_| {}));
        });
    });
    let mut scene = Scene::default();
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let (gap, button, right_end, below) = (at(20, 20), at(60, 20), at(450, 20), at(20, 60));
    assert_eq!(view.window_region_at(gap), Some(WindowRegion::Drag));
    assert_eq!(view.window_region_at(button), None);
    assert_eq!(view.window_region_at(right_end), Some(WindowRegion::Drag));
    assert_eq!(view.window_region_at(below), None);
    let narrower = Size::new(300.dp(), WINDOW_SIZE.height);
    advance(&mut scene, &mut view, narrower, 16)?;
    assert_eq!(view.window_region_at(right_end), None);
    assert_eq!(view.window_region_at(at(290, 20)), Some(WindowRegion::Drag));
    Ok(())
}

#[test]
fn the_view_routes_pointer_input_past_a_window_region() -> TestResult {
    let activations = Rc::new(Cell::new(0));
    let activated = activations.clone();
    let mut view = movable_view(move |ui| {
        let activated = activated.clone();
        let button = Modifier::new().on_activate(move |_| activated.set(activated.get() + 1));
        ui.spacer(40.dp(), 40.dp(), button);
        let region = Modifier::new().window_region(WindowRegion::Drag);
        ui.spacer(80.dp(), 40.dp(), region);
    });
    let mut scene = Scene::default();
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let (over_button, beside_button) = (at(20, 20), at(60, 20));
    assert_eq!(view.window_region_at(over_button), Some(WindowRegion::Drag));
    view.input(InputEvent::Move(beside_button));
    assert_eq!(view.cursor(), Cursor::Default);
    press_and_release_at(&mut view, beside_button);
    assert_eq!(view.focus(), None);
    press_and_release_at(&mut view, over_button);
    assert_eq!(activations.get(), 1);
    Ok(())
}

#[test]
fn a_component_painted_from_its_earlier_frame_still_has_its_drag_region() -> TestResult {
    let mut view = movable_view(|ui| {
        ui.scoped("bar", Modifier::new(), |ui| {
            let region = Modifier::new().window_region(WindowRegion::Drag);
            ui.spacer(40.dp(), 40.dp(), region);
        });
    });
    let mut scene = Scene::default();
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.invalidate();
    advance(&mut scene, &mut view, WINDOW_SIZE, 16)?;
    assert_eq!(view.stats().reused_components, 1);
    assert_eq!(view.window_region_at(at(20, 20)), Some(WindowRegion::Drag));
    Ok(())
}

#[test]
fn while_a_modal_is_open_only_the_drag_regions_inside_it_are_reported() -> TestResult {
    let open = State::new(false);
    let modal = open.clone();
    let mut view = movable_view(move |ui| {
        let region = || Modifier::new().window_region(WindowRegion::Drag);
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(40.dp(), 40.dp(), region());
            if modal.get() {
                ui.layout(Axis::Vertical, Modifier::new().modal(), |ui| {
                    ui.spacer(40.dp(), 40.dp(), region());
                });
            }
        });
    });
    let mut scene = Scene::default();
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let (in_bar, in_dialog) = (at(20, 20), at(20, 60));
    assert_eq!(view.window_region_at(in_bar), Some(WindowRegion::Drag));
    open.set(true);
    advance(&mut scene, &mut view, WINDOW_SIZE, 16)?;
    assert_eq!(view.window_region_at(in_bar), None);
    assert_eq!(view.window_region_at(in_dialog), Some(WindowRegion::Drag));
    Ok(())
}

#[test]
fn a_node_stops_being_a_drag_region_while_it_is_disabled_or_hidden() -> TestResult {
    let hidden = State::new(false);
    let hide = hidden.clone();
    let mut view = movable_view(move |ui| {
        let region = || Modifier::new().window_region(WindowRegion::Drag);
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(40.dp(), 40.dp(), region().enabled(false));
            ui.layout(Axis::Vertical, Modifier::new().enabled(false), |ui| {
                ui.spacer(40.dp(), 40.dp(), region());
            });
            let shown = Modifier::new().shown_when(!hide.get());
            ui.layout(Axis::Vertical, shown, |ui| {
                ui.spacer(40.dp(), 40.dp(), region());
            });
        });
    });
    let mut scene = Scene::default();
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let (disabled, in_disabled, in_hideable) = (at(20, 20), at(20, 60), at(20, 100));
    assert_eq!(view.window_region_at(disabled), None);
    assert_eq!(view.window_region_at(in_disabled), None);
    assert_eq!(view.window_region_at(in_hideable), Some(WindowRegion::Drag));
    hidden.set(true);
    advance(&mut scene, &mut view, WINDOW_SIZE, 16)?;
    assert_eq!(view.window_region_at(in_hideable), None);
    Ok(())
}

#[test]
fn a_drag_region_ends_at_the_edge_of_the_scroll_viewport_that_clips_it() -> TestResult {
    let scroll = crate::scroll::ScrollState::new(Axis::Vertical);
    let mut view = movable_view(move |ui| {
        ui.scroll(scroll.clone(), Modifier::new().height(60.dp()), |ui| {
            let taller_than_the_viewport = Modifier::new().window_region(WindowRegion::Drag);
            ui.spacer(80.dp(), 120.dp(), taller_than_the_viewport);
        });
    });
    advance(&mut Scene::default(), &mut view, WINDOW_SIZE, 0)?;
    let (in_viewport, below_viewport) = (at(20, 50), at(20, 70));
    assert_eq!(view.window_region_at(in_viewport), Some(WindowRegion::Drag));
    assert_eq!(view.window_region_at(below_viewport), None);
    Ok(())
}

#[test]
fn an_activatable_node_tagged_as_a_drag_region_is_a_region_and_activates_on_a_press_sent_to_the_view()
-> TestResult {
    let activations = Rc::new(Cell::new(0));
    let activated = activations.clone();
    let mut view = movable_view(move |ui| {
        let activated = activated.clone();
        let button = Modifier::new().on_activate(move |_| activated.set(activated.get() + 1));
        ui.spacer(40.dp(), 40.dp(), button.window_region(WindowRegion::Drag));
    });
    advance(&mut Scene::default(), &mut view, WINDOW_SIZE, 0)?;
    let on_node = at(20, 20);
    assert_eq!(view.window_region_at(on_node), Some(WindowRegion::Drag));
    view.input(InputEvent::Move(on_node));
    assert_eq!(view.cursor(), Cursor::Pointer);
    press_and_release_at(&mut view, on_node);
    assert_eq!(activations.get(), 1);
    Ok(())
}

#[test]
fn the_pointer_over_an_offered_resize_region_shows_the_resize_cursor_of_its_edge() -> TestResult {
    let corner = ResizeEdge::BottomRight;
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let region = Modifier::new().window_region(WindowRegion::Resize(corner));
        ui.spacer(40.dp(), 40.dp(), region);
    });
    advance(&mut Scene::default(), &mut view, WINDOW_SIZE, 0)?;
    let (in_region, outside) = (at(20, 20), at(20, 60));
    view.input(InputEvent::Move(in_region));
    assert_eq!(view.window_region_at(in_region), None);
    assert_eq!(view.cursor(), Cursor::Default);
    provide_window_offering(&view, WindowCapabilities::new([WindowControl::Resize]));
    let offered = Some(WindowRegion::Resize(corner));
    assert_eq!(view.window_region_at(in_region), offered);
    assert_eq!(view.cursor(), Cursor::Resize(corner));
    view.input(InputEvent::Move(outside));
    assert_eq!(view.cursor(), Cursor::Default);
    view.input(InputEvent::Move(in_region));
    view.input(InputEvent::Leave);
    assert_eq!(view.cursor(), Cursor::Default);
    Ok(())
}
