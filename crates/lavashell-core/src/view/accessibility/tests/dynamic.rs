// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::float_asserts::assert_f64_eq;

#[test]
fn semantic_only_state_wakes_and_publishes_without_visual_work() -> TestResult {
    let progress = crate::reactive::State::new(1.0);
    let read = progress.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let read = read.clone();
        ui.spacer(
            100.dp(),
            20.dp(),
            Modifier::new()
                .semantics(Semantics {
                    role: Some(Role::ProgressIndicator),
                    name: Some(String::from("Existing progress copy")),
                    ..Default::default()
                })
                .semantics_dynamic(move |semantics| {
                    semantics.value = Some(AccessibleValue::Number(read.get()));
                }),
        );
    });
    frame(&mut scene, &mut view)?;
    let node = target(&view, "Existing progress copy")?;
    assert!(!view.needs_frame());
    let compositions = view.stats().root_compositions;
    progress.set(2.0);
    assert!(view.needs_frame());
    assert!(view.needs_accessibility_update());
    assert!(!view.needs_visual_frame());
    view.update_accessibility();
    let update = view.accessibility();
    assert_eq!(
        update.nodes[&node].value,
        Some(AccessibleValue::Number(2.0))
    );
    assert_eq!(view.stats().root_compositions, compositions);
    assert!(!view.needs_accessibility_update());
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn removed_dynamic_provider_expires_its_reactive_dependency() -> TestResult {
    let visible = crate::reactive::State::new(true);
    let progress = crate::reactive::State::new(1.0);
    let show = visible.clone();
    let read = progress.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if show.get() {
            let read = read.clone();
            ui.text(
                "Existing text",
                Modifier::new().semantics_dynamic(move |semantics| {
                    semantics.value = Some(AccessibleValue::Number(read.get()));
                }),
            );
        }
    });
    frame(&mut scene, &mut view)?;
    visible.set(false);
    frame(&mut scene, &mut view)?;
    progress.set(2.0);
    assert!(!view.needs_accessibility_update());
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn metadata_updates_preserve_visual_dirtiness_and_dynamic_actions_use_latest_value() -> TestResult {
    let value = crate::reactive::State::new(1.0);
    let read = value.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let read = read.clone();
        let write = read.clone();
        ui.spacer(
            100.dp(),
            20.dp(),
            Modifier::new()
                .semantics(Semantics {
                    role: Some(Role::Slider),
                    name: Some(String::from("Existing slider copy")),
                    range: Some(RangeInfo {
                        min: 0.0,
                        max: 10.0,
                        step: Some(1.0),
                    }),
                    ..Default::default()
                })
                .semantics_dynamic(move |semantics| {
                    semantics.value = Some(AccessibleValue::Number(read.get()));
                })
                .on_accessibility_action(ActionKind::SetValue, move |action| {
                    if let AccessibilityAction::SetValue(AccessibleValue::Number(value)) = action {
                        write.set(value);
                    }
                    Ok(())
                }),
        );
    });
    frame(&mut scene, &mut view)?;
    let node = target(&view, "Existing slider copy")?;
    value.set(5.0);
    assert_eq!(
        request(&mut view, node, AccessibilityAction::Increment),
        PERFORMED
    );
    assert_f64_eq(value.get(), 6.0);
    assert_eq!(
        request(&mut view, node, AccessibilityAction::Increment),
        PERFORMED
    );
    assert_f64_eq(value.get(), 7.0);
    assert!(view.needs_visual_frame());
    view.update_accessibility();
    assert!(
        view.needs_visual_frame(),
        "semantic publication must not clear input-triggered redraw"
    );
    assert_eq!(
        view.accessibility.nodes[&node].value,
        Some(AccessibleValue::Number(7.0))
    );
    Ok(())
}

#[test]
fn dynamic_disabled_metadata_gates_real_input_and_reenables_without_composition() -> TestResult {
    let disabled = crate::reactive::State::new(false);
    let state = disabled.clone();
    let clicks = crate::reactive::State::new(0);
    let clicked = clicks.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let state = state.clone();
        let clicked = clicked.clone();
        ui.text(
            "Existing action copy",
            Modifier::new()
                .on_click(move || clicked.set(clicked.get() + 1))
                .semantics_dynamic(move |semantics| semantics.state.disabled = state.get()),
        );
    });
    frame(&mut scene, &mut view)?;
    let node = target(&view, "Existing action copy")?;
    assert_eq!(
        request(&mut view, node, AccessibilityAction::Focus),
        PERFORMED
    );
    frame(&mut scene, &mut view)?;
    let compositions = view.stats().root_compositions;
    let point = Point::new(1.dp(), 1.dp());
    disabled.set(true);
    view.update_accessibility();
    view.input(InputEvent::Press(point));
    view.input(InputEvent::Release(point));
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: false,
    });
    assert_eq!(clicks.get(), 0);
    disabled.set(false);
    view.update_accessibility();
    view.input(InputEvent::Press(point));
    view.input(InputEvent::Release(point));
    assert_eq!(clicks.get(), 1);
    assert_eq!(view.stats().root_compositions, compositions);
    Ok(())
}
