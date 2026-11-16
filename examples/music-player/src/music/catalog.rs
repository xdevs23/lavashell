// SPDX-License-Identifier: GPL-3.0-only
use lavashell_example_support::HARBOR_LIGHTS;
use std::{fmt, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AlbumId(usize);

impl AlbumId {
    pub fn all() -> impl Iterator<Item = Self> {
        (0..ALBUMS.len()).map(Self)
    }

    #[must_use]
    pub fn album(self) -> &'static Album {
        &ALBUMS[self.0]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrackId(usize);

impl TrackId {
    pub(super) const fn at(position: usize) -> Self {
        Self(position)
    }

    pub fn all() -> impl Iterator<Item = Self> {
        (0..TRACKS.len()).map(Self)
    }

    #[must_use]
    pub fn track(self) -> &'static Track {
        &TRACKS[self.0]
    }

    #[must_use]
    pub fn number(self) -> usize {
        self.0 + 1
    }
}

impl fmt::Display for TrackId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

#[derive(Clone, Copy)]
pub enum Cover {
    Waves,
    Panes,
    Bars,
    Orbs,
}

pub struct Album {
    pub title: &'static str,
    pub artist: &'static str,
    pub year: u32,
    pub genre: &'static str,
    pub cover: Cover,
    pub palette: [u32; 3],
}

pub const FEATURED_ALBUM: AlbumId = AlbumId(0);

pub static ALBUMS: [Album; 4] = [
    Album {
        title: "Harbor Radio",
        artist: HARBOR_LIGHTS.artist,
        year: 2025,
        genre: "Electronic",
        cover: Cover::Waves,
        palette: [0x0031_5b66, 0x001a_3547, 0x00e1_b59a],
    },
    Album {
        title: "Glasshouse",
        artist: "Mara Vale",
        year: 2024,
        genre: "Alternative",
        cover: Cover::Panes,
        palette: [0x0036_564c, 0x0017_2e2c, 0x00d4_daa5],
    },
    Album {
        title: "After Hours",
        artist: "Northline",
        year: 2025,
        genre: "Jazz",
        cover: Cover::Bars,
        palette: [0x0079_4336, 0x0034_2538, 0x00eb_c39b],
    },
    Album {
        title: "Soft Focus",
        artist: "June Atlas",
        year: 2023,
        genre: "Ambient",
        cover: Cover::Orbs,
        palette: [0x0078_7eaf, 0x0031_3e69, 0x00e9_cfc6],
    },
];

pub struct Track {
    pub title: &'static str,
    pub album: AlbumId,
    pub duration: Duration,
}

pub static TRACKS: [Track; 10] = [
    Track {
        title: HARBOR_LIGHTS.title,
        album: AlbumId(0),
        duration: Duration::from_secs(224),
    },
    Track {
        title: "Blue hour",
        album: AlbumId(0),
        duration: Duration::from_secs(198),
    },
    Track {
        title: "Low tide",
        album: AlbumId(0),
        duration: Duration::from_secs(256),
    },
    Track {
        title: "Between the waves",
        album: AlbumId(0),
        duration: Duration::from_secs(183),
    },
    Track {
        title: "Glasshouse",
        album: AlbumId(1),
        duration: Duration::from_secs(241),
    },
    Track {
        title: "Paper windows",
        album: AlbumId(1),
        duration: Duration::from_secs(209),
    },
    Track {
        title: "After hours",
        album: AlbumId(2),
        duration: Duration::from_secs(278),
    },
    Track {
        title: "The long way home",
        album: AlbumId(2),
        duration: Duration::from_secs(233),
    },
    Track {
        title: "Soft focus",
        album: AlbumId(3),
        duration: Duration::from_secs(304),
    },
    Track {
        title: "Drift",
        album: AlbumId(3),
        duration: Duration::from_secs(267),
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Playlist {
    pub name: &'static str,
    pub album: AlbumId,
}

pub const PLAYLISTS: [Playlist; 3] = [
    Playlist {
        name: "Open water",
        album: AlbumId(0),
    },
    Playlist {
        name: "Night ferry",
        album: AlbumId(1),
    },
    Playlist {
        name: "Analog hours",
        album: AlbumId(2),
    },
];
