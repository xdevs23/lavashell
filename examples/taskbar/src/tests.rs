// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    CopyKey, Error, copy,
    islands::{Island, TRACK},
    look::{SCREEN, SPACING},
    model::{App, Daylight},
    pause,
    player::Player,
    script::{CUES, Cue},
};
use lavashell::prelude::*;
use lavashell_example_support::testing::{control, interaction};
use std::{path::Path, time::Duration};

const ISLANDS: [Island; 3] = [Island::Apps, Island::Media, Island::Status];

impl Player {
    /// Half the time left until the next deadline of the view.
    fn halfway(&self) -> Result<Duration, Error> {
        let deadline = self.offscreen.next_deadline()?;
        Ok(deadline.saturating_sub(self.offscreen.time()) / 2)
    }

    /// Frames the view halfway to its next deadline, while the motions that end at that deadline are still running.
    fn midway(&mut self) -> Result<(), Error> {
        let time = self.offscreen.time() + self.halfway()?;
        Ok(self.offscreen.at(time)?)
    }

    /// Frames the view at time steps that start at one nanosecond and double after each frame, until `reached` returns
    /// `true` for the gap between the islands `left` and `right`. The short first steps put the first frames close to
    /// the start of a motion.
    fn play_until_gap(
        &mut self,
        left: Island,
        right: Island,
        reached: impl Fn(Dp) -> bool,
    ) -> Result<(), Error> {
        let mut step = Duration::from_nanos(1);
        while !reached(self.gap(left, right)?) {
            let time = (self.offscreen.time() + step).min(self.offscreen.next_deadline()?);
            self.offscreen.at(time)?;
            step *= 2;
        }
        Ok(())
    }

    fn settle(&mut self) -> Result<(), Error> {
        Ok(self.offscreen.settle()?)
    }

    fn capture(&mut self, directory: &Path, name: &str) -> Result<(), Error> {
        Ok((self.offscreen).save(directory.join(format!("{name}.png")))?)
    }

    fn bounds_of(&self, tag: &str) -> Result<Rect, Error> {
        Ok(control(&self.offscreen.view, tag)?)
    }

    fn island(&self, island: Island) -> Result<Rect, Error> {
        self.bounds_of(island.tag())
    }

    fn gap(&self, left: Island, right: Island) -> Result<Dp, Error> {
        let (left, right) = (self.island(left)?, self.island(right)?);
        Ok(right.x - (left.x + left.width))
    }

    fn state(&self, tag: &str) -> Result<InteractionState, Error> {
        Ok(interaction(&self.offscreen.view, tag)?)
    }

    /// The size of one pixel of the captures.
    fn pixel(&self) -> Dp {
        self.offscreen.viewport().pixel()
    }
}

#[test]
fn the_islands_trip_dock_and_morph_and_return_to_rest() -> Result<(), Error> {
    let captures = std::env::current_exe()
        .map_err(Error::TestBinary)?
        .with_file_name("taskbar-captures");
    let mut player = Player::new(SCREEN)?;
    let snapped = player.pixel() / 2.0;
    let [apps, media, status] = ISLANDS.map(|island| player.island(island));
    let (media, status) = (media?, status?);
    let rest = [apps?, media, status];
    assert!(
        player.gap(Island::Apps, Island::Media)? > SPACING
            && player.gap(Island::Media, Island::Status)? > SPACING
    );
    player.capture(&captures, "rest")?;

    player.cue()?;
    player.midway()?;
    let entering = player.island(Island::Status)?;
    assert!(
        entering.width > status.width,
        "the status island grows with its new part"
    );
    player.capture(&captures, "enter")?;
    player.settle()?;
    assert!(player.island(Island::Status)?.width > entering.width);

    player.cue()?;
    player.play_until_gap(Island::Apps, Island::Media, |gap| gap < SPACING)?;
    assert!(
        player.gap(Island::Apps, Island::Media)? > Dp::ZERO,
        "the islands fuse before they touch"
    );
    player.capture(&captures, "merge")?;
    player.settle()?;
    assert!(
        player.gap(Island::Apps, Island::Media)?.abs() < snapped,
        "the docked islands touch"
    );
    player.capture(&captures, "merged")?;
    player.cue()?;
    player.play_until_gap(Island::Apps, Island::Media, |gap| gap > Dp::ZERO)?;
    let gap = player.gap(Island::Apps, Island::Media)?;
    assert!(
        gap < SPACING,
        "the islands part after they move apart: {gap:?}"
    );
    player.capture(&captures, "split")?;
    player.settle()?;
    assert!(player.gap(Island::Apps, Island::Media)? > SPACING);

    let controls = player.bounds_of(TRACK)?;
    player.cue()?;
    player.midway()?;
    assert!(
        player.island(Island::Media)?.height > media.height,
        "the media island opens"
    );
    let moved = player.bounds_of(TRACK)?.y - controls.y;
    assert!(
        moved.abs() < player.pixel(),
        "the controls stay in place: {moved:?}"
    );
    player.capture(&captures, "morph")?;
    player.settle()?;
    assert!(
        player.island(Island::Media)?.height > media.height,
        "the media island rests open around the card"
    );
    player.capture(&captures, "card")?;
    player.cue()?;
    player.settle()?;
    assert!((player.island(Island::Media)?.height - media.height).abs() < snapped);
    controls_and_focus(&mut player, &captures)?;
    daylight(&mut player, &captures)?;

    player.settle()?;
    for (island, bounds) in ISLANDS.into_iter().zip(rest) {
        let offset = player
            .island(island)?
            .vector()
            .sub(&bounds.vector())
            .max_abs();
        assert!(offset < snapped.get(), "{island:?} returns to its place");
    }
    player.offscreen.view.provide(&REDUCED_MOTION, true);
    Cue::Dock(true).apply(&player.taskbar, &mut player.offscreen.view)?;
    player.offscreen.frame()?;
    assert!(player.gap(Island::Apps, Island::Media)?.abs() < snapped);
    assert!(
        !player.offscreen.view.motion().is_active(),
        "reduced motion settles every change"
    );
    Ok(())
}

#[test]
fn reduced_motion_keeps_each_cue_on_screen_as_long_as_its_motion_takes() -> Result<(), Error> {
    let mut moving = Player::new(SCREEN)?;
    let mut reduced = Player::new(SCREEN)?;
    reduced.offscreen.view.provide(&REDUCED_MOTION, true);
    for cue in CUES {
        let start = moving.offscreen.time();
        moving.cue()?;
        moving.settle()?;
        reduced.cue()?;
        reduced.settle()?;
        let held = pause(reduced.offscreen.view.motion());
        assert_eq!(start + held, moving.offscreen.time(), "{cue:?}");
        reduced.offscreen.at(reduced.offscreen.time() + held)?;
    }
    Ok(())
}

/// Plays the press and release of an app slot, the three tabs and the tap on the map.
fn controls_and_focus(player: &mut Player, captures: &Path) -> Result<(), Error> {
    let compositions = player.offscreen.view.stats().root_compositions;
    player.cue()?;
    player.settle()?;
    let pressed = player.state(App::Music.name())?;
    assert!(pressed.pointer.active && pressed.focus.focused);
    assert!(!pressed.focus.visible, "a tap hides the focus ring");
    player.capture(captures, "press")?;
    player.cue()?;
    player.settle()?;
    assert_eq!(player.taskbar.foreground.get(), App::Music);
    player.capture(captures, "tint")?;
    assert_eq!(
        player.offscreen.view.stats().root_compositions,
        compositions + 1,
        "only the change of the selected app composes the taskbar again"
    );

    let compositions = player.offscreen.view.stats().root_compositions;
    player.cue()?;
    player.settle()?;
    assert!(
        player.state(App::Radio.name())?.focus.visible,
        "a tab shows the focus ring on the next slot"
    );
    player.capture(captures, "focus")?;
    for _ in 0..2 {
        player.cue()?;
        player.settle()?;
    }
    assert!(player.state(copy(CopyKey::Previous))?.focus.visible);
    player.cue()?;
    player.settle()?;
    assert!(
        player.offscreen.view.focus().is_none(),
        "a tap on the map clears the focus"
    );
    assert_eq!(
        player.offscreen.view.stats().root_compositions,
        compositions,
        "focus changes only paint"
    );
    Ok(())
}

/// Plays the end of the trip, the change of the foreground app and the change to day and back.
fn daylight(player: &mut Player, captures: &Path) -> Result<(), Error> {
    for _ in 0..2 {
        player.cue()?;
        player.settle()?;
    }
    let compositions = player.offscreen.view.stats().root_compositions;
    player.cue()?;
    player.midway()?;
    player.capture(captures, "to-day")?;
    player.settle()?;
    assert_eq!(player.taskbar.daylight.get(), Daylight::Day);
    player.capture(captures, "day")?;
    assert_eq!(
        player.offscreen.view.stats().root_compositions,
        compositions + 1,
        "the glass of the bar moves to the day tint without composing again"
    );
    player.cue()?;
    player.midway()?;
    player.capture(captures, "to-night")
}
