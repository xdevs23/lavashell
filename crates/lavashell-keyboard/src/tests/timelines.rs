// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::look;
use crate::press::LINGER;
use icu_locale_core::langid;

fn previews(host: &mut Host, time: Duration) -> Result<Vec<Dp>, TestError> {
    host.at(time)?;
    let found = unions(&host.scene, 2);
    Ok(found.iter().map(|bounds| bounds.height).collect())
}

#[test]
fn a_preview_opens_with_the_press_is_open_after_the_lift_then_shrinks_to_the_key()
-> Result<(), TestError> {
    let mut host = Host::new(settings()?)?;
    host.start(TextHints::default(), "")?;
    let key = node(&host.view, &Role::Button, "y")?.bounds;
    let full = key.height + look::BUBBLE_HEIGHT;
    let half_opened = full * f64::midpoint(look::PREVIEW_OPENED_FROM.get(), 1.0);
    let half_closed = full * f64::midpoint(1.0, look::PREVIEW_CLOSED_AT.get());
    let opening_duration = look::preview_opening().duration;
    let closing_duration = look::preview_closing().duration;
    let start = host.time;
    host.view
        .input(InputEvent::TouchDown(finger(1, key.center())));
    assert!(
        near_length(
            previews(&mut host, start)?[0],
            full * look::PREVIEW_OPENED_FROM.get()
        ),
        "it opens from its smaller scale"
    );
    let opening = previews(&mut host, start + opening_duration / 2)?[0];
    assert!(
        opening > half_opened && opening < full,
        "it decelerates: {opening:?}"
    );
    let opened = start + opening_duration;
    assert!(near_length(previews(&mut host, opened)?[0], full));
    let bounds = unions(&host.scene, 2)[0];
    assert!(
        near_length(bounds.y + bounds.height, key.y + key.height),
        "it grows out of its key"
    );
    let lift = opened + millis(40);
    host.at(lift)?;
    host.view
        .input(InputEvent::TouchUp(finger(1, key.center())));
    let close_begins = lift + LINGER;
    assert!(
        near_length(previews(&mut host, just_before(close_begins)?)?[0], full),
        "it is still open after the lift"
    );
    assert!(near_length(previews(&mut host, close_begins)?[0], full));
    let closing = previews(&mut host, close_begins + closing_duration / 2)?[0];
    assert!(
        closing > half_closed && closing < full,
        "it accelerates: {closing:?}"
    );
    let bounds = unions(&host.scene, 2)[0];
    assert!(
        near_length(bounds.y + bounds.height, key.y + key.height),
        "it shrinks toward its key"
    );
    let close_ends = close_begins + closing_duration;
    assert!(previews(&mut host, just_before(close_ends)?)?[0] < half_closed);
    assert!(
        previews(&mut host, close_ends)?.is_empty(),
        "it disappears once it has closed"
    );
    Ok(())
}

#[test]
fn a_preview_removed_in_the_first_frame_after_its_release_closes_with_the_closing_motion()
-> Result<(), TestError> {
    let mut host = Host::new(settings()?)?;
    host.start(TextHints::default(), "")?;
    let key = node(&host.view, &Role::Button, "y")?.bounds;
    let full = key.height + look::BUBBLE_HEIGHT;
    let half_closed = full * f64::midpoint(1.0, look::PREVIEW_CLOSED_AT.get());
    let closing_duration = look::preview_closing().duration;
    let start = host.time;
    host.view
        .input(InputEvent::TouchDown(finger(1, key.center())));
    host.at(start)?;
    let opened = start + look::preview_opening().duration;
    host.at(opened)?;
    host.view
        .input(InputEvent::TouchUp(finger(1, key.center())));
    let close_begins = opened + LINGER;
    assert!(near_length(previews(&mut host, close_begins)?[0], full));
    let closing = previews(&mut host, close_begins + closing_duration / 2)?[0];
    assert!(
        closing > half_closed && closing < full,
        "it accelerates: {closing:?}"
    );
    assert!(previews(&mut host, close_begins + closing_duration)?.is_empty());
    Ok(())
}

#[test]
fn each_preview_runs_independently_and_a_later_one_shows_on_top() -> Result<(), TestError> {
    let mut host = Host::new(settings()?)?;
    host.start(TextHints::default(), "")?;
    let (y, u) = (key(&host.view, "y")?, key(&host.view, "u")?);
    let start = host.time;
    host.view.input(InputEvent::TouchDown(finger(1, y)));
    let lift = start + millis(40);
    host.at(lift)?;
    host.view.input(InputEvent::TouchUp(finger(1, y)));
    let u_pressed = lift + millis(20);
    host.at(u_pressed)?;
    host.view.input(InputEvent::TouchDown(finger(2, u)));
    let y_close_begins = lift + LINGER;
    host.at(y_close_begins)?;
    let painted = unions(&host.scene, 2);
    assert_eq!(painted.len(), 2);
    assert!(
        painted[0].x < painted[1].x,
        "the preview of u is painted last"
    );
    let y_close_ends = y_close_begins + look::preview_closing().duration;
    assert_eq!(
        previews(&mut host, just_before(y_close_ends)?)?.len(),
        2,
        "y is shown until it has closed"
    );
    let u_opened = u_pressed + look::preview_opening().duration;
    let remaining = previews(&mut host, y_close_ends.max(u_opened))?;
    assert_eq!(remaining.len(), 1);
    assert!(near_length(
        remaining[0],
        node(&host.view, &Role::Button, "u")?.bounds.height + look::BUBBLE_HEIGHT
    ));
    Ok(())
}

#[test]
fn under_reduced_motion_a_preview_opens_and_closes_without_scaling() -> Result<(), TestError> {
    let mut host = Host::new(settings()?)?;
    host.view.provide(&REDUCED_MOTION, true);
    host.start(TextHints::default(), "")?;
    let key = node(&host.view, &Role::Button, "y")?.bounds;
    let full = key.height + look::BUBBLE_HEIGHT;
    let start = host.time;
    host.view
        .input(InputEvent::TouchDown(finger(1, key.center())));
    assert!(near_length(previews(&mut host, start)?[0], full));
    host.view
        .input(InputEvent::TouchUp(finger(1, key.center())));
    assert!(near_length(
        previews(&mut host, just_before(start + LINGER)?)?[0],
        full
    ));
    assert!(previews(&mut host, start + LINGER)?.is_empty());
    Ok(())
}

#[test]
fn a_held_delete_key_repeats_until_it_is_released_or_cancelled() -> Result<(), TestError> {
    let mut host = Host::new(settings()?)?;
    host.start(TextHints::default(), "")?;
    let delete = key(&host.view, "Delete")?;
    let count = |host: &Host| host.sent().len();
    let start = host.time;
    host.view.input(InputEvent::TouchDown(finger(1, delete)));
    assert_eq!(count(&host), 1, "it deletes when pressed");
    host.at(just_before(start + REPEAT_DELAY)?)?;
    assert_eq!(count(&host), 1);
    host.at(start + REPEAT_DELAY)?;
    assert_eq!(count(&host), 2);
    host.at(start + REPEAT_DELAY + REPEAT_INTERVAL)?;
    host.at(start + REPEAT_DELAY + 2 * REPEAT_INTERVAL)?;
    assert_eq!(count(&host), 4, "then once in every interval");
    host.at(start + REPEAT_DELAY + 10 * REPEAT_INTERVAL)?;
    assert_eq!(count(&host), 5, "a late frame deletes once");
    host.view.input(InputEvent::TouchUp(finger(1, delete)));
    host.at(start + 2 * REPEAT_DELAY + 10 * REPEAT_INTERVAL)?;
    assert_eq!(count(&host), 5);
    let again = host.time;
    host.view.input(InputEvent::TouchDown(finger(1, delete)));
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
    let portuguese = &shipped()?.portuguese;
    let chosen = (portuguese.further_letters_of('e'))
        .and_then(|letters| letters.chars().nth(1))
        .ok_or(TestError::FewFurtherLetters {
            letter: 'e',
            count: 2,
        })?
        .to_string();
    let (config, facts) = offering(vec![portuguese])?;
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    let e = key(&host.view, "e")?;
    let start = host.time;
    host.view.input(InputEvent::TouchDown(finger(1, e)));
    host.at(just_before(start + LONG_PRESS)?)?;
    assert!(!has(&host.view, &Role::Menu, "More letters"));
    host.at(start + LONG_PRESS)?;
    assert!(has(&host.view, &Role::Menu, "More letters"));
    host.at(start + millis(400))?;
    assert!(
        unions(&host.scene, 2).is_empty(),
        "the popup replaces the preview"
    );
    let second = node(&host.view, &Role::MenuItem, &chosen)?.bounds.center();
    host.view.input(InputEvent::TouchMove(finger(1, second)));
    host.view.input(InputEvent::TouchUp(finger(1, second)));
    host.frame()?;
    assert!(!has(&host.view, &Role::Menu, "More letters"));
    assert_eq!(host.sent(), [Host::commit(&chosen)]);
    Ok(())
}

#[test]
fn a_long_press_on_the_globe_lists_the_layouts_to_choose_by_sliding_or_by_a_tap()
-> Result<(), TestError> {
    let (config, facts) = offering(vec![
        &shipped()?.english,
        &shipped()?.german,
        &shipped()?.portuguese,
    ])?;
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    let globe = key(&host.view, "Switch layout")?;
    host.view.input(InputEvent::TouchDown(finger(1, globe)));
    host.at(host.time + LONG_PRESS)?;
    let portuguese = node(&host.view, &Role::MenuItem, shipped()?.portuguese.name())?
        .bounds
        .center();
    host.view
        .input(InputEvent::TouchMove(finger(1, portuguese)));
    host.view.input(InputEvent::TouchUp(finger(1, portuguese)));
    host.frame()?;
    assert_eq!(host.layout.get().id(), shipped()?.portuguese.id());
    assert!(!has(&host.view, &Role::Menu, "Keyboard layouts"));
    host.view.input(InputEvent::TouchDown(finger(1, globe)));
    host.at(host.time + LONG_PRESS)?;
    host.view.input(InputEvent::TouchUp(finger(1, globe)));
    host.frame()?;
    assert!(
        has(&host.view, &Role::Menu, "Keyboard layouts"),
        "the list is open for a tap"
    );
    let current = node(&host.view, &Role::MenuItem, shipped()?.portuguese.name())?;
    assert_eq!(current.state.semantics.checked, Some(Checked::True));
    let deutsch = node(&host.view, &Role::MenuItem, shipped()?.german.name())?
        .bounds
        .center();
    host.view.input(InputEvent::Press(deutsch));
    host.view.input(InputEvent::Release(deutsch));
    host.frame()?;
    assert_eq!(host.layout.get().id(), shipped()?.german.id());
    assert!(!has(&host.view, &Role::Menu, "Keyboard layouts"));
    assert!(host.sent().is_empty());
    Ok(())
}

fn open_list() -> Result<Host, TestError> {
    let (config, facts) = offering(vec![
        &shipped()?.english,
        &shipped()?.german,
        &shipped()?.portuguese,
    ])?;
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    let globe = key(&host.view, "Switch layout")?;
    host.view.input(InputEvent::TouchDown(finger(1, globe)));
    host.at(host.time + LONG_PRESS)?;
    host.view.input(InputEvent::TouchUp(finger(1, globe)));
    host.frame()?;
    Ok(host)
}

#[test]
fn a_tap_on_a_layout_chooses_it_only_when_it_ends_on_that_layout() -> Result<(), TestError> {
    let mut host = open_list()?;
    let deutsch = node(&host.view, &Role::MenuItem, shipped()?.german.name())?
        .bounds
        .center();
    let above = Point::new(deutsch.x, 10.dp());
    host.view.input(InputEvent::Press(deutsch));
    host.view.input(InputEvent::Move(above));
    host.view.input(InputEvent::Release(above));
    host.frame()?;
    assert_eq!(host.layout.get().id(), shipped()?.english.id());
    assert!(has(&host.view, &Role::Menu, "Keyboard layouts"));
    Ok(())
}

#[test]
fn the_open_list_of_layouts_follows_the_languages_of_the_user() -> Result<(), TestError> {
    let mut host = open_list()?;
    let shipped = shipped()?;
    host.settings
        .update(|settings| settings.layouts = vec![&shipped.english, &shipped.french]);
    host.platform(|facts| facts.locales = vec![langid!("en"), langid!("fr")]);
    host.frame()?;
    assert!(has(&host.view, &Role::MenuItem, shipped.french.name()));
    assert!(!has(&host.view, &Role::MenuItem, shipped.german.name()));
    Ok(())
}

#[test]
fn a_finger_that_slides_off_the_letters_of_a_long_press_types_none() -> Result<(), TestError> {
    let (config, facts) = offering(vec![&shipped()?.portuguese])?;
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    let e = key(&host.view, "e")?;
    host.view.input(InputEvent::TouchDown(finger(1, e)));
    host.at(host.time + LONG_PRESS)?;
    let away = Point::new(e.x, e.y + 200.dp());
    host.view.input(InputEvent::TouchMove(finger(1, away)));
    host.view.input(InputEvent::TouchUp(finger(1, away)));
    host.frame()?;
    assert!(!has(&host.view, &Role::Menu, "More letters"));
    assert!(host.sent().is_empty());
    Ok(())
}
