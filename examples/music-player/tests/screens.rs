// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::prelude::*;
use lavashell_example_support::testing::{
    Shaping, capture_path, click, control, write_first_frames,
};
use lavashell_example_support::{Start, Steps, at_rest, default_app, node};
use lavashell_music_player::{
    Error, app,
    music::{self, CopyKey, PLAYER_HEIGHT, copy},
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::{cell::Cell, rc::Rc};

/// The size of the desktop window in which the tests show the library. The window is wide enough for the sidebar
/// next to the album row and the track list.
const WINDOW: Size = Size::new(Dp::new(1400.0), Dp::new(1000.0));

type Play = Steps<State<music::Model>, PathBuf, Error>;

/// The path of the capture `name`.
fn capture(name: &str) -> PathBuf {
    capture_path(
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("music-player"),
        name,
    )
}

/// Adds `action` as a step, and writes the frame that shows it to the capture `name`.
fn shot(
    play: &mut Play,
    name: &str,
    action: impl FnOnce(&mut View, Viewport, &State<music::Model>) -> Result<(), Error> + 'static,
) {
    play.push(Start::at_rest(), action);
    play.mark(Start::NEXT, capture(name));
}

/// Plays `play` on `app` and writes the captures.
fn record(play: Play, app: App) -> Result<(), Error> {
    let frames = play.play(app, Density::ONE)?;
    Ok(write_first_frames(frames)?)
}

/// The texts that a frame of `view` in `viewport` paints.
fn painted_texts(
    shaping: &Shaping,
    view: &mut View,
    viewport: Viewport,
) -> Result<BTreeSet<String>, Error> {
    let tree = shaping.scene(view, viewport)?;
    let now = view.motion().now();
    Ok((tree.at(now).leaf_commands())
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect())
}

#[test]
fn controls_update_the_shared_model_and_render_every_screen() -> Result<(), Error> {
    let model = State::new(music::Model::default());
    let shaping = Shaping::new()?;
    let mut play = Play::new(model.clone());
    play.mark(Start::at_rest(), capture("music"));
    controls(&mut play);
    shot(&mut play, "music-search", |view, _, model| {
        click(view, "search-input")?;
        view.input(InputEvent::Text("glass".into()));
        assert_eq!(model.get().query, "glass");
        Ok(())
    });
    play.check(Start::at_rest(), |view, _, _| {
        assert!(view.bounds_of("track-4").is_some());
        assert!(view.bounds_of("track-0").is_none());
        Ok(())
    });
    play.push(Start::at_rest(), |_, _, model| {
        model.update(|m| m.query.clear());
        Ok(())
    });
    shot(&mut play, "music-favorites", |view, _, _| {
        Ok(click(view, "nav-favorites")?)
    });
    play.check(Start::at_rest(), |view, _, _| {
        assert!(view.bounds_of("track-3").is_some());
        assert!(view.bounds_of("track-6").is_none());
        Ok(())
    });
    empty_states(&mut play, &shaping);
    play.push(Start::at_rest(), |_, _, model| {
        model.set(music::Model::default());
        Ok(())
    });
    shot(&mut play, "music-album", |view, _, model| {
        assert!(model.get().recent.is_empty());
        click(view, "play-album")?;
        assert_eq!(model.get().current, 0);
        assert_eq!(model.get().album, Some(0));
        assert!(model.get().playing);
        Ok(())
    });
    play.push(Start::at_rest(), |view, _, model| {
        click(view, "next")?;
        assert_eq!(model.get().current, 1);
        Ok(())
    });
    shot(&mut play, "music-recent", |view, _, model| {
        click(view, "nav-recent")?;
        assert_eq!(model.get().recent, vec![1, 0]);
        Ok(())
    });
    record(play, app(default_app()?, &model, Some(WINDOW))?)?;

    panel_reaches_the_library()?;
    compact()?;
    scrolled(shaping)
}

/// A check that a click changed the model.
type ModelChanged = fn(&music::Model) -> bool;

/// Plays the buttons and the volume slider of the player.
fn controls(play: &mut Play) {
    let clicks: [(&str, ModelChanged); 6] = [
        ("play-toggle", |m| m.playing),
        ("next", |m| m.current == 3),
        ("save-current", |m| m.favorites.contains(&3)),
        ("shuffle", |m| m.shuffle),
        ("repeat", |m| m.repeat),
        ("seek", |m| (m.position.as_secs_f64() - 91.5).abs() < 1.0),
    ];
    for (tag, changed) in clicks {
        play.push(Start::at_rest(), move |view, _, model| {
            click(view, tag)?;
            assert!(changed(&model.get()), "{tag}");
            Ok(())
        });
    }
    play.push(Start::at_rest(), |view, _, model| {
        let r = control(view, "volume")?;
        let p = Point::new(r.x + r.width * 0.3, r.y + r.height / 2.0);
        view.input(InputEvent::Press(p));
        view.input(InputEvent::Release(p));
        assert!((model.get().volume - 0.3).abs() < 0.01);
        Ok(())
    });
    play.push(Start::at_rest(), |view, _, model| {
        let volume = node(view, &Role::Slider, copy(CopyKey::Volume))?.id;
        assert_eq!(
            view.accessibility_action(ActionRequest {
                target: volume,
                action: AccessibilityAction::Focus
            }),
            Ok(ActionOutcome::Performed)
        );
        view.input(InputEvent::Key {
            key: Key::End,
            shift: false,
        });
        assert_eq!(
            model.get().volume.to_bits(),
            1.0_f64.to_bits(),
            "keyboard volume must use the same model callback"
        );
        assert_eq!(
            view.accessibility_action(ActionRequest {
                target: volume,
                action: AccessibilityAction::SetValue(AccessibleValue::Number(0.3)),
            }),
            Ok(ActionOutcome::Performed)
        );
        assert!((model.get().volume - 0.3).abs() < 0.01);
        Ok(())
    });
}

/// Checks the hints of an empty favorites list and of a search without results.
fn empty_states(play: &mut Play, shaping: &Shaping) {
    shot(play, "music-empty-favorites", |_, _, model| {
        model.update(|m| {
            m.favorites.clear();
            m.query.clear();
            m.section = music::Section::Favorites;
        });
        Ok(())
    });
    let favorites = shaping.clone();
    play.push(Start::at_rest(), move |view, viewport, _| {
        let texts = painted_texts(&favorites, view, viewport)?;
        assert!(texts.contains(copy(CopyKey::CollectionEmpty)));
        assert!(!texts.contains(copy(CopyKey::SearchEmpty)));
        Ok(())
    });
    shot(play, "music-empty-search", |_, _, model| {
        model.set(music::Model::default());
        model.update(|m| m.query = "not-in-the-sample-collection".into());
        Ok(())
    });
    let search = shaping.clone();
    play.push(Start::at_rest(), move |view, viewport, _| {
        let texts = painted_texts(&search, view, viewport)?;
        assert!(texts.contains(copy(CopyKey::SearchEmpty)));
        Ok(())
    });
}

/// Checks that the player panel, in a separate window, changes the model shown by the library. The library then
/// composes again.
fn panel_reaches_the_library() -> Result<(), Error> {
    let model = State::new(music::Model::default());
    let mut app = app(default_app()?, &model, Some(WINDOW))?;
    let (shown, icons) = (model.clone(), music::Icons::decode()?);
    app.window(
        Window::new(copy(CopyKey::Title), "lavashell.music.panel")
            .size(WINDOW.width, PLAYER_HEIGHT),
        move |ui| music::panel(ui, &shown, icons),
    )?;
    let is_library = |view: &View| view.bounds_of("nav-favorites").is_some();
    let mut play = Play::new(model);
    let composed = Rc::new(Cell::new(0));
    let counted = Rc::clone(&composed);
    let library_at_rest = Start::when(move |view| is_library(view) && at_rest(view));
    play.check(library_at_rest, move |view, _, _| {
        counted.set(view.stats().root_compositions);
        Ok(())
    });
    let panel_at_rest = Start::when(move |view| !is_library(view) && at_rest(view));
    play.push(panel_at_rest, |view, _, model| {
        let previous = model.get().current;
        click(view, "next")?;
        assert_ne!(model.get().current, previous);
        Ok(())
    });
    play.check(Start::when(is_library), move |view, _, _| {
        assert!(
            view.stats().root_compositions > composed.get(),
            "a panel action must invalidate the other view"
        );
        Ok(())
    });
    record(play, app)
}

/// Checks that the first track ends above the player panel in a compact window.
fn compact() -> Result<(), Error> {
    let model = State::new(music::Model::default());
    let mut play = Play::new(model.clone());
    play.mark(Start::at_rest(), capture("music-compact"));
    play.check(Start::at_rest(), |view, _, _| {
        let first = control(view, "track-0")?;
        assert!(first.y + first.height <= 850.dp() - PLAYER_HEIGHT);
        Ok(())
    });
    let compact = Size::new(900.dp(), 850.dp());
    record(play, app(default_app()?, &model, Some(compact))?)
}

/// Checks that the current track is visible above the player panel after a scroll, and that the pointer over the
/// play button shows the text Play.
fn scrolled(shaping: Shaping) -> Result<(), Error> {
    let model = State::new(music::Model::default());
    let mut play = Play::new(model.clone());
    shot(&mut play, "music-scrolled", |view, _, _| {
        let area = control(view, "collection-scroll")?;
        view.input(InputEvent::Scroll {
            at: Point::new(area.x + 20.dp(), area.y + 40.dp()),
            delta: Point::new(Dp::ZERO, 360.dp()),
        });
        Ok(())
    });
    play.check(Start::at_rest(), |view, _, _| {
        let area = control(view, "collection-scroll")?;
        let current = control(view, "track-2")?;
        assert!(current.y >= area.y && current.y + current.height < WINDOW.height - PLAYER_HEIGHT);
        Ok(())
    });
    play.push(Start::at_rest(), move |view, viewport, _| {
        let r = control(view, "play-toggle")?;
        view.input(InputEvent::Move(Point::new(r.x + 10.dp(), r.y + 10.dp())));
        assert!(painted_texts(&shaping, view, viewport)?.contains(copy(CopyKey::Play)));
        Ok(())
    });
    record(play, app(default_app()?, &model, Some(WINDOW))?)
}
