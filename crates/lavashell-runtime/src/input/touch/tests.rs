// SPDX-License-Identifier: GPL-3.0-only
use crate::{InputRouter, RoutedInput};
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::{PointerKind, TouchId};
use lavashell_core::units::DpExt;
use lavashell_core::view::{InputEvent, TouchSample};
use lavashell_platform::DeviceEvent;

const WINDOW: SurfaceId = SurfaceId(0);
const PANEL: SurfaceId = SurfaceId(1);

fn sample(seat: u32, point: i32, x: i32, y: i32) -> TouchSample {
    TouchSample {
        id: TouchId { seat, point },
        at: Point::new(x.dp(), y.dp()),
        kind: PointerKind::Finger,
    }
}

fn down(surface: SurfaceId, sample: TouchSample) -> DeviceEvent {
    DeviceEvent::TouchDown { surface, sample }
}

fn on(surface: SurfaceId, input: InputEvent) -> RoutedInput {
    RoutedInput::Input {
        surface,
        input,
        pointer: None,
    }
}

#[test]
fn a_touch_point_moves_and_goes_up_on_the_surface_where_it_went_down() {
    let mut router = InputRouter::new(None);
    let (pressed, moved) = (sample(4, 3, 10, 20), sample(4, 3, 30, 40));
    let lifted = sample(4, 3, 35, 45);
    assert_eq!(
        router.route(down(PANEL, pressed)),
        [on(PANEL, InputEvent::TouchDown(pressed))]
    );
    assert_eq!(
        router.route(DeviceEvent::TouchMotion(moved)),
        [on(PANEL, InputEvent::TouchMove(moved))]
    );
    assert_eq!(
        router.route(DeviceEvent::TouchUp(lifted)),
        [on(PANEL, InputEvent::TouchUp(lifted))]
    );
    assert!(router.route(DeviceEvent::TouchUp(lifted)).is_empty());
    assert!(router.route(DeviceEvent::TouchMotion(moved)).is_empty());
}

#[test]
fn a_cancel_ends_one_touch_point_on_its_surface_and_leaves_the_other_points_down() {
    let mut router = InputRouter::new(None);
    let (cancelled, other) = (sample(2, 1, 0, 0), sample(2, 3, 0, 0));
    for (surface, sample) in [(WINDOW, cancelled), (PANEL, other)] {
        assert_eq!(router.route(down(surface, sample)).len(), 1);
    }
    assert_eq!(
        router.route(DeviceEvent::TouchCancel(cancelled.id)),
        [on(WINDOW, InputEvent::TouchCancel(cancelled.id))]
    );
    assert!(router.route(DeviceEvent::TouchUp(cancelled)).is_empty());
    assert_eq!(
        router.route(DeviceEvent::TouchUp(other)),
        [on(PANEL, InputEvent::TouchUp(other))]
    );
}

#[test]
fn forgetting_a_surface_drops_the_touch_points_that_went_down_on_it() {
    let mut router = InputRouter::new(None);
    let (on_window, on_panel) = (sample(0, 1, 1, 1), sample(0, 2, 1, 1));
    assert_eq!(router.route(down(WINDOW, on_window)).len(), 1);
    assert_eq!(router.route(down(PANEL, on_panel)).len(), 1);
    router.forget_surface(WINDOW);
    assert!(router.route(DeviceEvent::TouchUp(on_window)).is_empty());
    assert!(
        router
            .route(DeviceEvent::TouchCancel(on_window.id))
            .is_empty()
    );
    assert_eq!(
        router.route(DeviceEvent::TouchCancel(on_panel.id)),
        [on(PANEL, InputEvent::TouchCancel(on_panel.id))]
    );
}
