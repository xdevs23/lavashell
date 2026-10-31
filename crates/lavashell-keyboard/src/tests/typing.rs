// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn acting(action: TextAction) -> TextHints {
    TextHints {
        action: Some(action),
        ..TextHints::default()
    }
}

#[test]
fn keys_type_into_the_focused_field_of_the_view_and_never_take_its_focus() -> Result<(), TestError>
{
    let mut fixture = Fixture::new(TextHints::default())?;
    fixture.focus("field")?;
    fixture.type_keys(&["h", "i", "Space", "t", "o", "o", "Delete", "Symbols", "!"])?;
    assert_eq!(fixture.text.get(), "hi to!");
    assert!(focused(&fixture.view, "field"));
    let panel = node(&fixture.view, &Role::Group, "Keyboard")?.bounds;
    let between = Point::new(panel.x + 6.dp(), panel.y + panel.height / 2.0);
    fixture.view.input(InputEvent::Press(between));
    fixture.view.input(InputEvent::Release(between));
    fixture.frame()?;
    assert!(focused(&fixture.view, "field"));
    assert!(fixture.keyboard.shown());
    Ok(())
}

#[test]
fn a_protected_field_takes_the_keys_like_any_field() -> Result<(), TestError> {
    let mut fixture = Fixture::new(TextHints::default())?;
    fixture.focus("secret")?;
    fixture.type_keys(&["p", "w", "Delete", "x"])?;
    assert_eq!(fixture.secret.masked('•').chars().count(), 2);
    assert_eq!(fixture.text.get(), "");
    Ok(())
}

#[test]
fn the_action_key_ends_the_input_of_the_field_as_the_action_of_the_field_requires()
-> Result<(), TestError> {
    let mut fixture = Fixture::new(acting(TextAction::Next))?;
    fixture.focus("field")?;
    fixture.type_keys(&["a", "Next"])?;
    assert_eq!(fixture.text.get(), "a");
    assert!(focused(&fixture.view, "second"));
    assert!(fixture.keyboard.shown(), "the next field takes the keys");
    let mut fixture = Fixture::new(acting(TextAction::Done))?;
    fixture.focus("field")?;
    fixture.type_keys(&["Done"])?;
    assert_eq!(fixture.view.focus(), None);
    fixture.settle()?;
    assert!(!fixture.keyboard.shown());
    Ok(())
}

#[test]
fn a_key_pressed_while_another_is_held_types_after_the_held_one() -> Result<(), TestError> {
    let mut fixture = Fixture::new(TextHints::default())?;
    fixture.focus("field")?;
    fixture.touch_down(1, "a")?;
    fixture.touch_down(2, "b")?;
    fixture.touch_up(2, "b")?;
    fixture.touch_up(1, "a")?;
    fixture.touch_down(1, "c")?;
    fixture.touch_down(2, "d")?;
    fixture.touch_up(1, "c")?;
    fixture.touch_up(2, "d")?;
    fixture.frame()?;
    assert_eq!(fixture.text.get(), "abcd");
    Ok(())
}

#[test]
fn keys_held_together_type_at_the_release_of_one_of_them_in_the_order_of_their_presses()
-> Result<(), TestError> {
    let mut fixture = Fixture::new(TextHints::default())?;
    fixture.focus("field")?;
    fixture.touch_down(1, "a")?;
    fixture.touch_down(2, "b")?;
    fixture.touch_down(3, "c")?;
    fixture.frame()?;
    assert_eq!(fixture.text.get(), "");
    fixture.touch_up(2, "b")?;
    fixture.frame()?;
    assert_eq!(fixture.text.get(), "ab");
    fixture.touch_up(3, "c")?;
    fixture.touch_up(1, "a")?;
    fixture.frame()?;
    assert_eq!(fixture.text.get(), "abc");
    Ok(())
}

#[test]
fn delete_pressed_while_another_key_is_held_deletes_after_the_held_key_types()
-> Result<(), TestError> {
    let mut fixture = Fixture::new(TextHints::default())?;
    fixture.focus("field")?;
    fixture.type_keys(&["x"])?;
    fixture.touch_down(1, "a")?;
    fixture.touch_down(2, "Delete")?;
    fixture.frame()?;
    assert_eq!(fixture.text.get(), "x");
    Ok(())
}

#[test]
fn a_held_delete_key_stops_repeating_once_another_key_is_pressed() -> Result<(), TestError> {
    let mut fixture = Fixture::new(TextHints::default())?;
    fixture.focus("field")?;
    fixture.type_keys(&["x", "y", "z"])?;
    fixture.touch_down(1, "Delete")?;
    fixture.touch_down(2, "b")?;
    fixture.at(fixture.time + REPEAT_DELAY + REPEAT_INTERVAL * 4)?;
    assert_eq!(fixture.text.get(), "xy");
    fixture.touch_up(2, "b")?;
    fixture.frame()?;
    assert_eq!(fixture.text.get(), "xyb");
    Ok(())
}

#[test]
fn a_key_held_past_the_long_press_after_another_key_was_pressed_types_itself()
-> Result<(), TestError> {
    let (config, facts) = offering(vec![&ENGLISH, &GERMAN]);
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    let (a, r) = (key(&host.view, "a")?, key(&host.view, "r")?);
    host.view.input(InputEvent::TouchDown(finger(1, a)));
    host.view.input(InputEvent::TouchDown(finger(2, r)));
    host.at(host.time + LONG_PRESS * 2)?;
    assert!(!has(&host.view, &Role::Menu, "More letters"));
    host.view.input(InputEvent::TouchUp(finger(2, r)));
    host.view.input(InputEvent::TouchUp(finger(1, a)));
    host.frame()?;
    assert_eq!(host.sent(), [Host::commit("a"), Host::commit("r")]);
    Ok(())
}

#[test]
fn arrow_keys_show_and_move_a_focus_ring_and_enter_types_the_key_in_it() -> Result<(), TestError> {
    let mut fixture = Fixture::new(TextHints::default())?;
    assert!(
        !fixture.keyboard.navigate(Key::Right),
        "a hidden keyboard takes no keys"
    );
    fixture.focus("field")?;
    assert!(rings(&fixture.scene).is_empty(), "no ring before key input");
    let ring_around = |fixture: &Fixture, key: Rect| {
        let rings = rings(&fixture.scene);
        rings.len() == 1
            && rings[0].x < key.x
            && rings[0].width > key.width
            && (rings[0].center().x - key.center().x).abs() < 0.01.dp()
    };
    assert!(fixture.keyboard.navigate(Key::Down));
    fixture.frame()?;
    assert!(
        ring_around(&fixture, key_bounds(&fixture.view, "q")?),
        "the first arrow shows the ring in place"
    );
    for key in [Key::Right, Key::Right, Key::Down, Key::Right] {
        assert!(fixture.keyboard.navigate(key));
    }
    fixture.frame()?;
    assert!(ring_around(&fixture, key_bounds(&fixture.view, "d")?));
    assert!(fixture.keyboard.navigate(Key::Enter));
    assert!(!fixture.keyboard.navigate(Key::Tab));
    fixture.frame()?;
    assert_eq!(fixture.text.get(), "d");
    assert!(
        ring_around(&fixture, key_bounds(&fixture.view, "d")?),
        "the ring is still on d after typing with the arrow keys and Enter"
    );
    fixture.type_keys(&["e"])?;
    assert!(rings(&fixture.scene).is_empty(), "a touch hides the ring");
    Ok(())
}

#[test]
fn a_keyboard_composed_in_a_new_view_types_into_the_field_of_that_view() -> Result<(), TestError> {
    let (settings, layout) = (State::new(settings()), State::new(&ENGLISH));
    let placement = State::new(Placement::FullWidth);
    let keyboard = Keyboard::new(Target::View, &settings, layout, placement)?;
    let view = |text: &State<String>| -> Result<(View, Scene), TestError> {
        let (value, shown) = (text.clone(), keyboard.clone());
        let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
            let value = value.clone();
            let field = |ui: &mut Ui| {
                ui.text_input(
                    value.get(),
                    TextFieldOptions::default(),
                    Modifier::new().tag("field").size(400.dp(), 40.dp()),
                    move |text| value.set(text),
                );
            };
            above_keyboard(ui, field, |ui| {
                ui.keyboard(&shown, Modifier::new());
            });
        });
        let mut scene = Scene::default();
        frame(&mut view, &mut scene, 0)?;
        let at = composed_bounds(&view, "field")?.center();
        view.input(InputEvent::Press(at));
        view.input(InputEvent::Release(at));
        frame(&mut view, &mut scene, 0)?;
        settle(&mut view, &mut scene)?;
        Ok((view, scene))
    };
    let (first, second) = (State::new(String::new()), State::new(String::new()));
    let (mut old, _) = view(&first)?;
    tap(&mut old, "a")?;
    let (mut new, mut scene) = view(&second)?;
    tap(&mut new, "b")?;
    settle(&mut new, &mut scene)?;
    assert_eq!(
        (first.get(), second.get()),
        ("a".to_string(), "b".to_string())
    );
    Ok(())
}

#[test]
fn a_held_key_ends_without_effect_when_another_field_gains_focus() -> Result<(), TestError> {
    let (config, facts) = offering(vec![&ENGLISH, &FRENCH]);
    let mut host = Host::on(config, facts)?;
    let held = LONG_PRESS.max(REPEAT_DELAY) + REPEAT_INTERVAL;
    for name in ["Delete", "e"] {
        host.start(TextHints::default(), "one")?;
        let at = key(&host.view, name)?;
        host.view.input(InputEvent::TouchDown(TouchSample {
            id: touch(1),
            at,
            kind: PointerKind::Finger,
        }));
        host.frame()?;
        let typed = host.sent();
        host.start(TextHints::default(), "two")?;
        host.at(host.time + held)?;
        assert!(!has(&host.view, &Role::Menu, "More letters"));
        host.view.input(InputEvent::TouchUp(TouchSample {
            id: touch(1),
            at,
            kind: PointerKind::Finger,
        }));
        host.frame()?;
        assert_eq!(host.sent(), typed, "after holding {name}");
    }
    Ok(())
}
