// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::input_method::{FieldText, TextAction, TextCommand};
use crate::protected_text::ProtectedText;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

type Keyboard = (View, State<String>, State<u32>, State<u32>);

/// A view with a search field tagged "field" and three keys below it that send text commands to the
/// focused field. The key "a" commits the text "a" and the key "search" sends the search action,
/// both when released. The key "backspace" deletes a character when pressed and repeats the
/// deletion while it is held. Returned with the view are the text of the field, the number of Enter
/// presses in the field and the number of runs of the shortcut on `a`.
fn keyboard(scene: &mut Scene) -> Result<Keyboard, TestError> {
    let (text, searches, shortcuts) = (State::new(String::new()), State::new(0), State::new(0));
    let (value, count, used) = (text.clone(), searches.clone(), shortcuts.clone());
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (value, count, used) = (value.clone(), count.clone(), used.clone());
        ui.on_shortcut(Shortcut::key(Key::Character('a')), move || {
            used.update(|n| *n += 1);
        });
        let sender = ui.text_sender();
        let motion = ui.motion().clone();
        let repeater = sender.clone();
        let repeat = ui.state("repeat", move || {
            motion.timer(move || repeater.send(TextCommand::DeleteBackward))
        });
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.text_input(
                value.get(),
                acting(TextAction::Search),
                Modifier::new()
                    .tag("field")
                    .then(sized())
                    .on_key(counting_enter(count.clone())),
                move |text| value.set(text),
            );
            let keys = [
                ("a", TextCommand::Commit("a".into())),
                ("search", TextCommand::Action(TextAction::Search)),
            ];
            for (tag, command) in keys {
                let sender = sender.clone();
                ui.spacer(
                    60.dp(),
                    40.dp(),
                    Modifier::new()
                        .tag(tag)
                        .focusable(false)
                        .on_pointer(move |input| {
                            if input.phase == PointerPhase::Released {
                                sender.send(command.clone());
                            }
                        }),
                );
            }
            let (sender, timer) = (sender.clone(), repeat.get());
            ui.spacer(
                60.dp(),
                40.dp(),
                Modifier::new()
                    .tag("backspace")
                    .focusable(false)
                    .on_pointer(move |input| match input.phase {
                        PointerPhase::Pressed => {
                            sender.send(TextCommand::DeleteBackward);
                            timer.repeat(Duration::from_millis(400), Duration::from_millis(50));
                        }
                        PointerPhase::Moved => {}
                        PointerPhase::Released | PointerPhase::Cancelled => timer.cancel(),
                    }),
            );
        });
    });
    advance(scene, &mut view, WINDOW_SIZE, 0)?;
    Ok((view, text, searches, shortcuts))
}

#[test]
fn keys_composed_in_the_view_type_into_its_focused_field() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, text, searches, shortcuts) = keyboard(&mut scene)?;
    tap(&mut view, "a")?;
    assert_eq!((text.get(), shortcuts.get()), (String::new(), 0));
    tap(&mut view, "field")?;
    for _ in 0..4 {
        tap(&mut view, "a")?;
    }
    assert_eq!(text.get(), "aaaa");
    assert!(view.tagged_state("field")?.focus.focused);
    tap(&mut view, "search")?;
    assert_eq!(searches.get(), 1);
    assert!(view.tagged_state("field")?.focus.focused);
    assert_eq!(shortcuts.get(), 0);
    Ok(())
}

#[test]
fn a_held_key_repeats_through_a_timer_until_it_is_released() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, text, ..) = keyboard(&mut scene)?;
    tap(&mut view, "field")?;
    for _ in 0..5 {
        tap(&mut view, "a")?;
    }
    let backspace = center(view.tagged_bounds("backspace")?);
    view.input(InputEvent::Press(backspace));
    assert_eq!(text.get(), "aaaa");
    advance(&mut scene, &mut view, WINDOW_SIZE, 399)?;
    assert_eq!(text.get(), "aaaa");
    advance(&mut scene, &mut view, WINDOW_SIZE, 400)?;
    assert_eq!(text.get(), "aaa");
    advance(&mut scene, &mut view, WINDOW_SIZE, 450)?;
    advance(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_eq!(text.get(), "a");
    view.input(InputEvent::Release(backspace));
    advance(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(text.get(), "a");
    Ok(())
}

#[test]
fn a_sender_returns_the_session_with_the_commands_applied_by_the_view() -> TestResult {
    let senders = Rc::new(RefCell::new(Vec::new()));
    let composed = senders.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        composed.borrow_mut().push(ui.text_sender());
        ui.text_input(
            "",
            TextFieldOptions::default(),
            Modifier::new().tag("field").then(sized()),
            |_| {},
        );
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let sender = senders.borrow()[0].clone();
    assert_eq!(sender.session(), None);
    tap(&mut view, "field")?;
    sender.send(TextCommand::Commit("ab".into()));
    view.input(InputEvent::Leave);
    let text = sender.session().map(|session| session.text);
    assert_eq!(text, Some(FieldText::BeforeCursor("ab".into())));
    Ok(())
}

#[test]
fn the_senders_of_one_view_are_equal_and_those_of_two_views_differ() -> TestResult {
    let senders = Rc::new(RefCell::new(Vec::new()));
    let compose = || {
        let composed = senders.clone();
        View::new(crate::platform::fixture::facts(), move |ui| {
            composed.borrow_mut().push(ui.text_sender());
            let inner = composed.clone();
            ui.scoped("inner", Modifier::new(), move |ui| {
                inner.borrow_mut().push(ui.text_sender());
            });
        })
    };
    let (mut first, mut second) = (compose(), compose());
    let (mut first_scene, mut second_scene) = (Scene::default(), Scene::default());
    advance(&mut first_scene, &mut first, WINDOW_SIZE, 0)?;
    advance(&mut second_scene, &mut second, WINDOW_SIZE, 0)?;
    let senders = senders.borrow();
    assert_eq!(senders.len(), 4);
    assert!(senders[0] == senders[1] && senders[2] == senders[3]);
    assert!(senders[0] != senders[2]);
    Ok(())
}

#[test]
fn a_host_applies_edits_and_actions_to_the_focused_field() -> TestResult {
    let (text, secret) = (State::new(String::new()), ProtectedText::new());
    let (value, protected) = (text.clone(), secret.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let value = value.clone();
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.text_input(
                value.get(),
                acting(TextAction::Next),
                Modifier::new().tag("name").then(sized()),
                move |text| value.set(text),
            );
            ui.protected_input(
                protected.clone(),
                acting(TextAction::Done),
                Modifier::new().tag("password").then(sized()),
            );
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.apply_text(TextCommand::Commit("ignored".into()));
    tap(&mut view, "name")?;
    let commands = [
        TextCommand::Commit("hi".into()),
        TextCommand::DeleteBackward,
        TextCommand::Commit("ey".into()),
        TextCommand::Key(Key::Home),
        TextCommand::Commit(">".into()),
        TextCommand::Action(TextAction::Next),
        TextCommand::Commit("pw".into()),
        TextCommand::Action(TextAction::Done),
        TextCommand::Commit("dropped".into()),
    ];
    for command in commands {
        view.apply_text(command);
    }
    assert_eq!(text.get(), ">hey");
    assert_eq!(secret.masked('•'), "••");
    assert_eq!(view.focus(), None);
    Ok(())
}
