// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::{
    CopyKey, copy,
    look::{self, Palette},
};
use lavashell::keyboard::{
    ENGLISH, GERMAN, Keyboard, KeyboardIcons, KeyboardSettings, KeyboardStyle, KeyboardText,
    Layout, PORTUGUESE, Target,
};
use lavashell::prelude::*;
use lavashell_example_support::{icon, lucide};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Night,
    Day,
}

impl Appearance {
    #[must_use]
    pub fn palette(self) -> &'static Palette {
        match self {
            Self::Night => &look::NIGHT,
            Self::Day => &look::DAY,
        }
    }

    #[must_use]
    pub fn keyboard_style(self) -> KeyboardStyle {
        match self {
            Self::Night => KeyboardStyle::night(),
            Self::Day => KeyboardStyle::day(),
        }
    }
}

/// The height of the keyboard on the car screen of the sample.
pub const PANEL_HEIGHT: NonNegativeDp = constant!(NonNegativeDp::try_new(Dp::new(350.0)));

/// The words on the keys, from the copy catalog of the sample.
fn words() -> KeyboardText {
    let word = |key| copy(key).to_owned();
    KeyboardText {
        keyboard: word(CopyKey::KeyboardPanel),
        layouts: word(CopyKey::KeyboardLayouts),
        more_letters: word(CopyKey::MoreLetters),
        search: word(CopyKey::ActionSearch),
        done: word(CopyKey::ActionDone),
        next: word(CopyKey::ActionNext),
        go: word(CopyKey::ActionGo),
        send: word(CopyKey::ActionSend),
        enter: word(CopyKey::ActionEnter),
        symbols_label: word(CopyKey::SymbolsLabel),
        letters_label: word(CopyKey::LettersLabel),
        more_symbols_label: word(CopyKey::MoreSymbolsLabel),
        symbols: word(CopyKey::Symbols),
        letters: word(CopyKey::Letters),
        more_symbols: word(CopyKey::MoreSymbols),
        shift: word(CopyKey::Shift),
        delete: word(CopyKey::Delete),
        globe: word(CopyKey::Globe),
        hide: word(CopyKey::Hide),
        space: word(CopyKey::Space),
    }
}

pub struct Track {
    pub title: &'static str,
    pub artist: &'static str,
    pub length: &'static str,
    pub hue: f64,
}

pub static TRACKS: [Track; 4] = [
    Track {
        title: "Glasshouse (Live at the Harbour Sessions, Extended)",
        artist: "Mara Vale",
        length: "9:12",
        hue: 25.0,
    },
    Track {
        title: "Glasshouse",
        artist: "Mara Vale",
        length: "4:18",
        hue: 40.0,
    },
    Track {
        title: "Harbor lights",
        artist: "Tide Theory",
        length: "3:55",
        hue: 220.0,
    },
    Track {
        title: "Undertow",
        artist: "Signal",
        length: "5:02",
        hue: 303.0,
    },
];

/// The icons of the scene around the keyboard.
#[derive(Clone, Copy)]
pub struct SceneIcons {
    pub back: &'static Icon,
    pub search: &'static Icon,
    pub clear: &'static Icon,
    pub launcher: &'static Icon,
    pub music: &'static Icon,
    pub navigation: &'static Icon,
    pub signal: &'static Icon,
    pub battery: &'static Icon,
}

impl SceneIcons {
    fn decode() -> Result<Self, Error> {
        Ok(Self {
            back: icon(&lucide::ARROW_LEFT)?,
            search: icon(&lucide::SEARCH)?,
            clear: icon(&lucide::X)?,
            launcher: icon(&lucide::GRIP)?,
            music: icon(&lucide::MUSIC)?,
            navigation: icon(&lucide::NAVIGATION)?,
            signal: icon(&lucide::SIGNAL)?,
            battery: icon(&lucide::BATTERY_MEDIUM)?,
        })
    }
}

fn keyboard_icons() -> Result<KeyboardIcons, Error> {
    Ok(KeyboardIcons {
        search: icon(&lucide::SEARCH)?,
        done: icon(&lucide::CHECK)?,
        next: icon(&lucide::ARROW_RIGHT_TO_LINE)?,
        go: icon(&lucide::ARROW_RIGHT)?,
        send: icon(&lucide::SEND)?,
        enter: icon(&lucide::CORNER_DOWN_LEFT)?,
        shift: icon(&lucide::ARROW_BIG_UP)?,
        caps_lock: icon(&lucide::ARROW_BIG_UP_DASH)?,
        delete: icon(&lucide::DELETE)?,
        globe: icon(&lucide::GLOBE)?,
        hide: icon(&lucide::KEYBOARD_OFF)?,
        active_layout: icon(&lucide::CHECK)?,
    })
}

#[derive(Clone)]
pub struct Demo {
    pub query: State<String>,
    pub action: State<TextAction>,
    pub appearance: State<Appearance>,
    pub settings: State<KeyboardSettings>,
    pub layout: State<&'static Layout>,
    pub keyboard: Keyboard,
    pub icons: SceneIcons,
}

impl Demo {
    /// # Errors
    /// Returns an error when an icon of the scene or the keyboard does not decode, or when the
    /// keyboard cannot be made.
    pub fn new(appearance: Appearance) -> Result<Self, Error> {
        let settings = State::new(KeyboardSettings {
            layouts: vec![&ENGLISH, &GERMAN, &PORTUGUESE],
            text: words(),
            icons: keyboard_icons()?,
            panel_height: PANEL_HEIGHT,
        });
        let layout = State::new(&ENGLISH);
        Ok(Self {
            query: State::new(String::new()),
            action: State::new(TextAction::Search),
            appearance: State::new(appearance),
            keyboard: Keyboard::new(Target::View, &settings, layout.clone())
                .map_err(Error::Keyboard)?,
            settings,
            layout,
            icons: SceneIcons::decode()?,
        })
    }

    #[must_use]
    pub fn route(&self, input: InputEvent) -> Option<InputEvent> {
        match input {
            InputEvent::Key { key, .. } if self.keyboard.navigate(key) => None,
            input => Some(input),
        }
    }
}
