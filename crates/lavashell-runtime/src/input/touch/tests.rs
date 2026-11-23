// SPDX-License-Identifier: GPL-3.0-only
use crate::input::{InputRouter, RoutedInput};
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::TouchId;
use lavashell_core::units::DpExt;
use lavashell_core::view::{InputEvent, TouchMove, TouchSample};
use lavashell_platform::{DeviceEvent, MovingTouch, TouchFrame, TouchMotion};
use lavashell_test_fixtures::{input_time_at_millis, touch_move_at};

const WINDOW: SurfaceId = SurfaceId::numbered(0);
const PANEL: SurfaceId = SurfaceId::numbered(1);

fn at(x: i32, y: i32) -> Point {
    Point::new(x.dp(), y.dp())
}

fn sample(seat: i64, point: i32, x: i32, y: i32) -> TouchSample {
    TouchSample::finger(TouchId { seat, point }, at(x, y))
}

fn moving(sample: TouchSample) -> MovingTouch {
    MovingTouch {
        id: sample.id,
        kind: sample.kind,
    }
}

fn down(surface: SurfaceId, sample: TouchSample) -> DeviceEvent {
    DeviceEvent::TouchDown { surface, sample }
}

fn motion(moved: TouchMove) -> DeviceEvent {
    DeviceEvent::TouchMotion(TouchMotion {
        touches: vec![moving(moved.sample)],
        earlier: Vec::new(),
        latest: TouchFrame {
            time: moved.time,
            positions: vec![moved.sample.at],
        },
    })
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
    let (pressed, movement) = (
        sample(4, 3, 10, 20),
        touch_move_at(sample(4, 3, 30, 40), 16),
    );
    let lifted = sample(4, 3, 35, 45);
    assert_eq!(
        router.route(down(PANEL, pressed)),
        [on(PANEL, InputEvent::TouchDown(pressed))]
    );
    assert_eq!(
        router.route(motion(movement)),
        [on(PANEL, InputEvent::TouchMove(movement))]
    );
    assert_eq!(
        router.route(DeviceEvent::TouchUp(lifted)),
        [on(PANEL, InputEvent::TouchUp(lifted))]
    );
    assert_eq!(router.route(DeviceEvent::TouchUp(lifted)), []);
    assert_eq!(router.route(motion(movement)), []);
}

#[test]
fn a_batched_touch_motion_moves_every_touch_through_each_position_in_the_order_taken() {
    let mut router = InputRouter::new(None);
    let (first, second) = (sample(1, 2, 2, 2), sample(1, 5, 9, 2));
    for pressed in [first, second] {
        assert_eq!(router.route(down(PANEL, pressed)).len(), 1);
    }
    let batched = DeviceEvent::TouchMotion(TouchMotion {
        touches: vec![moving(first), moving(second)],
        earlier: vec![TouchFrame {
            time: input_time_at_millis(8),
            positions: vec![at(3, 3), at(8, 3)],
        }],
        latest: TouchFrame {
            time: input_time_at_millis(16),
            positions: vec![at(4, 3), at(7, 3)],
        },
    });
    let movements = [
        touch_move_at(sample(1, 2, 3, 3), 8),
        touch_move_at(sample(1, 5, 8, 3), 8),
        touch_move_at(sample(1, 2, 4, 3), 16),
        touch_move_at(sample(1, 5, 7, 3), 16),
    ];
    let moves = movements.map(|movement| on(PANEL, InputEvent::TouchMove(movement)));
    assert_eq!(router.route(batched), moves);
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
    assert_eq!(router.route(DeviceEvent::TouchUp(cancelled)), []);
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
    assert_eq!(router.route(DeviceEvent::TouchUp(on_window)), []);
    assert_eq!(router.route(DeviceEvent::TouchCancel(on_window.id)), []);
    assert_eq!(
        router.route(DeviceEvent::TouchCancel(on_panel.id)),
        [on(PANEL, InputEvent::TouchCancel(on_panel.id))]
    );
}
