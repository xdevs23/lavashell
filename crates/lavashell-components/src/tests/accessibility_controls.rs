// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::accessibility::AccessibleNode;

fn named<'a>(view: &'a View, name: &str) -> Result<&'a AccessibleNode, TestError> {
    view.accessibility()
        .nodes
        .values()
        .find(|node| node.name.as_deref() == Some(name))
        .ok_or_else(|| TestError::NoNodeNamed(name.to_owned()))
}

fn act(
    view: &mut View,
    target: NodeId,
    action: AccessibilityAction,
) -> Result<ActionOutcome, ActionError> {
    view.accessibility_action(ActionRequest { target, action })
}

fn key(view: &mut View, key: Key) {
    view.input(InputEvent::Key { key, shift: false });
}

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
        &mut Metrics,
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
        &mut Metrics,
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
    render(&mut view, viewport, &mut Metrics)?;
    let id = named(&view, "Toggle")?.id;
    assert_eq!(act(&mut view, id, AccessibilityAction::Activate), PERFORMED);
    assert!(enabled.get());
    render(&mut view, viewport, &mut Metrics)?;
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
fn slider_keyboard_pointer_and_assistive_actions_share_one_bounded_value() -> Result<(), TestError>
{
    let value = State::new(0.5);
    let changed = Rc::new(RefCell::new(Vec::new()));
    let log = changed.clone();
    let controlled = value.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        let (read, controlled) = (controlled.clone(), controlled.clone());
        let log = log.clone();
        ui.slider(
            move || read.get(),
            None,
            Modifier::new().accessible_name("Range"),
            move |value| {
                controlled.set(value);
                log.borrow_mut().push(value);
            },
            |ui, _| {
                ui.spacer(100.dp(), 20.dp(), Modifier::new());
            },
        );
    });
    let size = Size::new(200.dp(), 100.dp());
    for _ in 0..2 {
        render(&mut view, Viewport::new(size, Density::ONE), &mut Metrics)?;
    }
    let id = named(&view, "Range")?.id;
    assert_eq!(act(&mut view, id, AccessibilityAction::Focus), PERFORMED);
    key(&mut view, Key::Right);
    key(&mut view, Key::Up);
    assert!(
        is_close(value.get(), 0.52),
        "successive keys must not use stale composition values"
    );
    key(&mut view, Key::Home);
    assert_close(value.get(), 0.0);
    key(&mut view, Key::End);
    key(&mut view, Key::PageDown);
    key(&mut view, Key::PageUp);
    assert_close(value.get(), 1.0);
    assert_eq!(
        act(
            &mut view,
            id,
            AccessibilityAction::SetValue(AccessibleValue::Number(0.2))
        ),
        PERFORMED
    );
    assert_eq!(
        act(&mut view, id, AccessibilityAction::Increment),
        PERFORMED
    );
    assert_eq!(
        act(&mut view, id, AccessibilityAction::Increment),
        PERFORMED
    );
    assert_close(value.get(), 0.22);
    assert_eq!(
        act(&mut view, id, AccessibilityAction::Decrement),
        PERFORMED
    );
    assert_close(value.get(), 0.21);
    assert_eq!(
        act(
            &mut view,
            id,
            AccessibilityAction::SetValue(AccessibleValue::Number(f64::MAX))
        ),
        PERFORMED
    );
    assert_close(value.get(), 1.0);
    let count = changed.borrow().len();
    assert_eq!(
        act(
            &mut view,
            id,
            AccessibilityAction::SetValue(AccessibleValue::Number(f64::NAN))
        ),
        IGNORED
    );
    assert_eq!(
        act(
            &mut view,
            id,
            AccessibilityAction::SetValue(AccessibleValue::Text("wrong type".into()))
        ),
        IGNORED
    );
    assert_eq!(changed.borrow().len(), count);
    click(&mut view, 25.dp(), 10.dp());
    assert_close(value.get(), 0.25);
    assert!(
        changed
            .borrow()
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    );
    Ok(())
}

#[test]
fn disabled_ancestry_disables_ranges_without_hiding_their_state() -> Result<(), TestError> {
    let calls = State::new(0);
    let changes = calls.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.column(Modifier::new().enabled(false), |ui| {
            ui.slider(
                || 0.5,
                None,
                Modifier::new(),
                changes.handler(|count, _| *count += 1),
                |ui, _| {
                    ui.text("Disabled", Modifier::new());
                },
            );
        });
    });
    render(
        &mut view,
        Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE),
        &mut Metrics,
    )?;
    let node = named(&view, "Disabled")?;
    let id = node.id;
    assert!(node.state.semantics.disabled);
    assert_eq!(node.value, Some(AccessibleValue::Number(0.5)));
    assert_eq!(act(&mut view, id, AccessibilityAction::Increment), IGNORED);
    assert_eq!(act(&mut view, id, AccessibilityAction::Focus), IGNORED);
    click(&mut view, 5.dp(), 5.dp());
    key(&mut view, Key::Tab);
    key(&mut view, Key::Right);
    assert_eq!(calls.get(), 0);
    Ok(())
}

#[test]
fn scrollbar_semantics_keyboard_and_assistive_requests_update_the_same_scroll_state()
-> Result<(), TestError> {
    let scroll = ScrollState::new(Axis::Vertical);
    let scroller = scroll.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.row(Modifier::new(), |ui| {
            ui.scroll(
                scroller.clone(),
                Modifier::new().size(100.dp(), 100.dp()),
                |ui| {
                    ui.spacer(100.dp(), 1000.dp(), Modifier::new());
                },
            );
            ui.scrollbar(
                scroller.clone(),
                Modifier::new().accessible_name("Scroll"),
                |ui, _| {
                    ui.spacer(20.dp(), 100.dp(), Modifier::new());
                },
            );
        });
    });
    let size = Size::new(200.dp(), 100.dp());
    for _ in 0..3 {
        render(&mut view, Viewport::new(size, Density::ONE), &mut Metrics)?;
    }
    let node = named(&view, "Scroll")?;
    let id = node.id;
    assert_eq!(node.role, Role::ScrollBar);
    let range = node
        .range
        .ok_or(TestError::NotExported("the range of the scrollbar"))?;
    assert!((range.max - 900.0).abs() < f64::EPSILON);
    assert!(
        view.accessibility()
            .nodes
            .values()
            .any(|n| n.role == Role::ScrollView)
    );
    assert_eq!(act(&mut view, id, AccessibilityAction::Focus), PERFORMED);
    key(&mut view, Key::Down);
    assert_eq!(scroll.offset(), 14.dp(), "a line of the 14 dp text");
    key(&mut view, Key::PageDown);
    assert_eq!(
        scroll.offset(),
        100.dp(),
        "a page is a viewport less a line"
    );
    key(&mut view, Key::PageUp);
    assert_eq!(scroll.offset(), 14.dp());
    assert_eq!(
        act(&mut view, id, AccessibilityAction::Increment),
        PERFORMED
    );
    assert_eq!(scroll.offset(), 28.dp());
    assert_eq!(
        act(
            &mut view,
            id,
            AccessibilityAction::SetValue(AccessibleValue::Number(9999.0))
        ),
        PERFORMED
    );
    assert_eq!(scroll.offset(), 900.dp());
    key(&mut view, Key::Home);
    assert_eq!(scroll.offset(), 0.dp());
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
        &mut Metrics,
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
        render(&mut view, Viewport::new(size, Density::ONE), &mut Metrics)?;
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
