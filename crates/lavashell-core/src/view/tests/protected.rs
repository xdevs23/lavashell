// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::accessibility::{AccessibilityAction, ActionKind, ActionOutcome, ActionRequest, Role};
use crate::protected_text::ProtectedText;

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(220.0), Dp::new(50.0)), Density::ONE);

fn field() -> Result<(View, Scene, ProtectedText), TestError> {
    let secret = ProtectedText::new();
    let state = secret.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.protected_input(
            state.clone(),
            TextFieldOptions::default(),
            Modifier::new().tag("private").size(200.dp(), 40.dp()),
        );
    });
    let mut scene = Scene::default();
    scene.show(view.frame(WINDOW, &mut Metrics)?)?;
    view.input(InputEvent::Key {
        key: Key::Tab,
        shift: false,
    });
    Ok((view, scene, secret))
}

#[test]
fn protected_unicode_editing_only_shapes_masks_and_blocks_exports() -> TestResult {
    let (mut view, mut scene, secret) = field()?;
    view.input(InputEvent::Text("aé💡z".into()));
    view.input(InputEvent::Key {
        key: Key::Left,
        shift: false,
    });
    view.input(InputEvent::Key {
        key: Key::Backspace,
        shift: false,
    });
    view.input(InputEvent::Text("中".into()));
    view.input(InputEvent::Key {
        key: Key::Home,
        shift: false,
    });
    view.input(InputEvent::Key {
        key: Key::Delete,
        shift: false,
    });
    assert_eq!(secret.masked('•'), "•••");
    scene.show(view.frame(WINDOW, &mut Metrics)?)?;
    let commands = scene.commands();
    let texts: Vec<_> = commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["•••"]);
    view.input(InputEvent::Shortcut(Shortcut {
        control: true,
        ..Shortcut::key(Key::Character('a'))
    }));
    let id = view.focus().required("a focused node")?;
    assert!(view.copy_selection(false).is_none());
    assert!(view.copy_selection(true).is_none());
    assert!(view.editing_target().is_none());
    assert!(view.text_paste_target(id).is_none());
    assert!(view.text_selection_source(id).is_none());
    assert!(
        view.text_drop_target(Point::new(10.dp(), 10.dp()))
            .is_none()
    );
    let handoff = secret.take();
    handoff.expose(|value| assert_eq!(value, "é中z"));
    assert!(secret.masked('•').is_empty());
    // `take` empties the field. A text event that arrives before the next composition must clamp
    // the selection from before `take` to the empty text.
    view.input(InputEvent::Text("新".into()));
    secret.take().expose(|value| assert_eq!(value, "新"));
    Ok(())
}

#[test]
fn a_protected_field_shows_the_mask_of_the_platform_and_edits_by_characters_of_it() -> TestResult {
    let (mut view, mut scene, secret) = field()?;
    view.input(InputEvent::Text("é💡".into()));
    view.provide_platform(PlatformFacts {
        password_mask: '*',
        ..crate::platform::fixture::facts()
    });
    scene.show(view.frame(WINDOW, &mut Metrics)?)?;
    let shown = |scene: &Scene| {
        (scene.commands().iter()).find_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
    };
    assert_eq!(shown(&scene).as_deref(), Some("**"));
    view.input(InputEvent::Key {
        key: Key::Backspace,
        shift: false,
    });
    view.input(InputEvent::Text("z".into()));
    scene.show(view.frame(WINDOW, &mut Metrics)?)?;
    assert_eq!(shown(&scene).as_deref(), Some("**"));
    assert_eq!(secret.masked('*'), "**");
    secret.take().expose(|value| assert_eq!(value, "éz"));
    Ok(())
}

#[test]
fn protected_accessibility_keeps_role_and_focus_without_value_or_text_actions() -> TestResult {
    let (mut view, mut scene, secret) = field()?;
    view.input(InputEvent::Text("example".into()));
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    let tree = view.accessibility();
    let field = tree
        .nodes
        .values()
        .find(|node| node.role == Role::PasswordInput)
        .required("a password input node")?;
    let id = field.id;
    assert!(field.value.is_none());
    let text = field.text.as_ref().required("the text of the node")?;
    assert!(text.value.is_empty() && text.runs.is_empty() && text.selection.is_none());
    assert!(field.actions.contains(&ActionKind::Focus));
    assert!(!field.actions.contains(&ActionKind::SetValue));
    assert!(!field.actions.contains(&ActionKind::ReplaceSelectedText));
    assert!(!field.actions.contains(&ActionKind::SetTextSelection));
    assert_eq!(
        view.accessibility_action(ActionRequest {
            target: id,
            action: AccessibilityAction::ReplaceSelectedText("ignored".into()),
        }),
        Ok(ActionOutcome::Ignored)
    );
    secret.take().expose(|value| assert_eq!(value, "example"));
    Ok(())
}

#[test]
fn explicit_reveal_keeps_unicode_editor_and_accessibility_masked_with_private_geometry()
-> TestResult {
    use crate::protected_text::PrivateText;
    struct PrivateMetrics;
    impl TextMeasurer for PrivateMetrics {
        fn measure(&mut self, value: &str, _: &TextStyle<Dp>, _: Dp) -> Size {
            assert!(!value.contains('W') && !value.contains('é') && !value.contains('💡'));
            Size::new(repeated(10.dp(), value.chars().count()), 20.dp())
        }
        fn private_geometry(
            &mut self,
            value: &PrivateText,
            _: &TextStyle<Dp>,
            _: Dp,
        ) -> TextGeometry {
            value.geometry(|value| {
                let mut x = Dp::ZERO;
                let mut carets = Vec::new();
                for (index, c) in value.char_indices() {
                    carets.push(TextCaret {
                        index,
                        rect: Rect::new(x, 0.dp(), 1.dp(), 20.dp()),
                        line: 0,
                    });
                    x += if c == 'i' { 2.dp() } else { 18.dp() };
                }
                carets.push(TextCaret {
                    index: value.len(),
                    rect: Rect::new(x, 0.dp(), 1.dp(), 20.dp()),
                    line: 0,
                });
                TextGeometry {
                    carets,
                    size: Size::new(x, 20.dp()),
                    word_starts: Some(vec![0, 2]),
                    ..Default::default()
                }
            })
        }
    }
    let (mut view, mut scene, secret) = field()?;
    view.input(InputEvent::Text("iWé💡".into()));
    secret.set_revealed(true);
    scene.show(view.frame(WINDOW, &mut PrivateMetrics)?)?;
    let commands = scene.commands();
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, DrawCommand::PrivateText { .. }))
    );
    let debug = format!("{commands:?}");
    assert!(!debug.contains("iWé💡"));
    assert!(debug.contains("redacted"));
    let tree = view.accessibility();
    let node = tree
        .nodes
        .values()
        .find(|node| node.role == Role::PasswordInput)
        .required("a password input node")?;
    assert!(node.value.is_none());
    let text = node.text.as_ref().required("the text of the node")?;
    assert!(text.value.is_empty());
    assert!(!node.actions.contains(&ActionKind::SetValue));
    view.input(InputEvent::Press(Point::new(19.dp(), 20.dp())));
    view.input(InputEvent::Release(Point::new(19.dp(), 20.dp())));
    view.input(InputEvent::Key {
        key: Key::Backspace,
        shift: false,
    });
    scene.show(view.frame(WINDOW, &mut PrivateMetrics)?)?;
    let commands = scene.commands();
    let private = commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::PrivateText { text, .. } => Some(text),
            _ => None,
        })
        .required("a private text")?;
    private.expose(|value| assert_eq!(value, "ié💡"));
    assert_eq!(secret.masked('•'), "•••");
    assert!(view.copy_selection(false).is_none());
    assert!(view.editing_target().is_none());
    secret.set_revealed(false);
    scene.show(view.frame(WINDOW, &mut PrivateMetrics)?)?;
    let commands = scene.commands();
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, DrawCommand::PrivateText { .. }))
    );
    assert!(commands.iter().any(|command| matches!(command,
        DrawCommand::Text { text, .. } if text == "•••")));
    Ok(())
}

#[test]
fn protected_field_with_key_handler_edits_text_and_space_in_a_retained_scope() -> TestResult {
    let secret = ProtectedText::new();
    let source = secret.clone();
    let accepted = State::new(0_u32);
    let events = accepted.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let source = source.clone();
        let events = events.clone();
        ui.scoped("private-parent", Modifier::new(), move |ui| {
            let events = events.clone();
            ui.protected_input(
                source.clone(),
                TextFieldOptions::default(),
                Modifier::new()
                    .tag("private-with-handler")
                    .on_key(move |shortcut| {
                        if shortcut.key == Key::Enter {
                            events.update(|count| *count += 1);
                            true
                        } else {
                            false
                        }
                    })
                    .size(200.dp(), 40.dp()),
            );
        });
    });
    let window = Viewport::new(Size::new(220.dp(), 60.dp()), Density::ONE);
    let mut scene = Scene::default();
    scene.show(view.frame(window, &mut Metrics)?)?;
    let point = Point::new(15.dp(), 20.dp());
    view.input(InputEvent::Press(point));
    view.input(InputEvent::Release(point));
    scene.show(view.frame(window, &mut Metrics)?)?;
    assert!(view.tagged_state("private-with-handler")?.focus.focused);
    view.input(InputEvent::Text("wadswads".into()));
    assert_eq!(secret.masked('•').chars().count(), 8);
    scene.show(view.frame(window, &mut Metrics)?)?;
    view.input(InputEvent::Key {
        key: Key::Space,
        shift: false,
    });
    assert_eq!(secret.masked('•').chars().count(), 9);
    scene.show(view.frame(window, &mut Metrics)?)?;
    assert!(scene.commands().iter().any(|command| matches!(command,
        DrawCommand::Text { text, .. } if *text == "•".repeat(9))));
    scene.show(view.frame(window, &mut Metrics)?)?;
    secret.set_revealed(true);
    scene.show(view.frame(window, &mut Metrics)?)?;
    view.input(InputEvent::Text("é".into()));
    view.input(InputEvent::Key {
        key: Key::Backspace,
        shift: false,
    });
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: false,
    });
    scene.show(view.frame(window, &mut Metrics)?)?;
    assert_eq!(accepted.get(), 1);
    assert!(!format!("{:?}", scene.commands()).contains("wadswads"));
    assert!(view.copy_selection(false).is_none());
    secret.take().expose(|value| assert_eq!(value, "wadswads "));
    Ok(())
}
