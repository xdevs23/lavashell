// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn checked(view: &View) -> Result<Option<Checked>, TestError> {
    Ok(node(view, &Role::Button, "Shift")?.state.semantics.checked)
}

#[test]
fn every_key_is_a_named_button_that_activates_without_taking_focus() -> Result<(), TestError> {
    let mut fixture = Fixture::new(TextHints::default())?;
    fixture.focus("field")?;
    let panel = node(&fixture.view, &Role::Group, "Keyboard")?;
    let keys: Vec<_> = (fixture.view.accessibility().nodes.values())
        .filter(|node| node.role == Role::Button)
        .collect();
    for letter in ENGLISH.rows().concat().chars() {
        let name = letter.to_string();
        assert!(
            keys.iter().any(|key| key.name == Some(name.clone())),
            "a key for {letter}"
        );
    }
    for key in &keys {
        assert!(key.name.as_ref().is_some_and(|name| !name.is_empty()));
        assert!(key.actions.contains(&ActionKind::Activate));
        assert!(
            !key.actions.contains(&ActionKind::Focus),
            "{:?} takes no focus",
            key.name
        );
        assert!(!key.state.focusable);
    }
    for name in [
        "Shift",
        "Delete",
        "Hide keyboard",
        "Symbols",
        "Space",
        "Enter",
        ".",
    ] {
        assert!(
            has(&fixture.view, &Role::Button, name),
            "a key named {name}"
        );
    }
    assert!(!panel.actions.contains(&ActionKind::Focus));
    let space = node(&fixture.view, &Role::Button, "Space")?;
    assert_eq!(
        space.value,
        Some(AccessibleValue::Text("English".to_string()))
    );
    assert_eq!(
        space.live,
        LiveRegion::Polite,
        "a change of layout is announced"
    );
    let k = node(&fixture.view, &Role::Button, "k")?.id;
    let request = ActionRequest {
        target: k,
        action: AccessibilityAction::Activate,
    };
    assert_eq!(
        fixture.view.accessibility_action(request),
        Ok(ActionOutcome::Performed)
    );
    assert_eq!(fixture.text.get(), "k");
    assert!(focused(&fixture.view, "field"));
    Ok(())
}

#[test]
fn shift_is_checked_for_one_letter_and_mixed_under_caps_lock() -> Result<(), TestError> {
    let mut fixture = Fixture::new(TextHints::default())?;
    fixture.focus("field")?;
    assert_eq!(checked(&fixture.view)?, Some(Checked::False));
    fixture.type_keys(&["Shift"])?;
    assert_eq!(checked(&fixture.view)?, Some(Checked::True));
    fixture.type_keys(&["Shift"])?;
    assert_eq!(checked(&fixture.view)?, Some(Checked::Mixed));
    fixture.type_keys(&["Shift"])?;
    assert_eq!(checked(&fixture.view)?, Some(Checked::False));
    Ok(())
}

#[test]
fn the_letters_of_a_long_press_are_a_menu_whose_items_activate() -> Result<(), TestError> {
    let (config, facts) = offering(vec![&ENGLISH, &FRENCH]);
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    let e = key(&host.view, "e")?;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: e,
        kind: PointerKind::Finger,
    }));
    host.at(host.time + LONG_PRESS)?;
    let items = (host.view.accessibility().nodes.values())
        .filter(|node| node.role == Role::MenuItem)
        .count();
    assert_eq!(
        Some(items),
        FRENCH
            .further_letters_of('e')
            .map(|letters| letters.chars().count())
    );
    let item = node(&host.view, &Role::MenuItem, "ê")?.id;
    let request = ActionRequest {
        target: item,
        action: AccessibilityAction::Activate,
    };
    assert_eq!(
        host.view.accessibility_action(request),
        Ok(ActionOutcome::Performed)
    );
    host.frame()?;
    assert!(!has(&host.view, &Role::Menu, "More letters"));
    assert_eq!(host.sent(), [Host::commit("ê")]);
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: e,
        kind: PointerKind::Finger,
    }));
    assert_eq!(host.sent().len(), 1, "the lifted finger types nothing more");
    Ok(())
}
