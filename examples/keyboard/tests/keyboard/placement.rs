// SPDX-License-Identifier: GPL-3.0-only
use super::{Play, near, set, settled, shot};
use lavashell::keyboard::{Placement, Side};
use lavashell::prelude::*;
use lavashell_on_screen_keyboard::{
    Error,
    model::{Demo, docked},
    scene::{self, FIELD},
    stage::{QUERY, finger, key, panel},
};

/// Whether `panel` is docked at `side`. A docked panel is narrower than the screen, ends above the
/// bottom edge of the screen and is nearer the edge at `side` than the other edge.
fn docked_at(panel: Rect, side: Side) -> bool {
    let screen = scene::screen();
    let (left, right) = (panel.x, screen.width - panel.x - panel.width);
    let nearer = match side {
        Side::Left => left < right,
        Side::Right => right < left,
    };
    nearer && panel.width < screen.width && panel.y + panel.height < screen.height
}

/// Whether `panel` lies across the whole bottom of the screen.
fn full_width(panel: Rect) -> bool {
    let screen = scene::screen();
    near(panel.x, Dp::ZERO)
        && near(panel.width, screen.width)
        && near(panel.y + panel.height, screen.height)
}

pub(super) fn dock(play: &mut Play) {
    for (side, name) in [(Side::Left, "left"), (Side::Right, "right")] {
        play.push(settled(), set(|demo| &demo.placement, docked(side)));
        play.check(settled(), move |view, _, demo| {
            let panel = panel(view, demo)?.bounds;
            assert!(docked_at(panel, side), "{panel:?} at {side:?}");
            let field = (view.bounds_of(FIELD)).ok_or(Error::MissingField)?;
            assert!(
                field.y + field.height <= panel.y,
                "the field is in view above the dock"
            );
            Ok(())
        });
        shot(play, settled(), &format!("dock-{name}"));
    }
    play.push(settled(), set(|demo| &demo.placement, Placement::FullWidth));
}

/// Puts one finger on the key named `first` and one on the key named `second`, moves the second
/// finger `by` sideways in two steps and lifts both. The first step starts the transform. The
/// second step changes the scale.
fn pinch_keys(play: &mut Play, first: &'static str, second: &'static str, by: Dp) {
    play.push(settled(), move |view, _, _| {
        let (start, end) = (key(view, first)?, key(view, second)?);
        let halfway = Point::new(end.x + by / 2.0, end.y);
        let moved = Point::new(end.x + by, end.y);
        for input in [
            InputEvent::TouchDown(finger(1, start)),
            InputEvent::TouchDown(finger(2, end)),
            InputEvent::TouchMove(finger(2, halfway)),
            InputEvent::TouchMove(finger(2, moved)),
            InputEvent::TouchUp(finger(2, moved)),
            InputEvent::TouchUp(finger(1, start)),
        ] {
            view.input(input);
        }
        Ok(())
    });
}

/// Checks that the demo has the placement `placement` and shows its panel where `shown` holds.
fn placed(
    placement: Placement,
    shown: impl FnOnce(Rect) -> bool + 'static,
) -> impl FnOnce(&View, Viewport, &Demo) -> Result<(), Error> {
    move |view, _, demo| {
        assert_eq!(demo.placement.get(), placement);
        let panel = panel(view, demo)?.bounds;
        assert!(shown(panel), "{panel:?}");
        Ok(())
    }
}

pub(super) fn pinch(play: &mut Play) {
    pinch_keys(play, "d", "k", -250.dp());
    let left = docked(Side::Left);
    play.check(
        settled(),
        placed(left, |panel| docked_at(panel, Side::Left)),
    );
    pinch_keys(play, "s", "k", 350.dp());
    play.check(settled(), placed(Placement::FullWidth, full_width));
    pinch_keys(play, "d", "k", -50.dp());
    play.check(settled(), |_, _, demo| {
        assert_eq!(
            demo.placement.get(),
            Placement::FullWidth,
            "fingers that drift a little leave the placement as it is"
        );
        assert_eq!(
            demo.query.get(),
            QUERY,
            "a pinch leaves the query unchanged"
        );
        Ok(())
    });
}
