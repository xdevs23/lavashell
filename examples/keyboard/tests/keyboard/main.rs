// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod layouts;
mod placement;

use lavashell::keyboard::ENGLISH;
use lavashell::prelude::*;
use lavashell_example_support::testing::{KeyboardKey, capture_path, write_first_frames};
use lavashell_example_support::{Start, Steps, default_app, node};
use lavashell_on_screen_keyboard::{
    CopyKey, Error, app, copy,
    model::{Appearance, Demo},
    scene::{self, FIELD},
    stage::{QUERY, finger, key, key_bounds, key_name, panel, tap, tap_field_end, touch},
};
use std::path::Path;

// The codes are the Linux evdev codes from include/uapi/linux/input-event-codes.h
// (https://github.com/torvalds/linux/blob/master/include/uapi/linux/input-event-codes.h).
const RIGHT: KeyboardKey = KeyboardKey {
    code: 106,
    key: Key::Right,
};
const DOWN: KeyboardKey = KeyboardKey {
    code: 108,
    key: Key::Down,
};
const ENTER: KeyboardKey = KeyboardKey {
    code: 28,
    key: Key::Enter,
};

/// Once a frame shows the scene with every motion at rest, while a timer such as a long press may
/// still wait.
fn settled() -> Start {
    Start::when(|view| !view.motion().is_active() && !view.needs_frame())
}

/// Once a frame shows the menu named `menu` with every motion at rest.
fn showing(menu: CopyKey) -> Start {
    Start::when(move |view| {
        !view.motion().is_active() && node(view, &Role::Menu, copy(menu)).is_ok()
    })
}

type Play = Steps<Demo, String, Error>;

/// Labels the frame of the scene, taken once `start` is met, as the shot `name`.
fn shot(play: &mut Play, start: Start, name: &str) {
    play.mark(start, name.to_owned());
}

/// Plays `parts` one after another on one app and writes each shot into the subdirectory `group` of the keyboard
/// captures. Each part starts in the state left by the part before it.
fn run(group: &str, parts: &[fn(&mut Play)]) -> Result<(), Error> {
    let demo = Demo::new(Appearance::Night)?;
    let mut play = Play::new(demo.clone());
    for part in parts {
        part(&mut play);
    }
    let app = app(default_app()?, &demo, scene::screen(), false)?;
    let frames = play.play(app, scene::density())?;
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("keyboard")
        .join(group);
    let shots = (frames.into_iter())
        .map(|(name, frame)| (capture_path(&directory, &name), frame))
        .collect();
    Ok(write_first_frames(shots)?)
}

fn pixel() -> Dp {
    Viewport::new(scene::screen(), scene::density()).pixel()
}

/// Puts the first finger down on, or lifts it from, the key named `name`, with the input that
/// `event` makes.
fn touch_key(
    view: &mut View,
    name: &str,
    event: fn(TouchSample) -> InputEvent,
) -> Result<(), Error> {
    let at = key(view, name)?;
    view.input(event(finger(1, at)));
    Ok(())
}

/// Lets `change` edit a copy of the current platform facts of the view and gives the edited facts
/// to the view.
fn platform(view: &View, change: impl FnOnce(&mut PlatformFacts)) {
    let mut facts = view.platform();
    change(&mut facts);
    view.provide_platform(facts);
}

fn near(length: Dp, expected: Dp) -> bool {
    (length - expected).abs() < 0.5.dp()
}

fn query(expected: String) -> impl FnOnce(&View, Viewport, &Demo) -> Result<(), Error> {
    move |_, _, demo| {
        assert_eq!(demo.query.get(), expected);
        Ok(())
    }
}

/// Sets the state of the demo that `state` selects to `value`.
fn set<T: 'static>(
    state: fn(&Demo) -> &State<T>,
    value: T,
) -> impl FnOnce(&mut View, Viewport, &Demo) -> Result<(), Error> {
    move |_, _, demo| {
        state(demo).set(value);
        Ok(())
    }
}

#[test]
fn the_keyboard_shows_types_shifts_and_switches_pages_and_appearances() -> Result<(), Error> {
    run("basics", &[show, typing, shift, pages, appearances])
}

#[test]
fn the_keyboard_switches_layouts_and_offers_further_letters() -> Result<(), Error> {
    run(
        "layouts",
        &[focused, layouts::layouts, layouts::further_letters],
    )
}

#[test]
fn the_keyboard_shows_the_action_of_the_field_and_follows_the_arrow_keys() -> Result<(), Error> {
    run("actions", &[focused, actions, arrow_keys])
}

#[test]
fn the_keyboard_docks_at_either_side_and_follows_a_pinch() -> Result<(), Error> {
    run("placement", &[focused, placement::dock, placement::pinch])
}

fn show(play: &mut Play) {
    play.check(settled(), |view, _, demo| {
        assert!(
            panel(view, demo).is_err(),
            "the keyboard hides while no field has focus"
        );
        Ok(())
    });
    shot(play, settled(), "hidden");
    tap_field(play);
    play.check(settled(), |view, _, demo| {
        let panel = panel(view, demo)?.bounds;
        let field = (view.bounds_of(FIELD)).ok_or(Error::MissingField)?;
        assert!(
            near(panel.y + panel.height, scene::screen().height),
            "{panel:?}"
        );
        assert!(
            field.y + field.height <= panel.y,
            "the field is in view above the keyboard"
        );
        let key = key_bounds(view, "q")?;
        assert!(
            near(key.height, 76.dp()),
            "keys are as tall as the touch target: {key:?}"
        );
        Ok(())
    });
}

/// Shows the keyboard and types the query, without the checks and shots of `show` and `typing`.
fn focused(play: &mut Play) {
    tap_field(play);
    type_query(play);
}

fn tap_field(play: &mut Play) {
    play.push(settled(), |view, _, _| tap_field_end(view, pixel()));
}

fn type_query(play: &mut Play) {
    play.push(settled(), |view, _, demo| {
        (QUERY.chars()).try_for_each(|character| tap(view, &key_name(demo, character)))
    });
    play.check(settled(), query(QUERY.to_owned()));
}

fn typing(play: &mut Play) {
    type_query(play);
    shot(play, settled(), "rest");
    play.push(settled(), |view, _, _| {
        platform(view, |facts| facts.keyboard.key_preview = false);
        touch_key(view, "y", InputEvent::TouchDown)
    });
    shot(play, settled(), "pressed");
    play.push(settled(), |view, _, _| {
        view.input(InputEvent::TouchCancel(touch(1)));
        platform(view, |facts| facts.keyboard.key_preview = true);
        Ok(())
    });
    play.check(settled(), |_, _, demo| {
        assert_eq!(demo.query.get(), QUERY, "a cancelled press types nothing");
        Ok(())
    });
    play.push(settled(), |view, _, _| {
        touch_key(view, "y", InputEvent::TouchDown)
    });
    shot(play, settled(), "preview");
    play.push(settled(), |view, _, _| {
        touch_key(view, "y", InputEvent::TouchUp)
    });
    play.check(Start::NEXT, query(format!("{QUERY}y")));
    play.push(Start::NEXT, |view, _, _| tap(view, copy(CopyKey::Delete)));
    play.check(Start::NEXT, query(QUERY.to_owned()));
}

fn tap_shift(view: &mut View, _: Viewport, _: &Demo) -> Result<(), Error> {
    tap(view, copy(CopyKey::Shift))
}

fn shift_checked(expected: Checked) -> impl FnOnce(&View, Viewport, &Demo) -> Result<(), Error> {
    move |view, _, _| {
        let shift = node(view, &Role::Button, copy(CopyKey::Shift))?;
        assert_eq!(shift.state.semantics.checked, Some(expected));
        Ok(())
    }
}

fn shift(play: &mut Play) {
    play.push(settled(), tap_shift);
    play.check(Start::NEXT, shift_checked(Checked::True));
    shot(play, settled(), "shift");
    play.push(settled(), tap_shift);
    play.push(Start::NEXT, tap_shift);
    play.check(Start::NEXT, shift_checked(Checked::Mixed));
    shot(play, settled(), "caps");
    play.push(settled(), tap_shift);
    play.check(Start::NEXT, shift_checked(Checked::False));
}

fn pages(play: &mut Play) {
    for (page, key_on_page, capture) in [
        (CopyKey::Symbols, ENGLISH.currency(), Some("symbols")),
        (CopyKey::MoreSymbols, "€", Some("more-symbols")),
        (CopyKey::Letters, "q", None),
    ] {
        play.push(settled(), move |view, _, _| tap(view, copy(page)));
        play.check(settled(), move |view, _, _| {
            assert!(
                node(view, &Role::Button, key_on_page).is_ok(),
                "{key_on_page}"
            );
            Ok(())
        });
        if let Some(name) = capture {
            shot(play, settled(), name);
        }
    }
}

fn actions(play: &mut Play) {
    for (action, word) in [
        (TextAction::Search, CopyKey::ActionSearch),
        (TextAction::Done, CopyKey::ActionDone),
        (TextAction::Next, CopyKey::ActionNext),
        (TextAction::Go, CopyKey::ActionGo),
        (TextAction::Send, CopyKey::ActionSend),
    ] {
        let word = copy(word);
        play.push(settled(), set(|demo| &demo.action, action));
        play.check(settled(), move |view, _, _| {
            assert!(
                node(view, &Role::Button, word).is_ok(),
                "the action key shows {word}"
            );
            Ok(())
        });
        shot(play, settled(), &format!("action-{}", word.to_lowercase()));
    }
    play.push(settled(), set(|demo| &demo.action, TextAction::Done));
    play.push(settled(), |view, _, _| tap(view, copy(CopyKey::ActionDone)));
    play.check(settled(), |view, _, demo| {
        assert!(panel(view, demo).is_err(), "Done hides the keyboard");
        assert_eq!(demo.query.get(), QUERY);
        Ok(())
    });
    tap_field(play);
    play.check(settled(), |view, _, demo| {
        assert!(
            panel(view, demo).is_ok(),
            "a press on the field shows it again"
        );
        Ok(())
    });
}

fn arrow_keys(play: &mut Play) {
    play.press(settled(), [RIGHT, DOWN]);
    play.press(settled(), [RIGHT; 6]);
    shot(play, settled(), "focus-ring");
    play.press(settled(), [DOWN, DOWN, RIGHT]);
    shot(play, settled(), "focus-action");
    play.press(settled(), [ENTER]);
    play.check(settled(), |view, _, demo| {
        assert!(
            panel(view, demo).is_err(),
            "Enter on the action key runs Done"
        );
        Ok(())
    });
    tap_field(play);
    play.push(settled(), |view, _, _| {
        touch_key(view, "q", InputEvent::TouchDown)?;
        view.input(InputEvent::TouchCancel(touch(1)));
        Ok(())
    });
    play.check(settled(), query(QUERY.to_owned()));
}

fn appearances(play: &mut Play) {
    play.push(settled(), set(|demo| &demo.appearance, Appearance::Day));
    shot(play, settled(), "day");
    play.push(settled(), |view, _, demo| {
        demo.appearance.set(Appearance::Night);
        view.provide(&REDUCED_TRANSPARENCY, true);
        Ok(())
    });
    shot(play, settled(), "opaque");
    play.push(settled(), |view, _, _| {
        view.provide(&REDUCED_TRANSPARENCY, false);
        Ok(())
    });
}
