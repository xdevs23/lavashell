// SPDX-License-Identifier: GPL-3.0-only
#[derive(Clone, Copy, Debug)]
pub struct SampleTrack {
    pub title: &'static str,
    pub artist: &'static str,
    pub hue: f64,
}

pub const HARBOR_LIGHTS: SampleTrack = SampleTrack {
    title: "Harbor lights",
    artist: "Tide Theory",
    hue: 220.0,
};

pub const UNDERTOW: SampleTrack = SampleTrack {
    title: "Undertow",
    artist: "Signal",
    hue: 303.0,
};
