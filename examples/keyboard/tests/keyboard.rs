// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use icu_locale_core::langid;
use lavashell::keyboard::{ENGLISH, FRENCH, GERMAN, Layout, PORTUGUESE};
use lavashell::prelude::*;
use lavashell_on_screen_keyboard::{
    CopyKey, Error, copy,
    model::{Appearance, Demo},
    scene::{self, FIELD},
    stage::{QUERY, Stage, touch},
};
use std::path::Path;

fn shot(stage: &mut Stage, name: &str) -> Result<(), Error> {
    let captures = Path::new(env!("CARGO_TARGET_TMPDIR")).join("keyboard");
    Ok(stage.offscreen.save(captures.join(format!("{name}.png")))?)
}

/// Lets `change` edit a copy of the current platform facts of the view and gives the edited facts to the view.
fn platform(stage: &mut Stage, change: impl FnOnce(&mut PlatformFacts)) {
    let mut facts = stage.offscreen.view.platform();
    change(&mut facts);
    stage.offscreen.view.provide_platform(facts);
}

fn near(length: Dp, expected: Dp) -> bool {
    (length - expected).abs() < 0.5.dp()
}

#[test]
fn the_keyboard_docks_types_switches_layouts_and_follows_the_arrow_keys() -> Result<(), Error> {
    let mut stage = Stage::new(Demo::new(Appearance::Night)?)?;
    show(&mut stage)?;
    typing(&mut stage)?;
    shift(&mut stage)?;
    pages(&mut stage)?;
    layouts(&mut stage)?;
    further_letters(&mut stage)?;
    actions(&mut stage)?;
    arrow_keys(&mut stage)?;
    appearances(&mut stage)?;
    Ok(())
}

fn show(stage: &mut Stage) -> Result<(), Error> {
    assert!(
        !stage.shown(),
        "the keyboard hides while no field has focus"
    );
    shot(stage, "hidden")?;
    stage.focus_field()?;
    stage.settle()?;
    let panel = stage.panel()?.bounds;
    let field = (stage.offscreen.view.bounds_of(FIELD)).ok_or(Error::MissingField)?;
    assert!(
        near(panel.y + panel.height, scene::screen().height),
        "{panel:?}"
    );
    assert!(
        field.y + field.height <= panel.y,
        "the field stays in view above the keyboard"
    );
    let key = stage.key_bounds("q")?;
    assert!(
        near(key.height, 76.dp()),
        "keys are as tall as the touch target: {key:?}"
    );
    Ok(())
}

fn typing(stage: &mut Stage) -> Result<(), Error> {
    stage.type_text(QUERY)?;
    assert_eq!(stage.demo.query.get(), QUERY);
    stage.settle()?;
    shot(stage, "rest")?;

    platform(stage, |facts| facts.keyboard.key_preview = false);
    let y = stage.key("y")?;
    stage.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: y,
        kind: PointerKind::Finger,
    }))?;
    stage.settle()?;
    shot(stage, "pressed")?;
    stage.input(InputEvent::TouchCancel(touch(1)))?;
    platform(stage, |facts| facts.keyboard.key_preview = true);
    stage.settle()?;
    assert_eq!(
        stage.demo.query.get(),
        QUERY,
        "a cancelled press types nothing"
    );

    stage.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: y,
        kind: PointerKind::Finger,
    }))?;
    stage.settle()?;
    shot(stage, "preview")?;
    stage.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: y,
        kind: PointerKind::Finger,
    }))?;
    assert_eq!(stage.demo.query.get(), format!("{QUERY}y"));
    stage.tap(copy(CopyKey::Delete))?;
    assert_eq!(stage.demo.query.get(), QUERY);
    stage.settle()?;
    Ok(())
}

fn shift(stage: &mut Stage) -> Result<(), Error> {
    let checked = |stage: &Stage| {
        let shift = stage.node(&Role::Button, copy(CopyKey::Shift));
        shift.and_then(|shift| shift.state.semantics.checked)
    };
    stage.tap(copy(CopyKey::Shift))?;
    assert_eq!(checked(stage), Some(Checked::True));
    stage.settle()?;
    shot(stage, "shift")?;
    stage.tap(copy(CopyKey::Shift))?;
    stage.tap(copy(CopyKey::Shift))?;
    assert_eq!(checked(stage), Some(Checked::Mixed));
    stage.settle()?;
    shot(stage, "caps")?;
    stage.tap(copy(CopyKey::Shift))?;
    assert_eq!(checked(stage), Some(Checked::False));
    stage.settle()?;
    Ok(())
}

fn pages(stage: &mut Stage) -> Result<(), Error> {
    stage.tap(copy(CopyKey::Symbols))?;
    stage.settle()?;
    assert!(stage.node(&Role::Button, ENGLISH.currency()).is_some());
    shot(stage, "symbols")?;
    stage.tap(copy(CopyKey::MoreSymbols))?;
    stage.settle()?;
    assert!(stage.node(&Role::Button, "€").is_some());
    shot(stage, "more-symbols")?;
    stage.tap(copy(CopyKey::Letters))?;
    stage.settle()?;
    assert!(stage.node(&Role::Button, "q").is_some());
    Ok(())
}

fn layouts(stage: &mut Stage) -> Result<(), Error> {
    for (layout, name) in [
        (&GERMAN, "deutsch"),
        (&PORTUGUESE, "portugues"),
        (&ENGLISH, "english"),
    ] {
        stage.tap(copy(CopyKey::Globe))?;
        stage.settle()?;
        assert_shows(stage, layout)?;
        shot(stage, &format!("layout-{name}"))?;
    }
    stage
        .demo
        .settings
        .update(|settings| settings.layouts.push(&FRENCH));
    platform(stage, |facts| facts.locales.push(langid!("fr")));
    stage.demo.layout.set(&FRENCH);
    stage.settle()?;
    assert_shows(stage, &FRENCH)?;
    shot(stage, "layout-francais")?;
    (stage.demo.settings)
        .update(|settings| settings.layouts.retain(|layout| layout.id() != FRENCH.id()));
    platform(stage, |facts| {
        facts.locales.retain(|locale| *locale != langid!("fr"));
    });
    stage.demo.layout.set(&ENGLISH);
    stage.settle()?;

    let english = stage.key_bounds("q")?.width;
    stage.tap(copy(CopyKey::Globe))?;
    stage.settle()?;
    assert!(
        stage.key_bounds("q")?.width < english,
        "German has eleven keys in a row"
    );
    let globe = stage.key("Switch layout")?;
    stage.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: globe,
        kind: PointerKind::Finger,
    }))?;
    stage.run_until(|stage| {
        stage
            .node(&Role::Menu, copy(CopyKey::KeyboardLayouts))
            .is_some()
    })?;
    stage.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: globe,
        kind: PointerKind::Finger,
    }))?;
    assert!(
        stage
            .node(&Role::Menu, copy(CopyKey::KeyboardLayouts))
            .is_some()
    );
    stage.settle()?;
    shot(stage, "globe-list")?;
    let portuguese = stage.find(&Role::MenuItem, "Português")?;
    stage.tap_at(portuguese.bounds.center())?;
    stage.settle()?;
    assert!(
        stage
            .node(&Role::Menu, copy(CopyKey::KeyboardLayouts))
            .is_none()
    );
    assert_shows(stage, &PORTUGUESE)?;
    stage.tap(copy(CopyKey::Globe))?;
    stage.tap(copy(CopyKey::Globe))?;
    stage.settle()?;
    assert_shows(stage, &GERMAN)?;
    Ok(())
}

fn assert_shows(stage: &Stage, layout: &Layout) -> Result<(), Error> {
    assert_eq!(stage.demo.layout.get().id(), layout.id());
    let space = stage.find(&Role::Button, copy(CopyKey::Space))?;
    assert_eq!(
        space.value,
        Some(AccessibleValue::Text(layout.name().to_string()))
    );
    Ok(())
}

fn further_letters(stage: &mut Stage) -> Result<(), Error> {
    let a = stage.key("a")?;
    hold(stage, "a")?;
    shot(stage, "accents-deutsch-a")?;
    assert!(stage.node(&Role::MenuItem, "á").is_some());
    assert!(
        stage.node(&Role::MenuItem, "ä").is_none(),
        "ä has a key on the German layout"
    );
    stage.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: a,
        kind: PointerKind::Finger,
    }))?;
    assert_eq!(stage.demo.query.get(), format!("{QUERY}à"));
    stage.tap(copy(CopyKey::Delete))?;
    stage.tap(copy(CopyKey::Globe))?;
    stage.settle()?;
    hold(stage, "c")?;
    shot(stage, "accents-portugues-c")?;
    stage.input(InputEvent::TouchCancel(touch(1)))?;
    stage.settle()?;
    hold(stage, "e")?;
    shot(stage, "accents-portugues-e")?;
    let circumflex = stage.find(&Role::MenuItem, "ê")?.bounds.center();
    stage.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: circumflex,
        kind: PointerKind::Finger,
    }))?;
    stage.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: circumflex,
        kind: PointerKind::Finger,
    }))?;
    assert_eq!(stage.demo.query.get(), format!("{QUERY}ê"));
    stage.tap(copy(CopyKey::Delete))?;
    assert_eq!(stage.demo.query.get(), QUERY);
    stage.tap(copy(CopyKey::Globe))?;
    stage.settle()?;
    assert_shows(stage, &ENGLISH)?;
    Ok(())
}

/// Touches the key named `name` without a release until the menu of more letters for that key appears. Then frames
/// the view until the motions of the scene come to rest.
fn hold(stage: &mut Stage, name: &str) -> Result<(), Error> {
    let at = stage.key(name)?;
    stage.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at,
        kind: PointerKind::Finger,
    }))?;
    stage.run_until(|stage| {
        stage
            .node(&Role::Menu, copy(CopyKey::MoreLetters))
            .is_some()
    })?;
    stage.settle()
}

fn actions(stage: &mut Stage) -> Result<(), Error> {
    for (action, word) in [
        (TextAction::Search, CopyKey::ActionSearch),
        (TextAction::Done, CopyKey::ActionDone),
        (TextAction::Next, CopyKey::ActionNext),
        (TextAction::Go, CopyKey::ActionGo),
        (TextAction::Send, CopyKey::ActionSend),
    ] {
        let word = copy(word);
        stage.demo.action.set(action);
        stage.offscreen.frame()?;
        stage.settle()?;
        assert!(
            stage.node(&Role::Button, word).is_some(),
            "the action key shows {word}"
        );
        shot(stage, &format!("action-{}", word.to_lowercase()))?;
    }
    stage.demo.action.set(TextAction::Done);
    stage.offscreen.frame()?;
    stage.settle()?;
    stage.tap(copy(CopyKey::ActionDone))?;
    stage.settle()?;
    assert!(!stage.shown(), "Done hides the keyboard");
    assert_eq!(stage.demo.query.get(), QUERY);
    stage.focus_field()?;
    stage.settle()?;
    assert!(stage.shown(), "a press on the field shows it again");
    Ok(())
}

fn arrow_keys(stage: &mut Stage) -> Result<(), Error> {
    let turn = |stage: &mut Stage, keys: &[Key]| {
        for key in keys {
            let input = InputEvent::Key {
                key: *key,
                shift: false,
            };
            assert!(
                stage.demo.route(input).is_none(),
                "the keyboard takes {key:?}"
            );
        }
        stage.settle()
    };
    turn(stage, &[Key::Right, Key::Down])?;
    turn(stage, &[Key::Right; 6])?;
    shot(stage, "focus-ring")?;
    turn(stage, &[Key::Down, Key::Down, Key::Right])?;
    shot(stage, "focus-action")?;
    turn(stage, &[Key::Enter])?;
    assert!(!stage.shown(), "Enter on the action key runs Done");
    stage.focus_field()?;
    stage.settle()?;
    let q = stage.key("q")?;
    stage.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: q,
        kind: PointerKind::Finger,
    }))?;
    stage.input(InputEvent::TouchCancel(touch(1)))?;
    stage.settle()?;
    assert_eq!(stage.demo.query.get(), QUERY);
    Ok(())
}

fn appearances(stage: &mut Stage) -> Result<(), Error> {
    stage.demo.appearance.set(Appearance::Day);
    stage.settle()?;
    shot(stage, "day")?;
    stage.demo.appearance.set(Appearance::Night);
    stage.offscreen.view.provide(&REDUCED_TRANSPARENCY, true);
    stage.settle()?;
    shot(stage, "opaque")?;
    stage.offscreen.view.provide(&REDUCED_TRANSPARENCY, false);
    Ok(())
}
