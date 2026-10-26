// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::board::Grid;

const MARGIN: Dp = Dp::new(12.0);

pub(super) fn docked(side: Side, width: Dp) -> Result<Placement, TestError> {
    Ok(Placement::Docked(Dock {
        side,
        width: NonNegativeDp::try_new(width)?,
        margin: NonNegativeDp::try_new(MARGIN)?,
    }))
}

/// The bounds of a panel `width` wide at `x`, at rest a margin above the bottom of the screen.
fn docked_panel(x: Dp, width: Dp) -> Rect {
    let height = PANEL.get();
    Rect::new(x, SCREEN.height - MARGIN - height, width, height)
}

/// The corners of the glass of the panel as painted in `scene`.
fn glass_corners(scene: &Scene) -> Option<Corners> {
    let mut commands = leaves(scene).into_iter().flat_map(|(_, commands)| commands);
    commands.find_map(|command| match command {
        DrawCommand::Backdrop {
            shape: Shape::Rounded(shape),
            ..
        } => Some(shape.corners),
        _ => None,
    })
}

fn panel(host: &Host) -> Result<Rect, TestError> {
    Ok(node(&host.view, &Role::Group, "Keyboard")?.bounds)
}

/// An icon of one filled path, which the tests give to the search key. The other icons of the tests
/// have no paths.
static MARK: Icon = Icon {
    grid: Size::new(Dp::new(24.0), Dp::new(24.0)),
    paths: &[IconPath {
        start: Point::new(Dp::ZERO, Dp::ZERO),
        segments: &[
            Segment::Line(Point::new(Dp::new(24.0), Dp::ZERO)),
            Segment::Line(Point::new(Dp::new(24.0), Dp::new(24.0))),
        ],
        closed: true,
        stroke: None,
    }],
};

/// The points on the screen where the paths of the icons in `scene` start.
fn icon_starts(scene: &Scene) -> Vec<Point> {
    let starts = painted(scene, &|command| match command {
        DrawCommand::Path { start, .. } => Some(Rect::new(start.x, start.y, Dp::ZERO, Dp::ZERO)),
        _ => None,
    });
    (starts.iter())
        .map(|start| Point::new(start.x, start.y))
        .collect()
}

/// The bounds on the screen of each painted `word` in `scene`.
fn word_bounds(scene: &Scene, word: &str) -> Vec<Rect> {
    painted(scene, &|command| match command {
        DrawCommand::Text { text, rect, .. } if text == word => Some(*rect),
        _ => None,
    })
}

fn searching() -> TextHints {
    TextHints {
        action: Some(TextAction::Search),
        ..TextHints::default()
    }
}

#[test]
fn a_docked_panel_rests_in_its_corner_above_the_margin_with_four_rounded_corners()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.placement.set(docked(Side::Left, 640.dp())?);
    host.start(TextHints::default(), "")?;
    assert!(near(panel(&host)?, docked_panel(MARGIN, 640.dp())));
    assert!(near(backdrops(&host.scene)[0], panel(&host)?));
    let corners = glass_corners(&host.scene);
    let equal =
        |corners: Corners| corners.top_left > Dp::ZERO && corners == Corners::all(corners.top_left);
    assert!(
        corners.is_some_and(equal),
        "four equal rounded corners: {corners:?}"
    );
    let keyboard = composed_bounds(&host.view, "keyboard")?;
    assert_eq!(keyboard.height, PANEL.get() + MARGIN);
    assert!(ends_at_glass(&host.view, &host.scene));
    let q = key_bounds(&host.view, "q")?;
    assert!(q.x > MARGIN && q.x + q.width < MARGIN + 640.dp());
    Ok(())
}

#[test]
fn a_panel_docked_on_the_right_ends_a_margin_before_the_right_edge() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.placement.set(docked(Side::Right, 640.dp())?);
    host.start(TextHints::default(), "")?;
    let x = SCREEN.width - MARGIN - 640.dp();
    assert!(near(panel(&host)?, docked_panel(x, 640.dp())));
    Ok(())
}

#[test]
fn a_dock_wider_than_the_screen_takes_the_width_between_both_margins() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.placement.set(docked(Side::Right, 2000.dp())?);
    host.start(TextHints::default(), "")?;
    let width = SCREEN.width - MARGIN * 2.0;
    assert!(near(panel(&host)?, docked_panel(MARGIN, width)));
    Ok(())
}

#[test]
fn a_panel_too_narrow_for_its_padding_and_gaps_gives_its_keys_no_negative_width() {
    let panel = Rect::new(Dp::ZERO, Dp::ZERO, 20.dp(), PANEL.get());
    let screen = Rect::from_size(SCREEN);
    let grid = Grid::new(panel, screen, 20.0, KEY_HEIGHT);
    assert!(grid.width(1.0) >= Dp::ZERO && grid.width(2.0) >= Dp::ZERO);
}

#[test]
fn the_open_panel_moves_to_a_new_placement_on_the_show_spring_with_the_content_above()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.start(TextHints::default(), "")?;
    let full = panel(&host)?;
    let corners = glass_corners(&host.scene);
    let top_only = |corners: Corners| {
        corners.top_left > Dp::ZERO
            && corners.top_right == corners.top_left
            && corners.bottom_left == Dp::ZERO
            && corners.bottom_right == Dp::ZERO
    };
    assert!(
        corners.is_some_and(top_only),
        "the top corners of a full-width panel are rounded: {corners:?}"
    );
    let start = host.time;
    host.placement.set(docked(Side::Left, 640.dp())?);
    host.frame()?;
    for time in [40, 80, 160, 240] {
        host.at(start + time)?;
        let glass = backdrops(&host.scene)[0];
        assert!(
            glass.width < full.width && glass.width > 600.dp(),
            "the glass narrows {time} ms after the change: {glass:?}"
        );
        assert!(ends_at_glass(&host.view, &host.scene), "at {time} ms");
    }
    host.settle()?;
    assert!(near(
        backdrops(&host.scene)[0],
        docked_panel(MARGIN, 640.dp())
    ));
    host.placement.set(Placement::FullWidth);
    host.settle()?;
    assert!(near(backdrops(&host.scene)[0], full));
    assert!(ends_at_glass(&host.view, &host.scene));
    Ok(())
}

#[test]
fn a_docked_panel_covers_the_margin_below_it_and_the_cover_moves_with_the_panel()
-> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.placement.set(docked(Side::Left, 640.dp())?);
    host.start(TextHints::default(), "")?;
    assert_eq!(host.keyboard.covered(), PANEL.get() + MARGIN);
    host.placement.set(Placement::FullWidth);
    host.frame()?;
    host.at(host.time + 60)?;
    let moving = host.keyboard.covered();
    assert!(
        moving > PANEL.get() && moving < PANEL.get() + MARGIN,
        "{moving:?} while the panel moves"
    );
    host.settle()?;
    assert_eq!(host.keyboard.covered(), PANEL.get());
    Ok(())
}

#[test]
fn a_placement_set_while_the_panel_rises_or_sinks_moves_it_without_a_jump() -> Result<(), TestError>
{
    let mut host = Host::new(settings())?;
    host.placement.set(docked(Side::Left, 640.dp())?);
    let empty = FieldText::BeforeCursor("".into());
    host.session
        .set(Some(session(SessionId(1), TextHints::default(), empty)));
    host.frame()?;
    host.at(60)?;
    let rising = host.keyboard.height();
    host.placement.set(Placement::FullWidth);
    host.frame()?;
    assert_eq!(host.keyboard.height(), rising, "while the panel rises");
    for time in [80, 160, 320] {
        host.at(time)?;
        assert!(ends_at_glass(&host.view, &host.scene), "at {time} ms");
    }
    host.settle()?;
    assert_eq!(host.keyboard.height(), PANEL.get());
    host.placement.set(docked(Side::Left, 640.dp())?);
    host.settle()?;
    host.session.set(None);
    host.frame()?;
    let start = host.time;
    host.at(start + 200)?;
    let sinking = host.keyboard.height();
    assert!(sinking > Dp::ZERO);
    host.placement.set(Placement::FullWidth);
    host.frame()?;
    assert_eq!(host.keyboard.height(), sinking, "while the panel sinks");
    host.at(start + 240)?;
    assert!(ends_at_glass(&host.view, &host.scene));
    host.settle()?;
    assert_eq!(host.keyboard.height(), Dp::ZERO);
    Ok(())
}

#[test]
fn a_dock_grows_out_of_the_island_and_the_content_ends_at_its_glass_while_it_moves()
-> Result<(), TestError> {
    let placement = State::new(docked(Side::Left, 640.dp())?);
    let (mut view, field, _) = super::stages::with_taskbar(placement.clone())?;
    let mut scene = Scene::default();
    frame(&mut view, &mut scene, 0)?;
    let empty = FieldText::BeforeCursor("".into());
    field.set(Some(session(SessionId(1), TextHints::default(), empty)));
    frame(&mut view, &mut scene, 0)?;
    settle(&mut view, &mut scene)?;
    assert!(near(backdrops(&scene)[0], docked_panel(MARGIN, 640.dp())));
    assert!(ends_at_glass(&view, &scene));
    placement.set(Placement::FullWidth);
    refresh(&mut view, &mut scene)?;
    let start = scene.now;
    for time in [40, 120, 240] {
        frame_at(&mut view, &mut scene, start + Duration::from_millis(time))?;
        assert!(ends_at_glass(&view, &scene), "{time} ms after the change");
    }
    settle(&mut view, &mut scene)?;
    assert!(ends_at_glass(&view, &scene), "at rest");
    Ok(())
}

#[test]
fn the_action_key_shows_its_icon_alone_where_its_word_does_not_fit() -> Result<(), TestError> {
    let marked = KeyboardSettings {
        icons: KeyboardIcons {
            search: &MARK,
            ..ICONS
        },
        ..settings()
    };
    let mut host = Host::new(marked)?;
    host.start(searching(), "")?;
    let key = key_bounds(&host.view, "Search")?;
    let words = word_bounds(&host.scene, "Search");
    assert!(
        words.len() == 1 && words.iter().all(|word| key.contains(word.center())),
        "the word is painted inside {key:?}: {words:?}"
    );
    let icons = icon_starts(&host.scene);
    assert!(
        icons.len() == 1 && icons.iter().all(|icon| key.contains(*icon)),
        "one icon beside the word inside {key:?}: {icons:?}"
    );
    host.placement.set(docked(Side::Left, 640.dp())?);
    host.settle()?;
    assert!(word_bounds(&host.scene, "Search").is_empty());
    let key = key_bounds(&host.view, "Search")?;
    let icons = icon_starts(&host.scene);
    assert!(
        icons.len() == 1 && icons.iter().all(|icon| key.contains(*icon)),
        "one icon alone inside {key:?}: {icons:?}"
    );
    tap(&mut host.view, "Search")?;
    assert_eq!(host.sent(), [TextCommand::Action(TextAction::Search)]);
    Ok(())
}
