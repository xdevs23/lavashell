// SPDX-License-Identifier: GPL-3.0-only
use super::coverage::RgbBlend;
use super::coverage_sum::CoverageSum;
use super::face_backdrop::FaceBackdrop;
use crate::Error;

pub(crate) struct TextBlends {
    pub(super) coverage_sum: CoverageSum,
    pub(super) rgb: RgbBlend,
    pub(super) face_backdrop: FaceBackdrop,
}

impl TextBlends {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            coverage_sum: CoverageSum::new()?,
            rgb: RgbBlend::new()?,
            face_backdrop: FaceBackdrop::new()?,
        })
    }
}
