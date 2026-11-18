// SPDX-License-Identifier: GPL-3.0-only
use crate::{CopyKey, Error};
use lavashell::prelude::*;
use lavashell_example_support::{
    HARBOR_LIGHTS, SampleTrack, Theme, ThemeChoice, UNDERTOW, icon, lucide,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum App {
    Navigation,
    Music,
    Radio,
}

impl App {
    pub const ALL: [Self; 3] = [Self::Navigation, Self::Music, Self::Radio];

    #[must_use]
    pub fn label(self) -> CopyKey {
        match self {
            Self::Navigation => CopyKey::Navigation,
            Self::Music => CopyKey::Music,
            Self::Radio => CopyKey::Radio,
        }
    }

    #[must_use]
    pub fn accent(self) -> Color {
        match self {
            Self::Navigation => Color::oklch(0.627, 0.141, 148.0),
            Self::Music => Color::oklch(0.564, 0.226, 303.0),
            Self::Radio => Color::oklch(0.594, 0.195, 258.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Playlist {
    #[default]
    Undertow,
    HarborLights,
    Glasshouse,
}

impl Playlist {
    #[must_use]
    pub fn current(self) -> SampleTrack {
        match self {
            Self::Undertow => UNDERTOW,
            Self::HarborLights => HARBOR_LIGHTS,
            Self::Glasshouse => SampleTrack {
                title: "Glasshouse",
                artist: "Mara Vale",
                hue: 25.0,
            },
        }
    }

    pub fn next(&mut self) {
        *self = match self {
            Self::Undertow => Self::HarborLights,
            Self::HarborLights => Self::Glasshouse,
            Self::Glasshouse => Self::Undertow,
        };
    }

    pub fn previous(&mut self) {
        *self = match self {
            Self::Undertow => Self::Glasshouse,
            Self::HarborLights => Self::Undertow,
            Self::Glasshouse => Self::HarborLights,
        };
    }
}

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

    #[must_use]
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
    pub theme: Theme,
    pub icons: Icons,
}

impl Taskbar {
    pub fn new(theme: ThemeChoice) -> Result<Self, Error> {
        Ok(Self {
            foreground: State::new(App::Navigation),
            playlist: State::new(Playlist::default()),
            playing: State::new(true),
            trip: State::new(false),
            docked: State::new(false),
            expanded: State::new(false),
            theme: Theme::new(theme)?,
            icons: Icons::decode()?,
        })
    }
}

#[cfg(test)]
mod tests;
