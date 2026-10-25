// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::reactive::State;
use crate::ui::Ui;

#[test]
fn disabled_button_clears_focus_without_capturing_global_shortcuts_in_an_editor() -> TestResult {
    for scoped in [false, true] {
        let enabled = State::new(true);
        let undo = State::new(0);
        let changes = State::new(0);
        let active = enabled.clone();
        let handled = undo.clone();
        let edited = changes.clone();
        let mut scene = Scene::default();
        let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
            let handled = handled.clone();
            ui.on_shortcut(Shortcut::control(Key::Character('z')), move || {
                handled.update(|count| *count += 1);
            });
            ui.layout(Axis::Vertical, Modifier::new(), |ui| {
                let edited = edited.clone();
                ui.text_input(
                    "output.png",
                    TextFieldOptions::default(),
                    Modifier::new().accessible_name("Output path"),
                    move |_| edited.update(|count| *count += 1),
                );
                let active = active.clone();
                let compose = move |ui: &mut Ui| {
                    let disable = active.clone();
                    ui.text(
                        "Delete",
                        Modifier::new()
                            .on_click(move || disable.set(false))
                            .enabled(active.get()),
                    );
                };
                if scoped {
                    ui.scoped("action", Modifier::new(), compose);
                } else {
                    compose(ui);
                }
            });
        });
        frame(&mut scene, &mut view)?;
        let button = target(&view, "Delete")?;
        let bounds = view.accessibility.nodes[&button].bounds;
        let point = Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
        );
        view.input(InputEvent::Press(point));
        view.input(InputEvent::Release(point));
        assert_eq!(view.focus(), Some(button));
        assert!(!enabled.get());
        frame(&mut scene, &mut view)?;
        assert_eq!(
            view.focus(),
            None,
            "disabled action must not select an unrelated editor"
        );
        assert_eq!(view.editing_target(), None);
        view.input(InputEvent::Shortcut(Shortcut::control(Key::Character('z'))));
        assert_eq!(undo.get(), 1);
        assert_eq!(changes.get(), 0);
    }
    Ok(())
}

#[test]
fn the_accessibility_focus_action_makes_focus_visible_after_a_tap() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.text("Play", Modifier::new().tag("play").on_click(|| {}));
            ui.text("Next", Modifier::new().tag("next").on_click(|| {}));
        });
    });
    frame(&mut scene, &mut view)?;
    let play = view.tagged_bounds("play")?;
    let at = Point::new(play.x + play.width / 2.0, play.y + play.height / 2.0);
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
    assert!(!view.tagged_state("play")?.focus.visible);
    let next = target(&view, "Next")?;
    assert_eq!(
        request(&mut view, next, AccessibilityAction::Focus),
        PERFORMED
    );
    let state = view.tagged_state("next")?;
    assert!(state.focus.focused);
    assert!(state.focus.visible);
    Ok(())
}
