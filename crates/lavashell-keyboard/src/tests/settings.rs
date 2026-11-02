// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use icu_locale_core::langid;
use lavashell_core::platform::KeyRepeat;

fn sentences() -> TextHints {
    TextHints {
        capitalization: Capitalization::Sentences,
        ..TextHints::default()
    }
}

fn shown() -> Result<Host, TestError> {
    let mut host = Host::new(settings())?;
    host.start(sentences(), "")?;
    Ok(host)
}

fn press(host: &mut Host, name: &str) -> Result<(), TestError> {
    let at = key(&host.view, name)?;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at,
        kind: PointerKind::Finger,
    }));
    Ok(())
}

fn lift(host: &mut Host, name: &str) -> Result<(), TestError> {
    let at = key(&host.view, name)?;
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at,
        kind: PointerKind::Finger,
    }));
    Ok(())
}

#[test]
fn turning_automatic_capitals_off_on_the_platform_applies_on_the_next_frame()
-> Result<(), TestError> {
    let mut host = shown()?;
    assert!(has(&host.view, &Role::Button, "Q"));
    host.platform(|facts| facts.keyboard.automatic_capitals = false);
    host.frame()?;
    assert!(has(&host.view, &Role::Button, "q"));
    Ok(())
}

#[test]
fn the_languages_of_the_user_choose_the_layouts_and_their_order_on_the_next_frame()
-> Result<(), TestError> {
    let (config, facts) = offering(vec![&ENGLISH, &FRENCH, &GERMAN]);
    let mut host = Host::on(config, lavashell_test_fixtures::facts())?;
    host.start(sentences(), "")?;
    assert!(
        !has(&host.view, &Role::Button, "Switch layout"),
        "one language has no globe key"
    );
    host.platform(|platform| platform.locales = facts.locales);
    host.frame()?;
    tap(&mut host.view, "Switch layout")?;
    host.frame()?;
    assert_eq!(host.layout.get().id(), FRENCH.id());
    host.platform(|platform| platform.locales = vec![langid!("de-AT"), langid!("en")]);
    host.frame()?;
    let space = node(&host.view, &Role::Button, "Space")?;
    let german = Some(AccessibleValue::Text(GERMAN.name().to_string()));
    assert_eq!(
        space.value, german,
        "a layout left out of the languages gives way to the first"
    );
    assert!(has(&host.view, &Role::Button, "Ü"));
    tap(&mut host.view, "Switch layout")?;
    host.frame()?;
    assert_eq!(host.layout.get().id(), ENGLISH.id());
    Ok(())
}

#[test]
fn languages_without_a_layout_offer_every_layout_and_show_the_first_one() -> Result<(), TestError> {
    let (config, mut facts) = offering(vec![&GERMAN, &FRENCH]);
    facts.locales = vec![langid!("ja-JP")];
    let mut host = Host::on(config, facts)?;
    host.start(sentences(), "")?;
    let space = node(&host.view, &Role::Button, "Space")?;
    let german = Some(AccessibleValue::Text(GERMAN.name().to_string()));
    assert_eq!(
        space.value, german,
        "the first layout shows while the layout of the app is not offered"
    );
    tap(&mut host.view, "Switch layout")?;
    host.frame()?;
    assert_eq!(host.layout.get().id(), FRENCH.id());
    Ok(())
}

#[test]
fn turning_the_key_preview_off_on_the_platform_applies_on_the_next_frame() -> Result<(), TestError>
{
    let mut host = shown()?;
    press(&mut host, "Q")?;
    host.frame()?;
    assert_eq!(unions(&host.scene, 2).len(), 1);
    host.platform(|facts| facts.keyboard.key_preview = false);
    host.frame()?;
    run_until(&mut host.view, &mut host.scene, |_, scene| {
        unions(scene, 2).is_empty()
    })?;
    lift(&mut host, "Q")?;
    host.frame()?;
    press(&mut host, "b")?;
    host.frame()?;
    assert!(unions(&host.scene, 2).is_empty());
    Ok(())
}

#[test]
fn changing_the_words_of_the_keys_applies_on_the_next_frame() -> Result<(), TestError> {
    let mut host = shown()?;
    host.settings.update(|settings| {
        settings.text.enter = "Zeilenumbruch".to_string();
        settings.text.keyboard = "Tastatur".to_string();
        settings.text.shift = "Umschalt".to_string();
    });
    host.frame()?;
    for (role, name) in [
        (Role::Button, "Zeilenumbruch"),
        (Role::Group, "Tastatur"),
        (Role::Button, "Umschalt"),
    ] {
        assert!(has(&host.view, &role, name), "a {role:?} named {name}");
    }
    Ok(())
}

#[test]
fn the_long_press_of_the_platform_applies_on_the_next_press() -> Result<(), TestError> {
    let longer = LONG_PRESS * 2;
    let (config, mut facts) = offering(vec![&ENGLISH, &FRENCH]);
    facts.long_press = Duration::from_millis(LONG_PRESS);
    let mut host = Host::on(config, facts)?;
    host.start(sentences(), "")?;
    host.platform(|facts| facts.long_press = Duration::from_millis(longer));
    host.frame()?;
    let start = host.time;
    press(&mut host, "A")?;
    host.at(start + longer - 1)?;
    assert!(!has(&host.view, &Role::Menu, "More letters"));
    host.at(start + longer)?;
    assert!(has(&host.view, &Role::Menu, "More letters"));
    Ok(())
}

#[test]
fn the_key_repeat_of_the_platform_applies_on_the_next_press_and_none_stops_it()
-> Result<(), TestError> {
    let mut host = shown()?;
    let (delay, interval) = (REPEAT_DELAY / 2, REPEAT_INTERVAL * 2);
    let repeat = KeyRepeat::new(
        Duration::from_millis(delay),
        Duration::from_millis(interval),
    );
    host.platform(|facts| facts.key_repeat = repeat);
    host.frame()?;
    let start = host.time;
    press(&mut host, "Delete")?;
    for (after, deleted) in [
        (delay - 1, 1),
        (delay, 2),
        (delay + interval - 1, 2),
        (delay + interval, 3),
    ] {
        host.at(start + after)?;
        assert_eq!(host.sent().len(), deleted, "after {after} ms");
    }
    lift(&mut host, "Delete")?;
    host.platform(|facts| facts.key_repeat = None);
    host.frame()?;
    let sent = host.sent().len();
    let again = host.time;
    press(&mut host, "Delete")?;
    host.at(again + REPEAT_DELAY + REPEAT_INTERVAL)?;
    assert_eq!(
        host.sent().len(),
        sent + 1,
        "a key without repeat deletes once"
    );
    Ok(())
}

#[test]
fn the_double_tap_time_of_the_platform_applies_on_the_next_tap() -> Result<(), TestError> {
    let mut host = shown()?;
    let window = DOUBLE_TAP * 2;
    host.platform(|facts| facts.double_tap = Duration::from_millis(window));
    host.frame()?;
    let shift = key(&host.view, "Shift")?;
    host.view.input(InputEvent::Press(shift));
    host.view.input(InputEvent::Release(shift));
    host.at(host.time + window - 1)?;
    host.view.input(InputEvent::Press(shift));
    host.view.input(InputEvent::Release(shift));
    host.frame()?;
    assert_eq!(
        node(&host.view, &Role::Button, "Shift")?
            .state
            .semantics
            .checked,
        Some(Checked::Mixed),
        "caps lock after a double tap within the window"
    );
    Ok(())
}

#[test]
fn the_panel_height_of_the_settings_sets_the_height_of_the_keys_on_the_next_frame()
-> Result<(), TestError> {
    let mut host = shown()?;
    let height = |host: &Host| -> Result<Dp, TestError> {
        Ok(node(&host.view, &Role::Button, "Q")?.bounds.height)
    };
    assert_eq!(height(&host)?, 76.dp());
    let taller = constant!(NonNegativeDp::try_new(Dp::new(400.0)));
    host.settings
        .update(|settings| settings.panel_height = taller);
    host.settle()?;
    assert_eq!(height(&host)?, 88.5.dp());
    let panel = node(&host.view, &Role::Group, "Keyboard")?.bounds;
    assert_eq!(panel.height, 400.dp());
    assert_eq!(host.keyboard.height(), 400.dp());
    Ok(())
}
