// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::input_method::{
    Capitalization, FieldPresses, FieldText, SessionId, TextAction, TextHints, TextPurpose,
    TextSession,
};
use crate::protected_text::ProtectedText;

fn key(key: Key, shift: bool) -> InputEvent {
    InputEvent::Key { key, shift }
}

/// A view with a search field, a plain field and a protected field, tagged "search", "plain" and
/// "secret". The returned state contains the text session read on each composition by a scoped
/// component in the same view.
fn fields(scene: &mut Scene) -> Result<(View, State<Option<TextSession>>), TestError> {
    let query = State::new(String::new());
    let read = State::new(None);
    let seen = read.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (query, seen) = (query.clone(), seen.clone());
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            let search = TextHints {
                action: Some(TextAction::Search),
                purpose: TextPurpose::Url,
                capitalization: Capitalization::Sentences,
            };
            ui.text_input(
                query.get(),
                TextFieldOptions {
                    hints: search,
                    ..TextFieldOptions::default()
                },
                Modifier::new().tag("search").then(sized()),
                move |value| query.set(value),
            );
            ui.text_input(
                "",
                TextFieldOptions::default(),
                Modifier::new().tag("plain").then(sized()),
                |_| {},
            );
            ui.protected_input(
                ProtectedText::new(),
                TextFieldOptions::default(),
                Modifier::new().tag("secret").then(sized()),
            );
            ui.scoped("observer", Modifier::new(), move |ui| {
                seen.set(ui.text_sender().session());
            });
        });
    });
    advance(scene, &mut view, WINDOW_SIZE, 0)?;
    Ok((view, read))
}

#[test]
fn a_field_starts_a_session_when_it_gains_focus_and_counts_the_presses_on_it() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, seen) = fields(&mut scene)?;
    let search = center(view.tagged_bounds("search")?);
    let plain = center(view.tagged_bounds("plain")?);
    assert_eq!(view.text_session(), None);
    tap_at(&mut view, search);
    let session = TextSession {
        id: SessionId(1),
        presses: FieldPresses(0),
        hints: TextHints {
            action: Some(TextAction::Search),
            purpose: TextPurpose::Url,
            capitalization: Capitalization::Sentences,
        },
        multiline: false,
        text: FieldText::BeforeCursor("".into()),
    };
    assert_eq!(view.text_session(), Some(session.clone()));
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(seen.get(), Some(session));
    tap_at(&mut view, search);
    let pressed = view.text_session().required("a text session")?;
    assert_eq!(
        (pressed.id, pressed.presses),
        (SessionId(1), FieldPresses(1))
    );
    tap_at(&mut view, plain);
    let plain_session = view.text_session().required("a text session")?;
    assert_eq!(
        (plain_session.id, plain_session.presses, plain_session.hints),
        (SessionId(2), FieldPresses(0), TextHints::default())
    );
    tap_at(&mut view, Point::new(450.dp(), 280.dp()));
    assert_eq!(view.text_session(), None);
    tap_at(&mut view, plain);
    assert_eq!(
        view.text_session().required("a text session")?.id,
        SessionId(3)
    );
    view.input(key(Key::Tab, false));
    let secret = view.text_session().required("a text session")?;
    assert_eq!(
        (secret.id, &secret.text),
        (SessionId(4), &FieldText::Protected)
    );
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(seen.get(), Some(secret));
    Ok(())
}

#[test]
fn a_session_has_the_text_before_the_cursor_or_the_selection() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, _) = fields(&mut scene)?;
    let search = center(view.tagged_bounds("search")?);
    tap_at(&mut view, search);
    view.input(InputEvent::Text("hello".into()));
    let before = |view: &View| view.text_session().map(|session| session.text);
    let text = |text: &str| FieldText::BeforeCursor(text.into());
    assert_eq!(before(&view), Some(text("hello")));
    view.input(key(Key::Left, false));
    assert_eq!(before(&view), Some(text("hell")));
    view.input(key(Key::Left, true));
    assert_eq!(before(&view), Some(text("hel")));
    Ok(())
}
