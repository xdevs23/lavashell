// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn capitals(capitalization: Capitalization) -> TextHints {
    TextHints {
        capitalization,
        ..TextHints::default()
    }
}

fn shows_capitals(host: &Host) -> bool {
    let capital = has(&host.view, &Role::Button, "Q");
    assert_ne!(capital, has(&host.view, &Role::Button, "q"));
    capital
}

#[test]
fn shift_capitalizes_one_letter_and_a_double_tap_locks_capitals_until_the_next_tap()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.start(TextHints::default(), "")?;
    tap(&mut host.view, "Shift")?;
    host.frame()?;
    tap(&mut host.view, "H")?;
    host.frame()?;
    tap(&mut host.view, "i")?;
    host.frame()?;
    tap(&mut host.view, "Shift")?;
    tap(&mut host.view, "Shift")?;
    host.frame()?;
    for name in ["O", "K"] {
        tap(&mut host.view, name)?;
        host.frame()?;
    }
    tap(&mut host.view, "Shift")?;
    host.frame()?;
    tap(&mut host.view, "x")?;
    let typed: Vec<_> = ["H", "i", "O", "K", "x"].map(Host::commit).into();
    assert_eq!(host.sent(), typed);
    Ok(())
}

#[test]
fn a_second_tap_after_the_double_tap_window_turns_shift_off() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.start(TextHints::default(), "")?;
    let shift = key(&host.view, "Shift")?;
    host.view.input(InputEvent::Press(shift));
    host.view.input(InputEvent::Release(shift));
    host.at(host.time + DOUBLE_TAP - 1)?;
    host.view.input(InputEvent::Press(shift));
    host.view.input(InputEvent::Release(shift));
    host.frame()?;
    assert!(
        has(&host.view, &Role::Button, "Q"),
        "caps lock after a double tap"
    );
    tap(&mut host.view, "Shift")?;
    host.at(host.time + DOUBLE_TAP)?;
    tap(&mut host.view, "Shift")?;
    host.frame()?;
    tap(&mut host.view, "A")?;
    host.frame()?;
    tap(&mut host.view, "b")?;
    assert_eq!(host.sent(), [Host::commit("A"), Host::commit("b")]);
    Ok(())
}

#[test]
fn a_shift_tapped_by_hand_lasts_over_space_delete_and_punctuation_until_a_letter()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.start(TextHints::default(), "hello")?;
    tap(&mut host.view, "Shift")?;
    host.frame()?;
    tap(&mut host.view, "Space")?;
    host.before_cursor("hello ")?;
    tap(&mut host.view, "Delete")?;
    host.before_cursor("hello")?;
    tap(&mut host.view, ".")?;
    host.before_cursor("hello.")?;
    tap(&mut host.view, "W")?;
    host.frame()?;
    tap(&mut host.view, "o")?;
    let sent = [
        Host::commit(" "),
        TextCommand::DeleteBackward,
        Host::commit("."),
        Host::commit("W"),
        Host::commit("o"),
    ];
    assert_eq!(host.sent(), sent);
    Ok(())
}

#[test]
fn automatic_capitals_give_sentences_and_words_one_capital_at_the_start_of_the_text()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    for capitalization in [Capitalization::Sentences, Capitalization::Words] {
        for (text, capital) in [
            ("", true),
            ("   ", true),
            ("\n ", true),
            ("Hello", false),
            ("Hello ", false),
            ("Hello. ", false),
            ("Really?  ", false),
            ("z. B. ", false),
        ] {
            host.start(capitals(capitalization), text)?;
            assert_eq!(
                shows_capitals(&host),
                capital,
                "{capitalization:?} after {text:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn automatic_capitals_give_characters_a_capital_at_every_letter_wherever_the_cursor_is()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    let characters = capitals(Capitalization::Characters);
    for text in ["", "ABC", "ABC. "] {
        host.start(characters, text)?;
        assert!(shows_capitals(&host), "after {text:?}");
    }
    host.start(capitals(Capitalization::None), "")?;
    assert!(!shows_capitals(&host));
    Ok(())
}

#[test]
fn without_automatic_capitals_shift_never_turns_on_by_itself() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.platform(|facts| facts.keyboard.automatic_capitals = false);
    for capitalization in [
        Capitalization::Sentences,
        Capitalization::Words,
        Capitalization::Characters,
    ] {
        host.start(capitals(capitalization), "")?;
        assert!(!shows_capitals(&host), "{capitalization:?}");
    }
    Ok(())
}

#[test]
fn an_automatic_capital_lasts_one_letter_even_before_the_host_reports_the_text()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.start(capitals(Capitalization::Sentences), "")?;
    tap(&mut host.view, "Q")?;
    host.frame()?;
    assert!(has(&host.view, &Role::Button, "q"));
    host.before_cursor("Q")?;
    tap(&mut host.view, "u")?;
    host.before_cursor("")?;
    assert!(
        has(&host.view, &Role::Button, "Q"),
        "the text is empty again"
    );
    assert_eq!(host.sent(), [Host::commit("Q"), Host::commit("u")]);
    Ok(())
}

#[test]
fn the_preview_of_a_key_pressed_while_another_is_held_shows_the_letter_it_types()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.start(capitals(Capitalization::Sentences), "")?;
    let (h, i) = (key(&host.view, "H")?, key(&host.view, "I")?);
    host.view.input(InputEvent::TouchDown(finger(1, h)));
    host.view.input(InputEvent::TouchDown(finger(2, i)));
    host.frame()?;
    assert!(texts(&host.scene).contains(&"i".to_string()));
    host.view.input(InputEvent::TouchUp(finger(2, i)));
    assert_eq!(host.sent(), [Host::commit("H"), Host::commit("i")]);
    Ok(())
}

#[test]
fn the_preview_of_a_letter_pressed_while_shift_is_held_shows_its_capital() -> Result<(), TestError>
{
    let mut host = Host::new(settings())?;
    host.start(TextHints::default(), "")?;
    let (shift, q) = (key(&host.view, "Shift")?, key(&host.view, "q")?);
    host.view.input(InputEvent::TouchDown(finger(1, shift)));
    host.view.input(InputEvent::TouchDown(finger(2, q)));
    host.frame()?;
    assert!(texts(&host.scene).contains(&"Q".to_string()));
    host.view.input(InputEvent::TouchUp(finger(2, q)));
    assert_eq!(host.sent(), [Host::commit("Q")]);
    Ok(())
}

#[test]
fn the_preview_of_a_letter_pressed_while_a_second_tap_on_shift_is_held_shows_caps_lock()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.start(TextHints::default(), "x")?;
    tap(&mut host.view, "Shift")?;
    host.frame()?;
    let (shift, q) = (key(&host.view, "Shift")?, key(&host.view, "Q")?);
    host.view.input(InputEvent::TouchDown(finger(1, shift)));
    host.view.input(InputEvent::TouchDown(finger(2, q)));
    host.frame()?;
    assert!(texts(&host.scene).contains(&"Q".to_string()));
    host.view.input(InputEvent::TouchUp(finger(2, q)));
    host.view.input(InputEvent::TouchUp(finger(1, shift)));
    host.frame()?;
    assert_eq!(host.sent(), [Host::commit("Q")]);
    assert!(has(&host.view, &Role::Button, "Q"), "caps lock is on");
    tap(&mut host.view, "Q")?;
    assert_eq!(host.sent(), [Host::commit("Q"), Host::commit("Q")]);
    Ok(())
}

#[test]
fn shift_leaves_a_letter_without_a_single_capital_as_it_is() -> Result<(), TestError> {
    let (config, facts) = offering(vec![&GERMAN]);
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    tap(&mut host.view, "Shift")?;
    host.frame()?;
    let s = key(&host.view, "S")?;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: s,
        kind: PointerKind::Finger,
    }));
    host.at(host.time + LONG_PRESS)?;
    assert!(has(&host.view, &Role::MenuItem, "ß"));
    let sharp = node(&host.view, &Role::MenuItem, "ß")?.bounds.center();
    host.view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: sharp,
        kind: PointerKind::Finger,
    }));
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: sharp,
        kind: PointerKind::Finger,
    }));
    assert_eq!(host.sent(), [Host::commit("ß")]);
    Ok(())
}

#[test]
fn a_protected_field_gets_no_automatic_capitals_and_no_preview() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    let hints = capitals(Capitalization::Sentences);
    host.session
        .set(Some(session(SessionId(1), hints, FieldText::Protected)));
    host.settle()?;
    let at = key(&host.view, "q")?;
    host.view.input(InputEvent::Press(at));
    host.frame()?;
    assert!(unions(&host.scene, 2).is_empty());
    host.view.input(InputEvent::Release(at));
    assert_eq!(host.sent(), [Host::commit("q")]);
    Ok(())
}

#[test]
fn the_page_keys_switch_pages_and_each_field_starts_on_the_page_for_its_purpose()
-> Result<(), TestError> {
    let mut fixture = Fixture::new(TextHints {
        purpose: TextPurpose::Number,
        ..TextHints::default()
    })?;
    fixture.focus("field")?;
    fixture.type_keys(&["4", "2", "More symbols", "π", "Symbols", "Letters", "x"])?;
    assert_eq!(fixture.text.get(), "42πx");
    fixture.focus("second")?;
    assert!(
        has(&fixture.view, &Role::Button, "q"),
        "a text field starts on the letters"
    );
    fixture.focus("field")?;
    assert!(
        has(&fixture.view, &Role::Button, "1"),
        "a number field starts on the digits"
    );
    Ok(())
}

#[test]
fn a_tap_into_the_focused_field_does_not_end_caps_lock_or_change_the_page() -> Result<(), TestError>
{
    let mut fixture = Fixture::new(TextHints::default())?;
    fixture.focus("field")?;
    fixture.type_keys(&["Shift", "Shift"])?;
    fixture.focus("field")?;
    assert!(has(&fixture.view, &Role::Button, "Q"), "caps lock lasts");
    fixture.type_keys(&["Symbols"])?;
    fixture.focus("field")?;
    assert!(has(&fixture.view, &Role::Button, "1"), "the page lasts");
    Ok(())
}

#[test]
fn the_globe_key_goes_through_the_layouts_in_their_order_and_back_to_the_first()
-> Result<(), TestError> {
    let (config, facts) = offering(vec![&ENGLISH, &GERMAN, &PORTUGUESE]);
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    let mut names = Vec::new();
    for _ in 0..4 {
        let space = node(&host.view, &Role::Button, "Space")?;
        names.push(space.value.clone());
        tap(&mut host.view, "Switch layout")?;
        host.frame()?;
    }
    let name = |layout: &Layout| Some(AccessibleValue::Text(layout.name().to_string()));
    assert_eq!(names, [&ENGLISH, &GERMAN, &PORTUGUESE, &ENGLISH].map(name));
    assert_eq!(host.layout.get().id(), GERMAN.id());
    Ok(())
}

#[test]
fn the_hide_key_hides_the_keyboard_until_the_field_is_pressed_again() -> Result<(), TestError> {
    let mut fixture = Fixture::new(TextHints::default())?;
    assert!(!fixture.keyboard.shown());
    fixture.focus("field")?;
    assert!(fixture.keyboard.shown());
    tap(&mut fixture.view, "Hide keyboard")?;
    fixture.settle()?;
    assert!(!fixture.keyboard.shown());
    assert!(!has(&fixture.view, &Role::Button, "q"));
    assert!(focused(&fixture.view, "field"));
    fixture.focus("field")?;
    assert!(fixture.keyboard.shown());
    let outside = Point::new(1000.dp(), 50.dp());
    fixture.view.input(InputEvent::Press(outside));
    fixture.view.input(InputEvent::Release(outside));
    fixture.settle()?;
    assert!(!fixture.keyboard.shown());
    Ok(())
}
