// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell::prelude::*;
use lavashell_example_support::{icon, lucide};

/// The icons of the music player, named as their Lucide files.
#[derive(Clone, Copy)]
pub struct Icons {
    pub music: &'static Icon,
    pub heart: &'static Icon,
    pub clock: &'static Icon,
    pub list: &'static Icon,
    pub search: &'static Icon,
    pub play: &'static Icon,
    pub pause: &'static Icon,
    pub shuffle: &'static Icon,
    pub skip_back: &'static Icon,
    pub skip_forward: &'static Icon,
    pub repeat: &'static Icon,
    pub volume: &'static Icon,
}

impl Icons {
    /// # Errors
    /// Returns an error when an icon does not decode.
    pub fn decode() -> Result<Self, Error> {
        Ok(Self {
            music: icon(&lucide::MUSIC)?,
            heart: icon(&lucide::HEART)?,
            clock: icon(&lucide::CLOCK)?,
            list: icon(&lucide::LIST)?,
            search: icon(&lucide::SEARCH)?,
            play: icon(&lucide::PLAY)?,
            pause: icon(&lucide::PAUSE)?,
            shuffle: icon(&lucide::SHUFFLE)?,
            skip_back: icon(&lucide::SKIP_BACK)?,
            skip_forward: icon(&lucide::SKIP_FORWARD)?,
            repeat: icon(&lucide::REPEAT)?,
            volume: icon(&lucide::VOLUME_2)?,
        })
    }
}
