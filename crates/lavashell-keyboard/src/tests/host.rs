// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn a_host_receives_every_edit_and_the_action_of_its_field() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    assert!(!host.keyboard.shown());
    assert!(!has(&host.view, &Role::Group, "Keyboard"));
    let search = TextHints {
        action: Some(TextAction::Search),
        ..TextHints::default()
    };
    host.start(search, "")?;
    assert!(host.keyboard.shown());
    for name in [
        "m", "a", "Delete", "Space", "Symbols", "$", "1", "Letters", "Search",
    ] {
        tap(&mut host.view, name)?;
        host.frame()?;
    }
    let sent = [
        Host::commit("m"),
        Host::commit("a"),
        TextCommand::DeleteBackward,
        Host::commit(" "),
        Host::commit("$"),
        Host::commit("1"),
        TextCommand::Action(TextAction::Search),
    ];
    assert_eq!(host.sent(), sent);
    Ok(())
}

#[test]
fn a_field_without_an_action_gets_an_enter_key() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.start(TextHints::default(), "")?;
    tap(&mut host.view, "Enter")?;
    assert_eq!(host.sent(), [TextCommand::Key(Key::Enter)]);
    Ok(())
}

#[test]
fn the_keyboard_follows_the_session_of_the_host_and_is_as_tall_as_its_panel()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    let height = |host: &Host| host.view.bounds_of("keyboard").map(|bounds| bounds.height);
    assert_eq!(height(&host), Some(Dp::ZERO));
    host.start(TextHints::default(), "")?;
    let panel = node(&host.view, &Role::Group, "Keyboard")?.bounds;
    assert_eq!(height(&host), Some(panel.height));
    assert_eq!(panel.y + panel.height, SCREEN.height);
    host.keyboard.hide();
    host.settle()?;
    assert!(!host.keyboard.shown());
    assert_eq!(height(&host), Some(Dp::ZERO));
    host.start(TextHints::default(), "")?;
    assert!(
        host.keyboard.shown(),
        "a new session shows the keyboard again"
    );
    host.session.set(None);
    host.settle()?;
    assert!(!host.keyboard.shown());
    Ok(())
}

/// The counts of the work done by `view` so far: compositions of the root, compositions of scoped
/// components, and scoped components measured again and painted again.
fn work(view: &View) -> [u64; 4] {
    let stats = view.stats();
    [
        stats.root_compositions,
        stats.component_compositions,
        stats.measured_components,
        stats.painted_components,
    ]
}

#[test]
fn the_content_above_ends_at_the_glass_in_every_frame_while_the_panel_rises_and_sinks()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    let empty = FieldText::BeforeCursor("".into());
    host.session
        .set(Some(session(SessionId(1), TextHints::default(), empty)));
    host.frame()?;
    let composed = work(&host.view);
    run_until(&mut host.view, &mut host.scene, |view, scene| {
        if !fused_rows(scene).is_empty() {
            return true;
        }
        assert!(ends_at_glass(view, scene), "while the panel rises");
        assert_eq!(work(view), composed, "each frame reuses the keyboard");
        false
    })?;
    host.settle()?;
    host.session.set(None);
    host.frame()?;
    run_until(&mut host.view, &mut host.scene, |view, _| {
        !has(view, &Role::Group, "Keyboard")
    })?;
    let composed = work(&host.view);
    run_until(&mut host.view, &mut host.scene, |view, scene| {
        if !view.motion().is_active() {
            return true;
        }
        assert!(ends_at_glass(view, scene), "while the panel sinks");
        assert_eq!(work(view), composed, "each frame reuses the keyboard");
        false
    })?;
    Ok(())
}

#[test]
fn a_keyboard_shown_in_one_view_rises_from_below_when_it_is_composed_in_another()
-> Result<(), TestError> {
    let field = State::new(None);
    let target = Target::Host {
        session: field.clone(),
        sink: Rc::new(|_| {}),
    };
    let settings = State::new(settings());
    let keyboard = Keyboard::new(target, &settings, State::new(&ENGLISH))?;
    let docked = |keyboard: &Keyboard| {
        let keyboard = keyboard.clone();
        View::new(lavashell_test_platform::facts(), move |ui| {
            above_keyboard(
                ui,
                |_| {},
                |ui| {
                    ui.keyboard(&keyboard, Modifier::new().tag("keyboard"));
                },
            );
        })
    };
    let height =
        |view: &View| -> Result<Dp, TestError> { Ok(composed_bounds(view, "keyboard")?.height) };
    let (mut first, mut scene) = (docked(&keyboard), Scene::default());
    let empty = FieldText::BeforeCursor("".into());
    field.set(Some(session(SessionId(1), TextHints::default(), empty)));
    frame(&mut first, &mut scene, 0)?;
    settle(&mut first, &mut scene)?;
    let panel = node(&first, &Role::Group, "Keyboard")?.bounds.height;
    assert_eq!(height(&first)?, panel);
    drop(first);
    let (mut second, mut scene) = (docked(&keyboard), Scene::default());
    frame(&mut second, &mut scene, 0)?;
    assert_eq!(height(&second)?, Dp::ZERO);
    settle(&mut second, &mut scene)?;
    assert_eq!(height(&second)?, panel);
    Ok(())
}

#[test]
fn a_keyboard_hidden_by_its_key_shows_for_no_session_until_the_field_is_pressed_again()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    assert_eq!(host.keyboard.session(), None);
    host.start(TextHints::default(), "")?;
    assert!(host.keyboard.session().is_some());
    host.keyboard.hide();
    host.settle()?;
    assert_eq!(host.keyboard.session(), None);
    assert!(host.keyboard.rests_hidden());
    let current = host.session.peek().ok_or(TestError::NoSession)?;
    host.session.set(Some(TextSession {
        presses: FieldPresses(current.presses.0 + 1),
        ..current
    }));
    assert!(host.keyboard.session().is_some());
    Ok(())
}
