// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::float_asserts::assert_f64_eq;
use crate::units::NumberError;

#[test]
fn finite_numeric_actions_clamp_and_use_registered_value_handler() -> TestResult {
    let value = crate::reactive::State::new(5.0);
    let current = value.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let current = current.clone();
        let setter = current.clone();
        ui.spacer(
            100.dp(),
            20.dp(),
            Modifier::new()
                .semantics(Semantics {
                    role: Some(Role::Slider),
                    name: Some(String::from("Volume")),
                    value: Some(AccessibleValue::Number(current.get())),
                    range: Some(RangeInfo {
                        min: 0.0,
                        max: 10.0,
                        step: Some(2.0),
                    }),
                    ..Default::default()
                })
                .on_accessibility_action(ActionKind::SetValue, move |action| {
                    if let AccessibilityAction::SetValue(AccessibleValue::Number(value)) = action {
                        setter.set(value);
                    }
                    Ok(())
                }),
        );
    });
    frame(&mut scene, &mut view)?;
    let slider = target(&view, "Volume")?;
    assert_eq!(
        request(
            &mut view,
            slider,
            AccessibilityAction::SetValue(AccessibleValue::Number(f64::NAN))
        ),
        IGNORED
    );
    assert_eq!(
        request(&mut view, slider, AccessibilityAction::Increment),
        PERFORMED
    );
    assert_f64_eq(value.get(), 7.0);
    frame(&mut scene, &mut view)?;
    assert_eq!(
        request(
            &mut view,
            slider,
            AccessibilityAction::SetValue(AccessibleValue::Number(900.0))
        ),
        PERFORMED
    );
    assert_f64_eq(value.get(), 10.0);
    assert_eq!(
        request(
            &mut view,
            slider,
            AccessibilityAction::SetValue(AccessibleValue::Text(String::from("bad")))
        ),
        IGNORED
    );
    Ok(())
}

#[test]
fn scroll_reveal_exposes_offscreen_items_and_retained_geometry_is_preserved() -> TestResult {
    let scroll = crate::scroll::ScrollState::new(Axis::Vertical);
    let scrolling = scroll.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let scrolling = scrolling.clone();
        ui.scoped("list", Modifier::new(), move |ui| {
            ui.scroll(scrolling.clone(), Modifier::new().height(60.dp()), |ui| {
                for i in 0..20 {
                    ui.text(format!("Item {i}"), Modifier::new());
                }
            });
        });
    });
    frame(&mut scene, &mut view)?;
    let item = target(&view, "Item 19")?;
    assert!(view.accessibility.nodes[&item].visible_bounds.is_none());
    let previous = view.accessibility.clone();
    frame(&mut scene, &mut view)?;
    assert!(view.stats().reused_components > 0);
    assert_eq!(view.accessibility, previous);
    assert_eq!(
        request(&mut view, item, AccessibilityAction::ScrollIntoView),
        PERFORMED
    );
    frame(&mut scene, &mut view)?;
    assert!(scroll.offset() > Dp::ZERO);
    assert!(view.accessibility.nodes[&item].visible_bounds.is_some());
    Ok(())
}

#[test]
fn a_handler_that_fails_returns_its_error_to_the_caller() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.spacer(
            100.dp(),
            20.dp(),
            Modifier::new()
                .semantics(Semantics {
                    role: Some(Role::Button),
                    name: Some(String::from("Fails")),
                    ..Default::default()
                })
                .on_accessibility_action(ActionKind::Activate, |_| {
                    Err(ActionError::Count(NumberError::OutOfRange))
                }),
        );
    });
    frame(&mut scene, &mut view)?;
    let button = target(&view, "Fails")?;
    assert_eq!(
        request(&mut view, button, AccessibilityAction::Activate),
        Err(ActionError::Count(NumberError::OutOfRange))
    );
    Ok(())
}
