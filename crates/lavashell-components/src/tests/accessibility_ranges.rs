// SPDX-License-Identifier: GPL-3.0-only
use super::*;

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
        render(
            &mut view,
            Viewport::new(size, Density::ONE),
            &mut HalfEmMetrics,
        )?;
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
        Ok(ActionOutcome::Ignored(IgnoredBecause::NotFinite))
    );
    assert_eq!(
        act(
            &mut view,
            id,
            AccessibilityAction::SetValue(AccessibleValue::Text("wrong type".into()))
        ),
        Ok(ActionOutcome::Ignored(IgnoredBecause::TextIntoRange))
    );
    assert_eq!(changed.borrow().len(), count);
    press_and_release_at(&mut view, Point::new(25.dp(), 10.dp()));
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
        &mut HalfEmMetrics,
    )?;
    let node = named(&view, "Disabled")?;
    let id = node.id;
    assert!(node.state.semantics.disabled);
    assert_eq!(node.value, Some(AccessibleValue::Number(0.5)));
    assert_eq!(
        act(&mut view, id, AccessibilityAction::Increment),
        Ok(ActionOutcome::Ignored(IgnoredBecause::Disabled))
    );
    assert_eq!(
        act(&mut view, id, AccessibilityAction::Focus),
        Ok(ActionOutcome::Ignored(IgnoredBecause::Disabled))
    );
    press_and_release_at(&mut view, Point::new(5.dp(), 5.dp()));
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
        render(
            &mut view,
            Viewport::new(size, Density::ONE),
            &mut HalfEmMetrics,
        )?;
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
