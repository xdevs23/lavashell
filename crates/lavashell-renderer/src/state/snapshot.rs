// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::image::Pixels;
use skia_safe::Canvas;

/// The whole frame painted into a surface, before the surface presents it.
pub struct Snapshot<'a> {
    canvas: &'a Canvas,
}

impl<'a> Snapshot<'a> {
    pub(crate) fn new(canvas: &'a Canvas) -> Self {
        Self { canvas }
    }

    /// The pixels of the frame, read in the format of every PNG export.
    ///
    /// # Errors
    /// Fails when Skia cannot read the pixels of the frame.
    pub fn pixels(&self) -> Result<Pixels, Error> {
        Pixels::read(self.canvas)
    }
}
