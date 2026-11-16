// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Error,
    model::{App, Taskbar},
    tag::Tag,
};
use lavashell::prelude::*;
use lavashell_example_support::ThemeChoice;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    Trip(bool),
    Dock(bool),
    Expand(bool),
    Foreground(App),
    Theme(ThemeChoice),
    Press(App),
    Release(App),
    Tab,
    TapMap,
}

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
    Cue::Theme(ThemeChoice::Light),
    Cue::Theme(ThemeChoice::Dark),
];

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

    pub fn play(&mut self, taskbar: &Taskbar, view: &mut View) -> Result<(), Error> {
        self.next_cue().apply(taskbar, view)
    }
}

impl Cue {
    pub fn apply(self, taskbar: &Taskbar, view: &mut View) -> Result<(), Error> {
        match self {
            Self::Trip(trip) => taskbar.trip.set(trip),
            Self::Dock(docked) => taskbar.docked.set(docked),
            Self::Expand(expanded) => taskbar.expanded.set(expanded),
            Self::Foreground(app) => taskbar.foreground.set(app),
            Self::Theme(choice) => taskbar.theme.choose(choice),
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
                let bar = view.bounds_of(&Tag::Bar).ok_or(Error::MissingBar)?;
                let at = Rect::new(bar.x, Dp::ZERO, bar.width, bar.y).center();
                view.input(InputEvent::Press(at));
                view.input(InputEvent::Release(at));
            }
        }
        Ok(())
    }
}

fn slot(view: &View, app: App) -> Result<Point, Error> {
    let bounds = view
        .bounds_of(&Tag::Slot(app))
        .ok_or(Error::MissingSlot(app))?;
    Ok(bounds.center())
}

#[cfg(test)]
mod tests;
