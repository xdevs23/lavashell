// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Error,
    islands::BAR,
    model::{App, Daylight, Taskbar},
};
use lavashell::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    Trip(bool),
    Dock(bool),
    Expand(bool),
    Foreground(App),
    Daylight(Daylight),
    Press(App),
    Release(App),
    Tab,
    TapMap,
}

/// The cues of one cycle of the demo, in order. Each cue plays once the scene has come to rest after the previous
/// cue.
pub const CUES: [Cue; 15] = [
    Cue::Trip(true),
    Cue::Dock(true),
    Cue::Dock(false),
    Cue::Expand(true),
    Cue::Expand(false),
    Cue::Press(App::Music),
    Cue::Release(App::Music),
    Cue::Tab,
    Cue::Tab,
    Cue::Tab,
    Cue::TapMap,
    Cue::Trip(false),
    Cue::Foreground(App::Navigation),
    Cue::Daylight(Daylight::Day),
    Cue::Daylight(Daylight::Night),
];

/// The position of the next cue in [`CUES`]. After the last cue, the script starts again at the first cue.
#[derive(Default)]
pub struct Script {
    next: usize,
}

impl Script {
    pub fn next_cue(&mut self) -> Cue {
        let cue = CUES[self.next];
        self.next = (self.next + 1) % CUES.len();
        cue
    }

    /// Plays the next cue on `view`. The offscreen player and the autoplay in the window both call this function, so
    /// both play the same cues.
    ///
    /// # Errors
    /// Returns an error when the view has no taskbar or no slot for the app of the cue.
    pub fn play(&mut self, taskbar: &Taskbar, view: &mut View) -> Result<(), Error> {
        self.next_cue().apply(taskbar, view)
    }
}

impl Cue {
    /// # Errors
    /// Returns an error when the scene has no taskbar or no slot for the app of the cue.
    pub fn apply(self, taskbar: &Taskbar, view: &mut View) -> Result<(), Error> {
        match self {
            Self::Trip(trip) => taskbar.trip.set(trip),
            Self::Dock(docked) => taskbar.docked.set(docked),
            Self::Expand(expanded) => taskbar.expanded.set(expanded),
            Self::Foreground(app) => taskbar.foreground.set(app),
            Self::Daylight(daylight) => taskbar.daylight.set(daylight),
            Self::Press(app) => {
                let at = slot(view, app)?;
                view.input(InputEvent::Move(at));
                view.input(InputEvent::Press(at));
            }
            Self::Release(app) => view.input(InputEvent::Release(slot(view, app)?)),
            Self::Tab => view.input(InputEvent::Key {
                key: Key::Tab,
                shift: false,
            }),
            Self::TapMap => {
                let bar = view.bounds_of(BAR).ok_or(Error::MissingBar)?;
                let at = Rect::new(bar.x, Dp::ZERO, bar.width, bar.y).center();
                view.input(InputEvent::Press(at));
                view.input(InputEvent::Release(at));
            }
        }
        Ok(())
    }
}

fn slot(view: &View, app: App) -> Result<Point, Error> {
    let bounds = view.bounds_of(app.name()).ok_or(Error::MissingSlot(app))?;
    Ok(bounds.center())
}

#[cfg(test)]
mod tests;
