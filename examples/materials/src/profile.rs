// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Profile {
    Srgb,
    #[value(name = "p3")]
    DisplayP3,
    Hdr10,
    Rec2020,
    #[value(name = "scrgb")]
    ScRgb,
}

impl Profile {
    pub fn request(self) -> OutputRequest {
        match self {
            Self::Srgb => OutputRequest::default(),
            Self::DisplayP3 => OutputRequest::new(OutputProfile::DisplayP3, Precision::Ten),
            Self::Hdr10 => OutputRequest::new(OutputProfile::Hdr10, Precision::Ten),
            Self::Rec2020 => OutputRequest::new(OutputProfile::Rec2020, Precision::Float16),
            Self::ScRgb => OutputRequest::new(OutputProfile::ScRgb, Precision::Float16),
        }
    }
}
