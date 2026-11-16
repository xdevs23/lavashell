// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn stock_controls() -> Result<View, TestError> {
    let mut view = View::new(lavashell_test_fixtures::facts(), |ui| {
        ui.column(Modifier::new(), |ui| {
            ui.button(
                Modifier::new(),
                || {},
                |ui| {
                    ui.text("Save", Modifier::new());
                },
            );
            ui.icon_button(
                &BLANK,
                IconOptions::default(),
                Modifier::new().accessible_name("Add"),
                || {},
            );
            ui.icon_button(
                &BLANK,
                IconOptions::default(),
                Modifier::new().tag("internal-only"),
                || {},
            );
            ui.toggle(
                true,
                Modifier::new(),
                |_| {},
                |ui, _| {
                    ui.text("Enabled", Modifier::new());
                },
            );
            ui.slider(
                || 0.5,
                None,
                Modifier::new(),
                |_| {},
                |ui, _| {
                    ui.text("Volume", Modifier::new());
                },
            );
            ui.text_field(
                "a value",
                TextFieldOptions {
                    placeholder: "an example".into(),
                    ..TextFieldOptions::default()
                },
                Modifier::new().accessible_name("Name"),
                |_| {},
            );
            ui.multiline_field(
                "first\nsecond",
                TextFieldOptions::default(),
                Modifier::new().accessible_name("Notes"),
                |_| {},
            );
        });
    });
    render(
        &mut view,
        Viewport::new(Size::new(400.dp(), 300.dp()), Density::ONE),
        &mut HalfEmMetrics,
    )?;
    Ok(view)
}

#[test]
fn stock_buttons_supply_roles_and_visible_names_without_duplicate_text() -> Result<(), TestError> {
    let view = stock_controls()?;
    assert_eq!(named(&view, "Save")?.role, Role::Button);
    assert!(named(&view, "Save")?.children.is_empty());
    assert_eq!(named(&view, "Add")?.role, Role::Button);
    assert!(
        !view
            .accessibility()
            .nodes
            .values()
            .any(|node| node.name.as_deref() == Some("internal-only"))
    );
    assert!(
        view.accessibility()
            .nodes
            .values()
            .any(|node| node.role == Role::Button && node.name.is_none())
    );
    assert_eq!(
        view.accessibility()
            .nodes
            .values()
            .filter(|n| n.name.as_deref() == Some("Save"))
            .count(),
        1
    );
    Ok(())
}

#[test]
fn stock_value_controls_supply_roles_and_values() -> Result<(), TestError> {
    let view = stock_controls()?;
    assert_eq!(named(&view, "Enabled")?.role, Role::Switch);
    assert_eq!(
        named(&view, "Enabled")?.state.semantics.checked,
        Some(Checked::True)
    );
    assert!(named(&view, "Enabled")?.children.is_empty());
    assert_eq!(named(&view, "Volume")?.role, Role::Slider);
    assert_eq!(
        named(&view, "Volume")?.value,
        Some(AccessibleValue::Number(0.5))
    );
    let field = named(&view, "Name")?;
    assert_eq!(field.role, Role::TextInput);
    assert_eq!(field.value, Some(AccessibleValue::Text("a value".into())));
    let field_text = field
        .text
        .as_ref()
        .ok_or(TestError::NotExported("the text of the field"))?;
    assert_eq!(field_text.placeholder.as_deref(), Some("an example"));
    let notes = named(&view, "Notes")?;
    assert_eq!(notes.role, Role::TextArea);
    let notes_text = notes
        .text
        .as_ref()
        .ok_or(TestError::NotExported("the text of the notes"))?;
    assert!(notes_text.multiline);
    Ok(())
}

#[test]
fn merged_control_preserves_an_independently_operable_child() -> Result<(), TestError> {
    let count = State::new(0);
    let inner = count.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.button(
            Modifier::new(),
            || {},
            |ui| {
                ui.text("Outer", Modifier::new());
                ui.button(Modifier::new(), inner.action(|n| *n += 1), |ui| {
                    ui.text("Inner", Modifier::new());
                });
            },
        );
    });
    render(
        &mut view,
        Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE),
        &mut HalfEmMetrics,
    )?;
    let outer = named(&view, "Outer")?.id;
    let inner = named(&view, "Inner")?.id;
    assert!(view.accessibility().nodes[&outer].children.contains(&inner));
    assert_eq!(
        act(&mut view, inner, AccessibilityAction::Activate),
        PERFORMED
    );
    assert_eq!(count.get(), 1);
    Ok(())
}

#[test]
fn toggle_assistive_and_keyboard_activation_use_the_application_callback() -> Result<(), TestError>
{
    let enabled = State::new(false);
    let controlled = enabled.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.toggle(
            controlled.get(),
            Modifier::new(),
            controlled.handler(|state, value| *state = value),
            |ui, _| {
                ui.text("Toggle", Modifier::new());
            },
        );
    });
    let size = Size::new(200.dp(), 100.dp());
    let viewport = Viewport::new(size, Density::ONE);
    render(&mut view, viewport, &mut HalfEmMetrics)?;
    let id = named(&view, "Toggle")?.id;
    assert_eq!(act(&mut view, id, AccessibilityAction::Activate), PERFORMED);
    assert!(enabled.get());
    render(&mut view, viewport, &mut HalfEmMetrics)?;
    assert_eq!(
        named(&view, "Toggle")?.state.semantics.checked,
        Some(Checked::True)
    );
    assert_eq!(act(&mut view, id, AccessibilityAction::Focus), PERFORMED);
    key(&mut view, Key::Space);
    assert!(!enabled.get());
    Ok(())
}

#[test]
fn a_dialog_exposes_its_scope_and_rejects_background_actions() -> Result<(), TestError> {
    let mut view = View::new(lavashell_test_fixtures::facts(), |ui| {
        ui.button(
            Modifier::new(),
            || {},
            |ui| {
                ui.text("Background", Modifier::new());
            },
        );
        ui.dialog(Modifier::new().accessible_name("Dialog"), |ui| {
            ui.button(
                Modifier::new(),
                || {},
                |ui| {
                    ui.text("Inside", Modifier::new());
                },
            );
        });
    });
    render(
        &mut view,
        Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE),
        &mut HalfEmMetrics,
    )?;
    assert!(
        !view
            .accessibility()
            .nodes
            .values()
            .any(|n| n.name.as_deref() == Some("Background"))
    );
    assert_eq!(named(&view, "Dialog")?.role, Role::Dialog);
    assert!(named(&view, "Dialog")?.state.semantics.modal);
    let id = named(&view, "Inside")?.id;
    assert_eq!(act(&mut view, id, AccessibilityAction::Focus), PERFORMED);
    assert_eq!(view.accessibility().focus, Some(id));
    Ok(())
}

#[test]
fn a_scroll_container_keyboard_targets_the_author_supplied_scroll_state() -> Result<(), TestError> {
    let scroll = ScrollState::new(Axis::Vertical);
    let controlled = scroll.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.scroll(
            controlled.clone(),
            Modifier::new()
                .accessible_name("Container")
                .size(100.dp(), 100.dp()),
            |ui| {
                ui.spacer(100.dp(), 1000.dp(), Modifier::new());
            },
        );
    });
    let size = Size::new(200.dp(), 100.dp());
    for _ in 0..3 {
        render(
            &mut view,
            Viewport::new(size, Density::ONE),
            &mut HalfEmMetrics,
        )?;
    }
    let id = named(&view, "Container")?.id;
    assert_eq!(named(&view, "Container")?.role, Role::ScrollView);
    assert_eq!(act(&mut view, id, AccessibilityAction::Focus), PERFORMED);
    key(&mut view, Key::End);
    assert_eq!(scroll.offset(), 900.dp());
    key(&mut view, Key::Home);
    assert_eq!(scroll.offset(), 0.dp());
    key(&mut view, Key::PageDown);
    assert_eq!(
        scroll.offset(),
        86.dp(),
        "a page keeps one line of 14 dp in view"
    );
    key(&mut view, Key::Down);
    assert_eq!(scroll.offset(), 100.dp());
    key(&mut view, Key::PageUp);
    assert!(scroll.offset() < 100.dp());
    Ok(())
}
