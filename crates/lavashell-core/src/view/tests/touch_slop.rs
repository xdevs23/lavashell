// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::scroll::ScrollState;

#[test]
fn without_a_touch_slop_of_the_platform_a_finger_scrolls_past_one_device_pixel() -> TestResult {
    let scroll = ScrollState::new(Axis::Vertical);
    let content = scroll.clone();
    let platform = PlatformFacts {
        touch_slop: None,
        ..crate::platform::fixture::facts()
    };
    let mut scene = Scene::default();
    let mut view = View::new(platform, move |ui| {
        ui.scroll(content.clone(), Modifier::new().height(100.dp()), |ui| {
            ui.spacer(200.dp(), 400.dp(), Modifier::new());
        });
    });
    let density = Density::from_scale(2.0).required("a density")?;
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(WINDOW_SIZE, density),
        &mut Metrics,
    )?;
    let start = Point::new(100.dp(), 50.dp());
    let finger = |at| TouchSample {
        id: touch(1),
        at,
        kind: PointerKind::Finger,
    };
    view.input(InputEvent::TouchDown(finger(start)));
    view.input(InputEvent::TouchMove(finger(
        start.offset(Dp::ZERO, -0.5.dp()),
    )));
    assert_eq!(scroll.offset(), Dp::ZERO);
    view.input(InputEvent::TouchMove(finger(
        start.offset(Dp::ZERO, -3.dp()),
    )));
    assert_eq!(scroll.offset(), 2.5.dp());
    Ok(())
}
