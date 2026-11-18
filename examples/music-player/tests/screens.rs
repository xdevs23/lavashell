// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::prelude::*;
use lavashell_example_support::testing::{
    Shaping, capture_path, click, control, write_first_frames,
};
use lavashell_example_support::{Start, Steps, Theme, ThemeChoice, at_rest, default_app, node};
use lavashell_music_player::{
    Error, app,
    music::{self, AlbumId, CopyKey, PLAYER_HEIGHT, Tag, TrackId, copy},
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::{cell::Cell, rc::Rc};

const WINDOW: Size = Size::new(Dp::new(1400.0), Dp::new(1000.0));

type Play = Steps<State<music::Model>, PathBuf, Error>;

fn tracks() -> Vec<TrackId> {
    TrackId::all().collect()
}

fn capture(name: &str) -> PathBuf {
    capture_path(
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("music-player"),
        name,
    )
}

fn shot(
    play: &mut Play,
    name: &str,
    action: impl FnOnce(&mut View, Viewport, &State<music::Model>) -> Result<(), Error> + 'static,
) {
    play.push_at_rest_and_mark(capture(name), action);
}

fn record(play: Play, app: App) -> Result<(), Error> {
    let frames = play.play(app, Density::ONE)?;
    Ok(write_first_frames(frames)?)
}

fn library(model: &State<music::Model>, size: Size) -> Result<App, Error> {
    let theme = Theme::new(ThemeChoice::System)?;
    app(default_app()?, model, &theme, Some(size))
}

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
        click(view, Tag::SearchInput)?;
        view.input(InputEvent::Text("glass".into()));
        assert_eq!(model.get().query, "glass");
        Ok(())
    });
    play.check(Start::at_rest(), |view, _, _| {
        assert!(view.bounds_of(&Tag::Track(tracks()[4])).is_some());
        assert!(view.bounds_of(&Tag::Track(tracks()[0])).is_none());
        Ok(())
    });
    play.push(Start::at_rest(), |_, _, model| {
        model.update(|m| m.query.clear());
        Ok(())
    });
    shot(&mut play, "music-favorites", |view, _, _| {
        Ok(click(view, Tag::Navigation(CopyKey::NavFavorites))?)
    });
    play.check(Start::at_rest(), |view, _, _| {
        assert!(view.bounds_of(&Tag::Track(tracks()[3])).is_some());
        assert!(view.bounds_of(&Tag::Track(tracks()[6])).is_none());
        Ok(())
    });
    empty_states(&mut play, &shaping);
    play.push(Start::at_rest(), |_, _, model| {
        model.set(music::Model::default());
        Ok(())
    });
    shot(&mut play, "music-album", |view, _, model| {
        assert_eq!(model.get().recent, []);
        click(view, Tag::PlayAlbum)?;
        assert_eq!(model.get().current, tracks()[0]);
        assert_eq!(model.get().album, AlbumId::all().next());
        assert!(model.get().playing);
        Ok(())
    });
    play.push(Start::at_rest(), |view, _, model| {
        click(view, Tag::Next)?;
        assert_eq!(model.get().current, tracks()[1]);
        Ok(())
    });
    shot(&mut play, "music-recent", |view, _, model| {
        click(view, Tag::Navigation(CopyKey::NavRecent))?;
        assert_eq!(model.get().recent, vec![tracks()[1], tracks()[0]]);
        Ok(())
    });
    record(play, library(&model, WINDOW)?)?;

    panel_reaches_the_library()?;
    compact()?;
    scrolled(shaping)
}

type ModelChanged = fn(&music::Model) -> bool;

fn controls(play: &mut Play) {
    let clicks: [(Tag, ModelChanged); 6] = [
        (Tag::PlayToggle, |m| m.playing),
        (Tag::Next, |m| m.current == tracks()[3]),
        (Tag::SaveCurrent, |m| m.favorites.contains(&tracks()[3])),
        (Tag::Shuffle, |m| m.shuffle),
        (Tag::Repeat, |m| m.repeat),
        (Tag::Seek, |m| (m.position.as_secs_f64() - 91.5).abs() < 1.0),
    ];
    for (tag, changed) in clicks {
        play.push(Start::at_rest(), move |view, _, model| {
            click(view, tag)?;
            assert!(changed(&model.get()), "{tag:?}");
            Ok(())
        });
    }
    play.push(Start::at_rest(), |view, _, model| {
        let r = control(view, Tag::Volume)?;
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

fn panel_reaches_the_library() -> Result<(), Error> {
    let model = State::new(music::Model::default());
    let mut app = library(&model, WINDOW)?;
    let (shown, theme) = (model.clone(), Theme::new(ThemeChoice::System)?);
    let icons = music::Icons::decode()?;
    app.window(
        Window::new(copy(CopyKey::Title), "lavashell.music.panel")
            .size(WINDOW.width, PLAYER_HEIGHT),
        move |ui| music::panel(ui, &shown, &theme, icons),
    )?;
    let is_library =
        |view: &View| (view.bounds_of(&Tag::Navigation(CopyKey::NavFavorites))).is_some();
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
        click(view, Tag::Next)?;
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

fn compact() -> Result<(), Error> {
    let model = State::new(music::Model::default());
    let mut play = Play::new(model.clone());
    play.mark(Start::at_rest(), capture("music-compact"));
    record(play, library(&model, Size::new(900.dp(), 850.dp()))?)
}

fn scrolled(shaping: Shaping) -> Result<(), Error> {
    let model = State::new(music::Model::default());
    let mut play = Play::new(model.clone());
    shot(&mut play, "music-scrolled", |view, _, _| {
        let area = control(view, Tag::CollectionScroll)?;
        view.input(InputEvent::Scroll {
            at: Point::new(area.x + 20.dp(), area.y + 40.dp()),
            delta: Point::new(Dp::ZERO, 360.dp()),
        });
        Ok(())
    });
    play.check(Start::at_rest(), |view, _, model| {
        let area = control(view, Tag::CollectionScroll)?;
        let current = control(view, Tag::Track(model.get().current))?;
        assert!(current.y >= area.y && current.y + current.height < WINDOW.height - PLAYER_HEIGHT);
        Ok(())
    });
    play.push(Start::at_rest(), move |view, viewport, _| {
        let r = control(view, Tag::PlayToggle)?;
        view.input(InputEvent::Move(Point::new(r.x + 10.dp(), r.y + 10.dp())));
        assert!(painted_texts(&shaping, view, viewport)?.contains(copy(CopyKey::Play)));
        Ok(())
    });
    record(play, library(&model, WINDOW)?)
}

#[test]
fn the_hero_is_shown_only_where_it_fits() -> Result<(), Error> {
    let model = State::new(music::Model::default());
    let fitting = Rc::new(Cell::new(Dp::ZERO));
    let mut tall = Play::new(model.clone());
    let measured = Rc::clone(&fitting);
    tall.check(Start::at_rest(), move |view, _, _| {
        let hero = control(view, Tag::Hero)?;
        let chrome = WINDOW.height - control(view, Tag::CollectionScroll)?.height;
        measured.set(chrome + hero.height);
        Ok(())
    });
    record(tall, library(&model, WINDOW)?)?;
    let height = fitting.get();
    for (hero_fits, height) in [(true, height), (false, height - 1.dp())] {
        let mut play = Play::new(model.clone());
        play.check(Start::at_rest(), move |view, _, _| {
            assert_eq!(view.bounds_of(&Tag::Hero).is_some(), hero_fits);
            Ok(())
        });
        record(play, library(&model, Size::new(WINDOW.width, height))?)?;
    }
    Ok(())
}

#[test]
fn the_theme_switch_repaints_the_screen() -> Result<(), Error> {
    let model = State::new(music::Model::default());
    let shaping = Shaping::new()?;
    let page_fill = move |view: &mut View, viewport| -> Result<Option<DrawCommand>, Error> {
        let tree = shaping.scene(view, viewport)?;
        let now = view.motion().now();
        Ok(tree.at(now).leaf_commands().next().cloned())
    };
    let mut play = Play::new(model.clone());
    play.push(Start::at_rest(), move |view, viewport, _| {
        let by_system = page_fill(view, viewport)?;
        click(view, ThemeChoice::Light)?;
        assert_ne!(page_fill(view, viewport)?, by_system);
        click(view, ThemeChoice::Dark)?;
        assert_eq!(page_fill(view, viewport)?, by_system);
        Ok(())
    });
    record(play, library(&model, WINDOW)?)
}
