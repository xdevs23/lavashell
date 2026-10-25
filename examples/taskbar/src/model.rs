// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell::prelude::*;
use lavashell_example_support::{icon, lucide};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum App {
    Navigation,
    Music,
    Radio,
}

impl App {
    pub const ALL: [Self; 3] = [Self::Navigation, Self::Music, Self::Radio];

    pub fn name(self) -> &'static str {
        match self {
            Self::Navigation => "Navigation",
            Self::Music => "Music",
            Self::Radio => "Radio",
        }
    }

    pub fn accent(self) -> Color {
        match self {
            Self::Navigation => Color::oklch(0.627, 0.141, 148.0),
            Self::Music => Color::oklch(0.564, 0.226, 303.0),
            Self::Radio => Color::oklch(0.594, 0.195, 258.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Daylight {
    Day,
    Night,
}

#[derive(Clone, Copy, Debug)]
pub struct Track {
    pub title: &'static str,
    pub artist: &'static str,
    pub hue: f64,
}

const TRACKS: [Track; 3] = [
    Track {
        title: "Undertow",
        artist: "Signal",
        hue: 303.0,
    },
    Track {
        title: "Harbor lights",
        artist: "Tide Theory",
        hue: 220.0,
    },
    Track {
        title: "Glasshouse",
        artist: "Mara Vale",
        hue: 25.0,
    },
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Playlist(usize);

impl Playlist {
    pub fn current(self) -> Track {
        TRACKS[self.0]
    }

    pub fn next(&mut self) {
        self.0 = (self.0 + 1) % TRACKS.len();
    }

    pub fn previous(&mut self) {
        self.0 = (self.0 + TRACKS.len() - 1) % TRACKS.len();
    }
}

/// The icons shown in the taskbar. Each field has the name of the Lucide icon file that it is decoded from.
#[derive(Clone, Copy)]
pub struct Icons {
    pub navigation: &'static Icon,
    pub music: &'static Icon,
    pub radio: &'static Icon,
    pub skip_back: &'static Icon,
    pub play: &'static Icon,
    pub pause: &'static Icon,
    pub skip_forward: &'static Icon,
}

impl Icons {
    fn decode() -> Result<Self, Error> {
        Ok(Self {
            navigation: icon(&lucide::NAVIGATION)?,
            music: icon(&lucide::MUSIC)?,
            radio: icon(&lucide::RADIO)?,
            skip_back: icon(&lucide::SKIP_BACK)?,
            play: icon(&lucide::PLAY)?,
            pause: icon(&lucide::PAUSE)?,
            skip_forward: icon(&lucide::SKIP_FORWARD)?,
        })
    }

    pub fn of(self, app: App) -> &'static Icon {
        match app {
            App::Navigation => self.navigation,
            App::Music => self.music,
            App::Radio => self.radio,
        }
    }
}

#[derive(Clone)]
pub struct Taskbar {
    pub foreground: State<App>,
    pub playlist: State<Playlist>,
    pub playing: State<bool>,
    pub trip: State<bool>,
    pub docked: State<bool>,
    pub expanded: State<bool>,
    pub daylight: State<Daylight>,
    pub icons: Icons,
}

impl Taskbar {
    /// # Errors
    /// Returns an error when an icon does not decode.
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            foreground: State::new(App::Navigation),
            playlist: State::new(Playlist::default()),
            playing: State::new(true),
            trip: State::new(false),
            docked: State::new(false),
            expanded: State::new(false),
            daylight: State::new(Daylight::Night),
            icons: Icons::decode()?,
        })
    }
}

#[cfg(test)]
mod tests;
