// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Error, compose,
    model::Taskbar,
    script::{CUES, Script},
};
use lavashell::prelude::*;
use lavashell_example_support::{OffscreenView, Recording};

pub struct Player {
    pub taskbar: Taskbar,
    script: Script,
    pub offscreen: OffscreenView,
}

impl Player {
    /// A player that plays the cues of the script on an offscreen view of `size`. The view has come to rest before
    /// the first cue plays.
    ///
    /// # Errors
    /// Returns an error when the offscreen view cannot be created or framed.
    pub fn new(size: Size) -> Result<Self, Error> {
        let taskbar = Taskbar::new()?;
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

    /// Plays the next cue of the script and frames the view right after it.
    ///
    /// # Errors
    /// Returns an error when the view has no taskbar or no slot for the app of the cue, or when the frame cannot be
    /// shown.
    pub fn cue(&mut self) -> Result<(), Error> {
        (self.script).play(&self.taskbar, &mut self.offscreen.view)?;
        Ok(self.offscreen.frame()?)
    }
}

/// Records one cycle of the script as numbered PNG files. After each cue, every frame until the scene comes to rest
/// is written into the directory of `recording`.
pub fn record(recording: &Recording, size: Size) -> Result<(), Error> {
    let mut player = Player::new(size)?;
    let mut index = 0_usize;
    for _ in CUES {
        player.cue()?;
        player
            .offscreen
            .record_until_rest(recording.interval, |offscreen| {
                let saved = offscreen.save(recording.directory.join(format!("{index:04}.png")));
                index += 1;
                saved
            })?;
    }
    Ok(())
}
