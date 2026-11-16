// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::prelude::*;
use lavashell_example_support::{
    Capture, Theme, ThemeChoice,
    testing::{click, control, node, scene},
    without_failures,
};
use lavashell_music_player::{
    Error,
    music::{self, AlbumId, CopyKey, PLAYER_HEIGHT, Tag, TrackId, copy},
};
use std::{path::Path, time::Duration};

const WINDOW: Size = Size::new(Dp::new(1400.0), Dp::new(1000.0));

fn show(
    view: &mut View,
    builder: &mut SceneBuilder,
    viewport: Viewport,
    capture: &mut Capture,
) -> Result<(), Error> {
    without_failures(view.frame_into(viewport, capture.painter.shaper(), builder))
        .map_err(Error::Example)
}

#[test]
fn controls_update_the_shared_model_and_render_every_screen() -> Result<(), Error> {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("music-player");
    let model = State::new(music::Model::default());
    let icons = music::Icons::decode()?;
    let theme = Theme::new(ThemeChoice::System)?;
    let mut capture = Capture::new()?;
    let state = model.clone();
    let shown_theme = theme.clone();
    let mut view = View::new(lavashell_example_support::platform(), move |ui| {
        music::music(ui, &state, &shown_theme, icons);
    });
    let mut shown = SceneBuilder::default();
    let tracks: Vec<_> = TrackId::all().collect();
    let albums: Vec<_> = AlbumId::all().collect();
    let size = WINDOW;
    let window = Viewport::new(size, Density::ONE);
    capture.save(&mut view, window, directory.join("music.png"))?;

    click(&mut view, Tag::PlayToggle)?;
    assert!(model.get().playing);
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, Tag::Next)?;
    assert_eq!(model.get().current, tracks[3]);
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, Tag::SaveCurrent)?;
    assert!(model.get().favorites.contains(&tracks[3]));
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, Tag::Shuffle)?;
    assert!(model.get().shuffle);
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, Tag::Repeat)?;
    assert!(model.get().repeat);
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, Tag::Seek)?;
    assert!((model.get().position.as_secs_f64() - 91.5).abs() < 1.0);
    show(&mut view, &mut shown, window, &mut capture)?;

    let r = control(&view, Tag::Volume)?;
    let p = Point::new(r.x + r.width * 0.3, r.y + r.height / 2.0);
    view.input(InputEvent::Press(p));
    view.input(InputEvent::Release(p));
    assert!((model.get().volume - 0.3).abs() < 0.01);
    show(&mut view, &mut shown, window, &mut capture)?;
    let volume = node(&view, &Role::Slider, copy(CopyKey::Volume))?.id;
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
    show(&mut view, &mut shown, window, &mut capture)?;

    click(&mut view, Tag::SearchInput)?;
    view.input(InputEvent::Text("glass".into()));
    assert_eq!(model.get().query, "glass");
    capture.save(&mut view, window, directory.join("music-search.png"))?;
    assert!(view.bounds_of(&Tag::Track(tracks[4])).is_some());
    assert!(view.bounds_of(&Tag::Track(tracks[0])).is_none());
    model.update(|m| m.query.clear());
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, Tag::Navigation(CopyKey::NavFavorites))?;
    capture.save(&mut view, window, directory.join("music-favorites.png"))?;
    assert!(view.bounds_of(&Tag::Track(tracks[3])).is_some());
    assert!(view.bounds_of(&Tag::Track(tracks[6])).is_none());

    panel_reaches_the_library(&view, &model, &theme, icons, &mut capture)?;

    empty_states(&mut view, &model, &mut capture, &directory)?;

    model.set(music::Model::default());
    show(&mut view, &mut shown, window, &mut capture)?;
    assert!(model.get().recent.is_empty());
    click(&mut view, Tag::PlayAlbum)?;
    assert_eq!(model.get().current, tracks[0]);
    assert_eq!(model.get().album, Some(albums[0]));
    assert!(model.get().playing);
    capture.save(&mut view, window, directory.join("music-album.png"))?;
    click(&mut view, Tag::Next)?;
    assert_eq!(model.get().current, tracks[1]);
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, Tag::Navigation(CopyKey::NavRecent))?;
    capture.save(&mut view, window, directory.join("music-recent.png"))?;
    assert_eq!(model.get().recent, vec![tracks[1], tracks[0]]);

    compact_and_scrolled(&model, &theme, icons, &mut capture, &directory)?;
    hero_is_shown_only_where_it_fits(&model, &theme, icons, &mut capture)?;
    the_theme_switch_repaints_the_screen(&model, &theme, icons, &mut capture)
}

fn settled_view(
    model: &State<music::Model>,
    theme: &Theme,
    icons: music::Icons,
    size: Size,
    capture: &mut Capture,
) -> Result<View, Error> {
    let state = model.clone();
    let theme = theme.clone();
    let mut view = View::new(lavashell_example_support::platform(), move |ui| {
        music::music(ui, &state, &theme, icons);
    });
    let window = Viewport::new(size, Density::ONE);
    let mut shown = SceneBuilder::default();
    show(&mut view, &mut shown, window, capture)?;
    while view.needs_frame() {
        show(&mut view, &mut shown, window, capture)?;
    }
    Ok(view)
}

fn hero_is_shown_only_where_it_fits(
    model: &State<music::Model>,
    theme: &Theme,
    icons: music::Icons,
    capture: &mut Capture,
) -> Result<(), Error> {
    model.set(music::Model::default());
    let tall = settled_view(model, theme, icons, WINDOW, capture)?;
    let hero = control(&tall, Tag::Hero)?;
    let chrome = WINDOW.height - control(&tall, Tag::CollectionScroll)?.height;
    for (hero_fits, height) in [
        (true, chrome + hero.height),
        (false, chrome + hero.height - 1.dp()),
    ] {
        let size = Size::new(WINDOW.width, height);
        let view = settled_view(model, theme, icons, size, capture)?;
        assert_eq!(view.bounds_of(&Tag::Hero).is_some(), hero_fits);
    }
    Ok(())
}

fn compact_and_scrolled(
    model: &State<music::Model>,
    theme: &Theme,
    icons: music::Icons,
    capture: &mut Capture,
    directory: &Path,
) -> Result<(), Error> {
    let window = Viewport::new(WINDOW, Density::ONE);
    let mut shown = SceneBuilder::default();
    model.set(music::Model::default());
    let state = model.clone();
    let shown_theme = theme.clone();
    let mut compact = View::new(lavashell_example_support::platform(), move |ui| {
        music::music(ui, &state, &shown_theme, icons);
    });
    capture.save(
        &mut compact,
        Viewport::new(Size::new(900.dp(), 850.dp()), Density::ONE),
        directory.join("music-compact.png"),
    )?;

    let state = model.clone();
    let shown_theme = theme.clone();
    let mut scrolled = View::new(lavashell_example_support::platform(), move |ui| {
        music::music(ui, &state, &shown_theme, icons);
    });
    show(&mut scrolled, &mut shown, window, capture)?;
    let area = control(&scrolled, Tag::CollectionScroll)?;
    scrolled.input(InputEvent::Scroll {
        at: Point::new(area.x + 20.dp(), area.y + 40.dp()),
        delta: Point::new(Dp::ZERO, 360.dp()),
    });
    capture.save(&mut scrolled, window, directory.join("music-scrolled.png"))?;
    let current = control(&scrolled, Tag::Track(model.get().current))?;
    assert!(current.y >= area.y && current.y + current.height < WINDOW.height - PLAYER_HEIGHT);
    let r = control(&scrolled, Tag::PlayToggle)?;
    scrolled.input(InputEvent::Move(Point::new(r.x + 10.dp(), r.y + 10.dp())));
    let tree = scene(&mut scrolled, window, capture.painter.shaper())?;
    assert!(
        (tree.at(Duration::ZERO).leaf_commands())
            .any(|c| matches!(c, DrawCommand::Text { text, .. } if text == copy(CopyKey::Play)))
    );
    Ok(())
}

fn empty_states(
    view: &mut View,
    model: &State<music::Model>,
    capture: &mut Capture,
    directory: &Path,
) -> Result<(), Error> {
    let window = Viewport::new(WINDOW, Density::ONE);
    model.update(|m| {
        m.favorites.clear();
        m.query.clear();
        m.section = music::Section::Favorites;
    });
    capture.save(view, window, directory.join("music-empty-favorites.png"))?;
    let tree = scene(view, window, capture.painter.shaper())?;
    let commands: Vec<_> = tree.at(Duration::ZERO).leaf_commands().collect();
    assert!(commands.iter().any(
        |c| matches!(c, DrawCommand::Text { text, .. } if text == copy(CopyKey::CollectionEmpty))
    ));
    assert!(!commands.iter().any(
        |c| matches!(c, DrawCommand::Text { text, .. } if text == copy(CopyKey::SearchEmpty))
    ));

    model.set(music::Model::default());
    model.update(|m| m.query = "not-in-the-sample-collection".into());
    capture.save(view, window, directory.join("music-empty-search.png"))?;
    let tree = scene(view, window, capture.painter.shaper())?;
    assert!(tree.at(Duration::ZERO).leaf_commands().any(
        |c| matches!(c, DrawCommand::Text { text, .. } if text == copy(CopyKey::SearchEmpty))
    ));
    Ok(())
}

fn panel_reaches_the_library(
    library: &View,
    model: &State<music::Model>,
    theme: &Theme,
    icons: music::Icons,
    capture: &mut Capture,
) -> Result<(), Error> {
    let state = model.clone();
    let theme = theme.clone();
    let mut panel = View::new(lavashell_example_support::platform(), move |ui| {
        music::panel(ui, &state, &theme, icons);
    });
    let bar = Viewport::new(Size::new(WINDOW.width, PLAYER_HEIGHT), Density::ONE);
    show(&mut panel, &mut SceneBuilder::default(), bar, capture)?;
    let previous = model.get().current;
    click(&mut panel, Tag::Next)?;
    assert_ne!(model.get().current, previous);
    assert!(
        library.needs_frame(),
        "a panel action must invalidate the other view"
    );
    Ok(())
}

fn the_theme_switch_repaints_the_screen(
    model: &State<music::Model>,
    theme: &Theme,
    icons: music::Icons,
    capture: &mut Capture,
) -> Result<(), Error> {
    model.set(music::Model::default());
    let window = Viewport::new(WINDOW, Density::ONE);
    let mut view = settled_view(model, theme, icons, WINDOW, capture)?;
    let mut page_fill = |view: &mut View| -> Result<Option<DrawCommand>, Error> {
        let tree = scene(view, window, capture.painter.shaper())?;
        Ok(tree.at(Duration::ZERO).leaf_commands().next().cloned())
    };
    let by_system = page_fill(&mut view)?;
    click(&mut view, ThemeChoice::Light)?;
    assert_ne!(page_fill(&mut view)?, by_system);
    click(&mut view, ThemeChoice::Dark)?;
    assert_eq!(page_fill(&mut view)?, by_system);
    Ok(())
}
