// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::prelude::*;
use lavashell_example_support::{
    Capture,
    testing::{click, control, node, scene, without_failures},
};
use lavashell_music_player::{
    Error,
    music::{self, CopyKey, PLAYER_HEIGHT, copy},
};
use std::{path::Path, time::Duration};

/// The size of the desktop window in which the tests show the library. The window is wide enough for the sidebar
/// next to the album row and the track list.
const WINDOW: Size = Size::new(Dp::new(1400.0), Dp::new(1000.0));

fn show(
    view: &mut View,
    tree: &mut SceneTree,
    viewport: Viewport,
    capture: &mut Capture,
) -> Result<(), Error> {
    without_failures(view.show_frame(tree, viewport, &mut capture.painter)).map_err(Error::Example)
}

#[test]
fn controls_update_the_shared_model_and_render_every_screen() -> Result<(), Error> {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("music-player");
    let model = State::new(music::Model::default());
    let icons = music::Icons::decode()?;
    let mut capture = Capture::new()?;
    let state = model.clone();
    let mut view = View::new(lavashell_example_support::platform(), move |ui| {
        music::music(ui, &state, icons);
    });
    let mut shown = SceneTree::default();
    let size = WINDOW;
    let window = Viewport::new(size, Density::ONE);
    capture.save(&mut view, window, directory.join("music.png"))?;

    click(&mut view, "play-toggle")?;
    assert!(model.get().playing);
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, "next")?;
    assert_eq!(model.get().current, 3);
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, "save-current")?;
    assert!(model.get().favorites.contains(&3));
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, "shuffle")?;
    assert!(model.get().shuffle);
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, "repeat")?;
    assert!(model.get().repeat);
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, "seek")?;
    assert!((model.get().position.as_secs_f64() - 91.5).abs() < 1.0);
    show(&mut view, &mut shown, window, &mut capture)?;

    let r = control(&view, "volume")?;
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

    click(&mut view, "search-input")?;
    view.input(InputEvent::Text("glass".into()));
    assert_eq!(model.get().query, "glass");
    capture.save(&mut view, window, directory.join("music-search.png"))?;
    assert!(view.bounds_of("track-4").is_some());
    assert!(view.bounds_of("track-0").is_none());
    model.update(|m| m.query.clear());
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, "nav-favorites")?;
    capture.save(&mut view, window, directory.join("music-favorites.png"))?;
    assert!(view.bounds_of("track-3").is_some());
    assert!(view.bounds_of("track-6").is_none());

    panel_reaches_the_library(&view, &model, icons, &mut capture)?;

    empty_states(&mut view, &model, &mut capture, &directory)?;

    model.set(music::Model::default());
    show(&mut view, &mut shown, window, &mut capture)?;
    assert!(model.get().recent.is_empty());
    click(&mut view, "play-album")?;
    assert_eq!(model.get().current, 0);
    assert_eq!(model.get().album, Some(0));
    assert!(model.get().playing);
    capture.save(&mut view, window, directory.join("music-album.png"))?;
    click(&mut view, "next")?;
    assert_eq!(model.get().current, 1);
    show(&mut view, &mut shown, window, &mut capture)?;
    click(&mut view, "nav-recent")?;
    capture.save(&mut view, window, directory.join("music-recent.png"))?;
    assert_eq!(model.get().recent, vec![1, 0]);

    compact_and_scrolled(&model, icons, &mut capture, &directory)
}

/// Checks that the first track ends above the player panel in a compact window. Then checks that the current track
/// is visible above the player panel after a scroll, and that the pointer over the play button shows the text Play.
fn compact_and_scrolled(
    model: &State<music::Model>,
    icons: music::Icons,
    capture: &mut Capture,
    directory: &Path,
) -> Result<(), Error> {
    let window = Viewport::new(WINDOW, Density::ONE);
    let mut shown = SceneTree::default();
    model.set(music::Model::default());
    let state = model.clone();
    let mut compact = View::new(lavashell_example_support::platform(), move |ui| {
        music::music(ui, &state, icons);
    });
    capture.save(
        &mut compact,
        Viewport::new(Size::new(900.dp(), 850.dp()), Density::ONE),
        directory.join("music-compact.png"),
    )?;
    let first = control(&compact, "track-0")?;
    assert!(first.y + first.height <= 850.dp() - PLAYER_HEIGHT);

    let state = model.clone();
    let mut scrolled = View::new(lavashell_example_support::platform(), move |ui| {
        music::music(ui, &state, icons);
    });
    show(&mut scrolled, &mut shown, window, capture)?;
    let area = control(&scrolled, "collection-scroll")?;
    scrolled.input(InputEvent::Scroll {
        at: Point::new(area.x + 20.dp(), area.y + 40.dp()),
        delta: Point::new(Dp::ZERO, 360.dp()),
    });
    capture.save(&mut scrolled, window, directory.join("music-scrolled.png"))?;
    let current = control(&scrolled, "track-2")?;
    assert!(current.y >= area.y && current.y + current.height < WINDOW.height - PLAYER_HEIGHT);
    let r = control(&scrolled, "play-toggle")?;
    scrolled.input(InputEvent::Move(Point::new(r.x + 10.dp(), r.y + 10.dp())));
    let tree = scene(&mut scrolled, window, &mut capture.painter)?;
    assert!(
        (tree.at(Duration::ZERO).leaf_commands())
            .any(|c| matches!(c, DrawCommand::Text { text, .. } if text == copy(CopyKey::Play)))
    );
    Ok(())
}

/// Checks the hints of an empty favorites list and of a search without results.
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
    let tree = scene(view, window, &mut capture.painter)?;
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
    let tree = scene(view, window, &mut capture.painter)?;
    assert!(tree.at(Duration::ZERO).leaf_commands().any(
        |c| matches!(c, DrawCommand::Text { text, .. } if text == copy(CopyKey::SearchEmpty))
    ));
    Ok(())
}

/// Checks that the player panel changes the model that the library view shows.
fn panel_reaches_the_library(
    library: &View,
    model: &State<music::Model>,
    icons: music::Icons,
    capture: &mut Capture,
) -> Result<(), Error> {
    let state = model.clone();
    let mut panel = View::new(lavashell_example_support::platform(), move |ui| {
        music::panel(ui, &state, icons);
    });
    let bar = Viewport::new(Size::new(WINDOW.width, PLAYER_HEIGHT), Density::ONE);
    show(&mut panel, &mut SceneTree::default(), bar, capture)?;
    let previous = model.get().current;
    click(&mut panel, "next")?;
    assert_ne!(model.get().current, previous);
    assert!(
        library.needs_frame(),
        "a panel action must invalidate the other view"
    );
    Ok(())
}
