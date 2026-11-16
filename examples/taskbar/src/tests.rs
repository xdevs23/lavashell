// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    CopyKey, Error, compose,
    islands::Island,
    look::{SCREEN, SPACING},
    model::{App, Taskbar},
    pause, player,
    script::{CUES, Cue, Script},
    tag::Tag,
};
use lavashell::prelude::*;
use lavashell_example_support::testing::{click, control, interaction};
use lavashell_example_support::{FrameInterval, OffscreenView, ThemeChoice};
use std::{collections::BTreeSet, path::Path, time::Duration};

const ISLANDS: [Island; 3] = [Island::Apps, Island::Media, Island::Status];

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Taskbar(#[from] Error),
    #[error("cannot find the test binary, beside which the captures are written")]
    TestBinary(#[source] std::io::Error),
    #[error("a recording needs a time between its frames")]
    NoInterval,
}

struct Player {
    taskbar: Taskbar,
    script: Script,
    offscreen: OffscreenView,
}

impl Player {
    fn new(size: Size) -> Result<Self, Error> {
        let taskbar = Taskbar::new(ThemeChoice::System)?;
        let composed = taskbar.clone();
        let view = View::new(lavashell_example_support::platform(), move |ui| {
            compose(ui, &composed);
        });
        let mut player = Self {
            taskbar,
            script: Script::default(),
            offscreen: OffscreenView::new(view, Some(size), Density::ONE)?,
        };
        player.offscreen.settle()?;
        Ok(player)
    }

    fn cue(&mut self) -> Result<(), Error> {
        (self.script).play(&self.taskbar, &mut self.offscreen.view)?;
        Ok(self.offscreen.frame()?)
    }

    fn halfway(&self) -> Result<Duration, Error> {
        let deadline = self.offscreen.next_deadline()?;
        Ok(deadline.saturating_sub(self.offscreen.time()) / 2)
    }

    fn midway(&mut self) -> Result<(), Error> {
        let time = self.offscreen.time() + self.halfway()?;
        Ok(self.offscreen.at(time)?)
    }

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

    fn bounds_of(&self, tag: Tag) -> Result<Rect, Error> {
        Ok(control(&self.offscreen.view, tag)?)
    }

    fn island(&self, island: Island) -> Result<Rect, Error> {
        self.bounds_of(Tag::Island(island))
    }

    fn gap(&self, left: Island, right: Island) -> Result<Dp, Error> {
        let (left, right) = (self.island(left)?, self.island(right)?);
        Ok(right.x - (left.x + left.width))
    }

    fn state(&self, tag: Tag) -> Result<InteractionState, Error> {
        Ok(interaction(&self.offscreen.view, tag)?)
    }

    fn pixel(&self) -> Dp {
        self.offscreen.viewport().pixel()
    }
}

#[test]
fn the_islands_trip_dock_and_morph_and_return_to_rest() -> Result<(), TestError> {
    let captures = std::env::current_exe()
        .map_err(TestError::TestBinary)?
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

    let controls = player.bounds_of(Tag::Track)?;
    player.cue()?;
    player.midway()?;
    assert!(
        player.island(Island::Media)?.height > media.height,
        "the media island opens"
    );
    let moved = player.bounds_of(Tag::Track)?.y - controls.y;
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
    theme(&mut player, &captures)?;

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
    player.offscreen.frame().map_err(Error::from)?;
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

fn controls_and_focus(player: &mut Player, captures: &Path) -> Result<(), Error> {
    let compositions = player.offscreen.view.stats().root_compositions;
    player.cue()?;
    player.settle()?;
    let pressed = player.state(Tag::Slot(App::Music))?;
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
        player.state(Tag::Slot(App::Radio))?.focus.visible,
        "a tab shows the focus ring on the next slot"
    );
    player.capture(captures, "focus")?;
    for _ in 0..2 {
        player.cue()?;
        player.settle()?;
    }
    assert!(player.state(Tag::Control(CopyKey::Previous))?.focus.visible);
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

fn theme(player: &mut Player, captures: &Path) -> Result<(), Error> {
    for _ in 0..2 {
        player.cue()?;
        player.settle()?;
    }
    let compositions = player.offscreen.view.stats().root_compositions;
    player.cue()?;
    player.midway()?;
    player.capture(captures, "to-light")?;
    player.settle()?;
    assert_eq!(player.taskbar.theme.choice(), ThemeChoice::Light);
    player.capture(captures, "light")?;
    assert_eq!(
        player.offscreen.view.stats().root_compositions,
        compositions + 1,
        "the glass of the bar moves to the light tint without composing again"
    );
    player.cue()?;
    player.midway()?;
    player.capture(captures, "to-dark")?;
    player.settle()?;
    click(&mut player.offscreen.view, ThemeChoice::System)?;
    assert_eq!(player.taskbar.theme.choice(), ThemeChoice::System);
    Ok(())
}

#[test]
fn a_recording_shows_every_cue_in_at_least_one_frame() -> Result<(), TestError> {
    let interval = FrameInterval::new(Duration::from_secs(1) / 10).ok_or(TestError::NoInterval)?;
    let shown: BTreeSet<_> = (player::record_cues(interval, SCREEN, ThemeChoice::System)?
        .into_iter())
    .map(|(cue, _)| cue)
    .collect();
    assert_eq!(shown, (0..CUES.len()).collect());
    Ok(())
}
