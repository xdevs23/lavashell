// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_merge::merged;
use crate::{Button, InputSerial, PressState};
use lavashell_actor::{Closed, mailbox};
use lavashell_core::geometry::Point;
use lavashell_core::platform::KeyRepeat;
use lavashell_core::surface::ConfigureToken;
use lavashell_core::ui::PointerKind;
use lavashell_core::units::{Density, Dp, DpExt, NumberError};
use lavashell_core::view::TouchSample;
use std::time::Duration;

const MOUSE: PointerDevice = PointerDevice(1);
const TOUCHPAD: PointerDevice = PointerDevice(2);
const SURFACE: SurfaceId = SurfaceId(1);
const OTHER: SurfaceId = SurfaceId(2);

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("the mailbox is closed")]
    Closed(#[from] Closed),
    #[error("a number is not valid")]
    Number(#[from] NumberError),
}

fn motion(x: f64) -> PlatformEvent {
    let at = Point::new(x.dp(), 0.dp());
    PlatformEvent::Device(DeviceEvent::PointerMotion { pointer: MOUSE, at })
}

fn press() -> PlatformEvent {
    PlatformEvent::Device(DeviceEvent::Button {
        pointer: MOUSE,
        button: Button::Primary,
        state: PressState::Pressed,
        at: Point::new(0.dp(), 0.dp()),
        serial: InputSerial(1),
    })
}

/// The horizontal position of a pointer motion, or `None` for every other event.
fn motion_x(event: &PlatformEvent) -> Option<Dp> {
    match event {
        PlatformEvent::Device(DeviceEvent::PointerMotion { at, .. }) => Some(at.x),
        _ => None,
    }
}

/// A scroll of `pointer` at the horizontal position `x` by the vertical delta `y`.
fn scroll(pointer: PointerDevice, x: f64, y: f64) -> PlatformEvent {
    PlatformEvent::Device(DeviceEvent::Axis {
        pointer,
        at: Point::new(x.dp(), 0.dp()),
        delta: Point::new(0.dp(), y.dp()),
    })
}

/// The pointer, the horizontal position and the vertical delta of a scroll, or `None` for every other event.
fn scroll_of(event: &PlatformEvent) -> Option<(PointerDevice, Dp, Dp)> {
    match event {
        PlatformEvent::Device(DeviceEvent::Axis {
            pointer, at, delta, ..
        }) => Some((*pointer, at.x, delta.y)),
        _ => None,
    }
}

fn touch_sample(point: i32, x: f64) -> TouchSample {
    TouchSample {
        id: TouchId { seat: 1, point },
        at: Point::new(x.dp(), 0.dp()),
        kind: PointerKind::Finger,
    }
}

fn touch_motion(point: i32, x: f64) -> PlatformEvent {
    PlatformEvent::Device(DeviceEvent::TouchMotion(touch_sample(point, x)))
}

fn touch_up(point: i32) -> PlatformEvent {
    PlatformEvent::Device(DeviceEvent::TouchUp(touch_sample(point, 0.0)))
}

/// The point and the horizontal position of a touch motion, or `None` for every other event.
fn touch_x(event: &PlatformEvent) -> Option<(i32, Dp)> {
    match event {
        PlatformEvent::Device(DeviceEvent::TouchMotion(sample)) => {
            Some((sample.id.point, sample.at.x))
        }
        _ => None,
    }
}

fn drag_motion(drag: u64, x: f64) -> PlatformEvent {
    let at = Point::new(x.dp(), 0.dp());
    PlatformEvent::DragTarget(DragTargetEvent::Motion(TransferId(drag), at))
}

/// The drag and the horizontal position of a drag motion, or `None` for every other event.
fn drag_x(event: &PlatformEvent) -> Option<(TransferId, Dp)> {
    match event {
        PlatformEvent::DragTarget(DragTargetEvent::Motion(drag, at)) => Some((*drag, at.x)),
        _ => None,
    }
}

fn density_changed(surface: SurfaceId, density: Density) -> PlatformEvent {
    PlatformEvent::Surface(surface, SurfaceEvent::DensityChanged(density))
}

/// The surface and the density of a density change, or `None` for every other event.
fn density_of(event: &PlatformEvent) -> Option<(SurfaceId, Density)> {
    match event {
        PlatformEvent::Surface(surface, SurfaceEvent::DensityChanged(density)) => {
            Some((*surface, *density))
        }
        _ => None,
    }
}

fn configured(surface: SurfaceId, token: ConfigureToken) -> PlatformEvent {
    let configure = SurfaceEvent::Configured {
        width: Some(100.dp()),
        height: None,
        token: Some(token),
    };
    PlatformEvent::Surface(surface, configure)
}

fn closed(surface: SurfaceId) -> PlatformEvent {
    PlatformEvent::Surface(surface, SurfaceEvent::Closed)
}

/// The surface and the token of a configure, or `None` for every other event.
fn configure_token(event: &PlatformEvent) -> Option<(SurfaceId, ConfigureToken)> {
    match event {
        PlatformEvent::Surface(surface, SurfaceEvent::Configured { token, .. }) => {
            token.map(|token| (*surface, token))
        }
        _ => None,
    }
}

#[test]
fn pointer_motions_merge_until_a_press_and_the_press_keeps_its_place() -> Result<(), Closed> {
    let (sender, receiver) = mailbox();
    for event in [motion(1.0), motion(2.0), press(), motion(3.0), motion(4.0)] {
        sender.push(event)?;
    }
    let taken: Vec<_> = receiver.take()?.iter().map(motion_x).collect();
    assert_eq!(taken, [Some(2.dp()), None, Some(4.dp())]);
    Ok(())
}

#[test]
fn a_setting_replaces_the_waiting_setting_of_its_kind_only() -> Result<(), Closed> {
    let (sender, receiver) = mailbox();
    let repeat = KeyRepeat::new(Duration::from_millis(600), Duration::from_millis(40));
    let settings = [
        SettingsEvent::KeyRepeat(None),
        SettingsEvent::Locales(Vec::new()),
        SettingsEvent::KeyRepeat(repeat),
    ];
    for setting in settings {
        sender.push(PlatformEvent::Settings(setting))?;
    }
    let taken: Vec<_> = (receiver.take()?.into_iter())
        .filter_map(|event| match event {
            PlatformEvent::Settings(setting) => Some(setting),
            _ => None,
        })
        .collect();
    let expected = [
        SettingsEvent::KeyRepeat(repeat),
        SettingsEvent::Locales(Vec::new()),
    ];
    assert_eq!(taken, expected);
    Ok(())
}

#[test]
fn neighbouring_scrolls_add_up_at_the_newest_position() -> Result<(), Closed> {
    let events = [scroll(MOUSE, 1.0, 1.0), scroll(MOUSE, 2.0, 2.0)];
    let taken: Vec<_> = merged(events)?.iter().map(scroll_of).collect();
    assert_eq!(taken, [Some((MOUSE, 2.dp(), 3.dp()))]);
    Ok(())
}

#[test]
fn a_motion_between_scrolls_keeps_both_scrolls_apart() -> Result<(), Closed> {
    let events = [
        scroll(MOUSE, 0.0, 1.0),
        motion(3.0),
        scroll(MOUSE, 3.0, 4.0),
    ];
    let taken: Vec<_> = merged(events)?.iter().map(scroll_of).collect();
    let expected = [
        Some((MOUSE, 0.dp(), 1.dp())),
        None,
        Some((MOUSE, 3.dp(), 4.dp())),
    ];
    assert_eq!(taken, expected);
    Ok(())
}

#[test]
fn a_press_between_scrolls_keeps_both_scrolls_apart() -> Result<(), Closed> {
    let events = [scroll(MOUSE, 0.0, 1.0), press(), scroll(MOUSE, 0.0, 4.0)];
    let taken: Vec<_> = merged(events)?.iter().map(scroll_of).collect();
    let expected = [
        Some((MOUSE, 0.dp(), 1.dp())),
        None,
        Some((MOUSE, 0.dp(), 4.dp())),
    ];
    assert_eq!(taken, expected);
    Ok(())
}

#[test]
fn scrolls_of_two_pointers_add_up_apart() -> Result<(), Closed> {
    let events = [
        scroll(MOUSE, 0.0, 1.0),
        scroll(TOUCHPAD, 0.0, 2.0),
        scroll(MOUSE, 0.0, 4.0),
    ];
    let taken: Vec<_> = merged(events)?.iter().map(scroll_of).collect();
    let expected = [
        Some((MOUSE, 0.dp(), 5.dp())),
        Some((TOUCHPAD, 0.dp(), 2.dp())),
    ];
    assert_eq!(taken, expected);
    Ok(())
}

#[test]
fn a_scroll_between_motions_keeps_both_motions_apart() -> Result<(), Closed> {
    let events = [motion(1.0), scroll(MOUSE, 1.0, 1.0), motion(2.0)];
    let taken: Vec<_> = merged(events)?.iter().map(motion_x).collect();
    assert_eq!(taken, [Some(1.dp()), None, Some(2.dp())]);
    Ok(())
}

#[test]
fn touch_motions_merge_per_point_until_another_touch_event() -> Result<(), Closed> {
    let events = [
        touch_motion(1, 1.0),
        touch_motion(2, 5.0),
        touch_motion(1, 2.0),
        touch_up(1),
        touch_motion(1, 3.0),
    ];
    let taken: Vec<_> = merged(events)?.iter().map(touch_x).collect();
    let expected = [
        Some((1, 2.dp())),
        Some((2, 5.dp())),
        None,
        Some((1, 3.dp())),
    ];
    assert_eq!(taken, expected);
    Ok(())
}

#[test]
fn drag_motions_merge_per_drag_until_another_drag_event() -> Result<(), Closed> {
    let dropped = PlatformEvent::DragTarget(DragTargetEvent::Drop(TransferId(1)));
    let events = [
        drag_motion(1, 1.0),
        drag_motion(2, 5.0),
        drag_motion(1, 2.0),
        dropped,
        drag_motion(1, 3.0),
    ];
    let taken: Vec<_> = merged(events)?.iter().map(drag_x).collect();
    let expected = [
        Some((TransferId(1), 2.dp())),
        Some((TransferId(2), 5.dp())),
        None,
        Some((TransferId(1), 3.dp())),
    ];
    assert_eq!(taken, expected);
    Ok(())
}

#[test]
fn a_density_replaces_the_waiting_density_of_its_surface_only() -> Result<(), TestError> {
    let double = Density::from_ratio(2.0, 1.0)?;
    let triple = Density::from_ratio(3.0, 1.0)?;
    let events = [
        density_changed(SURFACE, Density::ONE),
        density_changed(OTHER, double),
        density_changed(SURFACE, triple),
    ];
    let taken: Vec<_> = merged(events)?.iter().map(density_of).collect();
    assert_eq!(taken, [Some((SURFACE, triple)), Some((OTHER, double))]);
    Ok(())
}

#[test]
fn a_configure_replaces_the_waiting_configure_of_its_surface_only() -> Result<(), Closed> {
    let stale = ConfigureToken::default();
    let other = ConfigureToken::default();
    let newest = ConfigureToken::default();
    let events = [
        configured(SURFACE, stale),
        configured(OTHER, other),
        configured(SURFACE, newest),
    ];
    let taken: Vec<_> = merged(events)?.iter().map(configure_token).collect();
    assert_eq!(taken, [Some((SURFACE, newest)), Some((OTHER, other))]);
    Ok(())
}

#[test]
fn a_configure_after_a_close_keeps_its_place() -> Result<(), Closed> {
    let before = ConfigureToken::default();
    let after = ConfigureToken::default();
    let events = [
        configured(SURFACE, before),
        closed(SURFACE),
        configured(SURFACE, after),
    ];
    let taken: Vec<_> = merged(events)?.iter().map(configure_token).collect();
    let expected = [Some((SURFACE, before)), None, Some((SURFACE, after))];
    assert_eq!(taken, expected);
    Ok(())
}
