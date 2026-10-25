// SPDX-License-Identifier: GPL-3.0-only
use std::time::Duration;

pub struct Album {
    pub title: &'static str,
    pub artist: &'static str,
    pub year: u32,
    pub genre: &'static str,
}

pub const ALBUMS: [Album; 4] = [
    Album {
        title: "Harbor Radio",
        artist: "Tide Theory",
        year: 2025,
        genre: "Electronic",
    },
    Album {
        title: "Glasshouse",
        artist: "Mara Vale",
        year: 2024,
        genre: "Alternative",
    },
    Album {
        title: "After Hours",
        artist: "Northline",
        year: 2025,
        genre: "Jazz",
    },
    Album {
        title: "Soft Focus",
        artist: "June Atlas",
        year: 2023,
        genre: "Ambient",
    },
];

pub struct Track {
    pub title: &'static str,
    pub album: usize,
    pub duration: Duration,
}

pub const TRACKS: [Track; 10] = [
    Track {
        title: "Harbor lights",
        album: 0,
        duration: Duration::from_secs(224),
    },
    Track {
        title: "Blue hour",
        album: 0,
        duration: Duration::from_secs(198),
    },
    Track {
        title: "Low tide",
        album: 0,
        duration: Duration::from_secs(256),
    },
    Track {
        title: "Between the waves",
        album: 0,
        duration: Duration::from_secs(183),
    },
    Track {
        title: "Glasshouse",
        album: 1,
        duration: Duration::from_secs(241),
    },
    Track {
        title: "Paper windows",
        album: 1,
        duration: Duration::from_secs(209),
    },
    Track {
        title: "After hours",
        album: 2,
        duration: Duration::from_secs(278),
    },
    Track {
        title: "The long way home",
        album: 2,
        duration: Duration::from_secs(233),
    },
    Track {
        title: "Soft focus",
        album: 3,
        duration: Duration::from_secs(304),
    },
    Track {
        title: "Drift",
        album: 3,
        duration: Duration::from_secs(267),
    },
];

pub const PLAYLISTS: [&str; 3] = ["Open water", "Night ferry", "Analog hours"];
