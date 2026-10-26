// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use icu_locale_core::langid;

fn previews(host: &mut Host, milliseconds: u64) -> Result<Vec<Dp>, TestError> {
    host.at(milliseconds)?;
    let found = unions(&host.scene, 2);
    Ok(found.iter().map(|bounds| bounds.height).collect())
}

#[test]
fn a_preview_opens_with_the_press_is_open_after_the_lift_then_shrinks_to_the_key()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.start(TextHints::default(), "")?;
    let key = node(&host.view, &Role::Button, "y")?.bounds;
    let full = key.height + 100.dp();
    let start = host.time;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: key.center(),
        kind: PointerKind::Finger,
    }));
    assert!(
        near_length(previews(&mut host, start)?[0], full * 0.9),
        "it opens from 90 %"
    );
    let opening = previews(&mut host, start + 30)?[0];
    assert!(
        opening > full * 0.95 && opening < full,
        "it decelerates: {opening:?}"
    );
    assert!(near_length(previews(&mut host, start + 60)?[0], full));
    let bounds = unions(&host.scene, 2)[0];
    assert!(
        near_length(bounds.y + bounds.height, key.y + key.height),
        "it grows out of its key"
    );
    let lift = start + 100;
    host.at(lift)?;
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: key.center(),
        kind: PointerKind::Finger,
    }));
    assert!(
        near_length(previews(&mut host, lift + 79)?[0], full),
        "it is still open after the lift"
    );
    assert!(near_length(previews(&mut host, lift + 80)?[0], full));
    let closing = previews(&mut host, lift + 130)?[0];
    assert!(
        closing > full * 0.95 && closing < full,
        "it accelerates: {closing:?}"
    );
    let bounds = unions(&host.scene, 2)[0];
    assert!(
        near_length(bounds.y + bounds.height, key.y + key.height),
        "it shrinks toward its key"
    );
    assert!(previews(&mut host, lift + 179)?[0] < full * 0.95);
    assert!(
        previews(&mut host, lift + 180)?.is_empty(),
        "it disappears at 90 %"
    );
    Ok(())
}

#[test]
fn each_preview_runs_independently_and_a_later_one_shows_on_top() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.start(TextHints::default(), "")?;
    let (y, u) = (key(&host.view, "y")?, key(&host.view, "u")?);
    let start = host.time;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: y,
        kind: PointerKind::Finger,
    }));
    host.at(start + 40)?;
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: y,
        kind: PointerKind::Finger,
    }));
    host.at(start + 60)?;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(2),
        at: u,
        kind: PointerKind::Finger,
    }));
    host.at(start + 80)?;
    let painted = unions(&host.scene, 2);
    assert_eq!(painted.len(), 2);
    assert!(
        painted[0].x < painted[1].x,
        "the preview of u is painted last"
    );
    assert_eq!(
        previews(&mut host, start + 140)?.len(),
        2,
        "y closes 80 ms after it was released"
    );
    let left = previews(&mut host, start + 250)?;
    assert_eq!(left.len(), 1);
    assert!(near_length(
        left[0],
        node(&host.view, &Role::Button, "u")?.bounds.height + 100.dp()
    ));
    Ok(())
}

#[test]
fn under_reduced_motion_a_preview_opens_and_closes_without_scaling() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.view.provide(&REDUCED_MOTION, true);
    host.start(TextHints::default(), "")?;
    let key = node(&host.view, &Role::Button, "y")?.bounds;
    let full = key.height + 100.dp();
    let start = host.time;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: key.center(),
        kind: PointerKind::Finger,
    }));
    assert!(near_length(previews(&mut host, start)?[0], full));
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: key.center(),
        kind: PointerKind::Finger,
    }));
    assert!(near_length(previews(&mut host, start + 79)?[0], full));
    assert!(previews(&mut host, start + 80)?.is_empty());
    Ok(())
}

#[test]
fn a_held_delete_key_repeats_until_it_is_released_or_cancelled() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.start(TextHints::default(), "")?;
    let delete = key(&host.view, "Delete")?;
    let count = |host: &Host| host.sent().len();
    let start = host.time;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: delete,
        kind: PointerKind::Finger,
    }));
    assert_eq!(count(&host), 1, "it deletes when pressed");
    host.at(start + REPEAT_DELAY - 1)?;
    assert_eq!(count(&host), 1);
    host.at(start + REPEAT_DELAY)?;
    assert_eq!(count(&host), 2);
    host.at(start + REPEAT_DELAY + REPEAT_INTERVAL)?;
    host.at(start + REPEAT_DELAY + 2 * REPEAT_INTERVAL)?;
    assert_eq!(count(&host), 4, "then once in every interval");
    host.at(start + REPEAT_DELAY + 10 * REPEAT_INTERVAL)?;
    assert_eq!(count(&host), 5, "a late frame deletes once");
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: delete,
        kind: PointerKind::Finger,
    }));
    host.at(start + 2 * REPEAT_DELAY + 10 * REPEAT_INTERVAL)?;
    assert_eq!(count(&host), 5);
    let again = host.time;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: delete,
        kind: PointerKind::Finger,
    }));
    host.view.input(InputEvent::TouchCancel(touch(1)));
    host.at(again + 2 * REPEAT_DELAY)?;
    assert_eq!(count(&host), 6);
    assert!(
        host.sent()
            .iter()
            .all(|sent| *sent == TextCommand::DeleteBackward)
    );
    Ok(())
}

#[test]
fn a_long_press_offers_more_letters_and_sliding_chooses_one() -> Result<(), TestError> {
    let (config, facts) = offering(vec![&PORTUGUESE]);
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    let e = key(&host.view, "e")?;
    let start = host.time;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: e,
        kind: PointerKind::Finger,
    }));
    host.at(start + LONG_PRESS - 1)?;
    assert!(!has(&host.view, &Role::Menu, "More letters"));
    host.at(start + LONG_PRESS)?;
    assert!(has(&host.view, &Role::Menu, "More letters"));
    host.at(start + 400)?;
    assert!(
        unions(&host.scene, 2).is_empty(),
        "the popup replaces the preview"
    );
    let second = node(&host.view, &Role::MenuItem, "é")?.bounds.center();
    host.view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: second,
        kind: PointerKind::Finger,
    }));
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: second,
        kind: PointerKind::Finger,
    }));
    host.frame()?;
    assert!(!has(&host.view, &Role::Menu, "More letters"));
    assert_eq!(host.sent(), [Host::commit("é")]);
    Ok(())
}

#[test]
fn a_long_press_on_the_globe_lists_the_layouts_to_choose_by_sliding_or_by_a_tap()
-> Result<(), TestError> {
    let (config, facts) = offering(vec![&ENGLISH, &GERMAN, &PORTUGUESE]);
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    let globe = key(&host.view, "Switch layout")?;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: globe,
        kind: PointerKind::Finger,
    }));
    host.at(host.time + LONG_PRESS)?;
    let portuguese = node(&host.view, &Role::MenuItem, "Português")?
        .bounds
        .center();
    host.view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: portuguese,
        kind: PointerKind::Finger,
    }));
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: portuguese,
        kind: PointerKind::Finger,
    }));
    host.frame()?;
    assert_eq!(host.layout.get().id(), PORTUGUESE.id());
    assert!(!has(&host.view, &Role::Menu, "Keyboard layouts"));
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: globe,
        kind: PointerKind::Finger,
    }));
    host.at(host.time + LONG_PRESS)?;
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: globe,
        kind: PointerKind::Finger,
    }));
    host.frame()?;
    assert!(
        has(&host.view, &Role::Menu, "Keyboard layouts"),
        "the list is open for a tap"
    );
    let current = node(&host.view, &Role::MenuItem, "Português")?;
    assert_eq!(current.state.semantics.checked, Some(Checked::True));
    let deutsch = node(&host.view, &Role::MenuItem, "Deutsch")?
        .bounds
        .center();
    host.view.input(InputEvent::Press(deutsch));
    host.view.input(InputEvent::Release(deutsch));
    host.frame()?;
    assert_eq!(host.layout.get().id(), GERMAN.id());
    assert!(!has(&host.view, &Role::Menu, "Keyboard layouts"));
    assert!(host.sent().is_empty());
    Ok(())
}

fn open_list() -> Result<Host, TestError> {
    let (config, facts) = offering(vec![&ENGLISH, &GERMAN, &PORTUGUESE]);
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    let globe = key(&host.view, "Switch layout")?;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: globe,
        kind: PointerKind::Finger,
    }));
    host.at(host.time + LONG_PRESS)?;
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: globe,
        kind: PointerKind::Finger,
    }));
    host.frame()?;
    Ok(host)
}

#[test]
fn a_tap_on_a_layout_chooses_it_only_when_it_ends_on_that_layout() -> Result<(), TestError> {
    let mut host = open_list()?;
    let deutsch = node(&host.view, &Role::MenuItem, "Deutsch")?
        .bounds
        .center();
    let above = Point::new(deutsch.x, 10.dp());
    host.view.input(InputEvent::Press(deutsch));
    host.view.input(InputEvent::Move(above));
    host.view.input(InputEvent::Release(above));
    host.frame()?;
    assert_eq!(host.layout.get().id(), ENGLISH.id());
    assert!(has(&host.view, &Role::Menu, "Keyboard layouts"));
    Ok(())
}

#[test]
fn the_open_list_of_layouts_follows_the_languages_of_the_user() -> Result<(), TestError> {
    let mut host = open_list()?;
    host.settings
        .update(|settings| settings.layouts = vec![&ENGLISH, &FRENCH]);
    host.platform(|facts| facts.locales = vec![langid!("en"), langid!("fr")]);
    host.frame()?;
    assert!(has(&host.view, &Role::MenuItem, "Français"));
    assert!(!has(&host.view, &Role::MenuItem, "Deutsch"));
    Ok(())
}

#[test]
fn a_finger_that_slides_off_the_letters_of_a_long_press_types_none() -> Result<(), TestError> {
    let (config, facts) = offering(vec![&PORTUGUESE]);
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    let e = key(&host.view, "e")?;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: e,
        kind: PointerKind::Finger,
    }));
    host.at(host.time + LONG_PRESS)?;
    let away = Point::new(e.x, e.y + 200.dp());
    host.view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: away,
        kind: PointerKind::Finger,
    }));
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: away,
        kind: PointerKind::Finger,
    }));
    host.frame()?;
    assert!(!has(&host.view, &Role::Menu, "More letters"));
    assert!(host.sent().is_empty());
    Ok(())
}
