// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::units::NonNegativeDp;

#[test]
fn composite_focus_encloses_padded_content_and_leaves_tab_order_to_controls() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.layout(
                Axis::Vertical,
                Modifier::new()
                    .tag("card")
                    .focus_within(crate::style::BorderStyle {
                        color: Color::WHITE,
                        width: crate::constant!(NonNegativeDp::try_new(Dp::new(2.0))),
                        radius: 16.dp(),
                    })
                    .size(240.dp(), 96.dp())
                    .clip(Corners::all(16.dp()))
                    .background(Color::rgb(0))
                    .padding(20.dp()),
                |ui| {
                    ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
                        ui.text_area(
                            String::new(),
                            TextFieldOptions {
                                placeholder: "Message".into(),
                                ..TextFieldOptions::default()
                            },
                            Modifier::new().tag("editor").size(140.dp(), 40.dp()),
                            |_| {},
                        );
                    });
                },
            );
            ui.text_input(
                String::new(),
                TextFieldOptions::default(),
                Modifier::new().tag("other"),
                |_| {},
            );
        });
    });
    let window = Viewport::new(Size::new(300.dp(), 180.dp()), Density::ONE);
    let mut scene = Scene::default();
    scene.show(view.frame(window, &mut Metrics)?)?;
    let editor = view.tagged_bounds("editor")?;
    let card = view.tagged_bounds("card")?;
    assert_eq!(editor.x - card.x, 20.dp());
    assert_eq!(editor.y - card.y, 20.dp());
    view.input(InputEvent::Press(Point::new(
        editor.x + 5.dp(),
        editor.y + 5.dp(),
    )));
    view.input(InputEvent::Release(Point::new(
        editor.x + 5.dp(),
        editor.y + 5.dp(),
    )));
    scene.show(view.frame(window, &mut Metrics)?)?;
    let commands = scene.commands();
    let outline = commands
        .iter()
        .position(|command| {
            matches!(command, DrawCommand::Stroke { shape, .. }
            if *shape == Shape::rounded(card, 16.dp()))
        })
        .required("a focus outline around the card")?;
    let placeholder = commands
        .iter()
        .position(|command| matches!(command, DrawCommand::Text { text, .. } if text == "Message"))
        .required("the placeholder text")?;
    assert!(
        outline > placeholder,
        "child painting cannot cover the outline"
    );

    view.input(InputEvent::Key {
        key: Key::Tab,
        shift: false,
    });
    scene.show(view.frame(window, &mut Metrics)?)?;
    let commands = scene.commands();
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Stroke { .. }))
    );
    let other = view
        .nodes
        .iter()
        .find(|node| node.tag.as_deref() == Some("other"))
        .required("a node tagged other")?
        .id;
    assert_eq!(view.focus(), Some(other), "the card adds no extra tab stop");
    Ok(())
}

#[test]
fn asynchronous_paste_keeps_field_identity_and_native_selection_uses_utf8() -> TestResult {
    let first = State::new("雪🙂".to_string());
    let second = State::new("other".to_string());
    let a = first.clone();
    let b = second.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            let a = a.clone();
            ui.text_area(
                a.get(),
                TextFieldOptions::default(),
                Modifier::new().tag("first").size(180.dp(), 40.dp()),
                move |v| a.set(v),
            );
            let b = b.clone();
            ui.text_input(
                b.get(),
                TextFieldOptions::default(),
                Modifier::new().tag("second").size(180.dp(), 40.dp()),
                move |v| b.set(v),
            );
        });
    });
    let window = Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE);
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    view.input(InputEvent::Press(Point::new(5.dp(), 5.dp())));
    view.input(InputEvent::Release(Point::new(5.dp(), 5.dp())));
    let target = view.editing_target().required("an editing target")?;
    view.input(InputEvent::Shortcut(Shortcut::control(Key::Character('a'))));
    assert_eq!(view.copy_selection(false).as_deref(), Some("雪🙂"));
    assert_eq!(view.copy_selection(true).as_deref(), Some("雪🙂"));
    assert_eq!(first.get(), "");
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    let paste = view.text_paste_target(target).required("a paste target")?;
    let late = paste.clone();
    assert!(view.paste_text_at(&paste, "hello\n雪"));
    assert_eq!(first.get(), "hello\n雪");
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    view.input(InputEvent::Press(Point::new(5.dp(), 55.dp())));
    view.input(InputEvent::Release(Point::new(5.dp(), 55.dp())));
    assert!(!view.paste_text_at(&late, "late transfer"));
    assert_eq!(second.get(), "other");
    assert_eq!(first.get(), "hello\n雪");
    Ok(())
}

#[test]
fn multiline_editing_wraps_selects_rows_and_leaves_enter_policy_to_the_caller() -> TestResult {
    let value = State::new(String::new());
    let state = value.clone();
    let sends = State::new(0);
    let sent = sends.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let state = state.clone();
        let sent = sent.clone();
        ui.text_area(
            state.get(),
            TextFieldOptions::default(),
            Modifier::new()
                .tag("field")
                .on_key(move |k| {
                    if k.key == Key::Enter && !k.shift {
                        sent.update(|n| *n += 1);
                        true
                    } else {
                        false
                    }
                })
                .size(100.dp(), 55.dp()),
            move |v| state.set(v),
        );
    });
    let window = Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE);
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    view.input(InputEvent::Press(Point::new(4.dp(), 4.dp())));
    view.input(InputEvent::Release(Point::new(4.dp(), 4.dp())));
    view.input(InputEvent::Text("ab雪".into()));
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: true,
    });
    view.input(InputEvent::Text("cd🙂".into()));
    assert_eq!(value.get(), "ab雪\ncd🙂");
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    view.input(InputEvent::Key {
        key: Key::Home,
        shift: false,
    });
    view.input(InputEvent::Key {
        key: Key::Up,
        shift: false,
    });
    view.input(InputEvent::Text("x".into()));
    assert_eq!(value.get(), "xab雪\ncd🙂");
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: false,
    });
    assert_eq!(sends.get(), 1);
    assert_eq!(value.get(), "xab雪\ncd🙂");
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    view.input(InputEvent::Shortcut(Shortcut::control(Key::Character('a'))));
    view.input(InputEvent::Text("e\u{301}\n🙂".into()));
    assert_eq!(value.get(), "e\u{301}\n🙂");
    Ok(())
}

#[test]
fn multiline_default_enter_inserts_text_and_reports_natural_height() -> TestResult {
    let state = State::new("first".to_string());
    let value = state.clone();
    let measured = State::new(Size::ZERO);
    let result = measured.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let value = value.clone();
        let result = result.clone();
        ui.text_area(
            value.get(),
            TextFieldOptions::default(),
            Modifier::new()
                .on_measure(move |s| {
                    result.set_if_changed(s);
                })
                .width(80.dp()),
            move |v| value.set(v),
        );
    });
    let window = Viewport::new(Size::new(100.dp(), 100.dp()), Density::ONE);
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    view.input(InputEvent::Press(Point::new(78.dp(), 4.dp())));
    view.input(InputEvent::Release(Point::new(78.dp(), 4.dp())));
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: false,
    });
    assert_eq!(state.get(), "first\n");
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    // The height of one line measured by `Metrics` at 14 sp, the text size set by the platform
    // fixture.
    assert_eq!(measured.get().height, Dp::new(14.0 * 1.3));
    Ok(())
}
