// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, RecordedFrame};
use lavashell_actor::{Merge, MergeEffect, Sender};
use lavashell_core::frame::FrameTime;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{FrameOutcome, PresentFrame, SurfaceControl};
use lavashell_renderer::Pixels;
use lavashell_renderer::skia::Canvas;
use std::cell::OnceCell;

pub struct FrameCapture {
    surface: SurfaceId,
    mode: CaptureMode,
    shown: Sender<ShownFrame>,
    painted: Option<ShownFrame>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureMode {
    WithoutPixels,
    WithPixels,
}

pub(crate) struct ShownFrame {
    pub(crate) surface: SurfaceId,
    pub(crate) time: FrameTime,
    pub(crate) pixels: Option<Pixels>,
    pub(crate) png: OnceCell<Vec<u8>>,
}

impl FrameCapture {
    pub(crate) fn new(surface: SurfaceId, mode: CaptureMode, shown: Sender<ShownFrame>) -> Self {
        Self {
            surface,
            mode,
            shown,
            painted: None,
        }
    }
}

impl ShownFrame {
    pub(crate) fn recorded(&self) -> Result<Option<RecordedFrame>, Error> {
        let Some(pixels) = &self.pixels else {
            return Ok(None);
        };
        let surface = self.surface;
        let png = if let Some(png) = self.png.get() {
            png
        } else {
            let encoded =
                (pixels.png()).map_err(|source| Error::EncodeFrame { surface, source })?;
            self.png.get_or_init(|| encoded)
        };
        Ok(Some(RecordedFrame {
            surface,
            time: self.time,
            png: png.clone(),
        }))
    }
}

impl CaptureMode {
    fn read_pixels_if_wanted(
        self,
        canvas: &Canvas,
    ) -> Result<Option<Pixels>, lavashell_renderer::Error> {
        match self {
            Self::WithoutPixels => Ok(None),
            Self::WithPixels => Pixels::read(canvas).map(Some),
        }
    }
}

impl SurfaceControl for FrameCapture {
    type Error = Error;
    type Message = CaptureMode;

    fn receive(&mut self, mode: CaptureMode) {
        self.mode = mode;
    }

    fn painted(&mut self, frame: &PresentFrame, canvas: &Canvas) -> Result<(), Error> {
        let surface = self.surface;
        let pixels = (self.mode.read_pixels_if_wanted(canvas))
            .map_err(|source| Error::ReadFrame { surface, source })?;
        self.painted = Some(ShownFrame {
            surface,
            time: frame.time,
            pixels,
            png: OnceCell::new(),
        });
        Ok(())
    }

    fn after_present(&mut self, outcome: FrameOutcome) {
        if let (FrameOutcome::Shown, Some(frame)) = (outcome, self.painted.take()) {
            let _script_gone_after_the_run = self.shown.push(frame);
        }
    }
}

impl Merge for ShownFrame {
    type Key = ();

    fn effect(&self) -> MergeEffect<()> {
        MergeEffect::Appends
    }
}
