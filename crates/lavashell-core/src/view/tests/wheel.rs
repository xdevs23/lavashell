// SPDX-License-Identifier: GPL-3.0-only
use super::*;

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(100.0), Dp::new(100.0)), Density::ONE);

#[test]
fn wheel_bubbles_from_child_and_respects_visual_clip_overlay_and_disabled_state() -> TestResult {
    let deltas = State::new(Vec::<Dp>::new());
    let observed = deltas.clone();
    let overlay = State::new(false);
    let shown = overlay.clone();
    let enabled = State::new(true);
    let available = enabled.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let values = observed.clone();
        ui.layout(Arrangement::Overlay, Modifier::new(), |ui| {
            ui.layout(
                Arrangement::Overlay,
                Modifier::new()
                    .on_scroll(move |delta| values.update(|values| values.push(delta)))
                    .enabled(available.get())
                    .visual_scale(crate::constant!(Positive::try_new(0.5)))
                    .size(100.dp(), 100.dp())
                    .clip(Corners::all(50.dp())),
                |ui| {
                    ui.spacer(100.dp(), 100.dp(), Modifier::new().on_click(|| {}));
                },
            );
            if shown.get() {
                ui.spacer(100.dp(), 100.dp(), Modifier::new().on_click(|| {}));
            }
        });
    });
    let mut scene = Scene::default();
    let mut frame = |view: &mut View| advance_with(&mut scene, view, WINDOW, &mut Metrics);
    frame(&mut view)?;
    let wheel = |view: &mut View, x, y, delta| {
        view.input(InputEvent::Scroll {
            at: Point::new(x, y),
            delta: Point::new(Dp::ZERO, delta),
        });
    };
    wheel(&mut view, 50.dp(), 50.dp(), -24.dp());
    wheel(&mut view, 26.dp(), 26.dp(), 24.dp()); // Inside bounds, outside rounded clip.
    wheel(&mut view, 10.dp(), 50.dp(), 24.dp()); // Outside transformed bounds.
    wheel(&mut view, 50.dp(), 50.dp(), Dp::new(f64::NAN));
    wheel(&mut view, 50.dp(), 50.dp(), 0.dp());
    assert_eq!(deltas.get(), [-24.dp()]);
    overlay.set(true);
    frame(&mut view)?;
    wheel(&mut view, 50.dp(), 50.dp(), 24.dp());
    assert_eq!(deltas.get(), [-24.dp()]);
    overlay.set(false);
    enabled.set(false);
    frame(&mut view)?;
    wheel(&mut view, 50.dp(), 50.dp(), 24.dp());
    assert_eq!(deltas.get(), [-24.dp()]);
    Ok(())
}

#[test]
fn a_child_wheel_control_consumes_without_scrolling_its_parent() -> TestResult {
    let scroll = crate::scroll::ScrollState::new(Axis::Vertical);
    let scrolled = scroll.clone();
    let total = State::new(Dp::ZERO);
    let value = total.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let value = value.clone();
        ui.scroll(scrolled.clone(), Modifier::new().height(60.dp()), |ui| {
            ui.spacer(
                80.dp(),
                30.dp(),
                Modifier::new().on_scroll(move |delta| value.update(|v| *v += delta)),
            );
            ui.spacer(80.dp(), 120.dp(), Modifier::new().on_click(|| {}));
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    view.input(InputEvent::Scroll {
        at: Point::new(10.dp(), 15.dp()),
        delta: Point::new(Dp::ZERO, 24.dp()),
    });
    assert_eq!(total.get(), 24.dp());
    assert_eq!(scroll.offset(), Dp::ZERO);
    view.input(InputEvent::Scroll {
        at: Point::new(10.dp(), 45.dp()),
        delta: Point::new(Dp::ZERO, 24.dp()),
    });
    assert_eq!(scroll.offset(), 24.dp());
    Ok(())
}

#[test]
fn a_wheel_handler_receives_a_vertical_wheel_while_shift_is_held() -> TestResult {
    let total = State::new(Dp::ZERO);
    let value = total.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let value = value.clone();
        ui.spacer(
            80.dp(),
            30.dp(),
            Modifier::new().on_scroll(move |delta| value.update(|v| *v += delta)),
        );
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    view.input(InputEvent::Modifiers(InputModifiers {
        control: false,
        shift: true,
        alt: false,
    }));
    view.input(InputEvent::Scroll {
        at: Point::new(10.dp(), 15.dp()),
        delta: Point::new(Dp::ZERO, 24.dp()),
    });
    assert_eq!(total.get(), 24.dp());
    Ok(())
}
