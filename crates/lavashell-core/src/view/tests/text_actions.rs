// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::input_method::{TextAction, TextCommand};

const ENTER: InputEvent = InputEvent::Key {
    key: Key::Enter,
    shift: false,
};

#[test]
fn enter_runs_the_action_of_a_single_line_field_that_no_handler_takes() -> TestResult {
    let searches = State::new(0);
    let count = searches.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let count = count.clone();
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.text_input(
                "",
                acting(TextAction::Next),
                Modifier::new().tag("name").then(sized()),
                |_| {},
            );
            ui.spacer(
                100.dp(),
                20.dp(),
                Modifier::new().tag("button").on_click(|| {}),
            );
            ui.text_input(
                "",
                acting(TextAction::Done),
                Modifier::new().tag("city").then(sized()),
                |_| {},
            );
            ui.text_input(
                "",
                acting(TextAction::Search),
                Modifier::new()
                    .tag("query")
                    .then(sized())
                    .on_key(counting_enter(count.clone())),
                |_| {},
            );
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    tap(&mut view, "name")?;
    view.input(ENTER);
    assert!(view.tagged_state("city")?.focus.focused);
    view.input(ENTER);
    assert_eq!(view.focus(), None);
    tap(&mut view, "query")?;
    view.input(ENTER);
    assert_eq!(searches.get(), 1);
    assert!(view.tagged_state("query")?.focus.focused);
    Ok(())
}

#[test]
fn next_wraps_to_the_first_field_and_enter_means_the_same_elsewhere() -> TestResult {
    let notes = State::new(String::new());
    let text = notes.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let text = text.clone();
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.text_area(
                text.get(),
                acting(TextAction::Next),
                Modifier::new().tag("notes").then(sized()),
                move |value| text.set(value),
            );
            ui.text_input(
                "",
                TextFieldOptions::default(),
                Modifier::new().tag("plain").then(sized()),
                |_| {},
            );
            ui.text_input(
                "",
                acting(TextAction::Next),
                Modifier::new().tag("last").then(sized()),
                |_| {},
            );
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    tap(&mut view, "plain")?;
    view.input(ENTER);
    assert!(view.tagged_state("plain")?.focus.focused);
    tap(&mut view, "last")?;
    view.input(ENTER);
    assert!(view.tagged_state("notes")?.focus.focused);
    view.input(ENTER);
    assert!(view.tagged_state("notes")?.focus.focused);
    assert_eq!(notes.get(), "\n");
    Ok(())
}

#[test]
fn an_action_command_runs_in_a_multiline_field_where_enter_adds_a_line() -> TestResult {
    let notes = State::new(String::new());
    let text = notes.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let text = text.clone();
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.text_area(
                text.get(),
                TextFieldOptions::default(),
                Modifier::new().tag("notes").then(sized()),
                move |value| text.set(value),
            );
            ui.text_input(
                "",
                TextFieldOptions::default(),
                Modifier::new().tag("plain").then(sized()),
                |_| {},
            );
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    tap(&mut view, "notes")?;
    view.input(ENTER);
    assert_eq!(notes.get(), "\n");
    view.apply_text(TextCommand::Action(TextAction::Next));
    assert_eq!(notes.get(), "\n");
    assert!(view.tagged_state("plain")?.focus.focused);
    Ok(())
}
