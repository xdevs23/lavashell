// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use icu_locale_core::langid;
use lavashell::keyboard::{Layout, Placement, ShippedLayouts, Side};
use lavashell::prelude::*;
use lavashell_example_support::{FrameInterval, Recording, ThemeChoice, testing::click};
use lavashell_on_screen_keyboard::{
    CopyKey, Error, copy,
    model::{Demo, docked},
    scene::{self, FIELD},
    stage::{QUERY, Stage, touch},
};
use std::{path::Path, time::Duration};

fn shot(stage: &mut Stage, name: &str) -> Result<(), Error> {
    let captures = Path::new(env!("CARGO_TARGET_TMPDIR")).join("keyboard");
    Ok(stage.offscreen.save(captures.join(format!("{name}.png")))?)
}

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
    let mut stage = Stage::new(Demo::new(ThemeChoice::Dark)?)?;
    show(&mut stage)?;
    typing(&mut stage)?;
    shift(&mut stage)?;
    pages(&mut stage)?;
    layouts(&mut stage)?;
    further_letters(&mut stage)?;
    actions(&mut stage)?;
    arrow_keys(&mut stage)?;
    dock(&mut stage)?;
    pinch(&mut stage)?;
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
    let field = (stage.offscreen.view.bounds_of(&FIELD)).ok_or(Error::MissingField)?;
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
    stage.input(InputEvent::TouchDown(TouchSample::finger(touch(1), y)))?;
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

    stage.input(InputEvent::TouchDown(TouchSample::finger(touch(1), y)))?;
    stage.settle()?;
    shot(stage, "preview")?;
    stage.input(InputEvent::TouchUp(TouchSample::finger(touch(1), y)))?;
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
    let shipped = ShippedLayouts::load()?;
    stage.tap(copy(CopyKey::Symbols))?;
    stage.settle()?;
    assert!(
        stage
            .node(&Role::Button, shipped.english.currency().symbol())
            .is_some()
    );
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
    let shipped = ShippedLayouts::load()?;
    for (layout, name) in [
        (&shipped.german, "deutsch"),
        (&shipped.portuguese, "portugues"),
        (&shipped.english, "english"),
    ] {
        stage.tap(copy(CopyKey::Globe))?;
        stage.settle()?;
        assert_shows(stage, layout)?;
        shot(stage, &format!("layout-{name}"))?;
    }
    stage
        .demo
        .settings
        .update(|settings| settings.layouts.push(&shipped.french));
    platform(stage, |facts| facts.locales.push(langid!("fr")));
    stage.demo.layout.set(&shipped.french);
    stage.settle()?;
    assert_shows(stage, &shipped.french)?;
    shot(stage, "layout-francais")?;
    (stage.demo.settings)
        .update(|settings| settings.layouts.retain(|layout| **layout != shipped.french));
    platform(stage, |facts| {
        facts.locales.retain(|locale| *locale != langid!("fr"));
    });
    stage.demo.layout.set(&shipped.english);
    stage.settle()?;

    let english = stage.key_bounds("q")?.width;
    stage.tap(copy(CopyKey::Globe))?;
    stage.settle()?;
    assert!(
        stage.key_bounds("q")?.width < english,
        "German has eleven keys in a row"
    );
    let globe = stage.key("Switch layout")?;
    stage.input(InputEvent::TouchDown(TouchSample::finger(touch(1), globe)))?;
    stage.run_until(|stage| {
        stage
            .node(&Role::Menu, copy(CopyKey::KeyboardLayouts))
            .is_some()
    })?;
    stage.input(InputEvent::TouchUp(TouchSample::finger(touch(1), globe)))?;
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
    assert_shows(stage, &shipped.portuguese)?;
    stage.tap(copy(CopyKey::Globe))?;
    stage.tap(copy(CopyKey::Globe))?;
    stage.settle()?;
    assert_shows(stage, &shipped.german)?;
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
    let shipped = ShippedLayouts::load()?;
    let a = stage.key("a")?;
    hold(stage, "a")?;
    shot(stage, "accents-deutsch-a")?;
    assert!(stage.node(&Role::MenuItem, "á").is_some());
    assert!(
        stage.node(&Role::MenuItem, "ä").is_none(),
        "ä has a key on the German layout"
    );
    stage.input(InputEvent::TouchUp(TouchSample::finger(touch(1), a)))?;
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
    stage.input(InputEvent::TouchMove(TouchSample::finger(
        touch(1),
        circumflex,
    )))?;
    stage.input(InputEvent::TouchUp(TouchSample::finger(
        touch(1),
        circumflex,
    )))?;
    assert_eq!(stage.demo.query.get(), format!("{QUERY}ê"));
    stage.tap(copy(CopyKey::Delete))?;
    assert_eq!(stage.demo.query.get(), QUERY);
    stage.tap(copy(CopyKey::Globe))?;
    stage.settle()?;
    assert_shows(stage, &shipped.english)?;
    Ok(())
}

fn hold(stage: &mut Stage, name: &str) -> Result<(), Error> {
    let at = stage.key(name)?;
    stage.input(InputEvent::TouchDown(TouchSample::finger(touch(1), at)))?;
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
    stage.input(InputEvent::TouchDown(TouchSample::finger(touch(1), q)))?;
    stage.input(InputEvent::TouchCancel(touch(1)))?;
    stage.settle()?;
    assert_eq!(stage.demo.query.get(), QUERY);
    Ok(())
}

fn docked_at(panel: Rect, side: Side) -> bool {
    let screen = scene::screen();
    let (left, right) = (panel.x, screen.width - panel.x - panel.width);
    let nearer = match side {
        Side::Left => left < right,
        Side::Right => right < left,
    };
    nearer && panel.width < screen.width && panel.y + panel.height < screen.height
}

fn full_width(panel: Rect) -> bool {
    let screen = scene::screen();
    near(panel.x, Dp::ZERO)
        && near(panel.width, screen.width)
        && near(panel.y + panel.height, screen.height)
}

fn dock(stage: &mut Stage) -> Result<(), Error> {
    for (side, name) in [(Side::Left, "left"), (Side::Right, "right")] {
        stage.demo.placement.set(docked(side));
        stage.settle()?;
        let panel = stage.panel()?.bounds;
        assert!(docked_at(panel, side), "{panel:?} at {side:?}");
        let field = (stage.offscreen.view.bounds_of(&FIELD)).ok_or(Error::MissingField)?;
        assert!(
            field.y + field.height <= panel.y,
            "the field is in view above the dock"
        );
        shot(stage, &format!("dock-{name}"))?;
    }
    stage.demo.placement.set(Placement::FullWidth);
    stage.settle()
}

fn pinch_keys(stage: &mut Stage, first: &str, second: &str, by: Dp) -> Result<(), Error> {
    let (start, end) = (stage.key(first)?, stage.key(second)?);
    let halfway = Point::new(end.x + by / 2.0, end.y);
    let moved = Point::new(end.x + by, end.y);
    let finger = |point, at| TouchSample::finger(touch(point), at);
    stage.input(InputEvent::TouchDown(finger(1, start)))?;
    stage.input(InputEvent::TouchDown(finger(2, end)))?;
    stage.input(InputEvent::TouchMove(finger(2, halfway)))?;
    stage.input(InputEvent::TouchMove(finger(2, moved)))?;
    stage.input(InputEvent::TouchUp(finger(2, moved)))?;
    stage.input(InputEvent::TouchUp(finger(1, start)))?;
    stage.settle()
}

fn pinch(stage: &mut Stage) -> Result<(), Error> {
    pinch_keys(stage, "d", "k", -250.dp())?;
    assert_eq!(stage.demo.placement.get(), docked(Side::Left));
    let panel = stage.panel()?.bounds;
    assert!(docked_at(panel, Side::Left), "{panel:?}");
    pinch_keys(stage, "s", "k", 350.dp())?;
    assert_eq!(stage.demo.placement.get(), Placement::FullWidth);
    let panel = stage.panel()?.bounds;
    assert!(full_width(panel), "{panel:?}");
    pinch_keys(stage, "d", "k", -50.dp())?;
    assert_eq!(
        stage.demo.placement.get(),
        Placement::FullWidth,
        "fingers that drift a little leave the placement as it is"
    );
    assert_eq!(
        stage.demo.query.get(),
        QUERY,
        "a pinch leaves the query unchanged"
    );
    Ok(())
}

fn appearances(stage: &mut Stage) -> Result<(), Error> {
    stage.demo.theme.choose(ThemeChoice::Light);
    stage.settle()?;
    shot(stage, "day")?;
    stage.demo.theme.choose(ThemeChoice::Dark);
    stage.offscreen.view.provide(&REDUCED_TRANSPARENCY, true);
    stage.settle()?;
    shot(stage, "opaque")?;
    stage.offscreen.view.provide(&REDUCED_TRANSPARENCY, false);
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum RecordingError {
    #[error(transparent)]
    Keyboard(#[from] Error),
    #[error("a recording needs a time between its frames")]
    NoInterval,
}

#[test]
fn a_recording_writes_each_sequence_and_each_still() -> Result<(), RecordingError> {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("keyboard-recording");
    let _absent_before_the_first_run = std::fs::remove_dir_all(&directory);
    let interval =
        FrameInterval::new(Duration::from_secs(1) / 10).ok_or(RecordingError::NoInterval)?;
    let recording = Recording {
        directory,
        interval,
    };
    lavashell_on_screen_keyboard::recording::record(&recording, ThemeChoice::Dark)?;
    for sequence in ["show", "hide", "preview", "previews", "dock"] {
        let first = recording.directory.join(sequence).join("000.png");
        assert!(first.is_file(), "the sequence {sequence} has a first frame");
    }
    for still in ["reduced-motion-before", "reduced-motion-after"] {
        let path = recording.directory.join(format!("{still}.png"));
        assert!(path.is_file(), "the still {still} is written");
    }
    Ok(())
}

#[test]
fn the_theme_switch_in_the_taskbar_changes_the_theme() -> Result<(), Error> {
    let mut stage = Stage::new(Demo::new(ThemeChoice::Dark)?)?;
    for choice in [ThemeChoice::Light, ThemeChoice::System, ThemeChoice::Dark] {
        click(&mut stage.offscreen.view, choice)?;
        stage.settle()?;
        assert_eq!(stage.demo.theme.choice(), choice);
    }
    Ok(())
}
