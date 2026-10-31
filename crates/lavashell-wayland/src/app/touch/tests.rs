// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use lavashell_core::units::DpExt;

fn touch(seat: u32, point: i32) -> TouchId {
    TouchId { seat, point }
}

#[test]
fn a_touch_point_ends_where_it_last_moved_on_the_surface_it_went_down_on() {
    let mut touches = TouchPoints::new(4);
    let (down, moved) = (Point::new(10.dp(), 20.dp()), Point::new(30.dp(), 40.dp()));
    let id = touch(4, 3);
    assert!(matches!(
        touches.down(3, SurfaceId(1), down),
        (SurfaceId(1), InputEvent::TouchDown(sample))
            if sample.id == id && sample.at == down && sample.kind == PointerKind::Finger
    ));
    assert!(matches!(
        touches.motion(3, moved),
        Some((SurfaceId(1), InputEvent::TouchMove(sample))) if sample.id == id && sample.at == moved
    ));
    assert!(matches!(
        touches.up(3),
        Some((SurfaceId(1), InputEvent::TouchUp(sample))) if sample.id == id && sample.at == moved
    ));
    assert!(touches.up(3).is_none());
    assert!(touches.motion(3, moved).is_none());
}

#[test]
fn cancelling_ends_each_touch_point_of_the_seat_on_its_surface() -> Result<(), TestError> {
    let mut touches = TouchPoints::new(2);
    let (first, second) = (SurfaceId(0), SurfaceId(2));
    assert_eq!(touches.down(1, first, Point::default()).0, first);
    assert_eq!(touches.down(3, second, Point::default()).0, second);
    let cancels = (touches.cancel().into_iter())
        .map(|(surface, input)| match input {
            InputEvent::TouchCancel(id) => Ok((surface, id)),
            _ => Err(TestError::NotTouchCancel),
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(cancels, [(first, touch(2, 1)), (second, touch(2, 3))]);
    assert!(touches.up(1).is_none());
    Ok(())
}
